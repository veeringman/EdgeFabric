//! CLI configuration: server URL, credentials file, output format.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Resolved CLI configuration (file + env overrides).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Base URL of the EdgeFabric API server (e.g. `http://localhost:3000`).
    pub server_url: String,
    /// Default output format: table, json, yaml.
    #[serde(default = "default_format")]
    pub output_format: String,
}

fn default_format() -> String {
    "table".into()
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:3000".into(),
            output_format: default_format(),
        }
    }
}

/// Return `~/.edgefabric/` directory, creating it if needed.
pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .context("Cannot determine home directory")?
        .join(".edgefabric");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).context("Failed to create ~/.edgefabric")?;
    }
    Ok(dir)
}

/// Path to `~/.edgefabric/config.toml`.
pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

/// Path to `~/.edgefabric/credentials`.
pub fn credentials_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("credentials"))
}

/// Load CLI config from `~/.edgefabric/config.toml`, falling back to defaults.
/// Environment variable `EF_SERVER_URL` overrides the file value.
pub fn load_config() -> Result<CliConfig> {
    let path = config_path()?;
    let mut cfg = if path.exists() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        toml_parse(&text)?
    } else {
        CliConfig::default()
    };

    // Env override
    if let Ok(url) = std::env::var("EF_SERVER_URL") {
        cfg.server_url = url;
    }
    if let Ok(fmt) = std::env::var("EF_OUTPUT_FORMAT") {
        cfg.output_format = fmt;
    }

    Ok(cfg)
}

fn toml_parse(text: &str) -> Result<CliConfig> {
    // Use serde_json via a two-step: TOML → Value → struct.
    // The `config` crate is heavy; for a simple CLI, hand-parse TOML lines.
    // We keep it minimal: key = "value" pairs only.
    let mut cfg = CliConfig::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');
            match key {
                "server_url" => cfg.server_url = val.to_string(),
                "output_format" => cfg.output_format = val.to_string(),
                _ => {} // ignore unknown keys
            }
        }
    }
    Ok(cfg)
}

/// Stored credentials (JWT token).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    pub token: String,
    pub tenant_id: String,
}

/// Load stored credentials from `~/.edgefabric/credentials`.
pub fn load_credentials() -> Result<Option<Credentials>> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    let mut creds = Credentials::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "token" => creds.token = v.trim().trim_matches('"').to_string(),
                "tenant_id" => creds.tenant_id = v.trim().trim_matches('"').to_string(),
                _ => {}
            }
        }
    }
    if creds.token.is_empty() {
        Ok(None)
    } else {
        Ok(Some(creds))
    }
}

/// Save credentials to `~/.edgefabric/credentials`.
pub fn save_credentials(creds: &Credentials) -> Result<()> {
    let path = credentials_path()?;
    let content = format!("token = \"{}\"\ntenant_id = \"{}\"\n", creds.token, creds.tenant_id);
    std::fs::write(&path, content).context("Failed to write credentials")?;
    // Restrict file permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
