use chrono::Utc;
use ef_core::device::{Device, DeviceKind, DeviceStatus, Os};
use ef_core::error::Result;
use std::time::Duration;
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

        let mut devices = if let Ok(out) = output {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_xctrace_output(&stdout)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Fallback: idevice_id -l (libimobiledevice)
        if devices.is_empty() {
            let output = Command::new("idevice_id").arg("-l").output().await;
            if let Ok(out) = output {
                if out.status.success() {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    devices = stdout
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
                }
            }
        }

        if devices.is_empty() {
            tracing::warn!("No iOS discovery tools available (xcrun / idevice_id)");
        }

        // Enrich each device with model info from ideviceinfo
        for device in &mut devices {
            if let Some(ref udid) = device.serial {
                if let Some(model) = query_ios_model(udid).await {
                    device.capabilities.push(format!("model:{model}"));
                }
            }
        }

        info!(count = devices.len(), "iOS scan complete");
        Ok(devices)
    }

    /// Scan for IoT devices via USB serial ports (`/dev/ttyACM*`, `/dev/ttyUSB*`)
    /// and mDNS (`avahi-browse` on Linux, `dns-sd` on macOS).
    pub async fn scan_iot() -> Result<Vec<Device>> {
        info!("Scanning for IoT devices...");

        let mut all_devices = Vec::new();

        // ── USB serial port scan ──────────────────────────────────────
        all_devices.extend(scan_serial_ports().await);

        // ── mDNS scan ─────────────────────────────────────────────────
        all_devices.extend(scan_mdns_iot().await);

        info!(count = all_devices.len(), "IoT scan complete (serial + mDNS)");
        Ok(all_devices)
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

    /// Scan the local network for SSH servers (port 22) and register them
    /// as connectable terminal devices.
    ///
    /// Discovery methods (in order):
    ///   1. mDNS `_ssh._tcp` service browse (Avahi / dns-sd / Bonjour)
    ///   2. ARP table scan + TCP port-22 probe for nearby hosts
    ///
    /// Each discovered host becomes an `Os::Ssh` device whose `serial` field
    /// holds `user@host:port` (default user `root`, port `22`).
    pub async fn scan_ssh() -> Result<Vec<Device>> {
        info!("Scanning for SSH servers on the network...");

        let mut all = Vec::new();

        // ── mDNS discovery ────────────────────────────────────────
        all.extend(scan_ssh_mdns().await);

        // ── ARP + port probe (dedup against mDNS results) ────────
        let arp_hosts = scan_ssh_arp_probe().await;
        let known_ips: std::collections::HashSet<String> = all
            .iter()
            .filter_map(|d| {
                d.serial
                    .as_ref()
                    .and_then(|s| s.split('@').nth(1))
                    .map(|hp| hp.split(':').next().unwrap_or("").to_string())
            })
            .collect();

        for dev in arp_hosts {
            let ip = dev.serial.as_ref()
                .and_then(|s| s.split('@').nth(1))
                .map(|hp| hp.split(':').next().unwrap_or("").to_string())
                .unwrap_or_default();
            if !known_ips.contains(&ip) {
                all.push(dev);
            }
        }

        info!(count = all.len(), "SSH scan complete");
        Ok(all)
    }

    /// Run all platform scanners and return combined results.
    pub async fn scan_all() -> Result<Vec<Device>> {
        let (adb, ios, iot, sims, ssh) = tokio::join!(
            Self::scan_adb(),
            Self::scan_ios(),
            Self::scan_iot(),
            Self::scan_simulators(),
            Self::scan_ssh(),
        );

        let mut all = adb.unwrap_or_default();
        all.extend(ios.unwrap_or_default());
        all.extend(iot.unwrap_or_default());
        all.extend(sims.unwrap_or_default());
        all.extend(ssh.unwrap_or_default());

        info!(total = all.len(), "Full device scan complete");
        Ok(all)
    }
}

/// Scan USB serial ports for IoT devices (ESP32, Arduino, Raspberry Pi Pico, etc.).
/// Cross-platform: Linux `/dev/serial/by-id/` + `/dev/ttyACM*`, macOS `/dev/cu.usb*`,
/// Windows `Win32_SerialPort` via PowerShell.
async fn scan_serial_ports() -> Vec<Device> {
    if cfg!(target_os = "windows") {
        scan_serial_ports_windows().await
    } else if cfg!(target_os = "macos") {
        scan_serial_ports_macos().await
    } else {
        scan_serial_ports_linux().await
    }
}

