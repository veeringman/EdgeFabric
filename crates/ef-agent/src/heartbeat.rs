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

/// Best-effort host CPU usage reading.
/// Linux: /proc/loadavg; macOS: sysctl vm.loadavg
fn read_cpu_usage() -> Option<f64> {
    // Linux
    if let Ok(s) = std::fs::read_to_string("/proc/loadavg") {
        return s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok());
    }

    // macOS: sysctl -n vm.loadavg -> "{ 1.23 1.45 1.67 }"
    std::process::Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .ok()
        .and_then(|out| {
            let s = String::from_utf8_lossy(&out.stdout);
            s.trim()
                .trim_start_matches('{')
                .trim()
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
}

/// Best-effort host memory usage reading.
/// Linux: /proc/meminfo; macOS: sysctl hw.memsize + vm_stat
fn read_memory_usage() -> Option<f64> {
    // Linux
    if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
        let mut total: Option<f64> = None;
        let mut available: Option<f64> = None;

        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                total = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
            } else if line.starts_with("MemAvailable:") {
                available = line.split_whitespace().nth(1).and_then(|v| v.parse().ok());
            }
        }

        return match (total, available) {
            (Some(t), Some(a)) if t > 0.0 => Some(((t - a) / t) * 100.0),
            _ => None,
        };
    }

    // macOS: total via sysctl, free via vm_stat
    let total_bytes: f64 = std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .and_then(|out| {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<f64>()
                .ok()
        })?;

    let vm_stat = std::process::Command::new("vm_stat")
        .output()
        .ok()?;
    let vm_out = String::from_utf8_lossy(&vm_stat.stdout);

    // vm_stat reports in pages (typically 4096 or 16384 bytes)
    let page_size: f64 = vm_out
        .lines()
        .next()
        .and_then(|l| {
            // "Mach Virtual Memory Statistics: (page size of 16384 bytes)"
            l.rsplit("of ")
                .next()
                .and_then(|s| s.split_whitespace().next())
                .and_then(|v| v.parse::<f64>().ok())
        })
        .unwrap_or(4096.0);

    let mut free_pages: f64 = 0.0;
    let mut inactive_pages: f64 = 0.0;
    let mut speculative_pages: f64 = 0.0;

    for line in vm_out.lines() {
        let extract = |l: &str| -> f64 {
            l.split(':')
                .nth(1)
                .map(|v| v.trim().trim_end_matches('.'))
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        if line.starts_with("Pages free:") {
            free_pages = extract(line);
        } else if line.starts_with("Pages inactive:") {
            inactive_pages = extract(line);
        } else if line.starts_with("Pages speculative:") {
            speculative_pages = extract(line);
        }
    }

    let free_bytes = (free_pages + inactive_pages + speculative_pages) * page_size;
    if total_bytes > 0.0 {
        Some(((total_bytes - free_bytes) / total_bytes) * 100.0)
    } else {
        None
    }
}
