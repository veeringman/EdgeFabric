use super::CommandOutput;
use ef_core::error::{EdgeFabricError, Result};
use std::collections::HashMap;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Virtual device lifecycle management.
///
/// - Android emulators: `avdmanager` + `emulator` + ADB
/// - iOS simulators: `xcrun simctl`
/// - IoT QEMU VMs: `qemu-system-*`
pub struct VirtualDeviceManager;

impl VirtualDeviceManager {
    // ── Android Emulator ──────────────────────────────────────────

    /// Derive the ADB serial string for an emulator on the given console port.
    pub fn emulator_serial(port: u16) -> String {
        format!("emulator-{port}")
    }

    /// Create a new Android Virtual Device.
    pub async fn create_avd(name: &str, package: &str) -> Result<CommandOutput> {
        debug!(name, package, "Creating AVD");
        let output = Command::new("avdmanager")
            .args(["create", "avd", "-n", name, "-k", package, "--force"])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Start an emulator in headless mode. The process is spawned and detached.
    pub async fn start_emulator(avd_name: &str, port: u16) -> Result<()> {
        info!(avd_name, port, "Starting Android emulator");
        let port_str = port.to_string();
        Command::new("emulator")
            .args([
                "-avd", avd_name,
                "-port", &port_str,
                "-no-window",
                "-no-audio",
                "-no-boot-anim",
            ])
            .spawn()
            .map_err(|e| EdgeFabricError::Internal(format!("Failed to start emulator: {e}")))?;
        Ok(())
    }

    /// Poll `sys.boot_completed` until the emulator reports ready.
    pub async fn wait_for_boot(serial: &str, timeout_secs: u64) -> Result<()> {
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(EdgeFabricError::Timeout);
            }
            let output = Command::new("adb")
                .args(["-s", serial, "shell", "getprop", "sys.boot_completed"])
                .output()
                .await?;
            if String::from_utf8_lossy(&output.stdout).trim() == "1" {
                info!(serial, "Emulator boot complete");
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    /// Kill an emulator via ADB emu kill.
    pub async fn stop_emulator(serial: &str) -> Result<CommandOutput> {
        info!(serial, "Stopping emulator");
        super::adb::AdbDriver::emu_kill(serial).await
    }

    /// List available AVDs (names only).
    pub async fn list_avds() -> Result<Vec<String>> {
        let output = Command::new("avdmanager")
            .args(["list", "avd", "-c"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    /// Delete an AVD.
    pub async fn delete_avd(name: &str) -> Result<CommandOutput> {
        let output = Command::new("avdmanager")
            .args(["delete", "avd", "-n", name])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Save an emulator snapshot.
    pub async fn emulator_snapshot_save(serial: &str, name: &str) -> Result<CommandOutput> {
        super::adb::AdbDriver::shell(serial, &["snapshot", "save", name]).await
    }

    /// Load an emulator snapshot.
    pub async fn emulator_snapshot_load(serial: &str, name: &str) -> Result<CommandOutput> {
        super::adb::AdbDriver::shell(serial, &["snapshot", "load", name]).await
    }

    /// List snapshots for a running emulator.
    pub async fn emulator_snapshot_list(serial: &str) -> Result<Vec<String>> {
        let output = super::adb::AdbDriver::shell(serial, &["snapshot", "list"]).await?;
        Ok(output
            .stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    /// Delete an emulator snapshot.
    pub async fn emulator_snapshot_delete(serial: &str, name: &str) -> Result<CommandOutput> {
        super::adb::AdbDriver::shell(serial, &["snapshot", "delete", name]).await
    }

    /// Detect running emulator serials (`emulator-XXXX`) via `adb devices`.
    pub async fn list_running_emulators() -> Result<Vec<String>> {
        let output = Command::new("adb")
            .args(["devices"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 && parts[1] == "device" && parts[0].starts_with("emulator-") {
                    Some(parts[0].to_string())
                } else {
                    None
                }
            })
            .collect())
    }

    /// Check if a specific emulator serial is online.
    pub async fn is_emulator_running(serial: &str) -> Result<bool> {
        let running = Self::list_running_emulators().await?;
        Ok(running.iter().any(|s| s == serial))
    }

    /// Query system properties from a running emulator (ro.build.*, etc.).
    pub async fn emulator_properties(serial: &str) -> Result<HashMap<String, String>> {
        let output = Command::new("adb")
            .args(["-s", serial, "shell", "getprop"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut props = HashMap::new();
        for line in stdout.lines() {
            // Format: [key]: [value]
            if let Some((key, val)) = line.split_once("]: [") {
                let key = key.trim_start_matches('[').to_string();
                let val = val.trim_end_matches(']').to_string();
                props.insert(key, val);
            }
        }
        Ok(props)
    }

    /// Full lifecycle: create AVD, start emulator, wait for boot, save clean snapshot.
    pub async fn provision_emulator(
        avd_name: &str,
        package: &str,
        port: u16,
        boot_timeout_secs: u64,
    ) -> Result<String> {
        info!(avd_name, port, "Provisioning new emulator");

        // Create AVD
        let create_out = Self::create_avd(avd_name, package).await?;
        if !create_out.success() {
            return Err(EdgeFabricError::Internal(format!(
                "AVD creation failed: {}",
                create_out.stderr
            )));
        }

        // Start emulator
        Self::start_emulator(avd_name, port).await?;
        let serial = Self::emulator_serial(port);

        // Wait for boot
        if let Err(e) = Self::wait_for_boot(&serial, boot_timeout_secs).await {
            warn!(serial, "Boot failed, cleaning up: {e}");
            let _ = Self::stop_emulator(&serial).await;
            return Err(e);
        }

        // Save a clean snapshot for fast reset
        let _ = Self::emulator_snapshot_save(&serial, "clean").await;
        info!(serial, "Emulator provisioned with clean snapshot");

        Ok(serial)
    }

    // ── iOS Simulator ─────────────────────────────────────────────

    /// Create a new iOS simulator. Returns the new UDID.
    pub async fn create_simulator(
        name: &str,
        device_type: &str,
        runtime: &str,
    ) -> Result<String> {
        debug!(name, device_type, runtime, "Creating iOS simulator");
        let output = Command::new("xcrun")
            .args(["simctl", "create", name, device_type, runtime])
            .output()
            .await?;

        if !output.status.success() {
            return Err(EdgeFabricError::Internal(format!(
                "simctl create failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Boot an iOS simulator.
    pub async fn boot_simulator(udid: &str) -> Result<CommandOutput> {
        info!(udid, "Booting iOS simulator");
        let output = Command::new("xcrun")
            .args(["simctl", "boot", udid])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Shutdown an iOS simulator.
    pub async fn shutdown_simulator(udid: &str) -> Result<CommandOutput> {
        info!(udid, "Shutting down iOS simulator");
        let output = Command::new("xcrun")
            .args(["simctl", "shutdown", udid])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Delete an iOS simulator.
    pub async fn delete_simulator(udid: &str) -> Result<CommandOutput> {
        info!(udid, "Deleting iOS simulator");
        let output = Command::new("xcrun")
            .args(["simctl", "delete", udid])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Erase all content and settings from a simulator.
    pub async fn erase_simulator(udid: &str) -> Result<CommandOutput> {
        info!(udid, "Erasing iOS simulator");
        let output = Command::new("xcrun")
            .args(["simctl", "erase", udid])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Install an app on a booted simulator.
    pub async fn simulator_install(udid: &str, app_path: &str) -> Result<CommandOutput> {
        let output = Command::new("xcrun")
            .args(["simctl", "install", udid, app_path])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Take a screenshot of a booted simulator. Returns PNG bytes.
    pub async fn simulator_screenshot(udid: &str) -> Result<Vec<u8>> {
        let tmp = format!("/tmp/ef-sim-screenshot-{udid}.png");
        let output = Command::new("xcrun")
            .args(["simctl", "io", udid, "screenshot", &tmp])
            .output()
            .await?;

        if !output.status.success() {
            return Err(EdgeFabricError::Internal(format!(
                "simctl screenshot failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let data = tokio::fs::read(&tmp).await?;
        let _ = tokio::fs::remove_file(&tmp).await;
        Ok(data)
    }

    /// Clone an existing simulator into a new one. Returns the new UDID.
    pub async fn clone_simulator(source_udid: &str, new_name: &str) -> Result<String> {
        info!(source_udid, new_name, "Cloning iOS simulator");
        let output = Command::new("xcrun")
            .args(["simctl", "clone", source_udid, new_name])
            .output()
            .await?;

        if !output.status.success() {
            return Err(EdgeFabricError::Internal(format!(
                "simctl clone failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Get the state of a simulator (Shutdown, Booted, etc.).
    pub async fn simulator_status(udid: &str) -> Result<String> {
        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", "--json"])
            .output()
            .await?;

        if !output.status.success() {
            return Err(EdgeFabricError::Internal("simctl list failed".into()));
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|e| {
                EdgeFabricError::Internal(format!("Failed to parse simctl JSON: {e}"))
            })?;

        if let Some(devices) = json.get("devices").and_then(|d| d.as_object()) {
            for (_runtime, sims) in devices {
                if let Some(arr) = sims.as_array() {
                    for sim in arr {
                        if sim.get("udid").and_then(|u| u.as_str()) == Some(udid) {
                            return Ok(sim
                                .get("state")
                                .and_then(|s| s.as_str())
                                .unwrap_or("Unknown")
                                .to_string());
                        }
                    }
                }
            }
        }

        Err(EdgeFabricError::NotFound(format!("Simulator {udid} not found")))
    }

    /// List available simulator runtimes.
    pub async fn list_simulator_runtimes() -> Result<Vec<String>> {
        let output = Command::new("xcrun")
            .args(["simctl", "list", "runtimes", "-j"])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(vec![]);
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_default();
        let mut runtimes = Vec::new();
        if let Some(arr) = json.get("runtimes").and_then(|r| r.as_array()) {
            for rt in arr {
                if let Some(id) = rt.get("identifier").and_then(|i| i.as_str()) {
                    runtimes.push(id.to_string());
                }
            }
        }
        Ok(runtimes)
    }

    /// Full lifecycle: create simulator, boot, return UDID.
    pub async fn provision_simulator(
        name: &str,
        device_type: &str,
        runtime: &str,
    ) -> Result<String> {
        info!(name, device_type, runtime, "Provisioning iOS simulator");

        let udid = Self::create_simulator(name, device_type, runtime).await?;
        let boot_result = Self::boot_simulator(&udid).await?;
        if !boot_result.success() {
            warn!(udid, "Boot failed, cleaning up");
            let _ = Self::delete_simulator(&udid).await;
            return Err(EdgeFabricError::Internal(format!(
                "Simulator boot failed: {}",
                boot_result.stderr
            )));
        }

        info!(udid, "Simulator provisioned and booted");
        Ok(udid)
    }

    /// List simulators as JSON.
    pub async fn list_simulators() -> Result<CommandOutput> {
        let output = Command::new("xcrun")
            .args(["simctl", "list", "devices", "--json"])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

// ── QEMU IoT Emulation ───────────────────────────────────────────

/// QEMU-based IoT device emulation.
///
/// Creates lightweight VMs that simulate IoT hardware:
/// - ARM / RISC-V / x86 architectures
/// - Firmware loading from binary images
/// - Serial console bridged to a TCP port for SSH-like access
/// - Snapshot save/restore via QEMU monitor
pub struct QemuIotManager;

/// Supported QEMU target architectures.
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum QemuArch {
    Arm,
    Aarch64,
    RiscV64,
    X86_64,
}

impl QemuArch {
    /// QEMU binary name for this architecture.
    pub fn binary(&self) -> &'static str {
        match self {
            Self::Arm => "qemu-system-arm",
            Self::Aarch64 => "qemu-system-aarch64",
            Self::RiscV64 => "qemu-system-riscv64",
            Self::X86_64 => "qemu-system-x86_64",
        }
    }

    /// Default machine type for this architecture.
    pub fn default_machine(&self) -> &'static str {
        match self {
            Self::Arm => "virt",
            Self::Aarch64 => "virt",
            Self::RiscV64 => "virt",
            Self::X86_64 => "q35",
        }
    }
}

/// Configuration for a QEMU IoT VM.
#[derive(Debug, Clone, Hash, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QemuVmConfig {
    pub name: String,
    pub arch: QemuArch,
    pub ram_mb: u32,
    pub firmware_path: String,
    /// TCP port for the QEMU monitor (QMP).
    pub monitor_port: u16,
    /// TCP port for the serial console bridge.
    pub serial_port: u16,
    /// Optional SSH forwarded port (host → guest:22).
    pub ssh_forward_port: Option<u16>,
    /// Optional extra QEMU arguments.
    pub extra_args: Vec<String>,
}

impl QemuIotManager {
    /// Build the base directory for QEMU VM state.
    fn vm_dir(name: &str) -> String {
        format!("/var/lib/edgefabric/qemu/{name}")
    }

    /// Start a QEMU VM with the given configuration.
    ///
    /// The VM runs as a detached background process with:
    /// - Serial console on a TCP port (`-serial tcp::<port>,server,nowait`)
    /// - QEMU monitor on a TCP port (`-monitor tcp::<port>,server,nowait`)
    /// - Optional SSH port forwarding via user-mode networking
    pub async fn start_vm(config: &QemuVmConfig) -> Result<()> {
        let dir = Self::vm_dir(&config.name);
        tokio::fs::create_dir_all(&dir).await?;

        info!(
            name = config.name,
            arch = ?config.arch,
            ram = config.ram_mb,
            "Starting QEMU IoT VM"
        );

        let ram = config.ram_mb.to_string();
        let serial_addr = format!("tcp::{},server,nowait", config.serial_port);
        let monitor_addr = format!("tcp::{},server,nowait", config.monitor_port);
        let pidfile = format!("{dir}/qemu.pid");

        let mut args = vec![
            "-machine", config.arch.default_machine(),
            "-m", &ram,
            "-nographic",
            "-serial", &serial_addr,
            "-monitor", &monitor_addr,
            "-pidfile", &pidfile,
            "-daemonize",
        ];

        // Firmware / kernel
        args.extend_from_slice(&["-kernel", &config.firmware_path]);

        // Networking with optional SSH forward
        let netdev = if let Some(ssh_port) = config.ssh_forward_port {
            format!(
                "user,id=net0,hostfwd=tcp::{ssh_port}-:22"
            )
        } else {
            "user,id=net0".to_string()
        };
        args.extend_from_slice(&["-netdev", &netdev, "-device", "virtio-net-device,netdev=net0"]);

        // Extra user-supplied args
        let extra_refs: Vec<&str> = config.extra_args.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&extra_refs);

        let output = Command::new(config.arch.binary())
            .args(&args)
            .output()
            .await?;

        if !output.status.success() {
            return Err(EdgeFabricError::Internal(format!(
                "QEMU start failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        info!(name = config.name, "QEMU VM started");
        Ok(())
    }

    /// Stop a QEMU VM by sending `quit` to the monitor port.
    pub async fn stop_vm(name: &str, monitor_port: u16) -> Result<()> {
        info!(name, "Stopping QEMU VM via monitor");
        let addr = format!("127.0.0.1:{monitor_port}");
        match tokio::net::TcpStream::connect(&addr).await {
            Ok(mut stream) => {
                use tokio::io::AsyncWriteExt;
                let _ = stream.write_all(b"quit\n").await;
                let _ = stream.shutdown().await;
                info!(name, "QEMU VM stop command sent");
                Ok(())
            }
            Err(e) => {
                // Fall back to pid file
                let pidfile = format!("{}/qemu.pid", Self::vm_dir(name));
                Self::kill_by_pidfile(&pidfile).await.map_err(|_| {
                    EdgeFabricError::Internal(format!(
                        "Cannot connect to QEMU monitor ({addr}) and pidfile kill failed: {e}"
                    ))
                })
            }
        }
    }

    /// Force-kill a VM via its PID file.
    async fn kill_by_pidfile(pidfile: &str) -> Result<()> {
        let pid_str = tokio::fs::read_to_string(pidfile).await?;
        let pid = pid_str.trim();
        let output = Command::new("kill")
            .args(["-9", pid])
            .output()
            .await?;
        if output.status.success() {
            let _ = tokio::fs::remove_file(pidfile).await;
            Ok(())
        } else {
            Err(EdgeFabricError::Internal(format!(
                "Failed to kill PID {pid}: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Delete a VM's state directory.
    pub async fn delete_vm(name: &str) -> Result<()> {
        let dir = Self::vm_dir(name);
        if tokio::fs::try_exists(&dir).await.unwrap_or(false) {
            tokio::fs::remove_dir_all(&dir).await?;
            info!(name, "QEMU VM directory removed");
        }
        Ok(())
    }

    /// Check if a VM is running by probing the monitor port.
    pub async fn is_vm_running(monitor_port: u16) -> bool {
        tokio::net::TcpStream::connect(format!("127.0.0.1:{monitor_port}"))
            .await
            .is_ok()
    }

    /// Load firmware onto an existing VM by stopping, replacing the kernel, and restarting.
    pub async fn load_firmware(
        config: &QemuVmConfig,
        firmware_path: &str,
    ) -> Result<()> {
        info!(name = config.name, firmware_path, "Loading new firmware");

        // Stop if running
        if Self::is_vm_running(config.monitor_port).await {
            Self::stop_vm(&config.name, config.monitor_port).await?;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        // Start with new firmware
        let mut updated = config.clone();
        updated.firmware_path = firmware_path.to_string();
        Self::start_vm(&updated).await
    }

    /// Send a command to the QEMU monitor (QMP text mode).
    pub async fn monitor_command(monitor_port: u16, cmd: &str) -> Result<String> {
        let addr = format!("127.0.0.1:{monitor_port}");
        let mut stream = tokio::net::TcpStream::connect(&addr)
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("Monitor connect failed: {e}")))?;

        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Read greeting
        let mut buf = vec![0u8; 4096];
        let _ = stream.read(&mut buf).await;

        // Send command
        stream
            .write_all(format!("{cmd}\n").as_bytes())
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("Monitor write failed: {e}")))?;

        // Read response
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let mut response = vec![0u8; 8192];
        let n = stream.read(&mut response).await.unwrap_or(0);
        Ok(String::from_utf8_lossy(&response[..n]).to_string())
    }

    /// Save a VM snapshot via the QEMU monitor.
    pub async fn snapshot_save(monitor_port: u16, name: &str) -> Result<String> {
        Self::monitor_command(monitor_port, &format!("savevm {name}")).await
    }

    /// Load a VM snapshot via the QEMU monitor.
    pub async fn snapshot_load(monitor_port: u16, name: &str) -> Result<String> {
        Self::monitor_command(monitor_port, &format!("loadvm {name}")).await
    }

    /// Delete a VM snapshot via the QEMU monitor.
    pub async fn snapshot_delete(monitor_port: u16, name: &str) -> Result<String> {
        Self::monitor_command(monitor_port, &format!("delvm {name}")).await
    }

    /// List VM snapshots via the QEMU monitor.
    pub async fn snapshot_list(monitor_port: u16) -> Result<String> {
        Self::monitor_command(monitor_port, "info snapshots").await
    }
}