/// Linux: check `/dev/serial/by-id/` for friendly names, fallback to `/dev/ttyACM*` + `/dev/ttyUSB*`.
async fn scan_serial_ports_linux() -> Vec<Device> {
    let mut devices = Vec::new();

    // Try /dev/serial/by-id/ first — provides manufacturer info in the symlink name
    let by_id = Command::new("ls")
        .args(["-1", "/dev/serial/by-id/"])
        .output()
        .await;

    if let Ok(out) = by_id {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for name in stdout.lines() {
                let name = name.trim();
                if name.is_empty() { continue; }

                // Skip Android/iOS phones that expose a USB serial interface —
                // they are already discovered by ADB / idevice scanners.
                let lower = name.to_lowercase();
                if lower.contains("android")
                    || lower.contains("samsung")
                    || lower.contains("xiaomi")
                    || lower.contains("redmi")
                    || lower.contains("oneplus")
                    || lower.contains("google")
                    || lower.contains("pixel")
                    || lower.contains("huawei")
                    || lower.contains("oppo")
                    || lower.contains("vivo")
                    || lower.contains("motorola")
                    || lower.contains("sony_mobile")
                    || lower.contains("apple")
                    || lower.contains("iphone")
                    || lower.contains("ipad")
                {
                    tracing::debug!(name, "Skipping phone serial device (handled by ADB/iOS scanner)");
                    continue;
                }

                let target = format!("/dev/serial/by-id/{name}");
                // Use the by-id name as stable serial (persists across port changes)
                // and resolve the symlink for the actual device path
                let resolved = tokio::fs::canonicalize(&target).await
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| target.clone());

                let friendly = parse_serial_device_name(name);

                devices.push(Device {
                    id: Uuid::new_v4(),
                    name: friendly,
                    serial: Some(target),
                    os: Os::Iot,
                    os_version: String::new(),
                    kind: DeviceKind::Real,
                    status: DeviceStatus::Available,
                    region: "local".into(),
                    capabilities: vec!["serial".into(), "usb".into()],
                    registered_at: Utc::now(),
                    last_heartbeat: Some(Utc::now()),
                });
            }
        }
    }

    // Fallback: raw tty devices
    if devices.is_empty() {
        for pattern in &["/dev/ttyACM*", "/dev/ttyUSB*"] {
            if let Ok(out) = Command::new("sh")
                .args(["-c", &format!("ls -1 {pattern} 2>/dev/null")])
                .output()
                .await
            {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for dev in stdout.lines() {
                    let dev = dev.trim();
                    if dev.is_empty() { continue; }
                    let short = dev.rsplit('/').next().unwrap_or(dev);
                    devices.push(Device {
                        id: Uuid::new_v4(),
                        name: format!("IoT Serial ({short})"),
                        serial: Some(dev.to_string()),
                        os: Os::Iot,
                        os_version: String::new(),
                        kind: DeviceKind::Real,
                        status: DeviceStatus::Available,
                        region: "local".into(),
                        capabilities: vec!["serial".into(), "usb".into()],
                        registered_at: Utc::now(),
                        last_heartbeat: Some(Utc::now()),
                    });
                }
            }
        }
    }

    info!(count = devices.len(), "Serial port scan complete");
    devices
}

/// macOS: scan `/dev/cu.usbmodem*` and `/dev/cu.usbserial*`, use `system_profiler` for names.
async fn scan_serial_ports_macos() -> Vec<Device> {
    let mut devices = Vec::new();

    // List /dev/cu.usb* ports
    if let Ok(out) = Command::new("sh")
        .args(["-c", "ls -1 /dev/cu.usbmodem* /dev/cu.usbserial* /dev/cu.SLAB* 2>/dev/null"])
        .output()
        .await
    {
        let stdout = String::from_utf8_lossy(&out.stdout);

        // Try system_profiler for friendly USB device names
        let usb_info = Command::new("system_profiler")
            .args(["SPUSBDataType", "-detailLevel", "mini"])
            .output()
            .await
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        for dev in stdout.lines() {
            let dev = dev.trim();
            if dev.is_empty() { continue; }

            // Derive a friendly name from the dev path or USB info
            let short = dev.rsplit('/').next().unwrap_or(dev);
            let friendly = extract_macos_usb_name(&usb_info, short)
                .unwrap_or_else(|| format!("IoT Serial ({short})"));

            devices.push(Device {
                id: Uuid::new_v4(),
                name: friendly,
                serial: Some(dev.to_string()),
                os: Os::Iot,
                os_version: String::new(),
                kind: DeviceKind::Real,
                status: DeviceStatus::Available,
                region: "local".into(),
                capabilities: vec!["serial".into(), "usb".into()],
                registered_at: Utc::now(),
                last_heartbeat: Some(Utc::now()),
            });
        }
    }

    info!(count = devices.len(), "Serial port scan complete (macOS)");
    devices
}

