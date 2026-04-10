//! WebSocket live-view endpoint for device screen streaming.
//!
//! Route: `GET /api/v1/stream/{session_id}` — upgrades to WebSocket.
//!
//! Protocol:
//! 1. Client sends JSON `{"type":"start_stream","session_id":"..."}` → server looks
//!    up the session → device → serial, then starts capturing screenshots via ADB.
//! 2. Server continuously sends binary WebSocket frames containing JPEG screenshots.
//! 3. Client can send JSON `{"type":"input_event","event":{...}}` to inject touch/key.
//! 4. Either side sends `{"type":"stop_stream"}` to end.

use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::middleware::auth_extractor::AppState;

// ── WebSocket proxy to remote agent ───────────────────────────────

/// Bidirectional WebSocket relay between a dashboard client and a remote
/// agent's streaming server.  Messages flow both ways:
///   dashboard → agent   (text: input events, stop commands)
///   agent → dashboard   (binary: JPEG frames, text: log lines)
async fn proxy_ws_to_agent(
    mut client_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut client_rx: futures::stream::SplitStream<WebSocket>,
    agent_url: &str,
    id: Uuid,
) {
    use tokio_tungstenite::tungstenite::Message as TsMsg;

    let (agent_ws, _resp) = match tokio_tungstenite::connect_async(agent_url).await {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(%id, %agent_url, "Failed to connect to agent WS: {e}");
            let _ = client_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":format!("Agent unreachable: {e}")}).to_string().into()
            )).await;
            return;
        }
    };

    tracing::info!(%id, %agent_url, "Proxy connected to agent");

    let (mut agent_tx, mut agent_rx) = agent_ws.split();

    // agent → dashboard
    let id2 = id;
    let a2d = tokio::spawn(async move {
        while let Some(msg) = agent_rx.next().await {
            let relay = match msg {
                Ok(TsMsg::Binary(data)) => Some(Message::Binary(data.into())),
                Ok(TsMsg::Text(t)) => Some(Message::Text(t.to_string().into())),
                Ok(TsMsg::Close(_)) | Err(_) => break,
                _ => None,
            };
            if let Some(m) = relay {
                if client_tx.send(m).await.is_err() { break; }
            }
        }
        tracing::debug!(%id2, "agent→dashboard relay ended");
    });

    // dashboard → agent
    let id3 = id;
    let d2a = tokio::spawn(async move {
        while let Some(Ok(msg)) = client_rx.next().await {
            let relay = match msg {
                Message::Text(t) => Some(TsMsg::Text(t.to_string().into())),
                Message::Binary(b) => Some(TsMsg::Binary(b.into())),
                Message::Close(_) => break,
                _ => None,
            };
            if let Some(m) = relay {
                if agent_tx.send(m).await.is_err() { break; }
            }
        }
        tracing::debug!(%id3, "dashboard→agent relay ended");
    });

    // When either direction ends, abort the other
    tokio::select! {
        _ = a2d => {},
        _ = d2a => {},
    }

    tracing::info!(%id, "Proxy relay finished");
}

/// WebSocket upgrade handler.
pub async fn ws_stream_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    tracing::info!(%session_id, "WebSocket upgrade request for streaming");
    ws.on_upgrade(move |socket| handle_stream(socket, session_id, state))
}

#[derive(Deserialize)]
struct WsMessage {
    #[serde(rename = "type")]
    msg_type: String,
    session_id: Option<Uuid>,
    event: Option<serde_json::Value>,
}

