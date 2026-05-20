use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use ef_core::config::VeerOsConfig;
use ef_core::error::{EdgeFabricError, Result};
use quantum::{Circuit, QuantumBackend};
use quantum::simulator::SimulatorBackend;
use serde_json::{Value, json};

use super::CommandOutput;

#[derive(Debug, Clone)]
pub struct VeerOsShellInfo {
    pub vm_name: String,
    pub host: String,
    pub port: u16,
    pub connect_command: String,
    /// For folded VMs: path to the sensor-feed FIFO used as VM stdin.
    /// When set, the WS bridge writes keystrokes here instead of process stdin.
    pub sensor_feed_path: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum QuantumProgram {
    Bell,
    Ghz3,
    Qft3,
}

impl QuantumProgram {
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Bell => "bell",
            Self::Ghz3 => "ghz3",
            Self::Qft3 => "qft3",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum QuantumEndpointVariant {
    Default,
    Staging,
}

impl QuantumEndpointVariant {
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Staging => "staging",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuantumSimulationRequest {
    pub program: Option<QuantumProgram>,
    pub qasm: Option<String>,
    pub shots: u32,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct QuantumIbmRunRequest {
    pub program: Option<QuantumProgram>,
    pub qasm_file: Option<String>,
    pub shots: u32,
    pub backend: String,
    pub token: Option<String>,
    pub instance: Option<String>,
    pub poll_ms: u64,
    pub timeout_sec: u64,
    pub histogram: bool,
    pub endpoint: Option<QuantumEndpointVariant>,
    pub json: bool,
}

pub struct VeerOsDriver;

impl VeerOsDriver {
    const FOLD_SHELL_PORT: u16 = 2323;

    fn fold_binary_path(cfg: &VeerOsConfig) -> Option<String> {
        let p = PathBuf::from(&cfg.root_dir).join("target/debug/fold");
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    }

    fn veer_vm_binary_path(cfg: &VeerOsConfig) -> Option<String> {
        let p = PathBuf::from(&cfg.root_dir).join("target/debug/veer-vm");
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    }

    fn kernel_path_qemu(cfg: &VeerOsConfig, target: &str) -> Option<String> {
        let p = PathBuf::from(&cfg.root_dir).join(format!(
            "target/riscv32imc-unknown-none-elf/debug/kernel-qemu-{target}"
        ));
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    }

    fn kernel_path_fold(cfg: &VeerOsConfig, target: &str) -> Option<String> {
        let p = PathBuf::from(&cfg.root_dir).join(format!(
            "target/riscv32imc-unknown-none-elf/debug/kernel-fold-{target}"
        ));
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    }

    fn kernel_path_for_fold_runtime(cfg: &VeerOsConfig, target: &str) -> Option<String> {
        // Prefer a fold-specific kernel naming convention when available.
        // Fall back to qemu naming for backward compatibility.
        Self::kernel_path_fold(cfg, target).or_else(|| Self::kernel_path_qemu(cfg, target))
    }

    fn fold_manifest_path(vm_name: &str) -> String {
        format!("/tmp/edgefabric-{vm_name}.toml")
    }

    fn sensor_fifo_path(vm_name: &str) -> String {
        format!("/tmp/edgefabric-{vm_name}.sensor")
    }

    fn fold_log_path(vm_name: &str) -> String {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        format!("{home}/.local/state/veeros/fold/{vm_name}.log")
    }

    fn parse_dhcp_ip_from_log(log_content: &str) -> Option<String> {
        // Expected line shape:
        // [net] DHCP: acquired 10.77.0.12/24
        let mut found = None;
        for line in log_content.lines() {
            let Some((_, after)) = line.split_once("[net] DHCP: acquired ") else {
                continue;
            };
            let ip = after.split('/').next().unwrap_or_default().trim();
            if !ip.is_empty() && ip.chars().all(|c| c.is_ascii_digit() || c == '.') {
                found = Some(ip.to_string());
            }
        }
        found
    }

    async fn ensure_sensor_fifo(path: &str) -> Result<()> {
        use std::os::unix::fs::FileTypeExt;
        if let Ok(meta) = tokio::fs::metadata(path).await {
            if meta.file_type().is_fifo() {
                return Ok(());
            }
            // Exists but not a FIFO — remove and recreate.
            tokio::fs::remove_file(path).await.map_err(|e| {
                EdgeFabricError::Internal(format!("removing non-FIFO at sensor path {path}: {e}"))
            })?;
        }
        // mkfifo via shell (avoids adding libc as a direct dependency).
        let status = tokio::process::Command::new("mkfifo")
            .arg("-m")
            .arg("600")
            .arg(path)
            .status()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("spawning mkfifo: {e}")))?;
        if !status.success() {
            return Err(EdgeFabricError::Internal(format!("mkfifo {path} failed")));
        }
        Ok(())
    }