/// Try to extract a USB device name from `system_profiler SPUSBDataType` output
/// by matching the serial port suffix (e.g. "usbmodem14101").
fn extract_macos_usb_name(usb_info: &str, port_short: &str) -> Option<String> {
    // system_profiler lists entries like:
    //   ESP32-S3:
    //     ...
    //     Serial Number: E4:B3:23:B5:DD:28
    // We look for lines near each device heading.
    if usb_info.is_empty() { return None; }

    // Simple heuristic: find a device name line preceding a line that mentions our port number
    let port_id = port_short
        .strip_prefix("cu.usbmodem")
        .or_else(|| port_short.strip_prefix("cu.usbserial-"))
        .or_else(|| port_short.strip_prefix("cu.SLAB_USBtoUART"))
        .unwrap_or(port_short);

    let lines: Vec<&str> = usb_info.lines().collect();
    let mut last_device_name = None;

    for line in &lines {
        let trimmed = line.trim();
        // Device name lines end with ':'
        if trimmed.ends_with(':') && !trimmed.starts_with("USB") && !trimmed.contains("Bus") {
            last_device_name = Some(trimmed.trim_end_matches(':').to_string());
        }
        if trimmed.contains("Serial Number") || trimmed.contains("Location ID") {
            if trimmed.contains(port_id) || trimmed.contains(&port_id.replace("0", "")) {
                return last_device_name;
            }
        }
    }

    // If we found any Espressif/Arduino/Silicon Labs device, return it
    for line in &lines {
        let t = line.trim();
        if t.ends_with(':') {
            let name = t.trim_end_matches(':');
            if name.contains("Espressif") || name.contains("ESP") ||
               name.contains("Arduino") || name.contains("CP210") ||
               name.contains("Silicon Labs") || name.contains("FTDI") {
                return Some(name.to_string());
            }
        }
    }

    None
}

/// Windows: scan COM ports via `Get-WmiObject Win32_SerialPort` in PowerShell.
async fn scan_serial_ports_windows() -> Vec<Device> {
    let mut devices = Vec::new();

    // PowerShell command to list serial ports with description
    let ps_script = r#"Get-CimInstance Win32_SerialPort | ForEach-Object { "$($_.DeviceID)|$($_.Description)|$($_.PNPDeviceID)" }"#;

    if let Ok(out) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
        .await
    {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() { continue; }

                let parts: Vec<&str> = line.splitn(3, '|').collect();
                let com_port = parts.first().copied().unwrap_or("");
                let description = parts.get(1).copied().unwrap_or(com_port);
                let pnp_id = parts.get(2).copied().unwrap_or("");

                // Skip built-in serial ports (COM1 typically is motherboard)
                if com_port.is_empty() { continue; }

                // Extract a friendly name
                let friendly = if description.contains("Espressif") || description.contains("USB") ||
                                  description.contains("Arduino") || description.contains("Silicon") ||
                                  description.contains("FTDI") || description.contains("CH340") ||
                                  description.contains("CP210") {
                    format!("{description} ({com_port})")
                } else {
                    format!("Serial {com_port} — {description}")
                };

                // Extract serial number from PNP ID (e.g. USB\VID_303A&PID_1001\E4:B3:23:B5:DD:28)
                let serial_id = pnp_id.rsplit('\\').next().unwrap_or("");
                let name = if serial_id.contains(':') || serial_id.len() > 8 {
                    // Has a MAC or USB serial — include it
                    if description.contains("Espressif") {
                        format!("Espressif ({serial_id})")
                    } else {
                        format!("{description} ({serial_id})")
                    }
                } else {
                    friendly
                };

                devices.push(Device {
                    id: Uuid::new_v4(),
                    name,
                    serial: Some(com_port.to_string()),
                    os: Os::Iot,
                    os_version: String::new(),
                    kind: DeviceKind::Real,
                    status: DeviceStatus::Available,
                    region: "local".into(),
                    capabilities: vec!["serial".into(), "usb".into()],
                    registered_at: Utc::now(),
                    last_heartbeat: Some(Utc::now()),
                });
            }
        }
    }

    // Fallback: try [System.IO.Ports.SerialPort]::GetPortNames()
    if devices.is_empty() {
        if let Ok(out) = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                   "[System.IO.Ports.SerialPort]::GetPortNames()"])
            .output()
            .await
        {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for port in stdout.lines() {
                let port = port.trim();
                if port.is_empty() || !port.starts_with("COM") { continue; }
                devices.push(Device {
                    id: Uuid::new_v4(),
                    name: format!("IoT Serial ({port})"),
                    serial: Some(port.to_string()),
                    os: Os::Iot,
                    os_version: String::new(),
                    kind: DeviceKind::Real,
                    status: DeviceStatus::Available,
                    region: "local".into(),
                    capabilities: vec!["serial".into(), "usb".into()],
                    registered_at: Utc::now(),
                    last_heartbeat: Some(Utc::now()),
                });
            }
        }
    }

    info!(count = devices.len(), "Serial port scan complete (Windows)");
    devices
}

