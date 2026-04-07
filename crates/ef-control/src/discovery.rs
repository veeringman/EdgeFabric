use chrono::Utc;
use ef_core::device::{Device, DeviceKind, DeviceStatus, Os};
use ef_core::error::Result;
use tokio::process::Command;
use tracing::info;
use uuid::Uuid;

/// Discovers devices connected to this edge node.
pub struct DeviceDiscovery;

impl DeviceDiscovery {
    /// Scan for Android devices via `adb devices -l`.
    /// Parses output lines like: `R5CT900ABCD  device usb:1-1 product:oriole model:Pixel_6 ...`
    pub async fn scan_adb() -> Result<Vec<Device>> {
        info!("Scanning for ADB devices...");

        let output = Command::new("adb")
            .args(["devices", "-l"])
            .output()
            .await;

        let output = match output {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("adb not available: {e}");
                return Ok(vec![]);
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut devices = Vec::new();

        for line in stdout.lines().skip(1) {
            // skip the "List of devices attached" header
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let serial = parts[0];
            let state = parts[1]; // "device", "offline", "unauthorized"

            if state != "device" {
                continue;
            }

            // Extract model name from key:value pairs
            let model = parts.iter()
                .find_map(|p| p.strip_prefix("model:"))
                .unwrap_or(serial)
                .replace('_', " ");

            devices.push(Device {
                id: Uuid::new_v4(),
                name: model,
                serial: Some(serial.to_string()),
                os: Os::Android,
                os_version: String::new(), // filled later via getprop
                kind: DeviceKind::Real,
                status: DeviceStatus::Available,
                region: "local".into(),
                capabilities: vec!["adb".into()],
                registered_at: Utc::now(),
                last_heartbeat: Some(Utc::now()),
            });
        }

        info!(count = devices.len(), "ADB scan complete");
        Ok(devices)
    }

    /// Scan for iOS devices via `xcrun xctrace list devices`.
    /// Falls back to `idevice_id -l` if xctrace is unavailable.
    pub async fn scan_ios() -> Result<Vec<Device>> {
        info!("Scanning for iOS devices...");

        // Try xcrun first (macOS with Xcode)
        let output = Command::new("xcrun")
            .args(["xctrace", "list", "devices"])
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let devices = parse_xctrace_output(&stdout);
                info!(count = devices.len(), "iOS xctrace scan complete");
                return Ok(devices);
            }
        }

