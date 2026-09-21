//! `ef host join` — enroll this machine as a generic EdgeFabric worker.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::process::Command;

use crate::client::ApiClient;
use crate::commands::auth;
use crate::config::config_dir;
use crate::output::{OutputFormat, print_value};

pub struct JoinOpts {
    pub api_key: String,
    pub name: Option<String>,
    pub ssh_user: Option<String>,
    pub ssh_port: u16,
    pub region: String,
    pub with_ml: bool,
}

pub async fn join(base_url: &str, opts: JoinOpts, format: OutputFormat) -> Result<()> {
    auth::login(base_url, &opts.api_key, OutputFormat::Json).await?;
    let api = ApiClient::new(base_url)?;

    let host_os = detect_host_os();
    let arch = std::env::consts::ARCH.to_string();
    let node_id = sanitize_id(&opts.name.clone().unwrap_or_else(hostname));
    let ssh_user = opts
        .ssh_user
        .unwrap_or_else(|| whoami().unwrap_or_else(|| "user".into()));
    let ssh_port = opts.ssh_port;
    let serial = format!("{ssh_user}@127.0.0.1:{ssh_port}");
    let veersetu_service = format!("ssh-{node_id}");

    let mut caps = vec![
        "ssh".to_string(),
        "cicd_runner".to_string(),
        "artifact_upload".to_string(),
        "workspace_bootstrap".to_string(),
    ];
    match host_os.as_str() {
        "macos" => caps.extend([
            "macos_runner".into(),
            "macos_deploy".into(),
        ]),
        "windows" => caps.extend(["windows_runner".into(), "windows_deploy".into()]),
        _ => caps.extend(["linux_runner".into(), "linux_deploy".into()]),
    }
    if opts.with_ml {
        caps.extend(["llm_infer".into(), "pipeline_pp".into()]);
        if host_os == "macos" {
            caps.extend(["mlx".into(), "ane".into()]);
        }
    }

    let device_body = json!({
        "name": node_id,
        "serial": serial,
        "os": "ssh",
        "os_version": host_os,
        "kind": "real",
        "region": opts.region,
        "capabilities": caps,
    });
    let device_resp = api.post("/api/v1/devices", &device_body).await?;
    let device = device_resp.get("device").cloned().unwrap_or(device_resp.clone());
    let device_id = device
        .get("id")
        .and_then(|v| v.as_str())
        .context("device id missing from register response")?;

    let runner_body = json!({
        "name": node_id,
        "device_id": device_id,
        "runner_class": "generic",
        "host": "127.0.0.1",
        "port": ssh_port,
        "host_os": host_os,
        "architecture": arch,
        "region": opts.region,
        "labels": [format!("veersetu:{veersetu_service}"), format!("node:{node_id}")],
        "capabilities": caps,
        "max_concurrency": 2,
    });
    let runner_resp = api.post("/api/v1/runners/register", &runner_body).await?;
    let runner = runner_resp.get("runner").cloned().unwrap_or(runner_resp.clone());
    let runner_id = runner
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let _ = std::fs::write(
        crate::config::config_path()?,
        format!("server_url = \"{base_url}\"\noutput_format = \"json\"\n"),
    );

    let host_file = config_dir()?.join("host.toml");
    let host_toml = format!(
        "node_id = \"{node_id}\"\n\
         host_os = \"{host_os}\"\n\
         device_id = \"{device_id}\"\n\
         runner_id = \"{runner_id}\"\n\
         ssh_user = \"{ssh_user}\"\n\
         ssh_port = {ssh_port}\n\
         veersetu_service = \"{veersetu_service}\"\n\
         server_url = \"{base_url}\"\n"
    );
    std::fs::write(&host_file, host_toml).context("write ~/.edgefabric/host.toml")?;

    let summary = json!({
        "node_id": node_id,
        "host_os": host_os,
        "device": device,
        "runner": runner,
        "veersetu_service": veersetu_service,
        "next": [
            "Enable OpenSSH (sshd) on this machine",
            format!("Publish SSH through VeerSetu: veersetu-agent --service {veersetu_service} --upstream 127.0.0.1:{ssh_port} --node-id {node_id}"),
            "Optional ML: start ef-ml-worker with EF_RUNNER_ID and EF_TOKEN",
        ],
    });
    print_value(&summary, format);
    eprintln!(
        "Joined EdgeFabric as {node_id}. Config: {}",
        host_file.display()
    );
    Ok(())
}

pub async fn status(base_url: &str, format: OutputFormat) -> Result<()> {
    let host_file = config_dir()?.join("host.toml");
    if !host_file.exists() {
        bail!("This machine is not joined. Run `ef host join --api-key …`");
    }
    let text = std::fs::read_to_string(&host_file)?;
    let mut map = serde_json::Map::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(
                k.trim().to_string(),
                Value::String(v.trim().trim_matches('"').to_string()),
            );
        }
    }
    if let Some(runner_id) = map.get("runner_id").and_then(|v| v.as_str()) {
        if !runner_id.is_empty() {
            let api = ApiClient::new(base_url)?;
            if let Ok(remote) = api.get(&format!("/api/v1/runners/{runner_id}")).await {
                map.insert("remote".into(), remote);
            }
        }
    }
    print_value(&Value::Object(map), format);
    Ok(())
}

fn detect_host_os() -> String {
    match std::env::consts::OS {
        "macos" => "macos".into(),
        "windows" => "windows".into(),
        _ => "linux".into(),
    }
}

fn hostname() -> String {
    Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "edgefabric-host".into())
}

fn whoami() -> Option<String> {
    Command::new("whoami")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn sanitize_id(raw: &str) -> String {
    let s = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "edgefabric-host".into()
    } else {
        s
    }
}