    fn is_fold_runtime_available(cfg: &VeerOsConfig, target: &str) -> bool {
        target.eq_ignore_ascii_case("esp32c6")
            && Self::fold_binary_path(cfg).is_some()
            && Self::veer_vm_binary_path(cfg).is_some()
    }

    fn resolve_vm_script_path(cfg: &VeerOsConfig) -> Option<String> {
        let configured = Path::new(&cfg.vm_script_path);
        if configured.exists() {
            return Some(cfg.vm_script_path.clone());
        }

        // Backward/forward compatibility:
        // - legacy: scripts/veeros-vm
        // - folded microCM runtime: scripts/veer-vm
        let candidates = [
            format!("{}/scripts/veer-vm", cfg.root_dir),
            format!("{}/scripts/veeros-vm", cfg.root_dir),
        ];

        candidates
            .into_iter()
            .find(|candidate| Path::new(candidate).exists())
    }

    pub fn sanitize_vm_name(name: &str, suffix: &str) -> String {
        let mut out = String::with_capacity(name.len() + suffix.len() + 1);
        for ch in name.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
            } else if (ch == '-' || ch == '_' || ch == ' ') && !out.ends_with('-') {
                out.push('-');
            }
        }
        while out.ends_with('-') {
            out.pop();
        }
        if out.is_empty() {
            out.push_str("virtual-iot");
        }
        out.push('-');
        out.push_str(suffix);
        out
    }

    pub async fn create_vm(
        cfg: &VeerOsConfig,
        vm_name: &str,
        hostname: &str,
        target: &str,
        memory_mb: u32,
        net_mode: &str,
    ) -> Result<CommandOutput> {
        if Self::is_fold_runtime_available(cfg, target) {
            let veer_vm = Self::veer_vm_binary_path(cfg)
                .ok_or_else(|| EdgeFabricError::Internal("veer-vm binary not found".into()))?;
            let kernel = Self::kernel_path_for_fold_runtime(cfg, target).ok_or_else(|| {
                EdgeFabricError::Internal(format!("kernel image not found for target {target}"))
            })?;
            let tap = match net_mode {
                "" | "nat" | "bridge" => "tap0",
                other => other,
            };
            let manifest_path = Self::fold_manifest_path(vm_name);
            let fifo_path = Self::sensor_fifo_path(vm_name);
            let manifest = format!(
                "name = \"{vm_name}\"\ncmd  = \"{veer_vm}\"\nargs = [\n  \"--arch\", \"riscv32\",\n  \"--kernel\", \"{kernel}\",\n  \"--memory\", \"{memory_mb}\",\n  \"--tap\", \"{tap}\",\n  \"--sensor-feed\", \"{fifo_path}\",\n]\nhostname = \"{hostname}\"\nworkdir  = \"{}\"\n\n[env]\nPATH = \"/usr/local/bin:/usr/bin:/bin\"\nTERM = \"xterm-256color\"\n\n[namespaces]\npid   = true\nmount = true\nuts   = true\nipc   = true\nnet   = false\nuser  = true\n\n[seccomp]\nprofile = \"default\"\n",
                cfg.root_dir,
            );
            // Ensure the sensor FIFO exists before fold spawns veer-vm.
            Self::ensure_sensor_fifo(&fifo_path).await?;
            tokio::fs::write(&manifest_path, manifest).await.map_err(|e| {
                EdgeFabricError::Internal(format!("failed to write fold manifest: {e}"))
            })?;
            return Ok(CommandOutput {
                stdout: format!("manifest created at {manifest_path}"),
                stderr: String::new(),
                exit_code: 0,
            });
        }

        Self::run_vm_command(
            cfg,
            &[
                "create",
                vm_name,
                "--target",
                target,
                "--memory",
                &memory_mb.to_string(),
                "--net",
                net_mode,
                "--hostname",
                hostname,
            ],
        )
        .await
    }