        // Fallback: idevice_id -l (libimobiledevice)
        let output = Command::new("idevice_id").arg("-l").output().await;
        match output {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let devices: Vec<Device> = stdout
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|udid| Device {
                        id: Uuid::new_v4(),
                        name: format!("iOS-{}", &udid[..8.min(udid.len())]),
                        serial: Some(udid.trim().to_string()),
                        os: Os::Ios,
                        os_version: String::new(),
                        kind: DeviceKind::Real,
                        status: DeviceStatus::Available,
                        region: "local".into(),
                        capabilities: vec!["usb".into()],
                        registered_at: Utc::now(),
                        last_heartbeat: Some(Utc::now()),
                    })
                    .collect();
                info!(count = devices.len(), "iOS idevice_id scan complete");
                Ok(devices)
            }
            _ => {
                tracing::warn!("No iOS discovery tools available (xcrun / idevice_id)");
                Ok(vec![])
            }
        }
    }

    /// Scan for IoT devices via mDNS (`avahi-browse` on Linux, `dns-sd` on macOS).
    pub async fn scan_iot() -> Result<Vec<Device>> {
        info!("Scanning for IoT devices...");

        // Try avahi-browse first (Linux)
        let output = Command::new("avahi-browse")
            .args(["-t", "-r", "-p", "_edgefabric._tcp"])
            .output()
            .await;

        if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let devices = parse_avahi_output(&stdout);
                info!(count = devices.len(), "IoT avahi scan complete");
                return Ok(devices);
            }
        }

        // Fallback: dns-sd (macOS built-in)
        // dns-sd -B _edgefabric._tcp runs indefinitely, so we use a short timeout
        let output = Command::new("dns-sd")
            .args(["-Z", "_edgefabric._tcp", "local."])
            .output()
            .await;

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !stdout.trim().is_empty() {
                let devices = parse_dns_sd_output(&stdout);
                info!(count = devices.len(), "IoT dns-sd scan complete");
                return Ok(devices);
            }
        }

        // Also try generic IoT mDNS service types common in edge deployments
        let iot_services = ["_mqtt._tcp", "_coap._udp", "_http._tcp"];
        let mut iot_devices = Vec::new();

        for service in &iot_services {
            let result = scan_mdns_service(service).await;
            iot_devices.extend(result);
        }

        if !iot_devices.is_empty() {
            info!(count = iot_devices.len(), "IoT mDNS scan complete (generic services)");
            return Ok(iot_devices);
        }

        tracing::warn!("No mDNS tools available (avahi-browse / dns-sd); IoT scan skipped");
        Ok(vec![])
    }

    /// Scan for iOS/iPadOS simulators via `xcrun simctl list devices`.
    pub async fn scan_simulators() -> Result<Vec<Device>> {
        info!("Scanning for iOS/iPadOS simulators...");

        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", "--json"])
            .output()
            .await;

        let output = match output {
            Ok(o) if o.status.success() => o,
            Ok(_) => {
                tracing::debug!("xcrun simctl returned non-zero");
                return Ok(vec![]);
            }
            Err(e) => {
                tracing::debug!("xcrun simctl not available: {e}");
                return Ok(vec![]);
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let devices = parse_simctl_output(&stdout);
        info!(count = devices.len(), "Simulator scan complete");
        Ok(devices)
    }

    /// Run all platform scanners and return combined results.
    pub async fn scan_all() -> Result<Vec<Device>> {
        let (adb, ios, iot, sims) = tokio::join!(
            Self::scan_adb(),
            Self::scan_ios(),
            Self::scan_iot(),
            Self::scan_simulators(),
        );

        let mut all = adb.unwrap_or_default();
        all.extend(ios.unwrap_or_default());
        all.extend(iot.unwrap_or_default());
        all.extend(sims.unwrap_or_default());

        info!(total = all.len(), "Full device scan complete");
        Ok(all)
    }
}

/// Parse `xcrun xctrace list devices` output.
/// Lines look like: `iPhone 15 Pro (17.4) (00008030-001234560ABC002E)`
fn parse_xctrace_output(stdout: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    let mut in_devices_section = false;

    for line in stdout.lines() {
        let line = line.trim();
        if line.starts_with("== Devices ==") {
            in_devices_section = true;
            continue;
        }
        if line.starts_with("== Simulators ==") {
            // stop at simulators section (separate from real devices)
            break;
        }
        if !in_devices_section || line.is_empty() {
            continue;
        }

        // Pattern: "Name (version) (udid)"
        if let Some((name_ver, udid_part)) = line.rsplit_once('(') {
            let udid = udid_part.trim_end_matches(')').trim().to_string();
            let name_ver = name_ver.trim().trim_end_matches('(').trim();
            if let Some((name, version)) = name_ver.rsplit_once('(') {
                let name = name.trim().to_string();
                let version = version.trim_end_matches(')').trim().to_string();
                devices.push(Device {
                    id: Uuid::new_v4(),
                    name,
                    serial: Some(udid),
                    os: Os::Ios,
                    os_version: version,
                    kind: DeviceKind::Real,
                    status: DeviceStatus::Available,
                    region: "local".into(),
                    capabilities: vec!["usb".into()],
                    registered_at: Utc::now(),
                    last_heartbeat: Some(Utc::now()),
                });
            }
        }
    }
    devices
}

/// Parse `avahi-browse -t -r -p _edgefabric._tcp` output.
/// Parseable lines: `=;eth0;IPv4;hostname;_edgefabric._tcp;local;hostname.local;192.168.1.100;8080;`
fn parse_avahi_output(stdout: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(';').collect();
        // We want resolve lines starting with '='
        if fields.len() >= 8 && fields[0] == "=" {
            let hostname = fields[3];
            let ip = fields[7];
            devices.push(Device {
                id: Uuid::new_v4(),
                name: format!("{hostname} ({ip})"),
                serial: Some(ip.to_string()),
                os: Os::Iot,
                os_version: String::new(),
                kind: DeviceKind::Real,
                status: DeviceStatus::Available,
                region: "local".into(),
                capabilities: vec!["mdns".into(), "ssh".into()],
                registered_at: Utc::now(),
                last_heartbeat: Some(Utc::now()),
            });
        }
    }
    devices
}

