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
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::middleware::auth_extractor::AppState;

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
    let serial = if let Ok(Some(device)) = ef_db::repo::device::DeviceRepo::find_by_id(&state.pool, id).await {
        tracing::info!(%id, name = %device.name, "Found device directly");
        device.serial.unwrap_or_else(|| "emulator-5554".to_string())
    } else if let Ok(Some(session)) = ef_db::repo::session::SessionRepo::find_by_id(&state.pool, id).await {
        match ef_db::repo::device::DeviceRepo::find_by_id(&state.pool, session.device_id).await {
            Ok(Some(d)) => d.serial.unwrap_or_else(|| "emulator-5554".to_string()),
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

    tracing::info!(%id, %serial, "Starting screenshot stream");

    // Send stream_ready immediately — no handshake needed
    let _ = ws_tx.send(Message::Text(
        serde_json::json!({
            "type": "stream_ready",
            "device_id": id,
            "serial": serial,
        }).to_string().into()
    )).await;

    run_screenshot_loop(ws_tx, ws_rx, &serial, id).await;

    tracing::info!(%id, "Stream ended");
}

/// Continuously capture screenshots via ADB and send as binary WebSocket frames.
/// Also handles incoming input events.
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

    loop {
        // Check for stop
        if *stop_rx.borrow() {
            break;
        }

        // Capture screenshot via ADB
        match capture_screenshot(&serial_owned).await {
            Ok(png_data) => {
                frame_count += 1;
                if ws_tx.send(Message::Binary(png_data.into())).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(e) => {
                if frame_count == 0 {
                    tracing::error!(%session_id, "Screenshot capture failed: {e}");
                    let _ = ws_tx.send(Message::Text(
                        serde_json::json!({"type":"error","message": format!("Capture failed: {e}")}).to_string().into()
                    )).await;
                    break;
                }
                tracing::warn!(%session_id, "Screenshot capture error: {e}");
            }
        }

        // Handle pending input events
        while let Ok(event_json) = input_rx.try_recv() {
            handle_input_event(&serial_owned, &event_json).await;
        }

        // ~5 FPS
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {}
            _ = stop_rx.changed() => break,
        }
    }

    tracing::info!(%session_id, frame_count, "Screenshot stream ended");
}

/// Capture a single screenshot via `adb exec-out screencap -p`.
async fn capture_screenshot(serial: &str) -> anyhow::Result<Vec<u8>> {
    let output = tokio::process::Command::new("adb")
        .args(["-s", serial, "exec-out", "screencap", "-p"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("adb screencap failed: {stderr}");
    }

    if output.stdout.len() < 100 {
        anyhow::bail!("Screenshot too small ({} bytes)", output.stdout.len());
    }

    Ok(output.stdout)
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