async fn handle_stream(socket: WebSocket, id: Uuid, state: AppState) {
    let (mut ws_tx, ws_rx) = socket.split();

    tracing::info!(%id, "Stream WebSocket connected");

    // Try device lookup first (dashboard sends device_id), fall back to session lookup
    let device = if let Ok(Some(device)) = ef_db::repo::device::DeviceRepo::find_by_id(&state.pool, id).await {
        tracing::info!(%id, name = %device.name, "Found device directly");
        device
    } else if let Ok(Some(session)) = ef_db::repo::session::SessionRepo::find_by_id(&state.pool, id).await {
        match ef_db::repo::device::DeviceRepo::find_by_id(&state.pool, session.device_id).await {
            Ok(Some(d)) => d,
            _ => {
                let _ = ws_tx.send(Message::Text(
                    serde_json::json!({"type":"error","message":"Device not found for session"}).to_string().into()
                )).await;
                return;
            }
        }
    } else {
        let _ = ws_tx.send(Message::Text(
            serde_json::json!({"type":"error","message":"No device or session found"}).to_string().into()
        )).await;
        return;
    };

    let serial = device.serial.clone().unwrap_or_else(|| "emulator-5554".to_string());

    // Check if device has a remote agent with a streaming URL
    if let Some(agent_id) = device.agent_id {
        if let Ok(Some(agent)) = ef_db::repo::agent::AgentRepo::find_by_id(&state.pool, agent_id).await {
            if let Some(ref streaming_url) = agent.streaming_url {
                // SSH devices: proxy to agent's SSH endpoint with host/port query params
                if device.os == "ssh" {
                    tracing::info!(%id, %streaming_url, %serial, "Proxying SSH session to remote agent");
                    let (host, port) = match parse_ssh_serial(&serial) {
                        Some(hp) => hp,
                        None => {
                            let _ = ws_tx.send(Message::Text(
                                serde_json::json!({"type":"error","message":format!("Invalid SSH serial: {serial}")}).to_string().into()
                            )).await;
                            return;
                        }
                    };
                    let agent_ws_url = format!(
                        "{}/ws/ssh?host={}&port={}",
                        streaming_url.trim_end_matches('/'), host, port
                    ).replace("http://", "ws://").replace("https://", "wss://");
                    proxy_ws_to_agent(ws_tx, ws_rx, &agent_ws_url, id).await;
                    return;
                }

                tracing::info!(%id, %streaming_url, %serial, "Proxying stream to remote agent");
                let agent_ws_url = format!("{}/ws/stream/{}", streaming_url.trim_end_matches('/'), serial)
                    .replace("http://", "ws://").replace("https://", "wss://");
                proxy_ws_to_agent(ws_tx, ws_rx, &agent_ws_url, id).await;
                return;
            }
        }
    }

    // Local device — run capture directly

    // SSH devices: run SSH session locally (agent on this machine)
    if device.os == "ssh" {
        let (host, port) = match parse_ssh_serial(&serial) {
            Some(hp) => hp,
            None => {
                let _ = ws_tx.send(Message::Text(
                    serde_json::json!({"type":"error","message":format!("Invalid SSH serial: {serial}")}).to_string().into()
                )).await;
                return;
            }
        };
        tracing::info!(%id, serial = %serial, %host, port, "Starting local SSH terminal session");
        let _ = ws_tx.send(Message::Text(
            serde_json::json!({
                "type": "stream_ready",
                "device_id": id,
                "serial": serial,
                "device_type": "ssh",
            }).to_string().into()
        )).await;
        run_ssh_session(ws_tx, ws_rx, &host, port, id).await;
        tracing::info!(%id, "SSH session ended");
        return;
    }

    // IoT devices: stream serial console as raw terminal bytes
    if device.os == "iot" {
        tracing::info!(%id, serial = %serial, "Starting IoT terminal stream");
        let _ = ws_tx.send(Message::Text(
            serde_json::json!({
                "type": "stream_ready",
                "device_id": id,
                "serial": serial,
                "device_type": "iot",
            }).to_string().into()
        )).await;
        run_serial_stream(ws_tx, ws_rx, &serial, id).await;
        tracing::info!(%id, "IoT stream ended");
        return;
    }

    let effective_serial = if let Some(ref ip) = device.ip_address {
        let addr = format!("{ip}:5555");
        tracing::info!(%addr, "Auto-connecting ADB to remote device");
        let _ = tokio::process::Command::new("adb")
            .args(["connect", &addr])
            .output()
            .await;
        addr
    } else {
        serial.clone()
    };

    let resolution = get_device_resolution(&effective_serial).await;

    tracing::info!(%id, serial = %effective_serial, ?resolution, "Starting local screenshot stream");

    let _ = ws_tx.send(Message::Text(
        serde_json::json!({
            "type": "stream_ready",
            "device_id": id,
            "serial": effective_serial,
            "resolution": resolution,
        }).to_string().into()
    )).await;

    run_screenshot_loop(ws_tx, ws_rx, &effective_serial, id).await;

    tracing::info!(%id, "Stream ended");
}

