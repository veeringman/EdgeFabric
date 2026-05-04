//! Agent-local HTTP + WebSocket server for device streaming.
//!
//! Exposes:
//!   GET /ws/stream/{serial}  — screenshot stream (JPEG frames over WS)
//!   GET /ws/logs/{serial}    — device log stream (JSON lines over WS)
//!   GET /health              — agent health check
//!
//! The API server proxies dashboard WebSocket connections here when the
//! device is attached to a remote agent.

use axum::{
    Json, Router,
    extract::{Path, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::process::Stdio;
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

/// Resolve the ffmpeg binary path once (handles non-login shells missing /usr/local/bin).
#[cfg(target_os = "macos")]
fn ffmpeg_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        for p in ["/usr/local/bin/ffmpeg", "/opt/homebrew/bin/ffmpeg", "/usr/bin/ffmpeg"] {
            if std::path::Path::new(p).exists() { return p.to_string(); }
        }
        "ffmpeg".to_string() // fallback to PATH lookup
    })
}

#[cfg(target_os = "macos")]
fn idevicescreenshot_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        for p in [
            "/opt/homebrew/bin/idevicescreenshot",
            "/usr/local/bin/idevicescreenshot",
            "/usr/bin/idevicescreenshot",
        ] {
            if std::path::Path::new(p).exists() { return p.to_string(); }
        }
        "idevicescreenshot".to_string()
    })
}

#[cfg(target_os = "macos")]
fn idevicesyslog_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        for p in [
            "/opt/homebrew/bin/idevicesyslog",
            "/usr/local/bin/idevicesyslog",
            "/usr/bin/idevicesyslog",
        ] {
            if std::path::Path::new(p).exists() { return p.to_string(); }
        }
        "idevicesyslog".to_string()
    })
}

#[cfg(target_os = "macos")]
fn ideviceinfo_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        for p in [
            "/opt/homebrew/bin/ideviceinfo",
            "/usr/local/bin/ideviceinfo",
            "/usr/bin/ideviceinfo",
        ] {
            if std::path::Path::new(p).exists() { return p.to_string(); }
        }
        "ideviceinfo".to_string()
    })
}

fn adb_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        for p in [
            "/opt/homebrew/bin/adb",
            "/usr/local/bin/adb",
            "/usr/bin/adb",
        ] {
            if std::path::Path::new(p).exists() { return p.to_string(); }
        }
        if let Ok(home) = std::env::var("HOME") {
            let sdk = format!("{home}/Library/Android/sdk/platform-tools/adb");
            if std::path::Path::new(&sdk).exists() { return sdk; }
        }
        "adb".to_string()
    })
}

/// Start the agent's streaming HTTP server on the given port.
pub async fn start_streaming_server(
    port: u16,
    veeros_cfg: ef_core::config::VeerOsConfig,
    host_table: crate::veeros_discovery::HostTable,
) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/ws/stream/{*serial}", get(ws_stream_handler))
        .route("/ws/logs/{*serial}", get(ws_logs_handler))
        .route("/ws/ssh", get(ws_ssh_handler))
            .route("/ws/pty", get(ws_pty_handler))
            .route("/health", get(health))
        .merge(crate::veeros_api::router(veeros_cfg, host_table));

    let addr = format!("0.0.0.0:{port}");
    tracing::info!(%addr, "Agent streaming server starting");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({"status": "ok", "service": "ef-agent-streaming"}))
}

// ── Screen streaming ──────────────────────────────────────────────

async fn ws_stream_handler(
    ws: WebSocketUpgrade,
    Path(serial): Path<String>,
) -> impl IntoResponse {
    // Wildcard captures with leading '/' — keep device paths like /dev/ttyACM0 intact
    let serial = if serial.starts_with("/dev/") || serial.starts_with("/dev\\") {
        serial
    } else {
        serial.strip_prefix('/').unwrap_or(&serial).to_string()
    };
    tracing::info!(%serial, "Stream WebSocket upgrade");
    ws.on_upgrade(move |socket| handle_stream(socket, serial))
}

async fn handle_stream(socket: WebSocket, serial: String) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Detect device OS to choose capture method
    let os = detect_device_os(&serial).await;
    tracing::info!(%serial, %os, "Starting capture stream");

    // Resolve device resolution for coordinate mapping in the dashboard
    let resolution = get_device_resolution_for_os(&serial, &os).await;
    let stream_ready = if let Some([w, h]) = resolution {
        json!({"type": "stream_ready", "serial": &serial, "os": &os, "resolution": [w, h]})
    } else {
        json!({"type": "stream_ready", "serial": &serial, "os": &os})
    };
    let _ = ws_tx.send(Message::Text(stream_ready.to_string().into())).await;

    // IoT devices: stream serial console as text, not screenshots
    if os == "iot" {
        run_iot_serial_stream(ws_tx, ws_rx, &serial).await;
        tracing::info!(%serial, "IoT stream ended");
        return;
    }

    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();

    // Input handler task
    let serial_input = serial.clone();
    let os_input = os.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(t) => {
                    let text = t.to_string();
                    if text.contains("stop_stream") {
                        let _ = stop_tx2.send(true);
                        break;
                    }
                    if text.contains("input_event") {
                        handle_input_event(&serial_input, &os_input, &text).await;
                    }
                }
                Message::Close(_) => {
                    let _ = stop_tx2.send(true);
                    break;
                }
                _ => {}
            }
        }
    });

    let mut stop_rx = stop_tx.subscribe();
    let mut frame_count: u64 = 0;

    // Generate per-connection temp file path for iOS capture
    let conn_id = uuid::Uuid::new_v4().simple().to_string();
    let ios_tmp = format!("/tmp/ef-ios-{}-{}.tiff", &serial[..8.min(serial.len())], &conn_id[..8]);

    // Double-buffered capture loop
    let s = serial.clone();
    let o = os.clone();
    let t = ios_tmp.clone();
    let mut pending = tokio::spawn(async move { capture_frame(&s, &o, &t).await });

    loop {
        if *stop_rx.borrow() { break; }

        let result = match pending.await {
            Ok(r) => r,
            Err(_) => break,
        };

        let s = serial.clone();
        let o = os.clone();
        let t = ios_tmp.clone();
        pending = tokio::spawn(async move { capture_frame(&s, &o, &t).await });

        match result {
            Ok(data) => {
                frame_count += 1;
                if ws_tx.send(Message::Binary(data.into())).await.is_err() { break; }
            }
            Err(e) => {
                if frame_count == 0 {
                    tracing::error!(%serial, "Capture failed: {e}");
                    let _ = ws_tx.send(Message::Text(
                        json!({"type":"error","message": format!("{e}")}).to_string().into()
                    )).await;
                    break;
                }
            }
        }
    }

    // Clean up connection-scoped temp file
    let _ = tokio::fs::remove_file(&ios_tmp).await;

    tracing::info!(%serial, frame_count, "Stream ended");
}

