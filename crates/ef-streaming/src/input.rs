//! Input relay: WebRTC data channel → device command engine.
//!
//! Receives JSON input events from the browser (via the WebRTC data channel
//! or WebSocket fallback) and dispatches them to the appropriate device
//! command driver.

use ef_core::device::{Device, Os};
use ef_control::command::adb::AdbDriver;
use ef_control::command::ios::IosDriver;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::signaling::InputEventPayload;

/// Spawn a task that reads input events and relays them to the device.
///
/// Returns the join handle so the caller can abort it when the stream ends.
pub fn spawn_input_relay(
    session_id: Uuid,
    device: Device,
    mut input_rx: mpsc::Receiver<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let serial = device.serial.as_deref().unwrap_or("unknown");
        tracing::info!(%session_id, serial, "Input relay started");

        while let Some(raw) = input_rx.recv().await {
            let event: InputEventPayload = match serde_json::from_str(&raw) {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!(%session_id, "Invalid input event: {err}");
                    continue;
                }
            };

            if let Err(e) = dispatch_input(&device, serial, &event).await {
                tracing::warn!(%session_id, "Input dispatch error: {e}");
            }
        }

        tracing::info!(%session_id, "Input relay stopped");
    })
}

async fn dispatch_input(
    device: &Device,
    serial: &str,
    event: &InputEventPayload,
) -> anyhow::Result<()> {
    match device.os {
        Os::Android => dispatch_android(serial, event).await,
        Os::Ios => dispatch_ios(serial, event).await,
        Os::Iot => {
            tracing::debug!("Input relay not supported for IoT devices");
            Ok(())
        }
    }
}

async fn dispatch_android(serial: &str, event: &InputEventPayload) -> anyhow::Result<()> {
    match event {
        InputEventPayload::Tap { x, y } => {
            AdbDriver::input_tap(serial, *x as u32, *y as u32).await?;
        }
        InputEventPayload::Swipe {
            x1, y1, x2, y2, duration_ms,
        } => {
            AdbDriver::input_swipe(
                serial,
                *x1 as u32, *y1 as u32,
                *x2 as u32, *y2 as u32,
                *duration_ms,
            )
            .await?;
        }
        InputEventPayload::Key { keycode } => {
            AdbDriver::input_key(serial, keycode).await?;
        }
        InputEventPayload::Text { text } => {
            AdbDriver::input_text(serial, text).await?;
        }
        InputEventPayload::Home => {
            AdbDriver::input_key(serial, "KEYCODE_HOME").await?;
        }
        InputEventPayload::Back => {
            AdbDriver::input_key(serial, "KEYCODE_BACK").await?;
        }
        InputEventPayload::Scroll { x, y, dx, dy } => {
            // Android doesn't have a native scroll input command;
            // simulate with a short swipe in the scroll direction.
            let x = *x as u32;
            let y = *y as u32;
            AdbDriver::input_swipe(
                serial,
                x, y,
                (x as i64 + *dx as i64).max(0) as u32,
                (y as i64 + *dy as i64).max(0) as u32,
                200,
            )
            .await?;
        }
    }
    Ok(())
}

async fn dispatch_ios(udid: &str, event: &InputEventPayload) -> anyhow::Result<()> {
    // WDA base URL — in a full implementation this would come from device metadata.
    let wda_url = format!("http://localhost:8100");

    match event {
        InputEventPayload::Tap { x, y } => {
            IosDriver::tap(&wda_url, *x, *y).await?;
        }
        InputEventPayload::Swipe {
            x1, y1, x2, y2, duration_ms,
        } => {
            IosDriver::swipe(&wda_url, *x1, *y1, *x2, *y2, *duration_ms as f64 / 1000.0).await?;
        }
        InputEventPayload::Text { text } => {
            IosDriver::input_text(&wda_url, text).await?;
        }
        InputEventPayload::Home => {
            IosDriver::press_home(&wda_url).await?;
        }
        InputEventPayload::Key { .. } | InputEventPayload::Back | InputEventPayload::Scroll { .. } => {
            tracing::debug!(udid, "Input event not directly supported on iOS, skipping");
        }
    }
    Ok(())
}
