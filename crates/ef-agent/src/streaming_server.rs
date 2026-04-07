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
use tokio::io::AsyncBufReadExt;

/// Start the agent's streaming HTTP server on the given port.
pub async fn start_streaming_server(port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/ws/stream/{serial}", get(ws_stream_handler))
        .route("/ws/logs/{serial}", get(ws_logs_handler))
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

    // Double-buffered capture loop
    let s = serial.clone();
    let o = os.clone();
    let mut pending = tokio::spawn(async move { capture_frame(&s, &o).await });

    loop {
        if *stop_rx.borrow() { break; }

        let result = match pending.await {
            Ok(r) => r,
            Err(_) => break,
        };

        let s = serial.clone();
        let o = os.clone();
        pending = tokio::spawn(async move { capture_frame(&s, &o).await });

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

    tracing::info!(%serial, frame_count, "Stream ended");
}

// ── Log streaming ─────────────────────────────────────────────────

async fn ws_logs_handler(
    ws: WebSocketUpgrade,
    Path(serial): Path<String>,
) -> impl IntoResponse {
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

    // Spawn the appropriate log process
    let mut child = match os.as_str() {
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
                        let entry = if os == "ios" {
                            parse_ios_log_line(&line)
                        } else {
                            parse_logcat_line(&line)
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

async fn capture_frame(serial: &str, os: &str) -> anyhow::Result<Vec<u8>> {
    match os {
        "ios" => capture_ios_frame(serial).await,
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

/// iOS: idevicescreenshot → ffmpeg → JPEG
async fn capture_ios_frame(serial: &str) -> anyhow::Result<Vec<u8>> {
    // idevicescreenshot writes a TIFF to a file — we use a temp file then ffmpeg
    let tmp = format!("/tmp/ef-ios-{}.tiff", &serial[..8.min(serial.len())]);

    let status = tokio::process::Command::new("idevicescreenshot")
        .args(["-u", serial, &tmp])
        .stderr(Stdio::null())
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("idevicescreenshot failed");
    }

    // Convert TIFF → JPEG via ffmpeg
    let output = tokio::process::Command::new("ffmpeg")
        .args([
            "-y", "-hide_banner", "-loglevel", "error",
            "-i", &tmp,
            "-vframes", "1", "-f", "mjpeg", "-q:v", "15", "-vf", "scale=540:-1",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&tmp).await;

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
        "ios" => {
            // iOS input via idevice tools is very limited — log for now
            tracing::debug!(%serial, %etype, "iOS input event (not yet implemented)");
        }
        _ => {
            // Android via ADB
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
            let _ = tokio::process::Command::new("adb").args(&args).output().await;
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────

/// Detect whether a device serial belongs to an iOS or Android device.
async fn detect_device_os(serial: &str) -> String {
    // iOS UDIDs are 40 hex chars (or 25 chars for newer devices)
    // Also check if ideviceinfo can talk to it
    if serial.len() >= 24 && serial.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        // Likely iOS UDID — verify with ideviceinfo
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
    let parts: Vec<&str> = line.splitn(6, ' ').collect();
    if parts.len() >= 6 {
        let level = parts.get(4).unwrap_or(&"?");
        json!({
            "timestamp": format!("{} {}", parts.first().unwrap_or(&""), parts.get(1).unwrap_or(&"")),
            "level": level.trim(),
            "tag": parts.get(5).and_then(|s| s.split(':').next()).unwrap_or("").trim(),
            "message": line,
        })
    } else {
        json!({"message": line, "level": "V"})
    }
}

/// Parse iOS syslog line into JSON.
fn parse_ios_log_line(line: &str) -> serde_json::Value {
    // idevicesyslog format: "Apr  7 18:30:15 iPhone process[pid] <level>: message"
    json!({
        "message": line,
        "level": if line.contains("<Error>") { "E" }
                 else if line.contains("<Warning>") { "W" }
                 else if line.contains("<Notice>") { "I" }
                 else { "D" },
    })
}

fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || ".-_:".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}
