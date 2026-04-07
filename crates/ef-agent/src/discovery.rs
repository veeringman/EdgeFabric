//! Periodic device discovery — scans for ADB/iOS/IoT devices and
//! registers newly found devices (or deregisters disappeared ones)
//! with the control plane.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

use ef_control::discovery::DeviceDiscovery;
use ef_core::device::Device;

use crate::client::{ControlPlaneClient, RegisterDeviceRequest};

/// Tracks which devices have been seen and registered.
pub struct DeviceRegistry {
    /// serial → (device, control-plane device_id)
    known: RwLock<HashMap<String, (Device, Uuid)>>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self {
            known: RwLock::new(HashMap::new()),
        }
    }

    /// Return the list of currently tracked device IDs.
    pub async fn tracked_ids(&self) -> Vec<Uuid> {
        self.known.read().await.values().map(|(_, id)| *id).collect()
    }

    /// Return a device by its control-plane ID.
    pub async fn device_by_id(&self, id: Uuid) -> Option<Device> {
        let map = self.known.read().await;
        map.values()
            .find(|(_, cid)| *cid == id)
            .map(|(d, _)| d.clone())
    }
}

/// Spawn the discovery loop that periodically scans for devices
/// and reconciles with the control plane.
pub fn spawn_discovery_loop(
    client: Arc<ControlPlaneClient>,
    registry: Arc<DeviceRegistry>,
    agent_id: String,
    region: String,
    interval_secs: u64,
    streaming_url: Option<String>,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        let mut shutdown = shutdown;

        tracing::info!(
            interval_secs,
            "Device discovery loop started"
        );

        loop {
            tokio::select! {
                _ = interval.tick() => {},
                _ = shutdown.changed() => {
                    tracing::info!("Discovery loop received shutdown signal");
                    break;
                }
            }

            if let Err(e) = run_scan(&client, &registry, &agent_id, &region, streaming_url.as_deref()).await {
                tracing::error!("Discovery scan failed: {e}");
            }
        }
    })
}

async fn run_scan(
    client: &ControlPlaneClient,
    registry: &DeviceRegistry,
    agent_id: &str,
    region: &str,
    streaming_url: Option<&str>,
) -> anyhow::Result<()> {
    let discovered = DeviceDiscovery::scan_all().await?;

    tracing::debug!(count = discovered.len(), "Discovery scan complete");

    // Build a set of serials from the current scan
    let mut current_serials: HashMap<String, Device> = HashMap::new();
    for d in &discovered {
        if let Some(ref serial) = d.serial {
            current_serials.insert(serial.clone(), d.clone());
        }
    }

    // Register newly discovered devices
    for (serial, device) in &current_serials {
        let already_known = registry.known.read().await.contains_key(serial);
        if !already_known {
            // Try to get device IP for remote ADB access
            let ip_address = get_device_ip(serial).await;

            let req = RegisterDeviceRequest {
                name: device.name.clone(),
                serial: Some(serial.clone()),
                os: device.os.as_str().to_string(),
                os_version: device.os_version.clone(),
                kind: device.kind.as_str().to_string(),
                region: region.to_string(),
                capabilities: device.capabilities.clone(),
                agent_id: agent_id.to_string(),
                ip_address,
                streaming_url: streaming_url.map(|s| s.to_string()),
            };

            match client.register_device(&req).await {
                Ok(device_id) => {
                    tracing::info!(%device_id, serial, "Registered new device");
                    registry
                        .known
                        .write()
                        .await
                        .insert(serial.clone(), (device.clone(), device_id));
                }
                Err(e) => {
                    tracing::warn!(serial, "Failed to register device: {e}");
                }
            }
        }
    }

    // Deregister devices that have disappeared
    let disappeared: Vec<(String, Uuid)> = {
        let known = registry.known.read().await;
        known
            .iter()
            .filter(|(serial, _)| !current_serials.contains_key(*serial))
            .map(|(serial, (_, id))| (serial.clone(), *id))
            .collect()
    };

    for (serial, device_id) in &disappeared {
        tracing::info!(%device_id, serial, "Device disappeared — deregistering");
        if let Err(e) = client.deregister_device(*device_id).await {
            tracing::warn!(%device_id, "Deregister failed: {e}");
        }
        registry.known.write().await.remove(serial);
    }

    Ok(())
}

/// Get the WiFi IP address of an ADB device for remote streaming.
/// Runs `adb -s <serial> shell ip route` and parses the src address.
async fn get_device_ip(serial: &str) -> Option<String> {
    let output = tokio::process::Command::new("adb")
        .args(["-s", serial, "shell", "ip", "route"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse lines like: "192.168.29.0/24 dev wlan0 proto kernel scope link src 192.168.29.5"
    for line in stdout.lines() {
        if let Some(idx) = line.find("src ") {
            let ip = line[idx + 4..].split_whitespace().next()?;
            if !ip.starts_with("127.") {
                return Some(ip.to_string());
            }
        }
    }
    None
}
