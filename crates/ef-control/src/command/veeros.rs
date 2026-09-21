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

    fn resolved_root_dir(cfg: &VeerOsConfig) -> String {
        let configured = PathBuf::from(&cfg.root_dir);
        if configured.exists() {
            return configured.to_string_lossy().to_string();
        }

        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(PathBuf::from(&home).join("rnd/projects/VeerOS"));
            candidates.push(PathBuf::from(&home).join("VeerOS"));
        }
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join("../VeerOS"));
        }
        candidates.push(PathBuf::from("/home/vijay/rnd/projects/VeerOS"));
        candidates.push(PathBuf::from("/home/vijay/rnd/VeerOS"));

        for candidate in candidates {
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }

        cfg.root_dir.clone()
    }

    fn path_exists_in_root(cfg: &VeerOsConfig, rel: &str) -> Option<String> {
        let p = PathBuf::from(Self::resolved_root_dir(cfg)).join(rel);
        if p.exists() {
            Some(p.to_string_lossy().to_string())
        } else {
            None
        }
    }

    fn command_exists(cmd: &str) -> bool {
        std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn fold_binary_path(cfg: &VeerOsConfig) -> Option<String> {
        for rel in ["target/debug/fold", "target/release/fold"] {
            if let Some(found) = Self::path_exists_in_root(cfg, rel) {
                return Some(found);
            }
        }
        // PATH fallback: verify it is the VeerOS fold runtime, not macOS's
        // built-in text-folding utility (which rejects --version).
        if Self::command_exists("fold")
            && std::process::Command::new("fold")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        {
            return Some("fold".to_string());
        }
        None
    }

    fn veer_vm_binary_path(cfg: &VeerOsConfig) -> Option<String> {
        for rel in ["target/debug/veer-vm", "target/release/veer-vm"] {
            if let Some(found) = Self::path_exists_in_root(cfg, rel) {
                return Some(found);
            }
        }
        None
    }

    fn kernel_path_qemu(cfg: &VeerOsConfig, target: &str) -> Option<String> {
        for profile in ["debug", "release"] {
            let rel = format!("target/riscv32imc-unknown-none-elf/{profile}/kernel-qemu-{target}");
            if let Some(found) = Self::path_exists_in_root(cfg, &rel) {
                return Some(found);
            }
        }
        None
    }

    fn kernel_path_fold(cfg: &VeerOsConfig, target: &str) -> Option<String> {
        for profile in ["debug", "release"] {
            let rel = format!("target/riscv32imc-unknown-none-elf/{profile}/kernel-fold-{target}");
            if let Some(found) = Self::path_exists_in_root(cfg, &rel) {
                return Some(found);
            }
        }
        None
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
            return Some(configured.to_string_lossy().to_string());
        }
        if !configured.is_absolute() {
            let rel_from_root = PathBuf::from(Self::resolved_root_dir(cfg)).join(configured);
            if rel_from_root.exists() {
                return Some(rel_from_root.to_string_lossy().to_string());
            }
        }

        // Backward/forward compatibility:
        // - legacy: scripts/veeros-vm
        // - folded microCM runtime: scripts/veer-vm
        let root = Self::resolved_root_dir(cfg);
        let candidates = [format!("{root}/scripts/veer-vm"), format!("{root}/scripts/veeros-vm")];

        candidates
            .into_iter()
            .find(|candidate| Path::new(candidate).exists())
    }

    fn resolve_veer_cli_path(cfg: &VeerOsConfig) -> Option<String> {
        let configured = Path::new(&cfg.veer_cli_path);
        if configured.exists() {
            return Some(configured.to_string_lossy().to_string());
        }
        if !configured.is_absolute() && Self::command_exists(&cfg.veer_cli_path) {
            return Some(cfg.veer_cli_path.clone());
        }
        for rel in ["target/release/veer", "target/debug/veer"] {
            if let Some(found) = Self::path_exists_in_root(cfg, rel) {
                return Some(found);
            }
        }
        if Self::command_exists("veer") {
            return Some("veer".to_string());
        }
        None
    }

    fn resolve_veer_connect_path(cfg: &VeerOsConfig) -> Option<String> {
        let configured = Path::new(&cfg.veer_connect_path);
        if configured.exists() {
            return Some(configured.to_string_lossy().to_string());
        }
        if !configured.is_absolute() && Self::command_exists(&cfg.veer_connect_path) {
            return Some(cfg.veer_connect_path.clone());
        }
        for rel in [
            "target/release/veer-connect",
            "target/debug/veer-connect",
            "scripts/veeros-connect",
        ] {
            if let Some(found) = Self::path_exists_in_root(cfg, rel) {
                return Some(found);
            }
        }
        if Self::command_exists("veeros-connect") {
            return Some("veeros-connect".to_string());
        }
        None
    }

    fn normalize_net_mode(net_mode: &str) -> String {
        let host_os = std::env::consts::OS;
        let requested = net_mode.trim().to_ascii_lowercase();
        match requested.as_str() {
            "" => {
                if host_os == "macos" { "user".to_string() } else { "nat".to_string() }
            }
            // Backward-compat: older EdgeFabric configs used "tap0" but
            // veeros-vm expects nat|bridge|user.
            "tap0" => {
                if host_os == "macos" { "user".to_string() } else { "nat".to_string() }
            }
            // bridge mode is Linux-oriented; on macOS fall back to user mode.
            "bridge" if host_os == "macos" => "user".to_string(),
            other => other.to_string(),
        }
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
        let net_mode_owned = Self::normalize_net_mode(net_mode);
        let net_mode = net_mode_owned.as_str();
        if Self::is_fold_runtime_available(cfg, target) {
            let veer_vm = Self::veer_vm_binary_path(cfg)
                .ok_or_else(|| EdgeFabricError::Internal("veer-vm binary not found".into()))?;
            let kernel = Self::kernel_path_for_fold_runtime(cfg, target).ok_or_else(|| {
                EdgeFabricError::Internal(format!("kernel image not found for target {target}"))
            })?;
            // Network args are platform-specific: veer-vm on macOS uses
            // entitlement-free user-mode networking (`--net user`), while
            // Linux attaches a pre-created TAP interface (`--tap tapN`).
            // (normalize_net_mode maps macOS modes to "user", which must NOT
            // be passed through as a TAP device name.)
            let net_args = if std::env::consts::OS == "macos" {
                "  \"--net\", \"user\",\n".to_string()
            } else {
                let tap = match net_mode {
                    "" | "nat" | "bridge" | "user" => "tap0",
                    other => other,
                };
                format!("  \"--tap\", \"{tap}\",\n")
            };
            let manifest_path = Self::fold_manifest_path(vm_name);
            let fifo_path = Self::sensor_fifo_path(vm_name);
            // workdir must exist: fold chdirs into it before spawning, so use
            // the resolved checkout (cfg.root_dir may be a Linux default).
            let workdir = Self::resolved_root_dir(cfg);
            let manifest = format!(
                "name = \"{vm_name}\"\ncmd  = \"{veer_vm}\"\nargs = [\n  \"--arch\", \"riscv32\",\n  \"--kernel\", \"{kernel}\",\n  \"--memory\", \"{memory_mb}\",\n{net_args}  \"--sensor-feed\", \"{fifo_path}\",\n]\nhostname = \"{hostname}\"\nworkdir  = \"{workdir}\"\n\n[env]\nPATH = \"/usr/local/bin:/usr/bin:/bin\"\nTERM = \"xterm-256color\"\n\n[namespaces]\npid   = true\nmount = true\nuts   = true\nipc   = true\nnet   = false\nuser  = true\n\n[seccomp]\nprofile = \"default\"\n",
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
                        "{} {} {}",
                        Self::resolve_veer_connect_path(cfg).unwrap_or_else(|| cfg.veer_connect_path.clone()),
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
        let target = config
            .get("target")
            .map(|t| t.trim().to_ascii_lowercase())
            .unwrap_or_else(|| "x86_64".to_string());
        let host = "localhost".to_string();
        // veeros-vm uses SSH for x86_64 targets and VSC for esp32c6.
        // Keep shell command generation aligned with veeros-vm connect logic.
        let connect_command = if target == "x86_64" {
            format!(
                "ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=10 -o LogLevel=ERROR -p {} veeros@{}",
                port, host
            )
        } else {
            format!(
                "{} {} {}",
                Self::resolve_veer_connect_path(cfg).unwrap_or_else(|| cfg.veer_connect_path.clone()),
                host,
                port
            )
        };
        Ok(VeerOsShellInfo {
            vm_name: vm_name.to_string(),
            host: host.clone(),
            port,
            connect_command,
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
                    "cli_available": Self::resolve_veer_cli_path(cfg).is_some(),
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
        let root = Self::resolved_root_dir(cfg);
        let resolved_program = if Path::new(program).exists() {
            program.to_string()
        } else if program.contains('/') {
            let p = PathBuf::from(&root).join(program);
            if p.exists() {
                p.to_string_lossy().to_string()
            } else {
                return Err(EdgeFabricError::Internal(format!(
                    "VeerOS runtime command not found: {program}",
                )));
            }
        } else {
            program.to_string()
        };

        let output = match Command::new(&resolved_program)
            .current_dir(&root)
            .args(args)
            .output()
            .await
        {
            Ok(out) => out,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && Path::new(&resolved_program).exists() => {
                // Some script checkouts can lose +x; run them through bash as a fallback.
                Command::new("bash")
                    .current_dir(&root)
                    .arg(&resolved_program)
                    .args(args)
                    .output()
                    .await?
            }
            Err(e) => return Err(e.into()),
        };

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_veer_command(cfg: &VeerOsConfig, args: &[String]) -> Result<CommandOutput> {
        let veer_cli = Self::resolve_veer_cli_path(cfg).ok_or_else(|| {
            EdgeFabricError::Internal(format!(
                "VeerOS CLI not found: {} (also checked target/release/veer, target/debug/veer and PATH)",
                cfg.veer_cli_path
            ))
        })?;

        let output = Command::new(&veer_cli)
            .current_dir(Self::resolved_root_dir(cfg))
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

    /// Simulator-native gate set. Everything the parser emits must be in this
    /// list — the VeerOS state-vector backend rejects anything else.
    /// Multi-qubit / parametric QASM gates are decomposed into these.
    fn parse_quantum_circuit_from_qasm(qasm: &str) -> Result<Circuit> {
        use quantum::circuit::{CircuitOp, FixedQ16, Gate};

        let mut qubits = 2usize;
        for line in qasm.lines() {
            let trimmed = line.trim();
            if let Some(count) = Self::parse_reg_size(trimmed, "qreg")
                .or_else(|| Self::parse_reg_size(trimmed, "qubit"))
            {
                qubits = count;
                break;
            }
        }
        if qubits == 0 || qubits > 12 {
            return Err(EdgeFabricError::Internal(format!(
                "simulator supports 1–12 qubits, found declaration for {qubits}"
            )));
        }

        let mut circuit = Circuit::new(qubits as u16);
        // Single mutable emitter (separate closures would clash-borrow `circuit`).
        let mut push = |op: CircuitOp| -> Result<()> {
            circuit.push_op(op).map_err(|_| {
                EdgeFabricError::Internal("circuit exceeds simulator operation limit".into())
            })
        };
        // Bounds-checked 1-qubit emission + parametric Rz.
        macro_rules! g1 {
            ($gate:expr, $q:expr) => {{
                Self::check_qidx(qubits, $q)?;
                push(CircuitOp::gate1($gate, $q as u16))?;
            }};
        }
        macro_rules! rz1 {
            ($q:expr, $theta:expr) => {{
                Self::check_qidx(qubits, $q)?;
                push(CircuitOp::gate1(
                    Gate::Rz(FixedQ16(($theta * 65536.0) as i32)),
                    $q as u16,
                ))?;
            }};
        }

        for line in qasm.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with("OPENQASM")
                || trimmed.starts_with("include")
                || trimmed.starts_with("qreg")
                || trimmed.starts_with("creg")
                || trimmed.starts_with("qubit[")
                || trimmed.starts_with("bit[")
            {
                continue;
            }

            // Split "name(params) q[..],...;" into (name, params, rest).
            let (mut name, params, rest) = Self::split_gate_line(trimmed);
            name.make_ascii_lowercase();
            let qidx: Vec<usize> = Self::extract_all_qubit_indices(&rest);
            let need = |n: usize| -> Result<()> {
                if qidx.len() < n {
                    return Err(EdgeFabricError::Internal(format!(
                        "gate '{name}' needs {n} qubit operand(s): {trimmed}"
                    )));
                }
                for &q in qidx.iter().take(n) {
                    Self::check_qidx(qubits, q)?;
                }
                Ok(())
            };

            match name.as_str() {
                "h" => {
                    need(1)?;
                    g1!(Gate::H, qidx[0]);
                }
                "x" => {
                    need(1)?;
                    g1!(Gate::X, qidx[0]);
                }
                "z" => {
                    need(1)?;
                    g1!(Gate::Z, qidx[0]);
                }
                // Y ≡ X·Z up to (unobservable) global phase.
                "y" => {
                    need(1)?;
                    g1!(Gate::X, qidx[0]);
                    g1!(Gate::Z, qidx[0]);
                }
                "s" => {
                    need(1)?;
                    g1!(Gate::S, qidx[0]);
                }
                "sdg" | "si" => {
                    need(1)?;
                    g1!(Gate::Sdg, qidx[0]);
                }
                "t" => {
                    need(1)?;
                    g1!(Gate::T, qidx[0]);
                }
                "tdg" | "ti" => {
                    need(1)?;
                    g1!(Gate::Tdg, qidx[0]);
                }
                "rx" => {
                    need(1)?;
                    let theta = Self::parse_angle_param(&params, 0, trimmed)?;
                    // Rx(θ) = H·Rz(θ)·H
                    g1!(Gate::H, qidx[0]);
                    rz1!(qidx[0], theta);
                    g1!(Gate::H, qidx[0]);
                }
                "ry" => {
                    need(1)?;
                    let theta = Self::parse_angle_param(&params, 0, trimmed)?;
                    // Ry(θ) = S·H·Rz(θ)·H·Sdg
                    g1!(Gate::S, qidx[0]);
                    g1!(Gate::H, qidx[0]);
                    rz1!(qidx[0], theta);
                    g1!(Gate::H, qidx[0]);
                    g1!(Gate::Sdg, qidx[0]);
                }
                "rz" => {
                    need(1)?;
                    let theta = Self::parse_angle_param(&params, 0, trimmed)?;
                    rz1!(qidx[0], theta);
                }
                "u" | "u3" => {
                    need(1)?;
                    let (theta, phi, lam) = Self::parse_u_params(&params, trimmed)?;
                    // U(θ,φ,λ) = Rz(φ)·Ry(θ)·Rz(λ) up to global phase.
                    rz1!(qidx[0], phi);
                    g1!(Gate::S, qidx[0]);
                    g1!(Gate::H, qidx[0]);
                    rz1!(qidx[0], theta);
                    g1!(Gate::H, qidx[0]);
                    g1!(Gate::Sdg, qidx[0]);
                    rz1!(qidx[0], lam);
                }
                "cx" | "cnot" => {
                    need(2)?;
                    push(CircuitOp::gate2(Gate::Cx, qidx[0] as u16, qidx[1] as u16))?;
                }
                // CY = (I⊗S)·CX·(I⊗Sdg)
                "cy" => {
                    need(2)?;
                    g1!(Gate::Sdg, qidx[1]);
                    push(CircuitOp::gate2(Gate::Cx, qidx[0] as u16, qidx[1] as u16))?;
                    g1!(Gate::S, qidx[1]);
                }
                // CZ = (I⊗H)·CX·(I⊗H)
                "cz" => {
                    need(2)?;
                    g1!(Gate::H, qidx[1]);
                    push(CircuitOp::gate2(Gate::Cx, qidx[0] as u16, qidx[1] as u16))?;
                    g1!(Gate::H, qidx[1]);
                }
                // SWAP = 3 × CX
                "swap" => {
                    need(2)?;
                    let (a, b) = (qidx[0] as u16, qidx[1] as u16);
                    push(CircuitOp::gate2(Gate::Cx, a, b))?;
                    push(CircuitOp::gate2(Gate::Cx, b, a))?;
                    push(CircuitOp::gate2(Gate::Cx, a, b))?;
                }
                // Toffoli (Nielsen & Chuang Fig. 4.9): H/T/CX only.
                "ccx" | "toffoli" => {
                    need(3)?;
                    let (c0, c1, t) = (qidx[0] as u16, qidx[1] as u16, qidx[2] as u16);
                    macro_rules! cx2 {
                        ($a:expr, $b:expr) => {{
                            push(CircuitOp::gate2(Gate::Cx, $a, $b))?;
                        }};
                    }
                    g1!(Gate::H, t as usize);
                    cx2!(c1, t);
                    g1!(Gate::Tdg, t as usize);
                    cx2!(c0, t);
                    g1!(Gate::T, t as usize);
                    cx2!(c1, t);
                    g1!(Gate::Tdg, t as usize);
                    cx2!(c0, t);
                    g1!(Gate::T, c1 as usize);
                    g1!(Gate::T, t as usize);
                    g1!(Gate::H, t as usize);
                    cx2!(c0, c1);
                    g1!(Gate::T, c0 as usize);
                    g1!(Gate::Tdg, c1 as usize);
                    cx2!(c0, c1);
                }
                // Controlled-phase CP(λ): exact 5-gate Rz/CX form
                // (verified: equals diag(1,1,1,e^{iλ}) up to global phase).
                "cp" | "cu1" | "cphase" => {
                    need(2)?;
                    let lambda = Self::parse_angle_param(&params, 0, trimmed)?;
                    let (c, t) = (qidx[0], qidx[1]);
                    rz1!(c, lambda / 2.0);
                    push(CircuitOp::gate2(Gate::Cx, c as u16, t as u16))?;
                    rz1!(t, -lambda / 2.0);
                    push(CircuitOp::gate2(Gate::Cx, c as u16, t as u16))?;
                    rz1!(t, lambda / 2.0);
                }
                // Controlled-Rz: exact 4-gate form (no global-phase residue).
                "crz" => {
                    need(2)?;
                    let lambda = Self::parse_angle_param(&params, 0, trimmed)?;
                    let (c, t) = (qidx[0], qidx[1]);
                    rz1!(t, lambda / 2.0);
                    push(CircuitOp::gate2(Gate::Cx, c as u16, t as u16))?;
                    rz1!(t, -lambda / 2.0);
                    push(CircuitOp::gate2(Gate::Cx, c as u16, t as u16))?;
                }
                "id" => {
                    need(1)?;
                }
                "barrier" => {
                    push(CircuitOp::barrier())?;
                }
                "measure" => {
                    // `measure q -> c;` (or bare `measure;`) measures all qubits.
                    let operands = rest.trim();
                    if operands == "q -> c"
                        || operands == "q->c"
                        || operands.is_empty()
                        || qidx.is_empty()
                    {
                        for q in 0..qubits {
                            push(CircuitOp::measure(q as u16, q as u16))?;
                        }
                    } else {
                        let cbit = Self::extract_cbit_index(&rest).unwrap_or(qidx[0]);
                        Self::check_qidx(qubits, qidx[0])?;
                        push(CircuitOp::measure(qidx[0] as u16, cbit as u16))?;
                    }
                }
                _ => {
                    // Unknown / unsupported statement (reset, delay, …) — skip
                    // to stay backward compatible with the previous parser.
                }
            }
        }

        Ok(circuit)
    }

    /// Parse register sizes: `qreg name[N];` (QASM 2) and `qubit[N] name;`
    /// (QASM 3 prefix form).
    fn parse_reg_size(line: &str, kind: &str) -> Option<usize> {
        let t = line.trim().trim_end_matches(';').trim();
        let rest = t.strip_prefix(kind)?.trim();
        // QASM 3 prefix form: `qubit[3] q`
        if let Some(after_bracket) = rest.strip_prefix('[') {
            let end = after_bracket.find(']')?;
            return after_bracket[..end].trim().parse::<usize>().ok();
        }
        // QASM 2 form: `qreg q[3]`
        let name_end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))?;
        let bracket = rest[name_end..].trim();
        let inner = bracket.strip_prefix('[')?.strip_suffix(']')?;
        inner.trim().parse::<usize>().ok()
    }

    fn check_qidx(qubits: usize, idx: usize) -> Result<()> {
        if idx >= qubits {
            return Err(EdgeFabricError::Internal(format!(
                "qubit q[{idx}] out of range for a {qubits}-qubit circuit"
            )));
        }
        Ok(())
    }

    /// Split `name(params) operands;` into (name, params, operands).
    fn split_gate_line(line: &str) -> (String, String, String) {
        let t = line.trim().trim_end_matches(';').trim();
        if let Some(paren) = t.find('(') {
            let name = t[..paren].trim().to_string();
            let after = &t[paren + 1..];
            if let Some(close) = after.find(')') {
                let params = after[..close].to_string();
                let rest = after[close + 1..].trim().to_string();
                return (name, params, rest);
            }
            return (name, String::new(), after.to_string());
        }
        let mut parts = t.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").to_string();
        let rest = parts.next().unwrap_or("").trim().to_string();
        (name, String::new(), rest)
    }

    /// Evaluate an angle expression: floats, `pi`, and `*`/`/` products
    /// such as `pi/2`, `-pi`, `3*pi/2`.
    fn parse_angle_expr(expr: &str) -> Option<f64> {
        let e = expr.trim().replace(' ', "");
        if e.is_empty() {
            return None;
        }
        let lower = e.to_lowercase();
        let (neg, rest) = match lower.strip_prefix('-') {
            Some(r) => (true, r.to_string()),
            None => (false, lower.strip_prefix('+').unwrap_or(&lower).to_string()),
        };
        let mut value = 0.0;
        let mut first = true;
        let mut pending_mul = true; // operator before next factor
        let mut current = String::new();
        let mut chars = rest.chars().peekable();
        let flush = |current: &mut String,
                     value: &mut f64,
                     first: &mut bool,
                     pending_mul: bool|
         -> Option<()> {
            if current.is_empty() {
                return None;
            }
            let factor = if current == "pi" {
                std::f64::consts::PI
            } else {
                current.parse::<f64>().ok()?
            };
            if *first {
                *value = factor;
                *first = false;
            } else if pending_mul {
                *value *= factor;
            } else {
                if factor == 0.0 {
                    return None;
                }
                *value /= factor;
            }
            current.clear();
            Some(())
        };
        while let Some(ch) = chars.next() {
            if ch == '*' || ch == '/' {
                flush(&mut current, &mut value, &mut first, pending_mul)?;
                pending_mul = ch == '*';
            } else {
                current.push(ch);
            }
        }
        flush(&mut current, &mut value, &mut first, pending_mul)?;
        if first {
            return None;
        }
        Some(if neg { -value } else { value })
    }

    fn parse_angle_param(params: &str, index: usize, line: &str) -> Result<f64> {
        let part = params
            .split(',')
            .nth(index)
            .ok_or_else(|| {
                EdgeFabricError::Internal(format!("gate is missing angle parameter: {line}"))
            })?;
        Self::parse_angle_expr(part).ok_or_else(|| {
            EdgeFabricError::Internal(format!("cannot parse angle '{part}' in: {line}"))
        })
    }

    fn parse_u_params(params: &str, line: &str) -> Result<(f64, f64, f64)> {
        let mut it = params.split(',');
        let mut p = |i: usize| -> Result<f64> {
            let part = it.next().ok_or_else(|| {
                EdgeFabricError::Internal(format!("U gate needs 3 angles: {line}"))
            })?;
            Self::parse_angle_expr(part).ok_or_else(|| {
                EdgeFabricError::Internal(format!("cannot parse U angle #{i} in: {line}"))
            })
        };
        Ok((p(0)?, p(1)?, p(2)?))
    }

    /// All `q[N]` indices in a gate operand string, in order.
    fn extract_all_qubit_indices(line: &str) -> Vec<usize> {
        let mut out = Vec::new();
        let bytes = line.as_bytes();
        let mut i = 0;
        while i + 1 < bytes.len() {
            if bytes[i] == b'q' && bytes[i + 1] == b'[' {
                if let Some(end) = line[i..].find(']') {
                    if let Ok(idx) = line[i + 2..i + end].parse::<usize>() {
                        out.push(idx);
                    }
                    i += end + 1;
                    continue;
                }
            }
            i += 1;
        }
        out
    }

    /// Classical bit index from `-> c[N]`, if present.
    fn extract_cbit_index(line: &str) -> Option<usize> {
        let arrow = line.find("->")?;
        let after = line[arrow + 2..].trim();
        let after = after.strip_prefix('c')?;
        let after = after.trim_start_matches('[');
        let end = after.find(']')?;
        after[..end].trim().parse::<usize>().ok()
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

#[cfg(test)]
mod qasm_parser_tests {
    use super::VeerOsDriver;

    fn parse(qasm: &str) -> Result<quantum::Circuit, String> {
        VeerOsDriver::parse_quantum_circuit_from_qasm(qasm).map_err(|e| format!("{e:?}"))
    }

    fn op_count(qasm: &str) -> usize {
        parse(qasm).expect("parse ok").ops_len() as usize
    }

    #[test]
    fn qreg_size_and_basic_gates() {
        let c = parse(
            "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nh q[0];\ncx q[0],q[1];\nmeasure q -> c;",
        )
        .unwrap();
        assert_eq!(c.logical_qubits, 3);
        // h + cx + 3 measures
        assert_eq!(c.ops_len(), 5);
    }

    #[test]
    fn parametric_and_two_qubit_gates() {
        // s, t, rx, rz, cz, swap decompose into native ops
        let n = op_count(
            "qreg q[2];\ns q[0];\nt q[1];\nrx(pi/2) q[0];\nrz(0.5) q[1];\ncz q[0],q[1];\nswap q[0],q[1];\nmeasure q -> c;",
        );
        // s(1) + t(1) + rx(3) + rz(1) + cz(3) + swap(3) + measure(2)
        assert_eq!(n, 1 + 1 + 3 + 1 + 3 + 3 + 2);
    }

    #[test]
    fn toffoli_and_controlled_phase_decompose() {
        // ccx -> 15 ops, cp -> 5 ops (all native H/T/CX/Rz)
        let n = op_count("qreg q[3];\nccx q[0],q[1],q[2];\ncp(pi/4) q[0],q[1];\nmeasure q -> c;");
        assert_eq!(n, 15 + 5 + 3);
    }

    #[test]
    fn angle_expressions() {
        assert!((VeerOsDriver::parse_angle_expr("pi/2").unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert!((VeerOsDriver::parse_angle_expr("-pi").unwrap() + std::f64::consts::PI).abs() < 1e-9);
        assert!((VeerOsDriver::parse_angle_expr("3*pi/2").unwrap() - 4.71238898038).abs() < 1e-9);
        assert!((VeerOsDriver::parse_angle_expr("0.5").unwrap() - 0.5).abs() < 1e-9);
        assert!(VeerOsDriver::parse_angle_expr("banana").is_none());
    }

    #[test]
    fn rejects_out_of_range_qubits() {
        let err = parse("qreg q[2];\nh q[5];").unwrap_err();
        assert!(err.contains("out of range"), "unexpected: {err}");
    }

    #[test]
    fn rejects_too_many_qubits() {
        let err = parse("qreg q[13];\nh q[0];").unwrap_err();
        assert!(err.contains("12"), "unexpected: {err}");
    }

    #[test]
    fn y_gate_and_explicit_measure() {
        let n = op_count("qreg q[2];\ny q[0];\nmeasure q[0] -> c[1];\nmeasure q[1] -> c[0];");
        // y -> x+z (2 ops) + 2 measures
        assert_eq!(n, 4);
    }

    fn simulate(qasm: &str, shots: u32, seed: u64) -> serde_json::Value {
        super::VeerOsDriver::simulate_quantum(&super::QuantumSimulationRequest {
            program: None,
            qasm: Some(qasm.to_string()),
            shots,
            seed: Some(seed),
        })
        .expect("simulation ok")
    }

    fn hist_value(v: &serde_json::Value, state: &str) -> u32 {
        v.get("histogram")
            .and_then(|h| h.get(state))
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as u32
    }

    #[test]
    fn bell_state_simulating_correctly() {
        let v = simulate(
            "qreg q[2];\nh q[0];\ncx q[0],q[1];\nmeasure q -> c;",
            1000,
            7,
        );
        let total = hist_value(&v, "00") + hist_value(&v, "11");
        assert_eq!(total, 1000);
        assert_eq!(hist_value(&v, "01"), 0);
        assert_eq!(hist_value(&v, "10"), 0);
    }

    #[test]
    fn toffoli_truth_table() {
        // |110> -> |111>, |101> stays |101>
        let v = simulate(
            "qreg q[3];\nx q[0];\nx q[1];\nccx q[0],q[1],q[2];\nmeasure q -> c;",
            200,
            7,
        );
        assert_eq!(hist_value(&v, "111"), 200);
        let v2 = simulate(
            "qreg q[3];\nx q[0];\nx q[2];\nccx q[0],q[1],q[2];\nmeasure q -> c;",
            200,
            7,
        );
        assert_eq!(hist_value(&v2, "101"), 200);
    }

    #[test]
    fn qft2_uniform_output() {
        // QFT|00> is uniform over 4 states; QFT|10> peaks at |10>.
        let prep = "qreg q[2];\nh q[0];\ncp(pi/2) q[0],q[1];\nh q[1];\nmeasure q -> c;";
        let v = simulate(prep, 1200, 11);
        for state in ["00", "01", "10", "11"] {
            let c = hist_value(&v, state);
            assert!(
                (200..=400).contains(&c),
                "QFT|00> should be ~uniform, {state}={c}"
            );
        }
        // QFT followed by inverse QFT is the identity (also exercises
        // negative angles via cp(-pi/2)).
        let roundtrip = "qreg q[2];\nx q[1];\nh q[0];\ncp(pi/2) q[0],q[1];\nh q[1];\nh q[1];\ncp(-pi/2) q[0],q[1];\nh q[0];\nmeasure q -> c;";
        let v1 = simulate(roundtrip, 400, 11);
        assert_eq!(hist_value(&v1, "10"), 400);
    }

    const GROVER2_QASM: &str = "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\nh q[1];\nh q[1];\ncx q[0],q[1];\nh q[1];\nh q[0];\nh q[1];\nx q[0];\nx q[1];\nh q[1];\ncx q[0],q[1];\nh q[1];\nx q[0];\nx q[1];\nh q[0];\nh q[1];\nmeasure q -> c;";

    #[test]
    fn grover2_finds_marked_state() {
        let v = simulate(GROVER2_QASM, 300, 5);
        assert_eq!(hist_value(&v, "11"), 300);
    }

    const BV3_QASM: &str = "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nx q[2];\nh q[0];\nh q[1];\nh q[2];\ncx q[0],q[2];\ncx q[1],q[2];\nh q[0];\nh q[1];\nmeasure q[0] -> c[0];\nmeasure q[1] -> c[1];\nmeasure q[2] -> c[2];";

    #[test]
    fn bernstein_vazirani_recovers_secret() {
        let v = simulate(BV3_QASM, 200, 5);
        // bitstring order is q2q1q0: secret s=11 means q0=q1=1.
        let total12: u32 = ["011", "111"].iter().map(|s| hist_value(&v, s)).sum();
        assert_eq!(total12, 200, "q0q1 must read 11, got {v:?}");
    }

    const SUPERDENSE_QASM: &str = "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\ncx q[0],q[1];\nx q[0];\ncx q[0],q[1];\nh q[0];\nmeasure q -> c;";

    #[test]
    fn superdense_decodes_two_bits() {
        let v = simulate(SUPERDENSE_QASM, 200, 5);
        assert_eq!(hist_value(&v, "10"), 200);
    }

    const DJ_QASM: &str = "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nx q[2];\nh q[0];\nh q[1];\nh q[2];\ncx q[0],q[2];\nh q[0];\nh q[1];\nmeasure q[0] -> c[0];\nmeasure q[1] -> c[1];\nmeasure q[2] -> c[2];";

    #[test]
    fn deutsch_jozsa_detects_balanced() {
        let v = simulate(DJ_QASM, 200, 5);
        // bitstring order q2q1q0; balanced f(x)=x0 gives q0=1, q1=0.
        let total: u32 = ["001", "101"].iter().map(|s| hist_value(&v, s)).sum();
        assert_eq!(total, 200, "balanced f must give q0q1=10, got {v:?}");
    }

    const QEC_QASM: &str = "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[5];\ncreg c[5];\nx q[0];\ncx q[0],q[1];\ncx q[0],q[2];\nx q[1];\ncx q[0],q[3];\ncx q[1],q[3];\ncx q[1],q[4];\ncx q[2],q[4];\nccx q[3],q[4],q[1];\ncx q[0],q[1];\ncx q[0],q[2];\nmeasure q[0] -> c[0];";

    #[test]
    fn bitflip_code_corrects_error() {
        let v = simulate(QEC_QASM, 200, 5);
        assert_eq!(hist_value(&v, "00001"), 200);
    }

    // Teleportation in deferred-measurement form: the classically controlled
    // X/Z corrections become CX(q1,q2) / CZ(q0,q2), so q2 ends exactly in the
    // teleported state with all measurements at the end.
    const TELEPORT_QASM: &str = "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nx q[0];\nh q[1];\ncx q[1],q[2];\ncx q[0],q[1];\nh q[0];\ncx q[1],q[2];\ncz q[0],q[2];\nmeasure q -> c;";

    #[test]
    fn teleportation_delivers_state() {
        // Bitstring order is q2q1q0: the target q2 must read 1 in every shot
        // while Alice's Bell bits (q1q0) vary uniformly.
        let v = simulate(TELEPORT_QASM, 1000, 5);
        for s in ["000", "001", "010", "011"] {
            assert_eq!(hist_value(&v, s), 0, "q2 must be 1, got {v:?}");
        }
        let total: u32 = ["100", "101", "110", "111"]
            .iter()
            .map(|s| hist_value(&v, s))
            .sum();
        assert_eq!(total, 1000);
    }

    #[test]
    fn teleportation_transfers_one() {
        // Teleport |1>: Bell pair q1-q2, Bell-measure q0-q1, conditional X/Z on q2.
        // Conditional gates are classically controlled — instead verify the
        // no-correction distribution: q2 ends 0/1 uniformly when q0 was |1>.
        let v = simulate(
            "qreg q[3];\nx q[0];\nh q[1];\ncx q[1],q[2];\ncx q[0],q[1];\nh q[0];\nmeasure q -> c;",
            1000,
            3,
        );
        let total: u32 = ["000", "001", "010", "011", "100", "101", "110", "111"]
            .iter()
            .map(|s| hist_value(&v, s))
            .sum();
        assert_eq!(total, 1000);
    }
}