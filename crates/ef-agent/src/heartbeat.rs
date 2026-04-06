//! Periodic heartbeat reporter — sends device health to the control plane.

use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::client::{ControlPlaneClient, HeartbeatPayload};
use crate::discovery::DeviceRegistry;

/// Spawn a background task that sends heartbeats for all tracked devices.
pub fn spawn_heartbeat_reporter(
    client: Arc<ControlPlaneClient>,
    registry: Arc<DeviceRegistry>,
    agent_id: String,
    interval_secs: u64,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        let mut shutdown = shutdown;

        tracing::info!(interval_secs, "Heartbeat reporter started");

        loop {
            tokio::select! {
                _ = interval.tick() => {},
                _ = shutdown.changed() => {
                    tracing::info!("Heartbeat reporter received shutdown signal");
                    break;
                }
            }

            let device_ids = registry.tracked_ids().await;
            for device_id in device_ids {
                let payload = HeartbeatPayload {
                    agent_id: agent_id.clone(),
                    device_id,
                    status: "available".to_string(),
                    cpu_usage: read_cpu_usage(),
                    memory_usage: read_memory_usage(),
                    timestamp: chrono::Utc::now(),
                };

                if let Err(e) = client.send_heartbeat(&payload).await {
                    tracing::warn!(%device_id, "Heartbeat send failed: {e}");
                }
            }
        }
    })
}

/// Best-effort host CPU usage reading from /proc/loadavg (Linux).
fn read_cpu_usage() -> Option<f64> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
}

/// Best-effort host memory usage reading from /proc/meminfo (Linux).
fn read_memory_usage() -> Option<f64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total: Option<f64> = None;
    let mut available: Option<f64> = None;

    for line in contents.lines() {
        if line.starts_with("MemTotal:") {
            total = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        } else if line.starts_with("MemAvailable:") {
            available = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        }
    }

    match (total, available) {
        (Some(t), Some(a)) if t > 0.0 => Some(((t - a) / t) * 100.0),
        _ => None,
    }
}