// ── Log streaming ─────────────────────────────────────────────────

async fn ws_logs_handler(
    ws: WebSocketUpgrade,
    Path(serial): Path<String>,
) -> impl IntoResponse {
    // Wildcard captures with leading '/' — keep device paths like /dev/ttyACM0 intact
    let serial = if serial.starts_with("/dev/") || serial.starts_with("/dev\\") {
        serial
    } else {
        serial.strip_prefix('/').unwrap_or(&serial).to_string()
    };
    tracing::info!(%serial, "Logs WebSocket upgrade");
    ws.on_upgrade(move |socket| handle_logs(socket, serial))
}

async fn handle_logs(socket: WebSocket, serial: String) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let os = detect_device_os(&serial).await;
    tracing::info!(%serial, %os, "Starting log stream");

    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();

    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if matches!(msg, Message::Close(_)) {
                let _ = stop_tx2.send(true);
                break;
            }
        }
    });

    let mut stop_rx = stop_tx.subscribe();

    // IoT: read serial port directly
    if os == "iot" {
        let _ = tokio::process::Command::new("stty")
            .args(["-F", &serial, "115200", "raw", "-echo"])
            .output()
            .await;

        let mut child = match tokio::process::Command::new("cat")
            .arg(&serial)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = ws_tx.send(Message::Text(
                    json!({"type":"error","message":format!("Failed to open serial port: {e}")}).to_string().into()
                )).await;
                return;
            }
        };

        let stdout = child.stdout.take().unwrap();
        let reader = tokio::io::BufReader::new(stdout);
        let mut lines = reader.lines();

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    match line {
                        Ok(Some(text)) => {
                            let entry = parse_iot_log_line(&text);
                            if ws_tx.send(Message::Text(entry.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
                _ = stop_rx.changed() => break,
            }
        }
        let _ = child.kill().await;
        tracing::info!(%serial, "IoT log stream ended");
        return;
    }

    // Spawn the appropriate log process
    let mut child = match os.as_str() {
        #[cfg(target_os = "macos")]
        "simulator" => {
            // iOS Simulator: use xcrun simctl spawn to stream system log
            match tokio::process::Command::new("xcrun")
                .args(["simctl", "spawn", &serial, "log", "stream", "--level=debug", "--style=compact"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(c) => c,
                Err(_) => {
                    let _ = ws_tx.send(Message::Text(
                        json!({"type":"error","message":"xcrun simctl spawn log failed"}).to_string().into()
                    )).await;
                    return;
                }
            }
        }
        #[cfg(target_os = "macos")]
        "ios" => {
            // Try idevicesyslog first
            match tokio::process::Command::new(idevicesyslog_path())
                .args(["-u", &serial])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => {
                    let _ = ws_tx.send(Message::Text(
                        json!({"type":"error","message":"idevicesyslog not available"}).to_string().into()
                    )).await;
                    return;
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        "simulator" | "ios" => {
            let _ = ws_tx.send(Message::Text(
                json!({"type":"error","message":"iOS/simulator logs require macOS agent"}).to_string().into()
            )).await;
            return;
        }
        _ => {
            // Android: adb logcat
            match tokio::process::Command::new(adb_path())
                .args(["-s", &serial, "logcat", "-v", "threadtime"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => {
                    let _ = ws_tx.send(Message::Text(
                        json!({"type":"error","message":"adb not available"}).to_string().into()
                    )).await;
                    return;
                }
            }
        }
    };

    let stdout = child.stdout.take().unwrap();
    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        let entry = match os.as_str() {
                            "simulator" | "ios" => parse_ios_log_line(&line),
                            _ => parse_logcat_line(&line),
                        };
                        if ws_tx.send(Message::Text(entry.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            _ = stop_rx.changed() => break,
        }
    }

    let _ = child.kill().await;
    tracing::info!(%serial, "Log stream ended");
}

// ── Capture functions (Android + iOS) ─────────────────────────────

/// Returns [width, height] in logical device pixels for coordinate mapping.
/// For iOS: queries WDA /status which includes device screen info, or falls back to ideviceinfo.
/// For Android: queries adb shell wm size.
async fn get_device_resolution_for_os(serial: &str, os: &str) -> Option<[u32; 2]> {
    match os {
        #[cfg(target_os = "macos")]
        "ios" => {
            // Try WDA status first (already running for screenshots)
            if let Ok(resp) = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build().ok()?
                .get("http://127.0.0.1:8100/wda/screen")
                .send().await
            {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let val = &body["value"];
                    // WDA actions use logical points — send logical resolution so dashboard
                    // maps clicks to the same coordinate space WDA expects.
                    let w = val["screenSize"]["width"].as_f64()
                        .or_else(|| val["width"].as_f64())?;
                    let h = val["screenSize"]["height"].as_f64()
                        .or_else(|| val["height"].as_f64())?;
                    return Some([w as u32, h as u32]);
                }
            }
            // Fallback: ideviceinfo — use RenderedDisplayWidth/Height (logical points)
            // rather than PixelWidth/Height (physical pixels), matching WDA coordinate space.
            let out = tokio::process::Command::new(ideviceinfo_path())
                .args(["-u", serial])
                .output().await.ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            let w = text.lines()
                .find(|l| l.contains("RenderedDisplayWidth:"))
                .or_else(|| text.lines().find(|l| l.contains("PixelWidth:")))
                .and_then(|l| l.split(':').nth(1))
                .and_then(|v| v.trim().parse::<u32>().ok())?;
            let h = text.lines()
                .find(|l| l.contains("RenderedDisplayHeight:"))
                .or_else(|| text.lines().find(|l| l.contains("PixelHeight:")))
                .and_then(|l| l.split(':').nth(1))
                .and_then(|v| v.trim().parse::<u32>().ok())?;
            // Divide by scale to get logical points
            let scale = text.lines()
                .find(|l| l.contains("PointsPerPixel:"))
                .and_then(|l| l.split(':').nth(1))
                .and_then(|v| v.trim().parse::<f64>().ok())
                .map(|ppx| 1.0 / ppx)
                .unwrap_or(3.0);
            Some([(w as f64 / scale) as u32, (h as f64 / scale) as u32])
        }
        "android" => {
            let out = tokio::process::Command::new(adb_path())
                .args(["-s", serial, "shell", "wm", "size"])
                .output().await.ok()?;
            let s = String::from_utf8_lossy(&out.stdout);
            // "Physical size: 1080x2340"
            let size_part = s.lines()
                .find(|l| l.contains("Physical size"))
                .or_else(|| s.lines().next())?;
            let dims = size_part.split(':').nth(1)?.trim();
            let mut parts = dims.split('x');
            let w = parts.next()?.trim().parse::<u32>().ok()?;
            let h = parts.next()?.trim().parse::<u32>().ok()?;
            Some([w, h])
        }
        _ => None,
    }
}

async fn capture_frame(serial: &str, os: &str, ios_tmp: &str) -> anyhow::Result<Vec<u8>> {
    match os {
        #[cfg(target_os = "macos")]
        "simulator" => capture_simulator_frame(serial, ios_tmp).await,
        #[cfg(target_os = "macos")]
        "ios" => capture_ios_frame(serial, ios_tmp).await,
        #[cfg(not(target_os = "macos"))]
        "simulator" | "ios" => anyhow::bail!("iOS/simulator capture requires macOS agent"),
        _ => capture_android_frame(serial).await,
    }
}

/// Android: adb exec-out screencap | ffmpeg → JPEG
async fn capture_android_frame(serial: &str) -> anyhow::Result<Vec<u8>> {
    let output = tokio::process::Command::new("sh")
        .args([
            "-c",
            &format!(
                "{} -s {} exec-out screencap -p | {} -y -hide_banner -loglevel error -i - -vframes 1 -f mjpeg -q:v 15 -vf scale=540:-1 -",
                adb_path(),
                shell_escape(serial),
                ffmpeg_path()
            ),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    if output.status.success() && output.stdout.len() > 100 {
        return Ok(output.stdout);
    }

    // Fallback: raw PNG
    let output = tokio::process::Command::new(adb_path())
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .output()
        .await?;
    if output.stdout.len() < 100 {
        anyhow::bail!("screencap too small ({} bytes)", output.stdout.len());
    }
    Ok(output.stdout)
}

/// iOS Simulator: xcrun simctl io screenshot → (optionally ffmpeg → JPEG)
#[cfg(target_os = "macos")]
async fn capture_simulator_frame(serial: &str, tmp: &str) -> anyhow::Result<Vec<u8>> {
    let png_tmp = format!("{tmp}.png");

    let status = tokio::process::Command::new("xcrun")
        .args(["simctl", "io", serial, "screenshot", "--type=png", &png_tmp])
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("xcrun simctl io screenshot failed");
    }

    // Try to convert PNG → JPEG via ffmpeg (smaller, faster for streaming)
    if let Ok(output) = tokio::process::Command::new(ffmpeg_path())
        .args([
            "-y", "-hide_banner", "-loglevel", "error",
            "-i", &png_tmp,
            "-vframes", "1", "-f", "mjpeg", "-q:v", "15", "-vf", "scale=540:-1",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
    {
        if output.status.success() && output.stdout.len() > 100 {
            let _ = tokio::fs::remove_file(&png_tmp).await;
            return Ok(output.stdout);
        }
    }

    // Fallback: return raw PNG (larger but universally supported)
    let data = tokio::fs::read(&png_tmp).await?;
    let _ = tokio::fs::remove_file(&png_tmp).await;
    if data.len() > 100 {
        return Ok(data);
    }

    anyhow::bail!("simulator screenshot too small ({} bytes)", data.len())
}

/// iOS: WDA screenshot (iOS 17+) → fallback idevicescreenshot → ffmpeg → JPEG
#[cfg(target_os = "macos")]
async fn capture_ios_frame(serial: &str, tmp: &str) -> anyhow::Result<Vec<u8>> {
    // iOS 17+ path: try WebDriverAgent screenshot endpoint first.
    // WDA returns a base64-encoded PNG; decode and return as-is.
    if let Ok(png) = capture_ios_via_wda().await {
        return Ok(png);
    }

    // Legacy path (iOS ≤ 16): idevicescreenshot → ffmpeg → JPEG
    // On iOS 17+ this requires Xcode to have prepared the device (mounted DDI).
    let status = tokio::process::Command::new(idevicescreenshot_path())
        .args(["-u", serial, tmp])
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!(
            "idevicescreenshot failed (iOS 17+: start WDA for screenshot support, or open Xcode → Window → Devices to prepare device)"
        );
    }

    // Convert TIFF → JPEG via ffmpeg
    let output = tokio::process::Command::new(ffmpeg_path())
        .args([
            "-y", "-hide_banner", "-loglevel", "error",
            "-i", tmp,
            "-vframes", "1", "-f", "mjpeg", "-q:v", "15", "-vf", "scale=540:-1",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(tmp).await;

    if output.status.success() && output.stdout.len() > 100 {
        return Ok(output.stdout);
    }

    anyhow::bail!("ffmpeg TIFF→JPEG conversion failed")
}

/// Try to grab a screenshot from WDA (WebDriverAgent) at localhost:8100.
/// WDA is the reliable method on iOS 17+ where idevicescreenshot requires a mounted DDI.
/// Returns raw PNG bytes on success.
#[cfg(target_os = "macos")]
async fn capture_ios_via_wda() -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = client
        .get("http://127.0.0.1:8100/screenshot")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("WDA screenshot endpoint returned {}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    // WDA returns { value: "<base64 PNG>" }
    let b64 = body["value"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("WDA response missing value field"))?;

    use base64::Engine as _;
    let png = base64::engine::general_purpose::STANDARD.decode(b64)?;
    if png.len() < 100 {
        anyhow::bail!("WDA returned too-small image ({} bytes)", png.len());
    }
    Ok(png)
}

// ── WDA input helpers (iOS 17+ real device via WebDriverAgent) ────

/// Cached WDA session ID. WDA sessions persist until WDA is restarted.
#[cfg(target_os = "macos")]
static WDA_SESSION: OnceLock<tokio::sync::Mutex<Option<String>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn wda_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("reqwest client")
}

/// Get or create a WDA session, returning the session ID string.
#[cfg(target_os = "macos")]
async fn get_wda_session() -> anyhow::Result<String> {
    let mutex = WDA_SESSION.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut guard = mutex.lock().await;

    // Validate existing session
    if let Some(ref id) = *guard {
        let ok = wda_client()
            .get(format!("http://127.0.0.1:8100/session/{id}"))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        if ok {
            return Ok(id.clone());
        }
        *guard = None;
    }

    // Create new session
    let resp = wda_client()
        .post("http://127.0.0.1:8100/session")
        .json(&serde_json::json!({"capabilities": {"alwaysMatch": {}}}))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("WDA create session failed: {}", resp.status());
    }

    let body: serde_json::Value = resp.json().await?;
    let id = body["sessionId"]
        .as_str()
        .or_else(|| body["value"]["sessionId"].as_str())
        .ok_or_else(|| anyhow::anyhow!("WDA session response missing sessionId"))?
        .to_string();

    tracing::info!(%id, "Created WDA session");
    *guard = Some(id.clone());
    Ok(id)
}

/// POST W3C pointer actions to WDA session.
#[cfg(target_os = "macos")]
async fn wda_actions(actions: serde_json::Value) -> anyhow::Result<()> {
    let id = get_wda_session().await?;
    let resp = wda_client()
        .post(format!("http://127.0.0.1:8100/session/{id}/actions"))
        .json(&serde_json::json!({"actions": actions}))
        .send()
        .await?;
    if !resp.status().is_success() {
        // Invalidate session on failure so next call recreates it
        if let Some(mutex) = WDA_SESSION.get() {
            *mutex.lock().await = None;
        }
        anyhow::bail!("WDA actions failed: {}", resp.status());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn wda_tap(x: f64, y: f64) -> anyhow::Result<()> {
    wda_actions(serde_json::json!([{
        "type": "pointer",
        "id": "finger1",
        "parameters": {"pointerType": "touch"},
        "actions": [
            {"type": "pointerMove", "duration": 0, "x": x as i64, "y": y as i64},
            {"type": "pointerDown", "button": 0},
            {"type": "pause", "duration": 80},
            {"type": "pointerUp", "button": 0}
        ]
    }])).await
}

#[cfg(target_os = "macos")]
async fn wda_long_press(x: f64, y: f64, duration_ms: u64) -> anyhow::Result<()> {
    wda_actions(serde_json::json!([{
        "type": "pointer",
        "id": "finger1",
        "parameters": {"pointerType": "touch"},
        "actions": [
            {"type": "pointerMove", "duration": 0, "x": x as i64, "y": y as i64},
            {"type": "pointerDown", "button": 0},
            {"type": "pause", "duration": duration_ms},
            {"type": "pointerUp", "button": 0}
        ]
    }])).await
}

#[cfg(target_os = "macos")]
async fn wda_swipe(x1: f64, y1: f64, x2: f64, y2: f64, duration_ms: u64) -> anyhow::Result<()> {
    wda_actions(serde_json::json!([{
        "type": "pointer",
        "id": "finger1",
        "parameters": {"pointerType": "touch"},
        "actions": [
            {"type": "pointerMove", "duration": 0, "x": x1 as i64, "y": y1 as i64},
            {"type": "pointerDown", "button": 0},
            {"type": "pointerMove", "duration": duration_ms, "x": x2 as i64, "y": y2 as i64},
            {"type": "pointerUp", "button": 0}
        ]
    }])).await
}

#[cfg(target_os = "macos")]
async fn wda_text(text: &str) -> anyhow::Result<()> {
    let id = get_wda_session().await?;
    let chars: Vec<&str> = std::iter::once(text).collect();
    let resp = wda_client()
        .post(format!("http://127.0.0.1:8100/session/{id}/keys"))
        .json(&serde_json::json!({"value": chars}))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("WDA keys failed: {}", resp.status());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn wda_home() -> anyhow::Result<()> {
    let resp = wda_client()
        .post("http://127.0.0.1:8100/wda/homescreen")
        .json(&serde_json::json!({}))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("WDA homescreen failed: {}", resp.status());
    }
    Ok(())
}

// ── Input handling ────────────────────────────────────────────────

async fn handle_input_event(serial: &str, os: &str, event_json: &str) {
    let parsed: serde_json::Value = match serde_json::from_str(event_json) {
        Ok(v) => v,
        Err(_) => return,
    };
    let event = match parsed.get("event") {
        Some(e) => e,
        None => return,
    };
    let etype = event.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match os {
        #[cfg(target_os = "macos")]
        "simulator" => {
            tracing::info!(%serial, %etype, "Simulator input event");
            match etype {
                "home" => {
                    let _ = tokio::process::Command::new("xcrun")
                        .args(["simctl", "io", serial, "sendkey", "home"])
                        .stderr(Stdio::null())
                        .stdout(Stdio::null())
                        .status()
                        .await;
                }
                "tap" => {
                    let x = event["x"].as_f64().unwrap_or(0.0);
                    let y = event["y"].as_f64().unwrap_or(0.0);
                    let _ = tokio::process::Command::new("xcrun")
                        .args(["simctl", "io", serial, "tap", &format!("{x:.0}"), &format!("{y:.0}")])
                        .stderr(Stdio::null())
                        .stdout(Stdio::null())
                        .status()
                        .await;
                }
                "swipe" => {
                    let x1 = event["x1"].as_f64().unwrap_or(0.0);
                    let y1 = event["y1"].as_f64().unwrap_or(0.0);
                    let x2 = event["x2"].as_f64().unwrap_or(0.0);
                    let y2 = event["y2"].as_f64().unwrap_or(0.0);
                    let _ = tokio::process::Command::new("xcrun")
                        .args(["simctl", "io", serial, "swipe",
                               &format!("{x1:.0}"), &format!("{y1:.0}"),
                               &format!("{x2:.0}"), &format!("{y2:.0}")])
                        .stderr(Stdio::null())
                        .stdout(Stdio::null())
                        .status()
                        .await;
                }
                "key" => {
                    let code = event.get("keycode").and_then(|v| v.as_str()).unwrap_or("");
                    let simkey = match code {
                        "KEYCODE_POWER" => "power",
                        "KEYCODE_VOLUME_UP" => "volumeUp",
                        "KEYCODE_VOLUME_DOWN" => "volumeDown",
                        _ => code,
                    };
                    let _ = tokio::process::Command::new("xcrun")
                        .args(["simctl", "io", serial, "sendkey", simkey])
                        .stderr(Stdio::null())
                        .stdout(Stdio::null())
                        .status()
                        .await;
                }
                "text" => {
                    let text = event["text"].as_str().unwrap_or("");
                    let _ = tokio::process::Command::new("xcrun")
                        .args(["simctl", "io", serial, "sendtext", text])
                        .stderr(Stdio::null())
                        .stdout(Stdio::null())
                        .status()
                        .await;
                }
                _ => {
                    tracing::debug!(%serial, %etype, "Simulator input event not supported");
                }
            }
        }
        #[cfg(target_os = "macos")]
        "ios" => {
            tracing::info!(%serial, %etype, "iOS input event via WDA");
            match etype {
                "home" => {
                    if let Err(e) = wda_home().await {
                        tracing::warn!(%serial, %e, "WDA home failed, falling back to idevicediagnostics");
                        let _ = tokio::process::Command::new("idevicediagnostics")
                            .args(["-u", serial, "diagnostics", "HomeDiagnostics"])
                            .stderr(Stdio::null()).stdout(Stdio::null()).status().await;
                    }
                }
                "tap" => {
                    let x = event["x"].as_f64().unwrap_or(0.0);
                    let y = event["y"].as_f64().unwrap_or(0.0);
                    if let Err(e) = wda_tap(x, y).await {
                        tracing::warn!(%serial, %e, "WDA tap failed");
                    }
                }
                "long_press" => {
                    let x = event["x"].as_f64().unwrap_or(0.0);
                    let y = event["y"].as_f64().unwrap_or(0.0);
                    let dur = event["duration"].as_u64().unwrap_or(800);
                    if let Err(e) = wda_long_press(x, y, dur).await {
                        tracing::warn!(%serial, %e, "WDA long_press failed");
                    }
                }
                "swipe" | "drag" => {
                    let x1 = event["x1"].as_f64().unwrap_or(0.0);
                    let y1 = event["y1"].as_f64().unwrap_or(0.0);
                    let x2 = event["x2"].as_f64().unwrap_or(0.0);
                    let y2 = event["y2"].as_f64().unwrap_or(0.0);
                    let dur = event["duration"].as_u64().unwrap_or(400);
                    if let Err(e) = wda_swipe(x1, y1, x2, y2, dur).await {
                        tracing::warn!(%serial, %e, "WDA swipe failed");
                    }
                }
                "text" => {
                    let text = event["text"].as_str().unwrap_or("");
                    if let Err(e) = wda_text(text).await {
                        tracing::warn!(%serial, %e, "WDA text input failed");
                    }
                }
                "key" => {
                    let code = event.get("keycode").and_then(|v| v.as_str()).unwrap_or("");
                    match code {
                        "KEYCODE_POWER" => {
                            let _ = tokio::process::Command::new("idevicediagnostics")
                                .args(["-u", serial, "sleep"])
                                .stderr(Stdio::null()).stdout(Stdio::null()).status().await;
                        }
                        "KEYCODE_DEL" | "KEYCODE_BACKSPACE" => {
                            // Send Unicode delete character
                            if let Err(e) = wda_text("\u{E003}").await {
                                tracing::warn!(%serial, %e, "WDA backspace failed");
                            }
                        }
                        "KEYCODE_ENTER" => {
                            if let Err(e) = wda_text("\n").await {
                                tracing::warn!(%serial, %e, "WDA enter failed");
                            }
                        }
                        _ => {
                            tracing::debug!(%serial, %code, "iOS key event not mapped");
                        }
                    }
                }
                _ => {
                    tracing::debug!(%serial, %etype, "iOS input event not supported");
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        "simulator" | "ios" => {
            tracing::debug!(%serial, %etype, "iOS/simulator input requires macOS agent");
        }
        _ => {
            // Android via ADB
            tracing::info!(%serial, %etype, "Android input event");
            let args: Vec<String> = match etype {
                "tap" => {
                    let x = event["x"].as_f64().unwrap_or(0.0);
                    let y = event["y"].as_f64().unwrap_or(0.0);
                    vec!["-s".into(), serial.into(), "shell".into(), "input".into(),
                         "tap".into(), format!("{x:.0}"), format!("{y:.0}")]
                }
                "swipe" => {
                    let x1 = event["x1"].as_f64().unwrap_or(0.0);
                    let y1 = event["y1"].as_f64().unwrap_or(0.0);
                    let x2 = event["x2"].as_f64().unwrap_or(0.0);
                    let y2 = event["y2"].as_f64().unwrap_or(0.0);
                    let dur = event["duration"].as_u64().unwrap_or(300);
                    vec!["-s".into(), serial.into(), "shell".into(), "input".into(),
                         "swipe".into(), format!("{x1:.0}"), format!("{y1:.0}"),
                         format!("{x2:.0}"), format!("{y2:.0}"), format!("{dur}")]
                }
                "key" => {
                    let code = event["keycode"].as_str().unwrap_or("KEYCODE_HOME");
                    vec!["-s".into(), serial.into(), "shell".into(), "input".into(),
                         "keyevent".into(), code.into()]
                }
                "text" => {
                    let text = event["text"].as_str().unwrap_or("");
                    let escaped = text.replace(' ', "%s");
                    vec!["-s".into(), serial.into(), "shell".into(), "input".into(),
                         "text".into(), escaped]
                }
                "home" => vec!["-s".into(), serial.into(), "shell".into(), "input".into(),
                               "keyevent".into(), "KEYCODE_HOME".into()],
                "back" => vec!["-s".into(), serial.into(), "shell".into(), "input".into(),
                               "keyevent".into(), "KEYCODE_BACK".into()],
                "recent" => vec!["-s".into(), serial.into(), "shell".into(), "input".into(),
                                 "keyevent".into(), "KEYCODE_APP_SWITCH".into()],
                _ => return,
            };
            match tokio::process::Command::new(adb_path()).args(&args).output().await {
                Ok(out) => {
                    if !out.status.success() {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        tracing::warn!(%serial, %etype, %stderr, "ADB input command failed");
                    }
                }
                Err(e) => tracing::error!(%serial, %etype, %e, "Failed to run ADB input"),
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────

/// Detect whether a device serial belongs to an iOS, Android, IoT, or simulator device.
async fn detect_device_os(serial: &str) -> String {
    // IoT: serial port paths
    // Linux: /dev/ttyACM0, /dev/ttyUSB0, /dev/serial/by-id/...
    // macOS: /dev/cu.usbmodem*, /dev/cu.usbserial*, /dev/cu.SLAB*
    // Windows: COM3, COM4, etc.
    if serial.starts_with("/dev/tty") || serial.starts_with("/dev/serial/")
        || serial.starts_with("/dev/cu.")
        || (serial.starts_with("COM") && serial[3..].chars().all(|c| c.is_ascii_digit()))
    {
        return "iot".into();
    }

    // iOS Simulator & real iOS: only probe on macOS where xcrun/ideviceinfo exist
    #[cfg(target_os = "macos")]
    {
        // iOS Simulator: UUID format (8-4-4-4-12 hex with dashes)
        if serial.len() == 36 && serial.chars().filter(|c| *c == '-').count() == 4 {
            let result = tokio::process::Command::new("xcrun")
                .args(["simctl", "list", "devices", "-j"])
                .output()
                .await;
            if let Ok(out) = result {
                let s = String::from_utf8_lossy(&out.stdout);
                if s.contains(serial) {
                    return "simulator".into();
                }
            }
        }

        // iOS UDIDs are 40 hex chars (or 25 chars for newer devices)
        if serial.len() >= 24 && serial.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            let result = tokio::process::Command::new(ideviceinfo_path())
                .args(["-u", serial, "-k", "DeviceClass"])
                .output()
                .await;
            if let Ok(out) = result {
                let s = String::from_utf8_lossy(&out.stdout);
                if s.contains("iPhone") || s.contains("iPad") || s.contains("iPod") {
                    return "ios".into();
                }
            }

            let result = tokio::process::Command::new("xcrun")
                .args(["xctrace", "list", "devices"])
                .output()
                .await;
            if let Ok(out) = result {
                let s = String::from_utf8_lossy(&out.stdout);
                if s.contains(serial) {
                    return "ios".into();
                }
            }
        }
    }

    // Check if ADB knows this serial
    let result = tokio::process::Command::new(adb_path())
        .args(["-s", serial, "get-state"])
        .output()
        .await;
    if let Ok(out) = result {
        let s = String::from_utf8_lossy(&out.stdout);
        if s.trim() == "device" {
            return "android".into();
        }
    }

    "unknown".into()
}

/// Parse Android logcat threadtime line into JSON.
fn parse_logcat_line(line: &str) -> serde_json::Value {
    // Format: "04-07 18:30:15.123  1234  5678 I Tag     : Message"
    let mut parts = line.split_whitespace();
    let date = parts.next().unwrap_or("");
    let time = parts.next().unwrap_or("");
    let _pid = parts.next().unwrap_or("");
    let _tid = parts.next().unwrap_or("");
    let level_char = parts.next().unwrap_or("I");
    let rest: String = parts.collect::<Vec<_>>().join(" ");

    let level = match level_char {
        "V" => "verbose",
        "D" => "debug",
        "I" => "info",
        "W" => "warn",
        "E" => "error",
        "F" => "fatal",
        _ => "info",
    };

    let (tag, message) = if let Some(colon_pos) = rest.find(':') {
        (&rest[..colon_pos], rest[colon_pos + 1..].trim_start())
    } else {
        ("", rest.as_str())
    };

    json!({
        "type": "log",
        "timestamp": format!("{date} {time}"),
        "level": level,
        "tag": tag.trim(),
        "message": message,
    })
}

/// Parse iOS syslog line into JSON.
fn parse_ios_log_line(line: &str) -> serde_json::Value {
    // idevicesyslog format: "Apr  7 18:30:15 DeviceName process[pid] <Level>: message"
    let level = if line.contains("<Error>") { "error" }
        else if line.contains("<Warning>") { "warn" }
        else if line.contains("<Notice>") { "info" }
        else if line.contains("<Debug>") { "debug" }
        else { "info" };

    // Extract process name as tag: find "process[pid]" pattern
    let tag = line.split_whitespace()
        .find(|w| w.contains('[') && w.contains(']'))
        .and_then(|w| w.split('[').next())
        .unwrap_or("");

    // Extract timestamp (first 3 fields: "Apr  7 18:30:15")
    let ts_parts: Vec<&str> = line.splitn(4, ' ').collect();
    let timestamp = if ts_parts.len() >= 3 {
        format!("{} {} {}", ts_parts[0], ts_parts[1], ts_parts[2])
    } else {
        String::new()
    };

    json!({
        "type": "log",
        "timestamp": timestamp,
        "level": level,
        "tag": tag,
        "message": line,
    })
}

fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || ".-_:/".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

// ── IoT serial helpers ────────────────────────────────────────────

/// Stream IoT serial console as raw bytes over WebSocket, enabling full
/// terminal emulation (vi, top, etc.) via xterm.js on the client side.
/// Cross-platform: Linux (`stty -F` + `cat`), macOS (`stty -f` + `cat`),
/// Windows (PowerShell `[System.IO.Ports.SerialPort]`).
///
/// Protocol:
///   Server → Client: binary frames (raw terminal output bytes)
///   Client → Server:
///     - `{"type":"terminal_input","data":"..."}` — raw keystrokes from xterm.js
///     - `{"type":"terminal_resize","cols":N,"rows":M}` — terminal resize
///     - `{"type":"send_text","data":"..."}` — emergency command (fallback input)
///     - `{"type":"stop_stream"}` or `{"type":"stop"}` — disconnect
async fn run_iot_serial_stream(
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    serial_port: &str,
) {
    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();
    let port_for_input = serial_port.to_string();
    let is_windows = cfg!(target_os = "windows");

    // Channel for sending data to the serial port from the WS input handler
    let (serial_input_tx, mut serial_input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    // Input handler task — reads WS messages and forwards to serial
    let input_tx = serial_input_tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(t) => {
                    let text = t.to_string();
                    if text.contains("stop_stream") || text.contains("\"stop\"") {
                        let _ = stop_tx2.send(true);
                        break;
                    }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        match v.get("type").and_then(|t| t.as_str()) {
                            Some("terminal_input") => {
                                // Raw keystrokes from xterm.js
                                if let Some(data) = v.get("data").and_then(|d| d.as_str()) {
                                    let _ = input_tx.send(data.as_bytes().to_vec()).await;
                                }
                            }
                            Some("send_text") => {
                                // Emergency command input (fallback)
                                if let Some(data) = v.get("data").and_then(|d| d.as_str()) {
                                    let _ = input_tx.send(data.as_bytes().to_vec()).await;
                                }
                            }
                            Some("terminal_resize") => {
                                // Terminal resize — send stty command to IoT device
                                let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80);
                                let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24);
                                let resize_cmd = format!("stty cols {} rows {}\r\n", cols, rows);
                                let _ = input_tx.send(resize_cmd.into_bytes()).await;
                            }
                            _ => {}
                        }
                    }
                }
                Message::Binary(data) => {
                    // Raw binary input from client
                    let _ = input_tx.send(data.to_vec()).await;
                }
                Message::Close(_) => { let _ = stop_tx2.send(true); break; }
                _ => {}
            }
        }
    });

    let mut stop_rx = stop_tx.subscribe();

    // Spawn platform-appropriate serial reader/writer
    let mut child = match spawn_serial_rw(serial_port).await {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(
                json!({"type":"error","message":format!("Failed to open serial: {e}")}).to_string().into()
            )).await;
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stdin = child.stdin.take();
    let mut byte_count: u64 = 0;

    // Writer task: forward serial_input_rx → child stdin (serial port)
    let write_stop = stop_tx.clone();
    if let Some(mut stdin) = stdin {
        tokio::spawn(async move {
            let mut write_stop_rx = write_stop.subscribe();
            loop {
                tokio::select! {
                    data = serial_input_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                if stdin.write_all(&bytes).await.is_err() { break; }
                                let _ = stdin.flush().await;
                            }
                            None => break,
                        }
                    }
                    _ = write_stop_rx.changed() => break,
                }
            }
        });
    } else {
        // Fallback for platforms where stdin isn't piped: use direct serial write
        tokio::spawn(async move {
            let mut write_stop_rx = write_stop.subscribe();
            loop {
                tokio::select! {
                    data = serial_input_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                let data_str = String::from_utf8_lossy(&bytes).to_string();
                                serial_write(&port_for_input, &data_str, is_windows).await;
                            }
                            None => break,
                        }
                    }
                    _ = write_stop_rx.changed() => break,
                }
            }
        });
    }

    // Reader task: read raw bytes from serial stdout → WS binary frames
    let mut reader = tokio::io::BufReader::with_capacity(4096, stdout);
    let mut buf = [0u8; 4096];

    loop {
        tokio::select! {
            n = reader.read(&mut buf) => {
                match n {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        byte_count += n as u64;
                        if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            _ = stop_rx.changed() => break,
        }
    }

    let _ = child.kill().await;
    tracing::info!(serial_port, byte_count, "IoT terminal stream ended");
}

/// Spawn a platform-appropriate process to read/write a serial port.
/// Returns a child with both stdout (read) and stdin (write) piped.
/// Linux: `stty -F <port> 115200 raw -echo` then `socat` for bidirectional I/O.
/// macOS: `stty -f <port> 115200 raw -echo` then `socat`.
/// Windows: PowerShell `[System.IO.Ports.SerialPort]` with bidirectional streaming.
async fn spawn_serial_rw(serial_port: &str) -> Result<tokio::process::Child, std::io::Error> {
    if cfg!(target_os = "windows") {
        // Windows: PowerShell bidirectional serial streaming (raw bytes via stdin/stdout)
        let ps_script = format!(
            r#"$port = New-Object System.IO.Ports.SerialPort '{port}', 115200, 'None', 8, 'One'
$port.ReadTimeout = 100
$port.Open()
$buf = New-Object byte[] 4096
# Reader thread: serial → stdout
$reader = [System.Threading.Thread]::new({{ while ($true) {{
    try {{ $n = $port.Read($buf, 0, $buf.Length); if ($n -gt 0) {{ [Console]::OpenStandardOutput().Write($buf, 0, $n); [Console]::OpenStandardOutput().Flush() }} }}
    catch {{ Start-Sleep -Milliseconds 10 }}
}} }})
$reader.IsBackground = $true
$reader.Start()
# Writer: stdin → serial
$stdinBuf = New-Object byte[] 4096
$stdin = [Console]::OpenStandardInput()
while ($true) {{
    $n = $stdin.Read($stdinBuf, 0, $stdinBuf.Length)
    if ($n -le 0) {{ break }}
    $port.Write($stdinBuf, 0, $n)
}}
$port.Close()"#,
            port = serial_port,
        );
        tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
    } else {
        // Linux / macOS: configure with stty then use socat for bidirectional I/O
        // Resolve symlinks to avoid colons in /dev/serial/by-id/ paths
        // being misinterpreted as socat address option separators.
        let resolved = tokio::fs::canonicalize(serial_port)
            .await
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| serial_port.to_string());
        let stty_flag = if cfg!(target_os = "macos") { "-f" } else { "-F" };
        let _ = tokio::process::Command::new("stty")
            .args([stty_flag, &resolved, "115200", "raw", "-echo", "-echoe", "-echok"])
            .output()
            .await;

        // Try socat first (proper bidirectional serial)
        if which_exists("socat").await {
            tokio::process::Command::new("socat")
                .args([
                    "STDIO",
                    &format!("file:{},b115200,raw,echo=0,crnl", resolved),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
        } else {
            // Fallback: cat for reading (write via direct file write)
            tokio::process::Command::new("cat")
                .arg(serial_port)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
        }
    }
}

/// Check if a command exists on PATH.
async fn which_exists(cmd: &str) -> bool {
    tokio::process::Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Write data to a serial port (cross-platform).
/// Linux/macOS: direct file write. Windows: PowerShell SerialPort.Write().
async fn serial_write(serial_port: &str, data: &str, is_windows: bool) {
    if is_windows {
        let ps = format!(
            r#"$p = New-Object System.IO.Ports.SerialPort '{port}', 115200; $p.Open(); $p.Write('{data}'); $p.Close()"#,
            port = serial_port,
            data = data.replace('\'', "''"),
        );
        let _ = tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
            .output()
            .await;
    } else {
        let _ = tokio::fs::write(serial_port, data.as_bytes()).await;
    }
}

/// Parse an IoT serial line into a structured log entry.
/// Handles ESP-IDF format: `I (12345) TAG: message`
fn parse_iot_log_line(line: &str) -> serde_json::Value {
    let trimmed = line.trim();

    if trimmed.len() >= 3 {
        let first_char = &trimmed[..1];
        if matches!(first_char, "I" | "W" | "E" | "D" | "V") && trimmed[1..].starts_with(" (") {
            let level = match first_char {
                "I" => "info", "W" => "warn", "E" => "error", "D" => "debug", "V" => "verbose",
                _ => "info",
            };
            let ts = trimmed.find(')').map(|end| &trimmed[3..end]).unwrap_or("");
            let after = trimmed.find(')').map(|i| &trimmed[i + 1..]).unwrap_or("");
            let (tag, message) = if let Some(c) = after.find(':') {
                (after[..c].trim(), after[c + 1..].trim())
            } else {
                ("", after.trim())
            };
            return json!({ "type": "log", "timestamp": ts, "level": level, "tag": tag, "message": message });
        }
    }

    let level = if trimmed.contains("ERROR") || trimmed.contains("[error]") { "error" }
        else if trimmed.contains("WARN") || trimmed.contains("[warn]") { "warn" }
        else if trimmed.contains("DEBUG") || trimmed.contains("[debug]") { "debug" }
        else { "info" };

    json!({ "type": "log", "timestamp": "", "level": level, "tag": "serial", "message": trimmed })
}

// ── SSH terminal streaming ────────────────────────────────────────

/// WebSocket upgrade handler for SSH terminal sessions.
/// Query parameters: `?host=IP&port=22[&username=foo]`
///
/// SSH runs inside a real PTY via `script`, so host-key confirmation, username,
/// password prompts, and the interactive shell all appear directly in xterm.
async fn ws_ssh_handler(
    ws: WebSocketUpgrade,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let host = params.get("host").cloned().unwrap_or_default();
    let port: u16 = params.get("port").and_then(|p| p.parse().ok()).unwrap_or(22);
    let username = params.get("username").cloned();
    tracing::info!(%host, %port, username = ?username, "SSH terminal WebSocket upgrade");
    ws.on_upgrade(move |socket| handle_ssh_session(socket, username, host, port))
}

async fn handle_ssh_session(socket: WebSocket, username: Option<String>, host: String, port: u16) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    if host.is_empty() {
        let _ = ws_tx.send(Message::Text(
            json!({"type":"error","message":"Missing host parameter"}).to_string().into()
        )).await;
        return;
    }

    // Always prompt for username — never auto-login with a pre-selected user.
    let _ = username;
    let ssh_opts = format!(
        "ssh -tt -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o ConnectTimeout=10 -p {}",
        port
    );
    let host_esc = shell_escape(&host);
    let ssh_cmd = format!(
        "printf 'login as: '; IFS= read -r EF_USER; [ -z \"$EF_USER\" ] && {{ echo 'no username entered'; exit 1; }}; exec {} \"$EF_USER\"@{}",
        ssh_opts, host_esc
    );

    #[cfg(target_os = "macos")]
    let child = tokio::process::Command::new("script")
        .args(["-q", "/dev/null", "sh", "-lc", &ssh_cmd])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();

    #[cfg(not(target_os = "macos"))]
    let child = tokio::process::Command::new("script")
        .args(["-q", "-c", &ssh_cmd, "/dev/null"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(
                json!({"type":"error","message":format!("Failed to spawn ssh: {e}")}).to_string().into()
            )).await;
            return;
        }
    };

    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();

    // Channel for feeding data to ssh stdin
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    // WS → ssh stdin handler
    let input_tx_clone = input_tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(t) => {
                    let text = t.to_string();
                    if text.contains("stop_stream") || text.contains("\"stop\"") {
                        let _ = stop_tx2.send(true);
                        break;
                    }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        match v.get("type").and_then(|t| t.as_str()) {
                            Some("terminal_input") | Some("send_text") => {
                                if let Some(data) = v.get("data").and_then(|d| d.as_str()) {
                                    let _ = input_tx_clone.send(data.as_bytes().to_vec()).await;
                                }
                            }
                            Some("terminal_resize") => {
                                // Resize is handled by the PTY automatically for SSH
                                // (ssh protocol WINDOW_CHANGE). We can't easily inject it
                                // when using the ssh command-line client, but the remote
                                // side will adapt on next output.
                            }
                            _ => {}
                        }
                    }
                }
                Message::Binary(data) => {
                    let _ = input_tx_clone.send(data.to_vec()).await;
                }
                Message::Close(_) => { let _ = stop_tx2.send(true); break; }
                _ => {}
            }
        }
    });

    // stdin writer task
    let stop_for_writer = stop_tx.clone();
    if let Some(mut stdin) = child.stdin.take() {
        tokio::spawn(async move {
            let mut stop_rx = stop_for_writer.subscribe();
            loop {
                tokio::select! {
                    data = input_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                if stdin.write_all(&bytes).await.is_err() { break; }
                                let _ = stdin.flush().await;
                            }
                            None => break,
                        }
                    }
                    _ = stop_rx.changed() => break,
                }
            }
        });
    }

    // stdout reader → WS binary frames
    let mut stop_rx = stop_tx.subscribe();
    if let Some(stdout) = child.stdout.take() {
        let mut reader = tokio::io::BufReader::with_capacity(4096, stdout);
        let mut buf = [0u8; 4096];
        let mut byte_count: u64 = 0;

        loop {
            tokio::select! {
                n = reader.read(&mut buf) => {
                    match n {
                        Ok(0) => break,
                        Ok(n) => {
                            byte_count += n as u64;
                            if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                _ = stop_rx.changed() => break,
            }
        }

        tracing::info!(%host, byte_count, "SSH session ended");
    }

    let _ = child.kill().await;
}

    // ── Local PTY shell (no credentials needed) ───────────────────────

    /// WebSocket handler that spawns a local bash shell via `script` for full PTY
    /// emulation.  No credentials required — this endpoint is only reachable when
    /// the API proxies a device that has the `local_pty` capability, meaning the
    /// agent itself is the host machine being accessed.
    ///
    /// Protocol identical to IoT serial stream:
    ///   Server → Client: binary frames (raw PTY output)
    ///   Client → Server:
    ///     `{"type":"terminal_input","data":"..."}` — raw keystrokes
    ///     `{"type":"terminal_resize","cols":N,"rows":M}` — resize
    ///     binary frames — raw keystrokes (fallback)
    ///     `{"type":"stop_stream"}` — disconnect
    async fn ws_pty_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
        tracing::info!("Local PTY WebSocket upgrade");
        ws.on_upgrade(handle_pty_session)
    }

    async fn handle_pty_session(socket: WebSocket) {
        let (mut ws_tx, mut ws_rx) = socket.split();

        let (stop_tx, _) = tokio::sync::watch::channel(false);
        let stop_tx2 = stop_tx.clone();
        let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

        // WS message handler
        tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_rx.next().await {
                match msg {
                    Message::Text(t) => {
                        let text = t.to_string();
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            match v.get("type").and_then(|t| t.as_str()) {
                                Some("terminal_input") | Some("send_text") => {
                                    if let Some(data) = v.get("data").and_then(|d| d.as_str()) {
                                        let _ = input_tx.send(data.as_bytes().to_vec()).await;
                                    }
                                }
                                Some("terminal_resize") => {
                                    // Pass resize as TIOCSWINSZ via stty inside the shell
                                    let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80);
                                    let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24);
                                    let cmd = format!("stty cols {} rows {}\r", cols, rows);
                                    let _ = input_tx.send(cmd.into_bytes()).await;
                                }
                                Some("stop_stream") | Some("stop") => {
                                    let _ = stop_tx2.send(true);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    Message::Binary(data) => {
                        let _ = input_tx.send(data.to_vec()).await;
                    }
                    Message::Close(_) => {
                        let _ = stop_tx2.send(true);
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Spawn shell: `script` allocates a real PTY so interactive programs work.
        // Linux: script -q -c /bin/bash /dev/null
        // macOS: script -q /dev/null /bin/bash
        #[cfg(target_os = "macos")]
        let mut child = {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            tokio::process::Command::new("script")
                .args(["-q", "/dev/null", &shell])
                .env("TERM", "xterm-256color")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
        };

        #[cfg(not(target_os = "macos"))]
        let mut child = {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            tokio::process::Command::new("script")
                .args(["-q", "-c", &shell, "/dev/null"])
                .env("TERM", "xterm-256color")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
        };

        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                let _ = ws_tx.send(Message::Text(
                    serde_json::json!({"type":"error","message":format!("Failed to spawn shell: {e}")}).to_string().into()
                )).await;
                return;
            }
        };

        let stdout = child.stdout.take().unwrap();
        let mut stdin = child.stdin.take();

        // stdin writer
        let write_stop = stop_tx.clone();
        tokio::spawn(async move {
            let mut stop_rx = write_stop.subscribe();
            loop {
                tokio::select! {
                    data = input_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                if let Some(ref mut s) = stdin {
                                    if s.write_all(&bytes).await.is_err() { break; }
                                    let _ = s.flush().await;
                                }
                            }
                            None => break,
                        }
                    }
                    _ = stop_rx.changed() => break,
                }
            }
        });

        // stdout → WS binary frames
        let mut stop_rx = stop_tx.subscribe();
        let mut reader = tokio::io::BufReader::with_capacity(4096, stdout);
        let mut buf = [0u8; 4096];
        let mut byte_count: u64 = 0;

        loop {
            tokio::select! {
                n = reader.read(&mut buf) => {
                    match n {
                        Ok(0) => break,
                        Ok(n) => {
                            byte_count += n as u64;
                            if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                _ = stop_rx.changed() => break,
            }
        }

        let _ = child.kill().await;
        tracing::info!(byte_count, "Local PTY session ended");
    }