/// Extract a friendly name from a `/dev/serial/by-id/` symlink name.
/// e.g. `usb-Espressif_USB_JTAG_serial_debug_unit_E4:B3:23:B5:DD:28-if00` → `Espressif USB JTAG (E4:B3:23:B5:DD:28)`
fn parse_serial_device_name(by_id_name: &str) -> String {
    // Strip "usb-" prefix
    let s = by_id_name.strip_prefix("usb-").unwrap_or(by_id_name);
    // Strip "-ifNN" suffix (interface number) precisely
    let s = if let Some(pos) = s.rfind("-if") {
        let suffix = &s[pos + 3..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            &s[..pos]
        } else {
            s
        }
    } else {
        s
    };

    // Split on underscores
    let parts: Vec<&str> = s.split('_').collect();

    // Try to extract manufacturer and a MAC/serial identifier
    if parts.len() >= 2 {
        let manufacturer = parts[0].replace('_', " ");
        // Look for a MAC-like pattern (XX:XX:XX...) or long serial at the end
        let maybe_id = parts.last().unwrap_or(&"");
        if maybe_id.contains(':') || maybe_id.len() > 8 {
            format!("{manufacturer} ({maybe_id})")
        } else {
            parts.join(" ")
        }
    } else {
        s.replace('_', " ")
    }
}

/// Scan for IoT devices via mDNS.
/// Linux: `avahi-browse`, macOS: `dns-sd`, Windows: `dns-sd` (Bonjour) or skip.
async fn scan_mdns_iot() -> Vec<Device> {
    if cfg!(target_os = "windows") {
        // Windows: try dns-sd if Bonjour is installed (comes with iTunes / Bonjour SDK)
        if let Ok(Ok(out)) = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            Command::new("dns-sd")
                .args(["-Z", "_edgefabric._tcp", "local."])
                .kill_on_drop(true)
                .output(),
        ).await {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !stdout.trim().is_empty() {
                let devices = parse_dns_sd_output(&stdout);
                if !devices.is_empty() {
                    return devices;
                }
            }
        } else {
            info!("mDNS scan skipped on Windows (dns-sd / Bonjour not installed)");
        }

        return vec![];
    }

    // Try avahi-browse first (Linux)
    if let Ok(Ok(out)) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        Command::new("avahi-browse")
            .args(["-t", "-r", "-p", "_edgefabric._tcp"])
            .output(),
    ).await {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let devices = parse_avahi_output(&stdout);
            if !devices.is_empty() {
                info!(count = devices.len(), "IoT avahi scan complete");
                return devices;
            }
        }
    }

    // Fallback: dns-sd (macOS)
    if let Ok(Ok(out)) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        Command::new("dns-sd")
            .args(["-Z", "_edgefabric._tcp", "local."])
            .kill_on_drop(true)
            .output(),
    ).await {
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !stdout.trim().is_empty() {
            let devices = parse_dns_sd_output(&stdout);
            if !devices.is_empty() {
                return devices;
            }
        }
    }

    // Try generic mDNS service types
    let mut devices = Vec::new();
    for service in &["_mqtt._tcp", "_coap._udp", "_http._tcp"] {
        devices.extend(scan_mdns_service(service).await);
    }
    devices
}