/// Continuously capture screenshots via ADB and send over WebSocket.
/// Uses double-buffering: next capture starts while current frame is being sent.
async fn run_screenshot_loop(
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    serial: &str,
    session_id: Uuid,
) {
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<String>(32);
    let serial_owned = serial.to_string();
    let (stop_tx, _) = tokio::sync::watch::channel(false);

    // Spawn input handler — reads WS text messages for input events / stop
    let stop_tx2 = stop_tx.clone();
    let mut stop_rx_input = stop_tx.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(t))) => {
                            let text = t.to_string();
                            if let Ok(parsed) = serde_json::from_str::<WsMessage>(&text) {
                                if parsed.msg_type == "stop_stream" {
                                    let _ = stop_tx2.send(true);
                                    break;
                                }
                                if parsed.msg_type == "input_event" {
                                    let _ = input_tx.send(text).await;
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            let _ = stop_tx2.send(true);
                            break;
                        }
                        _ => {}
                    }
                }
                _ = stop_rx_input.changed() => break,
            }
        }
    });

    let mut stop_rx = stop_tx.subscribe();
    let mut frame_count: u64 = 0;

    // Double-buffered pipeline: start first capture, then overlap subsequent
    // captures with sending the previous frame.
    let s = serial_owned.clone();
    let mut pending = tokio::spawn(async move { capture_frame(&s).await });

    loop {
        if *stop_rx.borrow() { break; }

        // Wait for current capture to finish
        let result = match pending.await {
            Ok(r) => r,
            Err(_) => break,
        };

        // Immediately start next capture (overlaps with send + input handling)
        let s = serial_owned.clone();
        pending = tokio::spawn(async move { capture_frame(&s).await });

        match result {
            Ok(data) => {
                frame_count += 1;
                if ws_tx.send(Message::Binary(data.into())).await.is_err() { break; }
            }
            Err(e) => {
                if frame_count == 0 {
                    tracing::error!(%session_id, "Capture failed: {e}");
                    let _ = ws_tx.send(Message::Text(
                        serde_json::json!({"type":"error","message": format!("Capture failed: {e}")}).to_string().into()
                    )).await;
                    break;
                }
            }
        }

        // Handle pending input events
        while let Ok(event_json) = input_rx.try_recv() {
            handle_input_event(&serial_owned, &event_json).await;
        }
    }

    tracing::info!(%session_id, frame_count, "Screenshot stream ended");
}

// ── Capture helpers ────────────────────────────────────────────────

/// Capture and compress a single frame. Runs adb screencap piped directly
/// into ffmpeg via OS-level pipe (no shell, no intermediate files).
async fn capture_frame(serial: &str) -> anyhow::Result<Vec<u8>> {
    // Single shell pipeline: adb screencap → ffmpeg JPEG. Using shell pipe
    // is actually faster than Rust fd plumbing because it avoids async→sync
    // conversion overhead and lets the OS handle the pipe directly.
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

    // Fallback: raw PNG (no ffmpeg)
    let output = tokio::process::Command::new("adb")
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .output()
        .await?;
    if output.stdout.len() < 100 {
        anyhow::bail!("screencap too small ({} bytes)", output.stdout.len());
    }
    Ok(output.stdout)
}