    pub async fn start_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        if let Some(fold) = Self::fold_binary_path(cfg) {
            let manifest = Self::fold_manifest_path(vm_name);
            if Path::new(&manifest).exists() {
                // If already running, this is a no-op.
                if let Ok(status) = Self::fold_status_of(cfg, &fold, vm_name).await {
                    if status == "running" {
                        return Ok(CommandOutput {
                            stdout: format!("fold '{vm_name}' already running"),
                            stderr: String::new(),
                            exit_code: 0,
                        });
                    }
                    // Clean up any stale stopped record so `spawn` can recreate it.
                    if status == "stopped" {
                        let _ = Self::run_command(cfg, &fold, &["rm", vm_name]).await;
                    }
                }
                // Ensure the sensor FIFO still exists (may have been cleared
                // by /tmp cleanup between boots).
                let fifo_path = Self::sensor_fifo_path(vm_name);
                Self::ensure_sensor_fifo(&fifo_path).await?;

                let out = Self::run_command(cfg, &fold, &["spawn", "--manifest", &manifest]).await?;
                if !out.success() {
                    return Err(EdgeFabricError::Internal(format!(
                        "fold spawn failed (exit {}): {}{}",
                        out.exit_code,
                        out.stderr.trim(),
                        if out.stdout.trim().is_empty() { String::new() } else { format!(" stdout={}", out.stdout.trim()) },
                    )));
                }
                return Ok(out);
            }
        }
        let out = Self::run_vm_command(cfg, &["start", vm_name]).await?;
        if !out.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veer-vm start failed (exit {}): {}",
                out.exit_code, out.stderr.trim()
            )));
        }
        Ok(out)
    }

    pub async fn stop_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        if let Some(fold) = Self::fold_binary_path(cfg) {
            let manifest = Self::fold_manifest_path(vm_name);
            if Path::new(&manifest).exists() {
                // If already stopped, succeed.
                if let Ok(status) = Self::fold_status_of(cfg, &fold, vm_name).await {
                    if status == "stopped" {
                        return Ok(CommandOutput {
                            stdout: format!("fold '{vm_name}' already stopped"),
                            stderr: String::new(),
                            exit_code: 0,
                        });
                    }
                }
                let out = Self::run_command(cfg, &fold, &["stop", vm_name]).await?;
                if !out.success() {
                    return Err(EdgeFabricError::Internal(format!(
                        "fold stop failed (exit {}): {}",
                        out.exit_code, out.stderr.trim()
                    )));
                }
                return Ok(out);
            }
        }
        let out = Self::run_vm_command(cfg, &["stop", vm_name]).await?;
        if !out.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veer-vm stop failed (exit {}): {}",
                out.exit_code, out.stderr.trim()
            )));
        }
        Ok(out)
    }

    pub async fn restart_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        // Manifest-managed fold runtime: stop-then-start explicitly so that
        // the stale state record is cleaned before re-spawn.
        if let Some(fold) = Self::fold_binary_path(cfg) {
            let manifest = Self::fold_manifest_path(vm_name);
            if Path::new(&manifest).exists() {
                let _ = Self::stop_vm(cfg, vm_name).await;
                return Self::start_vm(cfg, vm_name).await;
            }
            // Keep `fold` in scope for future use (e.g. env-prefixed invocations).
            let _ = fold;
        }
        let out = Self::run_vm_command(cfg, &["restart", vm_name]).await?;
        if !out.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veer-vm restart failed (exit {}): {}",
                out.exit_code, out.stderr.trim()
            )));
        }
        Ok(out)
    }

    pub async fn destroy_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        if let Some(fold) = Self::fold_binary_path(cfg) {
            let manifest = Self::fold_manifest_path(vm_name);
            if Path::new(&manifest).exists() {
                // Make sure it's stopped first so `rm` doesn't refuse.
                if let Ok(status) = Self::fold_status_of(cfg, &fold, vm_name).await {
                    if status == "running" {
                        let _ = Self::run_command(cfg, &fold, &["stop", vm_name]).await;
                    }
                }
                // rm --force should succeed whether the fold exists or not.
                let out = Self::run_command(cfg, &fold, &["rm", vm_name, "--force"]).await?;
                // Ignore "not found" style errors — the manifest cleanup below
                // is the real source of truth for our API.
                let _ = tokio::fs::remove_file(&manifest).await;
                let _ = tokio::fs::remove_file(Self::sensor_fifo_path(vm_name)).await;
                return Ok(out);
            }
        }
        let out = Self::run_vm_command(cfg, &["destroy", vm_name]).await?;
        if !out.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veer-vm destroy failed (exit {}): {}",
                out.exit_code, out.stderr.trim()
            )));
        }
        Ok(out)
    }

    pub async fn vm_config(cfg: &VeerOsConfig, vm_name: &str) -> Result<HashMap<String, String>> {
        let manifest = Self::fold_manifest_path(vm_name);
        if Path::new(&manifest).exists() && Self::fold_binary_path(cfg).is_some() {
            let mut config = HashMap::new();
            config.insert("runtime".to_string(), "fold+veer-vm".to_string());
            config.insert("manifest".to_string(), manifest);
            // fold+veer-vm currently does not expose a host-forwarded VSC TCP port.
            config.insert("ssh_port".to_string(), "0".to_string());
            config.insert("shell_host".to_string(), "localhost".to_string());
            return Ok(config);
        }

        let output = Self::run_vm_command(cfg, &["config", vm_name]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veeros-vm config failed: {}",
                output.stderr
            )));
        }
        let mut config = HashMap::new();
        for line in output.stdout.lines() {
            if line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                config.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        Ok(config)
    }

    pub async fn vm_status(cfg: &VeerOsConfig, vm_name: &str) -> Result<String> {
        let manifest = Self::fold_manifest_path(vm_name);
        if Path::new(&manifest).exists() {
            if let Some(fold) = Self::fold_binary_path(cfg) {
                return Self::fold_status_of(cfg, &fold, vm_name).await;
            }
        }

        let output = Self::run_vm_command(cfg, &["status", vm_name]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veeros-vm status failed: {}",
                output.stderr
            )));
        }
        let line = output.stdout.lines().next().unwrap_or_default().to_ascii_lowercase();
        if line.contains("running") {
            Ok("running".into())
        } else {
            Ok("stopped".into())
        }
    }

    pub async fn list_vms(cfg: &VeerOsConfig) -> Result<Vec<serde_json::Value>> {
        let output = Self::run_vm_command(cfg, &["list-json"]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "veeros-vm list-json failed: {}",
                output.stderr
            )));
        }
        let vms: Vec<serde_json::Value> = serde_json::from_str(output.stdout.trim())
            .map_err(|e| EdgeFabricError::Internal(format!("failed to parse VM list: {e}")))?;
        Ok(vms)
    }

    /// Parse `fold list` output and return the status ("running"/"stopped"/"unknown")
    /// for the given fold name. Returns "unknown" when the fold is not present
    /// in the list (i.e. no state record exists yet).
    async fn fold_status_of(cfg: &VeerOsConfig, fold: &str, vm_name: &str) -> Result<String> {
        let output = Self::run_command(cfg, fold, &["list"]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "fold list failed: {}",
                output.stderr
            )));
        }
        for line in output.stdout.lines() {
            // `fold list` columns: NAME  PID  STATUS  STARTED  CMD
            let mut cols = line.split_whitespace();
            let Some(name) = cols.next() else { continue };
            if name != vm_name {
                continue;
            }
            let _pid = cols.next();
            let status = cols.next().unwrap_or("").to_ascii_lowercase();
            return Ok(match status.as_str() {
                "running" => "running".into(),
                "stopped" | "error" | "exited" => "stopped".into(),
                _ => "unknown".into(),
            });
        }
        Ok("unknown".into())
    }

    pub async fn sensor_inject(
        cfg: &VeerOsConfig,
        vm_name: &str,
        sensor: &str,
        value: &str,
    ) -> Result<CommandOutput> {
        let manifest = Self::fold_manifest_path(vm_name);
        if Path::new(&manifest).exists() && Self::fold_binary_path(cfg).is_some() {
            return Self::inject_via_sensor_fifo(vm_name, sensor, value).await;
        }
        Self::run_vm_command(cfg, &["sensor", "inject", vm_name, sensor, value]).await
    }

    /// Write a `sensor set <name> <value_centi>\n` command to the VM's sensor
    /// FIFO.  veer-vm's sensor_feed_thread will pick it up and inject the bytes
    /// into the guest UART RX queue, where the VeerOS shell executes them.
    ///
    /// The VeerOS `sensor set` command expects the value as an integer ×100
    /// (fixed-point centis).  We accept a float string here and convert.
    async fn inject_via_sensor_fifo(
        vm_name: &str,
        sensor: &str,
        value: &str,
    ) -> Result<CommandOutput> {
        let fifo_path = Self::sensor_fifo_path(vm_name);

        // Validate sensor name — only alphanumeric + underscore allowed.
        if sensor.is_empty() || !sensor.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(EdgeFabricError::Internal(format!(
                "invalid sensor name '{sensor}': only alphanumeric and underscore allowed"
            )));
        }

        // Convert value string to centis (×100 fixed point).
        let value_f: f64 = value.parse().map_err(|_| {
            EdgeFabricError::Internal(format!("sensor value '{value}' is not a number"))
        })?;
        let value_centi = (value_f * 100.0).round() as i64;

        // Command the VeerOS shell understands: `sensor set <name> <centi>\n`
        let cmd = format!("sensor set {sensor} {value_centi}\n");

        // Open the FIFO in non-blocking write mode so we don't stall if
        // veer-vm is not yet ready.  We use std::fs::OpenOptions with
        // O_NONBLOCK (via raw open flags on Unix).
        let fifo_path_clone = fifo_path.clone();
        let cmd_clone = cmd.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            // O_NONBLOCK: open fails with ENXIO if no reader has the FIFO open.
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo_path_clone)?;
            f.write_all(cmd_clone.as_bytes())?;
            Ok(())
        })
        .await
        .map_err(|e| EdgeFabricError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| EdgeFabricError::Internal(format!(
            "writing to sensor FIFO {fifo_path}: {e}; is veer-vm running and ready?"
        )))?;

        Ok(CommandOutput {
            stdout: format!("injected sensor '{sensor}' = {value_f} ({value_centi} centi) into {vm_name}"),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    /// Write a GPIO actuator command to the VM via the sensor FIFO.
    /// Emits `gpio write <pin> <level>\n` which the VeerOS shell executes,
    /// flipping the simulated pin and publishing an `[actuator]` sentinel line.
    pub async fn actuator_gpio_write(
        _cfg: &VeerOsConfig,
        vm_name: &str,
        pin: u32,
        level: u32,
    ) -> Result<CommandOutput> {
        if pin >= 30 {
            return Err(EdgeFabricError::Internal(format!(
                "invalid gpio pin {pin}: must be 0..=29"
            )));
        }
        if level > 1 {
            return Err(EdgeFabricError::Internal(format!(
                "invalid gpio level {level}: must be 0 or 1"
            )));
        }
        let fifo_path = Self::sensor_fifo_path(vm_name);
        let cmd = format!("gpio write {pin} {level}\n");

        let fifo_path_clone = fifo_path.clone();
        let cmd_clone = cmd.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo_path_clone)?;
            f.write_all(cmd_clone.as_bytes())?;
            Ok(())
        })
        .await
        .map_err(|e| EdgeFabricError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| EdgeFabricError::Internal(format!(
            "writing to sensor FIFO {fifo_path}: {e}; is veer-vm running and ready?"
        )))?;

        Ok(CommandOutput {
            stdout: format!("gpio write {pin} {level} -> {vm_name}"),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    /// Stream a shell script into the VM via the sensor FIFO.
    ///
    /// Each non-empty line in `source` is written to the FIFO followed by
    /// `\n`, where the VeerOS shell consumes them one by one. The shell's
    /// existing output flows back through the VM log tail and the sensor
    /// stream, so live-viewer actuator/sensor events fire in real time.
    ///
    /// This is a synchronous best-effort push: if the FIFO is backed up the
    /// write may block briefly, but each line is tiny so in practice the
    /// kernel consumes them well under a millisecond.
    pub async fn script_run(
        _cfg: &VeerOsConfig,
        vm_name: &str,
        source: &str,
    ) -> Result<CommandOutput> {
        // Hard cap to avoid runaway uploads.
        const MAX_BYTES: usize = 16 * 1024;
        if source.len() > MAX_BYTES {
            return Err(EdgeFabricError::Internal(format!(
                "script too large: {} bytes (max {MAX_BYTES})",
                source.len()
            )));
        }

        let fifo_path = Self::sensor_fifo_path(vm_name);

        // Normalize line endings to `\n` and ensure trailing newline.
        let mut payload = source.replace("\r\n", "\n").replace('\r', "\n");
        if !payload.ends_with('\n') {
            payload.push('\n');
        }

        let fifo_path_clone = fifo_path.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo_path_clone)?;
            f.write_all(payload.as_bytes())?;
            Ok(())
        })
        .await
        .map_err(|e| EdgeFabricError::Internal(format!("spawn_blocking: {e}")))?
        .map_err(|e| EdgeFabricError::Internal(format!(
            "writing to sensor FIFO {fifo_path}: {e}; is veer-vm running and ready?"
        )))?;

        let line_count = source.lines().count();
        Ok(CommandOutput {
            stdout: format!("script run -> {vm_name} ({line_count} line(s), {} bytes)", source.len()),
            stderr: String::new(),
            exit_code: 0,
        })
    }

    pub async fn shell_info(cfg: &VeerOsConfig, vm_name: &str) -> Result<VeerOsShellInfo> {
        let manifest = Self::fold_manifest_path(vm_name);
        let fold_bin = Self::fold_binary_path(cfg);
        if Path::new(&manifest).exists() && fold_bin.is_some() {
            // Prefer the real TCP shell once the folded guest has acquired a DHCP
            // lease. Until then, fall back to the folded runtime's live log stream
            // plus sensor FIFO bridge while TAP/DHCP is still being brought up.
            let log_path = Self::fold_log_path(vm_name);
            let log_bytes = tokio::fs::read(&log_path).await.map_err(|e| {
                EdgeFabricError::Internal(format!(
                    "reading fold log {log_path} for VM {vm_name}: {e}"
                ))
            })?;
            let log_text = String::from_utf8_lossy(&log_bytes);
            if let Some(guest_ip) = Self::parse_dhcp_ip_from_log(&log_text) {
                return Ok(VeerOsShellInfo {
                    vm_name: vm_name.to_string(),
                    host: guest_ip.clone(),
                    port: Self::FOLD_SHELL_PORT,
                    connect_command: format!(
                        "{} shell {} {}",
                        cfg.veer_connect_path,
                        guest_ip,
                        Self::FOLD_SHELL_PORT
                    ),
                    sensor_feed_path: None,
                });
            }

            // Stream only NEW bytes from the fold log via `tail -n 0 -F`.
            // Using `fold logs -f` would replay the VM's entire accumulated
            // history on every reconnect (every prior `root@veeros-…>` prompt
            // and command), which is confusing and makes the shell appear
            // stuck on old state. Tailing from end gives a clean session.
            let _ = fold_bin; // path already validated above; not needed downstream
            return Ok(VeerOsShellInfo {
                vm_name: vm_name.to_string(),
                host: "localhost".to_string(),
                port: 0,
                connect_command: format!("tail -n 0 -F {}", log_path),
                sensor_feed_path: Some(Self::sensor_fifo_path(vm_name)),
            });
        }

        let config = Self::vm_config(cfg, vm_name).await?;
        let port = config
            .get("ssh_port")
            .and_then(|v| v.parse::<u16>().ok())
            .ok_or_else(|| EdgeFabricError::Internal("VeerOS VM is missing ssh_port".into()))?;
        let host = "localhost".to_string();
        Ok(VeerOsShellInfo {
            vm_name: vm_name.to_string(),
            host: host.clone(),
            port,
            connect_command: format!("{} shell {} {}", cfg.veer_connect_path, host, port),
            sensor_feed_path: None,
        })
    }

    pub fn quantum_backend_catalog(cfg: &VeerOsConfig) -> Vec<Value> {
        let sim = SimulatorBackend::new();
        let sim_info = sim.backend_info();

        vec![
            json!({
                "id": "veeros-local-sim",
                "provider": "veeros",
                "kind": "simulator",
                "name": sim_info.name,
                "max_qubits": sim_info.max_qubits,
                "queue_depth": sim_info.queue_depth,
                "supports": {
                    "program_templates": ["bell", "ghz3", "qft3"],
                    "qasm": true,
                    "histogram": true
                }
            }),
            json!({
                "id": "ibm-quantum-cloud",
                "provider": "ibm-quantum",
                "kind": "cloud",
                "name": "IBM Quantum",
                "access": {
                    "via_cli": cfg.veer_cli_path,
                    "cli_available": Path::new(&cfg.veer_cli_path).exists(),
                    "endpoint_variants": ["default", "staging"]
                },
                "supports": {
                    "program_templates": ["bell", "ghz3", "qft3"],
                    "qasm_file": true,
                    "histogram": true
                },
                "required_credentials": ["token", "instance", "backend"]
            }),
        ]
    }

    pub fn simulate_quantum(req: &QuantumSimulationRequest) -> Result<Value> {
        let circuit = Self::build_circuit(req.program, req.qasm.as_deref())?;
        circuit
            .status()
            .map_err(|e| EdgeFabricError::Internal(format!("invalid quantum circuit: {:?}", e)))?;

        let mut backend = if let Some(seed) = req.seed {
            SimulatorBackend::with_seed(seed)
        } else {
            SimulatorBackend::new()
        };

        let hist = backend
            .execute_shots(&circuit, req.shots)
            .map_err(|e| EdgeFabricError::Internal(format!("simulation failed: {:?}", e)))?;

        let qubits = circuit.logical_qubits as usize;
        let mut histogram = serde_json::Map::new();
        let mut top_bitstring = String::new();
        let mut top_count = 0u32;

        let mut slot = 0usize;
        while let Some((key, count)) = hist.bin(slot) {
            let bitstring = Self::histogram_key_to_bitstring(key, qubits);
            if count > top_count {
                top_count = count;
                top_bitstring = bitstring.clone();
            }
            histogram.insert(bitstring, json!(count));
            slot += 1;
        }

        Ok(json!({
            "provider": "veeros",
            "backend": "veeros-local-sim",
            "shots": req.shots,
            "top_bitstring": top_bitstring,
            "qubits": qubits,
            "histogram": Value::Object(histogram),
        }))
    }

    pub async fn run_quantum_ibm(
        cfg: &VeerOsConfig,
        req: &QuantumIbmRunRequest,
    ) -> Result<CommandOutput> {
        let mut args: Vec<String> = vec![
            "quantum".into(),
            "ibm-run".into(),
            "--backend".into(),
            req.backend.clone(),
            "--shots".into(),
            req.shots.to_string(),
            "--poll-ms".into(),
            req.poll_ms.to_string(),
            "--timeout-sec".into(),
            req.timeout_sec.to_string(),
        ];

        if let Some(program) = req.program {
            args.push("--program".into());
            args.push(program.as_cli_value().into());
        }
        if let Some(qasm_file) = req.qasm_file.as_ref() {
            args.push("--qasm-file".into());
            args.push(qasm_file.clone());
        }
        if let Some(token) = req.token.as_ref() {
            args.push("--token".into());
            args.push(token.clone());
        }
        if let Some(instance) = req.instance.as_ref() {
            args.push("--instance".into());
            args.push(instance.clone());
        }
        if req.histogram {
            args.push("--histogram".into());
        }
        if req.json {
            args.push("--json".into());
        }
        if let Some(endpoint) = req.endpoint {
            args.push("--endpoint".into());
            args.push(endpoint.as_cli_value().into());
        }

        Self::run_veer_command(cfg, &args).await
    }

    async fn run_vm_command(cfg: &VeerOsConfig, args: &[&str]) -> Result<CommandOutput> {
        let vm_script_path = Self::resolve_vm_script_path(cfg).ok_or_else(|| {
            EdgeFabricError::Internal(format!(
                "VeerOS VM script not found: {} (also checked {}/scripts/veer-vm and {}/scripts/veeros-vm)",
                cfg.vm_script_path,
                cfg.root_dir,
                cfg.root_dir,
            ))
        })?;

        Self::run_command(cfg, &vm_script_path, args).await
    }

    async fn run_command(cfg: &VeerOsConfig, program: &str, args: &[&str]) -> Result<CommandOutput> {
        if !Path::new(program).exists() {
            return Err(EdgeFabricError::Internal(format!(
                "VeerOS runtime command not found: {program}",
            )));
        }

        let output = Command::new(program)
            .current_dir(&cfg.root_dir)
            .args(args)
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_veer_command(cfg: &VeerOsConfig, args: &[String]) -> Result<CommandOutput> {
        if !Path::new(&cfg.veer_cli_path).exists() {
            return Err(EdgeFabricError::Internal(format!(
                "VeerOS CLI not found: {}",
                cfg.veer_cli_path
            )));
        }

        let output = Command::new(&cfg.veer_cli_path)
            .current_dir(&cfg.root_dir)
            .args(args)
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    fn build_circuit(program: Option<QuantumProgram>, qasm: Option<&str>) -> Result<Circuit> {
        if let Some(qasm_text) = qasm {
            Self::parse_quantum_circuit_from_qasm(qasm_text)
        } else {
            Ok(Self::build_quantum_program(program.unwrap_or(QuantumProgram::Bell)))
        }
    }

    fn build_quantum_program(program: QuantumProgram) -> Circuit {
        match program {
            QuantumProgram::Bell => Circuit::new(2).h(0).cx(0, 1).measure_all(),
            QuantumProgram::Ghz3 => Circuit::new(3).h(0).cx(0, 1).cx(1, 2).measure_all(),
            QuantumProgram::Qft3 => Circuit::new(3)
                .h(0)
                .cx(0, 1)
                .h(1)
                .cx(1, 2)
                .h(2)
                .measure_all(),
        }
    }

    fn parse_quantum_circuit_from_qasm(qasm: &str) -> Result<Circuit> {
        let mut qubits = 2usize;

        for line in qasm.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("qubit[") && trimmed.contains(']') {
                if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.find(']')) {
                    if let Ok(count) = trimmed[start + 1..end].parse::<usize>() {
                        qubits = count;
                    }
                }
            }
        }

        let mut circuit = Circuit::new(qubits as u16);
        for line in qasm.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("OPENQASM")
                || trimmed.starts_with("include")
                || trimmed.starts_with("qubit[")
                || trimmed.starts_with("bit[")
            {
                continue;
            }

            if let Some(gate_part) = trimmed.split(' ').next() {
                match gate_part {
                    "h" | "x" | "y" | "z" => {
                        if let Some(idx) = Self::extract_qubit_index(trimmed) {
                            circuit = match gate_part {
                                "h" => circuit.h(idx as u16),
                                "x" => circuit.x(idx as u16),
                                "y" => circuit.y(idx as u16),
                                "z" => circuit.z(idx as u16),
                                _ => circuit,
                            };
                        }
                    }
                    "cx" => {
                        if let Some((q0, q1)) = Self::extract_two_qubit_indices(trimmed) {
                            circuit = circuit.cx(q0 as u16, q1 as u16);
                        }
                    }
                    "measure" => {
                        if let Some(idx) = Self::extract_qubit_index(trimmed) {
                            circuit = circuit.measure(idx as u16);
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(circuit)
    }

    fn extract_qubit_index(line: &str) -> Option<usize> {
        if let Some(start) = line.find("q[") {
            if let Some(end) = line[start..].find(']') {
                return line[start + 2..start + end].parse::<usize>().ok();
            }
        }
        None
    }

    fn extract_two_qubit_indices(line: &str) -> Option<(usize, usize)> {
        let mut indices = Vec::new();
        let mut current = 0usize;
        while current < line.len() {
            if let Some(start) = line[current..].find("q[") {
                if let Some(end) = line[current + start..].find(']') {
                    if let Ok(idx) = line[current + start + 2..current + start + end].parse::<usize>() {
                        indices.push(idx);
                        current += start + end + 1;
                        if indices.len() == 2 {
                            return Some((indices[0], indices[1]));
                        }
                    } else {
                        current += start + 1;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        None
    }

    fn histogram_key_to_bitstring(key: u16, width: usize) -> String {
        let mut bits = String::with_capacity(width.max(1));
        let mut index = width;
        while index > 0 {
            index -= 1;
            let bit = if ((key >> index) & 1) == 1 { '1' } else { '0' };
            bits.push(bit);
        }
        bits
    }
}