/// Query ideviceinfo for the ProductType (e.g. "iPhone7,2", "iPhone14,3").
async fn query_ios_model(udid: &str) -> Option<String> {
    let out = Command::new("ideviceinfo")
        .args(["-u", udid, "-k", "ProductType"])
        .output()
        .await
        .ok()?;
    if out.status.success() {
        let model = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !model.is_empty() {
            return Some(model);
        }
    }
    None
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
    // Try dns-sd (macOS) first with timeout — dns-sd runs indefinitely
    if let Ok(Ok(out)) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        Command::new("dns-sd")
            .args(["-Z", service, "local."])
            .kill_on_drop(true)
            .output(),
    ).await
    {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            return parse_dns_sd_output(&stdout);
        }
    }

    // avahi-browse with -t flag terminates after scanning, but add timeout as safeguard
    if let Ok(Ok(out)) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        Command::new("avahi-browse")
            .args(["-t", "-r", "-p", service])
            .output(),
    ).await
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

// ── SSH server discovery ──────────────────────────────────────────

/// Discover SSH servers via mDNS (`_ssh._tcp`).
/// Uses `avahi-browse` on Linux, `dns-sd` on macOS, Bonjour `dns-sd` on Windows.
async fn scan_ssh_mdns() -> Vec<Device> {
    let mut devices = Vec::new();

    // Try avahi-browse (Linux)
    if let Ok(Ok(out)) = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new("avahi-browse")
            .args(["-t", "-r", "-p", "_ssh._tcp"])
            .output(),
    ).await {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            devices.extend(parse_ssh_avahi_output(&stdout));
            if !devices.is_empty() {
                return devices;
            }
        }
    }

    // Fallback: dns-sd (macOS / Windows with Bonjour)
    if let Ok(Ok(out)) = tokio::time::timeout(
        Duration::from_secs(3),
        Command::new("dns-sd")
            .args(["-Z", "_ssh._tcp", "local."])
            .kill_on_drop(true)
            .output(),
    ).await {
        let stdout = String::from_utf8_lossy(&out.stdout);
        devices.extend(parse_ssh_dns_sd_output(&stdout));
    }

    devices
}

/// Parse `avahi-browse -t -r -p _ssh._tcp` output for SSH servers.
/// Resolved lines: `=;eth0;IPv4;hostname;_ssh._tcp;local;hostname.local;192.168.1.100;22;`
fn parse_ssh_avahi_output(stdout: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() >= 9 && fields[0] == "=" {
            let hostname = fields[3];
            let ip = fields[7];
            let port: u16 = fields[8].parse().unwrap_or(22);

            // Skip localhost
            if ip == "127.0.0.1" || ip == "::1" { continue; }

            // serial = "ip:port" — username prompted interactively at connect time
            let serial = format!("{ip}:{port}");
            let name = if hostname.is_empty() || hostname == ip {
                format!("SSH Server ({ip})")
            } else {
                format!("{hostname} ({ip})")
            };

            devices.push(Device {
                id: Uuid::new_v4(),
                name,
                serial: Some(serial),
                os: Os::Ssh,
                os_version: String::new(),
                kind: DeviceKind::Real,
                status: DeviceStatus::Available,
                region: "local".into(),
                capabilities: vec!["ssh".into(), "terminal".into()],
                registered_at: Utc::now(),
                last_heartbeat: Some(Utc::now()),
            });
        }
    }
    devices
}

/// Parse `dns-sd -Z _ssh._tcp local.` output for SSH servers.
fn parse_ssh_dns_sd_output(stdout: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.contains("SRV") || line.contains("_ssh") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(host) = parts.last() {
                let host = host.trim_end_matches('.');
                if host.is_empty() || host == "." { continue; }

                let port: u16 = parts.iter().rev().nth(1)
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(22);

                let serial = format!("{host}:{port}");
                devices.push(Device {
                    id: Uuid::new_v4(),
                    name: format!("{host} (SSH)"),
                    serial: Some(serial),
                    os: Os::Ssh,
                    os_version: String::new(),
                    kind: DeviceKind::Real,
                    status: DeviceStatus::Available,
                    region: "local".into(),
                    capabilities: vec!["ssh".into(), "terminal".into()],
                    registered_at: Utc::now(),
                    last_heartbeat: Some(Utc::now()),
                });
            }
        }
    }
    devices
}

