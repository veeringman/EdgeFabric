//! Periodic device discovery — scans for ADB/iOS/IoT devices and
//! registers newly found devices (or deregisters disappeared ones)
//! with the control plane.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

use ef_control::discovery::DeviceDiscovery;
use ef_core::device::{Device, DeviceKind, DeviceStatus, Os};

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

    // Always include the agent's own host machine as an SSH device so operators
    // can open a terminal session directly to this node from the Live Viewer.
    // Overrides any ARP/mDNS-discovered entry for the same IP so we get the
    // proper hostname and kind (Real host vs anonymous "SSH Server").
    if let Some((host_serial, host_device)) = build_host_ssh_device(streaming_url, region) {
        current_serials.insert(host_serial, host_device);
    }

    // Register newly discovered devices
    for (serial, device) in &current_serials {
        let already_known = registry.known.read().await.contains_key(serial);
        if !already_known {
            // Check if this device name was previously known under a different serial
            // (e.g., IoT device moved to another USB port). If so, deregister the old one first.
            let stale: Option<(String, Uuid)> = {
                let known = registry.known.read().await;
                known.iter()
                    .find(|(_, (d, _))| d.name == device.name)
                    .map(|(s, (_, id))| (s.clone(), *id))
            };
            if let Some((old_serial, old_id)) = stale {
                tracing::info!(%old_id, old = %old_serial, new = %serial, "Device changed serial — deregistering old entry");
                let _ = client.deregister_device(old_id).await;
                registry.known.write().await.remove(&old_serial);
            }

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

/// Build a `Device` entry representing the agent's host machine for SSH terminal access.
///
/// The device serial is `"<host_ip>:22"` which is the format expected by `parse_ssh_serial`
/// in the streaming route.  Returns `None` if no routable IP can be determined.
fn build_host_ssh_device(streaming_url: Option<&str>, region: &str) -> Option<(String, Device)> {
    // Extract the advertised IP from the streaming URL ("http://192.168.x.x:7070").
    let ip = streaming_url
        .and_then(|url| {
            url.strip_prefix("http://")
                .or_else(|| url.strip_prefix("https://"))
        })
        .and_then(|host_port| host_port.splitn(2, ':').next())
        .filter(|ip| !ip.is_empty() && *ip != "127.0.0.1" && *ip != "0.0.0.0")
        .map(|s| s.to_string())?;

    let serial = format!("{ip}:22");

    let (hostname, os_version) = detect_host_info();

    let device = Device {
        id: Uuid::nil(),
        name: hostname,
        serial: Some(serial.clone()),
        os: Os::Ssh,
        os_version,
        kind: DeviceKind::Real,
        status: DeviceStatus::Available,
        region: region.to_string(),
            capabilities: vec!["ssh".into()],
        registered_at: chrono::Utc::now(),
        last_heartbeat: None,
    };

    Some((serial, device))
}

/// Return the current machine's hostname and OS version string.
fn detect_host_info() -> (String, String) {
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "agent-host".to_string());

    #[cfg(target_os = "linux")]
    let os_version = std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| format!("Linux {}", s.trim()))
        .unwrap_or_else(|| "Linux".to_string());

    #[cfg(target_os = "macos")]
    let os_version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| format!("macOS {}", s.trim()))
        .unwrap_or_else(|| "macOS".to_string());

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let os_version = "Unknown OS".to_string();

    (hostname, os_version)
}