/// Parse `dns-sd -Z` output (macOS).
/// Lines look like: `hostname._edgefabric._tcp.local. ... A 192.168.1.100`
/// or SRV records: `_edgefabric._tcp  SRV  0 0 8080 hostname.local.`
fn parse_dns_sd_output(stdout: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        // Look for SRV records containing the host
        if line.contains("SRV") || line.contains("_edgefabric") {
            // Extract hostname from SRV record
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(host) = parts.last() {
                let host = host.trim_end_matches('.');
                if !host.is_empty() && host != "." {
                    devices.push(Device {
                        id: Uuid::new_v4(),
                        name: host.to_string(),
                        serial: Some(host.to_string()),
                        os: Os::Iot,
                        os_version: String::new(),
                        kind: DeviceKind::Real,
                        status: DeviceStatus::Available,
                        region: "local".into(),
                        capabilities: vec!["mdns".into(), "ssh".into()],
                        registered_at: Utc::now(),
                        last_heartbeat: Some(Utc::now()),
                    });
                }
            }
        }
    }
    devices
}

/// Scan a generic mDNS service type using platform-appropriate tools.
async fn scan_mdns_service(service: &str) -> Vec<Device> {
    // Try dns-sd (macOS) first, then avahi-browse (Linux)
    if let Ok(out) = Command::new("dns-sd")
        .args(["-Z", service, "local."])
        .output()
        .await
    {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            return parse_dns_sd_output(&stdout);
        }
    }

    if let Ok(out) = Command::new("avahi-browse")
        .args(["-t", "-r", "-p", service])
        .output()
        .await
    {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            return parse_avahi_output(&stdout);
        }
    }

    vec![]
}

/// Parse `xcrun simctl list devices --json` output.
/// JSON structure: { "devices": { "com.apple.CoreSimulator.SimRuntime.iOS-17-4": [ { ... } ] } }
fn parse_simctl_output(stdout: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    let parsed: serde_json::Value = match serde_json::from_str(stdout) {
        Ok(v) => v,
        Err(_) => return devices,
    };

    let device_map = match parsed.get("devices").and_then(|d| d.as_object()) {
        Some(m) => m,
        None => return devices,
    };

    for (runtime, sims) in device_map {
        let sims = match sims.as_array() {
            Some(a) => a,
            None => continue,
        };

        // Extract OS version from runtime string
        // e.g. "com.apple.CoreSimulator.SimRuntime.iOS-17-4" -> "17.4"
        let os_version = runtime
            .rsplit("SimRuntime.")
            .next()
            .unwrap_or("")
            .replace("iOS-", "")
            .replace("iPadOS-", "")
            .replace('-', ".");

        let is_ios = runtime.contains("iOS") || runtime.contains("iPadOS");
        if !is_ios {
            continue;
        }

        for sim in sims {
            let state = sim.get("state").and_then(|s| s.as_str()).unwrap_or("");
            if state != "Booted" {
                continue;
            }

            let name = sim.get("name").and_then(|s| s.as_str()).unwrap_or("Simulator");
            let udid = sim.get("udid").and_then(|s| s.as_str()).unwrap_or("");

            devices.push(Device {
                id: Uuid::new_v4(),
                name: name.to_string(),
                serial: Some(udid.to_string()),
                os: Os::Ios,
                os_version: os_version.clone(),
                kind: DeviceKind::Virtual,
                status: DeviceStatus::Available,
                region: "local".into(),
                capabilities: vec!["simctl".into()],
                registered_at: Utc::now(),
                last_heartbeat: Some(Utc::now()),
            });
        }
    }
    devices
}