/// Simple shell argument escaping (allow alphanumeric, dots, colons, dashes).
fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || ".-_:".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Handle an input event by dispatching ADB commands.
async fn handle_input_event(serial: &str, event_json: &str) {
    let parsed: serde_json::Value = match serde_json::from_str(event_json) {
        Ok(v) => v,
        Err(_) => return,
    };

    let event = match parsed.get("event") {
        Some(e) => e,
        None => return,
    };

    let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");

    let args: Vec<String> = match event_type {
        "tap" => {
            let x = event.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
            let y = event.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
            vec!["-s".into(), serial.into(), "shell".into(), "input".into(), "tap".into(), x.to_string(), y.to_string()]
        }
        "swipe" => {
            let x1 = event.get("x1").and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
            let y1 = event.get("y1").and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
            let x2 = event.get("x2").and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
            let y2 = event.get("y2").and_then(|v| v.as_f64()).unwrap_or(0.0) as i32;
            let dur = event.get("duration").and_then(|v| v.as_i64()).unwrap_or(300);
            vec!["-s".into(), serial.into(), "shell".into(), "input".into(), "swipe".into(),
                 x1.to_string(), y1.to_string(), x2.to_string(), y2.to_string(), dur.to_string()]
        }
        "key" => {
            let keycode = event.get("keycode").and_then(|v| v.as_str()).unwrap_or("KEYCODE_HOME");
            vec!["-s".into(), serial.into(), "shell".into(), "input".into(), "keyevent".into(), keycode.into()]
        }
        "text" => {
            let text = event.get("text").and_then(|v| v.as_str()).unwrap_or("");
            vec!["-s".into(), serial.into(), "shell".into(), "input".into(), "text".into(), text.into()]
        }
        "home" => vec!["-s".into(), serial.into(), "shell".into(), "input".into(), "keyevent".into(), "KEYCODE_HOME".into()],
        "back" => vec!["-s".into(), serial.into(), "shell".into(), "input".into(), "keyevent".into(), "KEYCODE_BACK".into()],
        "recent" => vec!["-s".into(), serial.into(), "shell".into(), "input".into(), "keyevent".into(), "KEYCODE_APP_SWITCH".into()],
        _ => return,
    };

    let _ = tokio::process::Command::new("adb")
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

/// Query device screen resolution via `adb shell wm size`.
/// Returns [width, height] e.g. [1080, 2340].
async fn get_device_resolution(serial: &str) -> Option<[u32; 2]> {
    let output = tokio::process::Command::new("adb")
        .args(["-s", serial, "shell", "wm", "size"])
        .output()
        .await
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // "Physical size: 1080x2340"
    let line = stdout.lines().find(|l| l.contains("Physical size"))?;
    let dims = line.split(':').nth(1)?.trim();
    let mut parts = dims.split('x');
    let w: u32 = parts.next()?.parse().ok()?;
    let h: u32 = parts.next()?.parse().ok()?;
    Some([w, h])
}

// ── Logcat streaming ──────────────────────────────────────────────

/// WebSocket upgrade handler for device log streaming.
pub async fn ws_logs_handler(
    ws: WebSocketUpgrade,
    Path(device_id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    tracing::info!(%device_id, "WebSocket upgrade for log streaming");
    ws.on_upgrade(move |socket| handle_logs(socket, device_id, state))
}

async fn handle_logs(socket: WebSocket, device_id: Uuid, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Look up device
    let device = match ef_db::repo::device::DeviceRepo::find_by_id(&state.pool, device_id).await {
        Ok(Some(d)) => d,
        _ => {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":"Device not found"}).to_string().into()
            )).await;
            return;
        }
    };

    let serial = device.serial.clone().unwrap_or_else(|| "emulator-5554".to_string());

    // Check for remote agent with streaming URL → proxy logs
    if let Some(agent_id) = device.agent_id {
        if let Ok(Some(agent)) = ef_db::repo::agent::AgentRepo::find_by_id(&state.pool, agent_id).await {
            if let Some(ref streaming_url) = agent.streaming_url {
                tracing::info!(%device_id, %streaming_url, %serial, "Proxying logs to remote agent");
                let agent_ws_url = format!("{}/ws/logs/{}", streaming_url.trim_end_matches('/'), serial)
                    .replace("http://", "ws://").replace("https://", "wss://");
                proxy_ws_to_agent(ws_tx, ws_rx, &agent_ws_url, device_id).await;
                return;
            }
        }
    }

    // IoT devices: stream serial console as log lines
    if device.os == "iot" {
        tracing::info!(%device_id, serial = %serial, "Starting IoT serial log stream");
        run_serial_log_stream(ws_tx, ws_rx, &serial, device_id).await;
        tracing::info!(%device_id, "IoT log stream ended");
        return;
    }

    // Local device: run ADB logcat directly
    let effective_serial = if let Some(ref ip) = device.ip_address {
        let addr = format!("{ip}:5555");
        let _ = tokio::process::Command::new("adb")
            .args(["connect", &addr])
            .output()
            .await;
        addr
    } else {
        serial
    };

    tracing::info!(%device_id, serial = %effective_serial, "Starting logcat stream");

    // Clear logcat for a fresh start
    let _ = tokio::process::Command::new("adb")
        .args(["-s", &effective_serial, "logcat", "-c"])
        .output()
        .await;

    // Spawn persistent logcat subprocess
    let mut child = match tokio::process::Command::new("adb")
        .args(["-s", &effective_serial, "logcat", "-v", "threadtime"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":format!("Failed to start logcat: {e}")}).to_string().into()
            )).await;
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let reader = tokio::io::BufReader::new(stdout);
    let mut lines = reader.lines();

    let (stop_tx, _) = tokio::sync::watch::channel(false);

    // Handle incoming WS messages (stop, clear)
    let stop_tx2 = stop_tx.clone();
    let serial_ctl = effective_serial.clone();
    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(t) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t.to_string()) {
                        match v.get("type").and_then(|t| t.as_str()) {
                            Some("stop") => { let _ = stop_tx2.send(true); break; }
                            Some("clear") => {
                                let _ = tokio::process::Command::new("adb")
                                    .args(["-s", &serial_ctl, "logcat", "-c"])
                                    .output()
                                    .await;
                            }
                            _ => {}
                        }
                    }
                }
                Message::Close(_) => { let _ = stop_tx2.send(true); break; }
                _ => {}
            }
        }
    });

    let mut stop_rx = stop_tx.subscribe();
    let mut line_count: u64 = 0;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(text)) => {
                        let parsed = parse_logcat_line(&text);
                        line_count += 1;
                        if ws_tx.send(Message::Text(parsed.to_string().into())).await.is_err() {
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

    tracing::info!(%device_id, line_count, "Log stream ended");
}

/// Parse an Android logcat threadtime line into a structured JSON value.
/// Format: `MM-DD HH:MM:SS.mmm  PID  TID LEVEL TAG     : message`
fn parse_logcat_line(line: &str) -> serde_json::Value {
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

    serde_json::json!({
        "type": "log",
        "timestamp": format!("{date} {time}"),
        "level": level,
        "tag": tag.trim(),
        "message": message,
    })
}

// ── IoT serial streaming (raw bytes for xterm.js) ─────────────────

/// Stream IoT serial console as raw bytes over WebSocket, enabling full
/// terminal emulation (vi, top, etc.) via xterm.js on the client side.
///
/// Protocol:
///   Server → Client: binary frames (raw terminal output)
///   Client → Server:
///     - `{"type":"terminal_input","data":"..."}` — raw keystrokes from xterm.js
///     - `{"type":"terminal_resize","cols":N,"rows":M}` — terminal resize
///     - `{"type":"send_text","data":"..."}` — emergency command
///     - `{"type":"stop_stream"}` / `{"type":"stop"}` — disconnect
async fn run_serial_stream(
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    serial_port: &str,
    device_id: Uuid,
) {
    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();

    let port_for_input = serial_port.to_string();
    let is_windows = cfg!(target_os = "windows");

    // Channel for forwarding data to serial port stdin
    let (serial_input_tx, mut serial_input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    // WS message handler task
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
                            Some("terminal_input") | Some("send_text") => {
                                if let Some(data) = v.get("data").and_then(|d| d.as_str()) {
                                    let _ = input_tx.send(data.as_bytes().to_vec()).await;
                                }
                            }
                            Some("terminal_resize") => {
                                let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80);
                                let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24);
                                let cmd = format!("stty cols {} rows {}\r\n", cols, rows);
                                let _ = input_tx.send(cmd.into_bytes()).await;
                            }
                            _ => {}
                        }
                    }
                }
                Message::Binary(data) => {
                    let _ = input_tx.send(data.to_vec()).await;
                }
                Message::Close(_) => { let _ = stop_tx2.send(true); break; }
                _ => {}
            }
        }
    });

    let mut stop_rx = stop_tx.subscribe();

    // Spawn bidirectional serial reader/writer
    let mut child = match spawn_serial_rw_local(serial_port).await {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":format!("Failed to open {serial_port}: {e}")}).to_string().into()
            )).await;
            return;
        }
    };

    let stdout = child.stdout.take().unwrap();
    let stdin = child.stdin.take();

    // stdin writer task
    let write_stop = stop_tx.clone();
    if let Some(mut stdin) = stdin {
        tokio::spawn(async move {
            let mut stop_rx = write_stop.subscribe();
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
                    _ = stop_rx.changed() => break,
                }
            }
        });
    } else {
        // Fallback: direct serial write
        tokio::spawn(async move {
            let mut stop_rx = write_stop.subscribe();
            loop {
                tokio::select! {
                    data = serial_input_rx.recv() => {
                        match data {
                            Some(bytes) => {
                                let s = String::from_utf8_lossy(&bytes).to_string();
                                serial_write_local(&port_for_input, &s, is_windows).await;
                            }
                            None => break,
                        }
                    }
                    _ = stop_rx.changed() => break,
                }
            }
        });
    }

    // stdout → WS binary frames
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
    tracing::info!(%device_id, byte_count, "IoT terminal stream ended");
}