/// Discover SSH servers by reading the ARP table and probing port 22.
/// This catches hosts that don't advertise via mDNS.
async fn scan_ssh_arp_probe() -> Vec<Device> {
    let arp_hosts = read_arp_table().await;
    if arp_hosts.is_empty() {
        return vec![];
    }

    info!(candidates = arp_hosts.len(), "Probing ARP hosts for SSH (port 22)");

    // Probe hosts concurrently with a reasonable concurrency limit
    let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(32));
    let mut handles = Vec::new();

    for (ip, hostname) in arp_hosts {
        let permit = sem.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit.acquire().await;
            if probe_tcp_port(&ip, 22, Duration::from_millis(500)).await {
                // Try to read SSH banner for version info
                let version = read_ssh_banner(&ip, 22).await.unwrap_or_default();
                let name = if hostname.is_empty() || hostname == "?" {
                    format!("SSH Server ({ip})")
                } else {
                    format!("{hostname} ({ip})")
                };
                let serial = format!("{ip}:22");
                Some(Device {
                    id: Uuid::new_v4(),
                    name,
                    serial: Some(serial),
                    os: Os::Ssh,
                    os_version: version,
                    kind: DeviceKind::Real,
                    status: DeviceStatus::Available,
                    region: "local".into(),
                    capabilities: vec!["ssh".into(), "terminal".into()],
                    registered_at: Utc::now(),
                    last_heartbeat: Some(Utc::now()),
                })
            } else {
                None
            }
        }));
    }

    let mut devices = Vec::new();
    for handle in handles {
        if let Ok(Some(dev)) = handle.await {
            devices.push(dev);
        }
    }

    info!(count = devices.len(), "ARP SSH probe complete");
    devices
}

/// Read the ARP table and return (ip, hostname) pairs.
/// Cross-platform: `arp -a` works on Linux, macOS, and Windows.
async fn read_arp_table() -> Vec<(String, String)> {
    let output = match Command::new("arp").arg("-a").output().await {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut hosts = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 { continue; }

        // Linux/macOS: "hostname (192.168.1.x) at aa:bb:cc:dd:ee:ff ..."
        // Windows:     "  192.168.1.x  aa-bb-cc-dd-ee-ff  dynamic"
        let (hostname, ip) = if let Some(paren_ip) = parts.iter().find(|p| p.starts_with('(') && p.ends_with(')')) {
            let ip = paren_ip.trim_start_matches('(').trim_end_matches(')').to_string();
            let name = if parts[0].starts_with('(') { String::new() } else { parts[0].to_string() };
            (name, ip)
        } else if !parts.is_empty() && parts[0].contains('.') {
            (String::new(), parts[0].to_string())
        } else {
            continue;
        };

        // Skip broadcast / multicast / localhost
        if ip.starts_with("224.") || ip.starts_with("255.") || ip.starts_with("239.")
            || ip == "127.0.0.1" || ip.ends_with(".255")
        {
            continue;
        }

        if ip.split('.').count() == 4 && ip.split('.').all(|o| o.parse::<u8>().is_ok()) {
            hosts.push((ip, hostname));
        }
    }

    hosts
}

/// Attempt a TCP connection to check if a port is open.
async fn probe_tcp_port(host: &str, port: u16, timeout: Duration) -> bool {
    let addr = format!("{host}:{port}");
    tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr))
        .await
        .is_ok_and(|r| r.is_ok())
}

/// Read the SSH version banner (first line sent by an SSH server).
/// Returns something like "SSH-2.0-OpenSSH_9.6" or empty string on failure.
async fn read_ssh_banner(host: &str, port: u16) -> Option<String> {
    let addr = format!("{host}:{port}");
    let stream = tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(&addr),
    ).await.ok()?.ok()?;

    let mut reader = tokio::io::BufReader::new(stream);
    let mut banner = String::new();
    tokio::time::timeout(
        Duration::from_millis(500),
        tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut banner),
    ).await.ok()?.ok()?;

    let banner = banner.trim().to_string();
    if banner.starts_with("SSH-") {
        Some(banner)
    } else {
        None
    }
}
