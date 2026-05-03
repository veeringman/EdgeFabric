use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;

use ef_core::config::VeerOsConfig;
use ef_core::error::{EdgeFabricError, Result};

use super::CommandOutput;

#[derive(Debug, Clone)]
pub struct VeerOsShellInfo {
    pub vm_name: String,
    pub host: String,
    pub port: u16,
    pub connect_command: String,
}

pub struct VeerOsDriver;

impl VeerOsDriver {
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
        Self::run_vm_command(cfg, &["start", vm_name]).await
    }

    pub async fn stop_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["stop", vm_name]).await
    }

    pub async fn restart_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["restart", vm_name]).await
    }

    pub async fn destroy_vm(cfg: &VeerOsConfig, vm_name: &str) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["destroy", vm_name]).await
    }

    pub async fn vm_config(cfg: &VeerOsConfig, vm_name: &str) -> Result<HashMap<String, String>> {
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

    pub async fn sensor_inject(
        cfg: &VeerOsConfig,
        vm_name: &str,
        sensor: &str,
        value: &str,
    ) -> Result<CommandOutput> {
        Self::run_vm_command(cfg, &["sensor", "inject", vm_name, sensor, value]).await
    }

    pub async fn shell_info(cfg: &VeerOsConfig, vm_name: &str) -> Result<VeerOsShellInfo> {
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
        })
    }

    async fn run_vm_command(cfg: &VeerOsConfig, args: &[&str]) -> Result<CommandOutput> {
        if !Path::new(&cfg.vm_script_path).exists() {
            return Err(EdgeFabricError::Internal(format!(
                "VeerOS VM script not found: {}",
                cfg.vm_script_path
            )));
        }
        let output = Command::new(&cfg.vm_script_path)
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
}