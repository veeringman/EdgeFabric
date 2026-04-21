//! Proxy helpers that forward VeerOS virtual-IoT operations to a remote
//! ef-agent when a backend has been bound to one (`agent_id` set).
//!
//! The agent exposes the same VeerOS surface over HTTP at
//! `<streaming_url>/veeros/*` (see `crates/ef-agent/src/veeros_api.rs`).
//!
//! Callers decide local vs proxy by inspecting `backend.agent_id`:
//!   - `Some(_)` → call these helpers
//!   - `None`    → fall back to `ef_control::command::veeros::VeerOsDriver`
//!                 (the legacy local path, kept for backwards compatibility)

use anyhow::{Context, Result, anyhow};
use ef_control::command::CommandOutput;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use ef_db::repo::agent::AgentRepo;

/// Resolve the agent's base URL. Prefers `agent_base_url` recorded on the
/// backend row (populated when the backend was bound), falling back to the
/// agent's currently-registered `streaming_url`.
pub async fn resolve_agent_base_url(
    pool: &PgPool,
    agent_id: Uuid,
    recorded: Option<&str>,
) -> Result<String> {
    if let Some(url) = recorded {
        if !url.is_empty() {
            return Ok(url.trim_end_matches('/').to_string());
        }
    }
    let agent = AgentRepo::find_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| anyhow!("agent {agent_id} not found"))?;
    let url = agent
        .streaming_url
        .ok_or_else(|| anyhow!("agent {agent_id} has no streaming_url registered"))?;
    Ok(url.trim_end_matches('/').to_string())
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("build reqwest client")
}

fn parse_command_output(v: &Value) -> CommandOutput {
    CommandOutput {
        stdout: v.get("stdout").and_then(Value::as_str).unwrap_or_default().to_string(),
        stderr: v.get("stderr").and_then(Value::as_str).unwrap_or_default().to_string(),
        exit_code: v.get("exit_code").and_then(Value::as_i64).unwrap_or(0) as i32,
    }
}

async fn post_json(base: &str, path: &str, body: Value) -> Result<Value> {
    let url = format!("{base}{path}");
    let resp = client()?
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("agent {url} returned {status}: {text}"));
    }
    Ok(resp.json::<Value>().await.unwrap_or(json!({})))
}

async fn delete_json(base: &str, path: &str) -> Result<Value> {
    let url = format!("{base}{path}");
    let resp = client()?
        .delete(&url)
        .send()
        .await
        .with_context(|| format!("DELETE {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("agent {url} returned {status}: {text}"));
    }
    Ok(resp.json::<Value>().await.unwrap_or(json!({})))
}

async fn get_json(base: &str, path: &str) -> Result<Value> {
    let url = format!("{base}{path}");
    let resp = client()?
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("agent {url} returned {status}: {text}"));
    }
    Ok(resp.json::<Value>().await.unwrap_or(json!({})))
}

pub async fn create_vm(
    base: &str,
    vm_name: &str,
    hostname: &str,
    target: &str,
    memory_mb: u32,
    net_mode: &str,
) -> Result<CommandOutput> {
    let v = post_json(
        base,
        "/veeros/vms",
        json!({
            "vm_name": vm_name,
            "hostname": hostname,
            "target": target,
            "memory_mb": memory_mb,
            "net_mode": net_mode,
        }),
    )
    .await?;
    Ok(parse_command_output(&v))
}

pub async fn start_vm(base: &str, vm_name: &str) -> Result<CommandOutput> {
    let v = post_json(base, &format!("/veeros/vms/{vm_name}/start"), json!({})).await?;
    Ok(parse_command_output(&v))
}

pub async fn stop_vm(base: &str, vm_name: &str) -> Result<CommandOutput> {
    let v = post_json(base, &format!("/veeros/vms/{vm_name}/stop"), json!({})).await?;
    Ok(parse_command_output(&v))
}

pub async fn destroy_vm(base: &str, vm_name: &str) -> Result<CommandOutput> {
    let v = delete_json(base, &format!("/veeros/vms/{vm_name}")).await?;
    Ok(parse_command_output(&v))
}

pub async fn sensor_inject(
    base: &str,
    vm_name: &str,
    sensor: &str,
    value: &str,
) -> Result<CommandOutput> {
    let v = post_json(
        base,
        &format!("/veeros/vms/{vm_name}/sensor"),
        json!({ "sensor": sensor, "value": value }),
    )
    .await?;
    Ok(parse_command_output(&v))
}

pub async fn vm_status(base: &str, vm_name: &str) -> Result<String> {
    let v = get_json(base, &format!("/veeros/vms/{vm_name}")).await?;
    Ok(v.get("status").and_then(Value::as_str).unwrap_or("unknown").to_string())
}

/// Fetch the VM's real shell host/port/connect_command from the agent.
pub async fn shell_info(base: &str, vm_name: &str) -> Result<(String, u16, String)> {
    let v = get_json(base, &format!("/veeros/vms/{vm_name}/shell_info")).await?;
    let host = v.get("host").and_then(Value::as_str).unwrap_or("").to_string();
    let port = v.get("port").and_then(Value::as_u64).unwrap_or(0) as u16;
    let cmd = v.get("connect_command").and_then(Value::as_str).unwrap_or("").to_string();
    Ok((host, port, cmd))
}