/// Stream IoT serial console as structured log lines (cross-platform).
async fn run_serial_log_stream(
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    serial_port: &str,
    device_id: Uuid,
) {
    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();

    tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(t) => {
                    let text = t.to_string();
                    if text.contains("stop") {
                        let _ = stop_tx2.send(true);
                        break;
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

    // Spawn platform-appropriate serial reader
    let mut child = match spawn_serial_reader_local(serial_port).await {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":format!("Failed to open {serial_port}: {e}")}).to_string().into()
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
    tracing::info!(%device_id, line_count, "IoT serial log stream ended");
}

/// Spawn a platform-appropriate process to read from a serial port (read-only, for log streams).
/// Linux: `stty -F` + `cat`, macOS: `stty -f` + `cat`, Windows: PowerShell SerialPort.
async fn spawn_serial_reader_local(serial_port: &str) -> Result<tokio::process::Child, std::io::Error> {
    if cfg!(target_os = "windows") {
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

/// Spawn a bidirectional serial process for full terminal emulation.
/// Uses `socat` on Unix for bidirectional I/O, falls back to `cat` + direct write.
/// Windows uses PowerShell with a reader thread + stdin writer.
async fn spawn_serial_rw_local(serial_port: &str) -> Result<tokio::process::Child, std::io::Error> {
    if cfg!(target_os = "windows") {
        let ps_script = format!(
            r#"$port = New-Object System.IO.Ports.SerialPort '{port}', 115200, 'None', 8, 'One'
$port.ReadTimeout = 100
$port.Open()
$reader = [System.Threading.Thread]::new({{ param($p) try {{ while ($true) {{ try {{ $b = $p.ReadByte(); if ($b -ge 0) {{ [Console]::OpenStandardOutput().WriteByte($b) }} }} catch [System.TimeoutException] {{ }} }} }} catch {{ }} }})
$reader.Start($port)
try {{ $stdin = [Console]::OpenStandardInput(); $buf = New-Object byte[] 1; while ($stdin.Read($buf,0,1) -gt 0) {{ $port.Write($buf,0,1) }} }}
finally {{ $port.Close() }}"#,
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
        // Try socat for proper bidirectional I/O first
        let stty_flag = if cfg!(target_os = "macos") { "-f" } else { "-F" };
        let _ = tokio::process::Command::new("stty")
            .args([stty_flag, serial_port, "115200", "raw", "-echo"])
            .output()
            .await;

        // Check if socat is available
        let has_socat = tokio::process::Command::new("which")
            .arg("socat")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        if has_socat {
            tokio::process::Command::new("socat")
                .args(["STDIO", &format!("FILE:{serial_port},b115200,raw,echo=0")])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
        } else {
            // Fallback: cat for reading, no stdin (serial_write_local used as fallback)
            tokio::process::Command::new("cat")
                .arg(serial_port)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
        }
    }
}

/// Write data to a serial port (cross-platform).
/// Linux/macOS: direct file write. Windows: PowerShell SerialPort.Write().
async fn serial_write_local(serial_port: &str, data: &str, is_windows: bool) {
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
/// Tries to detect ESP-IDF log format: `I (12345) tag: message`
/// Falls back to raw text with level detection.
fn parse_iot_log_line(line: &str) -> serde_json::Value {
    let trimmed = line.trim();

    // ESP-IDF format: "I (12345) TAG: message" or "E (12345) TAG: message"
    if trimmed.len() >= 3 {
        let first_char = &trimmed[..1];
        if matches!(first_char, "I" | "W" | "E" | "D" | "V")
            && trimmed[1..].starts_with(" (")
        {
            let level = match first_char {
                "I" => "info",
                "W" => "warn",
                "E" => "error",
                "D" => "debug",
                "V" => "verbose",
                _ => "info",
            };

            // Extract timestamp from parentheses
            let ts = trimmed
                .find(')')
                .map(|end| &trimmed[3..end])
                .unwrap_or("");

            // Extract tag and message after ")"
            let after_paren = trimmed.find(')').map(|i| &trimmed[i + 1..]).unwrap_or("");
            let (tag, message) = if let Some(colon) = after_paren.find(':') {
                (after_paren[..colon].trim(), after_paren[colon + 1..].trim())
            } else {
                ("", after_paren.trim())
            };

            return serde_json::json!({
                "type": "log",
                "timestamp": ts,
                "level": level,
                "tag": tag,
                "message": message,
            });
        }
    }

    // Fallback: detect level from keywords
    let level = if trimmed.contains("[error]") || trimmed.contains("ERROR") {
        "error"
    } else if trimmed.contains("[warn]") || trimmed.contains("WARN") {
        "warn"
    } else if trimmed.contains("[debug]") || trimmed.contains("DEBUG") {
        "debug"
    } else {
        "info"
    };

    serde_json::json!({
        "type": "log",
        "timestamp": "",
        "level": level,
        "tag": "serial",
        "message": trimmed,
    })
}

// ── SSH terminal streaming ────────────────────────────────────────

/// Parse SSH device serial string (`"user@host:port"` or `"host:port"`) into (host, port).
fn parse_ssh_serial(serial: &str) -> Option<(String, u16)> {
    // Strip optional user@ prefix
    let host_port = serial
        .rfind('@')
        .map(|i| &serial[i + 1..])
        .unwrap_or(serial);

    if let Some(colon) = host_port.rfind(':') {
        let host = host_port[..colon].to_string();
        let port = host_port[colon + 1..].parse::<u16>().ok()?;
        if host.is_empty() {
            return None;
        }
        Some((host, port))
    } else {
        // No port specified, default to 22
        let host = host_port.to_string();
        if host.is_empty() {
            return None;
        }
        Some((host, 22))
    }
}

/// Stream SSH terminal session as raw bytes over WebSocket (API-local, no agent proxy).
///
/// Protocol:
///   1. Server sends `{"type":"ssh_auth_required","host":"...","port":N}`
///   2. Client sends `{"type":"ssh_auth","username":"...","password":"..."}` or
///      `{"type":"ssh_auth","username":"...","key":"..."}`
///   3. After auth, bidirectional raw byte relay begins
///   4. Client → Server: `terminal_input`, `send_text`, `terminal_resize`, or binary frames
///   5. Server → Client: binary frames (raw SSH output)
async fn run_ssh_session(
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    host: &str,
    port: u16,
    device_id: Uuid,
) {
    // 1. Request auth
    let auth_req = serde_json::json!({
        "type": "ssh_auth_required",
        "host": host,
        "port": port,
    });
    if ws_tx.send(Message::Text(auth_req.to_string().into())).await.is_err() {
        return;
    }

    // 2. Wait for auth response (timeout 60s)
    let auth = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if let Message::Text(t) = msg {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t.to_string()) {
                    if v.get("type").and_then(|t| t.as_str()) == Some("ssh_auth") {
                        let username = v.get("username").and_then(|u| u.as_str()).unwrap_or("root").to_string();
                        let password = v.get("password").and_then(|p| p.as_str()).map(|s| s.to_string());
                        let key = v.get("key").and_then(|k| k.as_str()).map(|s| s.to_string());
                        return Some((username, password, key));
                    }
                }
            }
        }
        None
    }).await;

    let (username, password, key) = match auth {
        Ok(Some(creds)) => creds,
        _ => {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":"SSH authentication timeout"}).to_string().into()
            )).await;
            return;
        }
    };

    // 3. Spawn SSH process
    let mut child = if let Some(ref key_data) = key {
        // Write key to temp file
        let key_path = std::env::temp_dir().join(format!("ef_ssh_{}", device_id));
        if tokio::fs::write(&key_path, key_data.as_bytes()).await.is_err() {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":"Failed to write SSH key"}).to_string().into()
            )).await;
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
        }
        let child = tokio::process::Command::new("ssh")
            .args([
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-i", key_path.to_str().unwrap_or(""),
                "-p", &port.to_string(),
                "-tt",
                &format!("{}@{}", username, host),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        // Clean up key file after spawn
        let _ = tokio::fs::remove_file(&key_path).await;
        child
    } else if let Some(ref pw) = password {
        // Use sshpass for password auth
        let sshpass_available = tokio::process::Command::new(
            if cfg!(target_os = "windows") { "where" } else { "which" }
        )
            .arg("sshpass")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        if sshpass_available {
            tokio::process::Command::new("sshpass")
                .args([
                    "-p", pw,
                    "ssh",
                    "-o", "StrictHostKeyChecking=no",
                    "-o", "UserKnownHostsFile=/dev/null",
                    "-p", &port.to_string(),
                    "-tt",
                    &format!("{}@{}", username, host),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
        } else {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":"sshpass not installed — password auth unavailable. Use SSH key auth or install sshpass."}).to_string().into()
            )).await;
            return;
        }
    } else {
        let _ = ws_tx.send(Message::Text(
            serde_json::json!({"type":"error","message":"No password or key provided"}).to_string().into()
        )).await;
        return;
    };

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(
                serde_json::json!({"type":"error","message":format!("Failed to spawn SSH: {e}")}).to_string().into()
            )).await;
            return;
        }
    };

    let (stop_tx, _) = tokio::sync::watch::channel(false);
    let stop_tx2 = stop_tx.clone();

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take().unwrap();

    // Notify client that connection is established
    let _ = ws_tx.send(Message::Text(
        serde_json::json!({"type":"ssh_connected","host":host,"port":port}).to_string().into()
    )).await;

    // stdin writer: WS messages → SSH stdin
    let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
    let write_stop = stop_tx.clone();
    tokio::spawn(async move {
        let mut stdin = stdin;
        let mut stop_rx = write_stop.subscribe();
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

    // stderr → merge channel → will be read in main loop
    let (stderr_data_tx, mut stderr_data_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
    if let Some(stderr) = stderr {
        let err_stop = stop_tx.clone();
        tokio::spawn(async move {
            let mut stop_rx = err_stop.subscribe();
            let mut reader = tokio::io::BufReader::with_capacity(4096, stderr);
            let mut buf = [0u8; 4096];
            loop {
                tokio::select! {
                    n = reader.read(&mut buf) => {
                        match n {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if stderr_data_tx.send(buf[..n].to_vec()).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    _ = stop_rx.changed() => break,
                }
            }
        });
    }

    // WS → input channel
    let input_tx2 = input_tx.clone();
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
                                    let _ = input_tx2.send(data.as_bytes().to_vec()).await;
                                }
                            }
                            Some("terminal_resize") => {
                                let cols = v.get("cols").and_then(|c| c.as_u64()).unwrap_or(80);
                                let rows = v.get("rows").and_then(|r| r.as_u64()).unwrap_or(24);
                                // Send SIGWINCH-equivalent resize via stty
                                let cmd = format!("stty cols {} rows {}\r\n", cols, rows);
                                let _ = input_tx2.send(cmd.into_bytes()).await;
                            }
                            _ => {}
                        }
                    }
                }
                Message::Binary(data) => {
                    let _ = input_tx2.send(data.to_vec()).await;
                }
                Message::Close(_) => { let _ = stop_tx2.send(true); break; }
                _ => {}
            }
        }
    });

    // stdout + stderr → WS binary frames
    let mut reader = tokio::io::BufReader::with_capacity(4096, stdout);
    let mut buf = [0u8; 4096];
    let mut byte_count: u64 = 0;
    let mut stop_rx = stop_tx.subscribe();

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
            stderr_data = stderr_data_rx.recv() => {
                if let Some(data) = stderr_data {
                    byte_count += data.len() as u64;
                    if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }
            }
            _ = stop_rx.changed() => break,
        }
    }

    let _ = child.kill().await;
    tracing::info!(%device_id, byte_count, host, port, "SSH terminal session ended");
}
