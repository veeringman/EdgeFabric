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
use std::sync::OnceLock;
use tokio::io::AsyncBufReadExt;

/// Resolve the ffmpeg binary path once (handles non-login shells missing /usr/local/bin).
fn ffmpeg_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        for p in ["/usr/local/bin/ffmpeg", "/opt/homebrew/bin/ffmpeg", "/usr/bin/ffmpeg"] {
            if std::path::Path::new(p).exists() { return p.to_string(); }
        }
        "ffmpeg".to_string() // fallback to PATH lookup
    })
}

/// Start the agent's streaming HTTP server on the given port.
pub async fn start_streaming_server(port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/ws/stream/{*serial}", get(ws_stream_handler))
        .route("/ws/logs/{*serial}", get(ws_logs_handler))
        .route("/health", get(health));

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

    let _ = ws_tx.send(Message::Text(
        json!({"type": "stream_ready", "serial": &serial, "os": &os}).to_string().into()
    )).await;

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
        "ios" => {
            // Try idevicesyslog first
            match tokio::process::Command::new("idevicesyslog")
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
        _ => {
            // Android: adb logcat
            match tokio::process::Command::new("adb")
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

async fn capture_frame(serial: &str, os: &str, ios_tmp: &str) -> anyhow::Result<Vec<u8>> {
    match os {
        "simulator" => capture_simulator_frame(serial, ios_tmp).await,
        "ios" => capture_ios_frame(serial, ios_tmp).await,
        _ => capture_android_frame(serial).await,
    }
}

/// Android: adb exec-out screencap | ffmpeg → JPEG
async fn capture_android_frame(serial: &str) -> anyhow::Result<Vec<u8>> {
    let output = tokio::process::Command::new("sh")
        .args([
            "-c",
            &format!(
                "adb -s {} exec-out screencap -p | ffmpeg -y -hide_banner -loglevel error -i - -vframes 1 -f mjpeg -q:v 15 -vf scale=540:-1 -",
                shell_escape(serial)
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
    let output = tokio::process::Command::new("adb")
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .output()
        .await?;
    if output.stdout.len() < 100 {
        anyhow::bail!("screencap too small ({} bytes)", output.stdout.len());
    }
    Ok(output.stdout)
}

/// iOS Simulator: xcrun simctl io screenshot → (optionally ffmpeg → JPEG)
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

/// iOS: idevicescreenshot → ffmpeg → JPEG
async fn capture_ios_frame(serial: &str, tmp: &str) -> anyhow::Result<Vec<u8>> {
    // idevicescreenshot writes a TIFF to a file — we use a temp file then ffmpeg

    let status = tokio::process::Command::new("idevicescreenshot")
        .args(["-u", serial, tmp])
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("idevicescreenshot failed");
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

    // Fallback: return raw TIFF (browser won't display, but at least it's data)
    anyhow::bail!("ffmpeg TIFF→JPEG conversion failed")
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
        "ios" => {
            tracing::info!(%serial, %etype, "iOS input event");
            match etype {
                "home" => {
                    // Simulate home press via Instruments protocol (DDI must be mounted)
                    // Fallback: use idevicediagnostics to trigger diagnostics that returns to home
                    let _ = tokio::process::Command::new("idevicediagnostics")
                        .args(["-u", serial, "diagnostics", "HomeDiagnostics"])
                        .stderr(Stdio::null())
                        .stdout(Stdio::null())
                        .status()
                        .await;
                }
                "key" => {
                    let code = event.get("keycode").and_then(|v| v.as_str()).unwrap_or("");
                    match code {
                        "KEYCODE_POWER" => {
                            let _ = tokio::process::Command::new("idevicediagnostics")
                                .args(["-u", serial, "sleep"])
                                .stderr(Stdio::null())
                                .stdout(Stdio::null())
                                .status()
                                .await;
                        }
                        _ => {
                            tracing::debug!(%serial, %code, "iOS key event not supported without WDA");
                        }
                    }
                }
                "tap" | "swipe" => {
                    // Touch input requires WebDriverAgent (Xcode/XCTest)
                    // or pymobiledevice3 DVT (iOS 16+ only)
                    tracing::debug!(%serial, %etype, "iOS touch input requires WDA — not available");
                }
                _ => {
                    tracing::debug!(%serial, %etype, "iOS input event not supported");
                }
            }
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
            match tokio::process::Command::new("adb").args(&args).output().await {
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

    // iOS Simulator: UUID format (8-4-4-4-12 hex with dashes)
    // Check before real iOS since simulator UUIDs also pass the hex check
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
        let result = tokio::process::Command::new("ideviceinfo")
            .args(["-u", serial, "-k", "DeviceClass"])
            .output()
            .await;
        if let Ok(out) = result {
            let s = String::from_utf8_lossy(&out.stdout);
            if s.contains("iPhone") || s.contains("iPad") || s.contains("iPod") {
                return "ios".into();
            }
        }
    }

    // Check if ADB knows this serial
    let result = tokio::process::Command::new("adb")
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

/// Stream IoT serial console as text WebSocket frames.
/// Cross-platform: Linux (`stty -F` + `cat`), macOS (`stty -f` + `cat`),
/// Windows (PowerShell `[System.IO.Ports.SerialPort]`).
async fn run_iot_serial_stream(
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    serial_port: &str,
) {
    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();
    let port_for_input = serial_port.to_string();
    let is_windows = cfg!(target_os = "windows");

    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(t) => {
                    let text = t.to_string();
                    if text.contains("stop_stream") || text.contains("stop") {
                        let _ = stop_tx2.send(true);
                        break;
                    }
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if v.get("type").and_then(|t| t.as_str()) == Some("send_text") {
                            if let Some(data) = v.get("data").and_then(|d| d.as_str()) {
                                serial_write(&port_for_input, data, is_windows).await;
                            }
                        }
                    }
                }
                Message::Close(_) => { let _ = stop_tx2.send(true); break; }
                _ => {}
            }
        }
    });

    let mut stop_rx = stop_tx.subscribe();

    // Spawn platform-appropriate serial reader
    let mut child = match spawn_serial_reader(serial_port).await {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(
                json!({"type":"error","message":format!("Failed to open serial: {e}")}).to_string().into()
            )).await;
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut line_count: u64 = 0;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(text)) => {
                        line_count += 1;
                        let msg = json!({
                            "type": "serial_data",
                            "line": line_count,
                            "data": text,
                        });
                        if ws_tx.send(Message::Text(msg.to_string().into())).await.is_err() {
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
    tracing::info!(serial_port, line_count, "IoT serial stream ended");
}

/// Spawn a platform-appropriate process to read from a serial port.
/// Linux: `stty -F <port> 115200 raw -echo` then `cat <port>`
/// macOS: `stty -f <port> 115200 raw -echo` then `cat <port>`
/// Windows: PowerShell script using `[System.IO.Ports.SerialPort]`
async fn spawn_serial_reader(serial_port: &str) -> Result<tokio::process::Child, std::io::Error> {
    if cfg!(target_os = "windows") {
        // Windows: use PowerShell to read serial output line by line
        let ps_script = format!(
            r#"$port = New-Object System.IO.Ports.SerialPort '{port}', 115200, 'None', 8, 'One'
$port.ReadTimeout = -1
$port.Open()
try {{ while ($true) {{ $line = $port.ReadLine(); Write-Output $line }} }}
finally {{ $port.Close() }}"#,
            port = serial_port,
        );
        tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
    } else {
        // Linux / macOS: configure with stty then `cat`
        let stty_flag = if cfg!(target_os = "macos") { "-f" } else { "-F" };
        let _ = tokio::process::Command::new("stty")
            .args([stty_flag, serial_port, "115200", "raw", "-echo"])
            .output()
            .await;

        tokio::process::Command::new("cat")
            .arg(serial_port)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
    }
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
