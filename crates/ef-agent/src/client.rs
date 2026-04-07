//! gRPC client for connecting the edge agent to the control plane.
//!
//! The agent acts as a **client** to the control plane's gRPC services:
//! - `DeviceService` (register/deregister discovered devices, send health)
//! - `SessionService` (receive command requests, send results)

use std::time::Duration;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents the connection to the control plane.
pub struct ControlPlaneClient {
    base_url: String,
    agent_id: String,
    http: reqwest::Client,
}

/// Device registration payload sent to the control plane.
#[derive(Debug, Serialize)]
pub struct RegisterDeviceRequest {
    pub name: String,
    pub serial: Option<String>,
    pub os: String,
    pub os_version: String,
    pub kind: String,
    pub region: String,
    pub capabilities: Vec<String>,
    pub agent_id: String,
    pub ip_address: Option<String>,
}

/// Heartbeat payload sent periodically.
#[derive(Debug, Serialize)]
pub struct HeartbeatPayload {
    pub agent_id: String,
    pub device_id: Uuid,
    pub status: String,
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<f64>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Command received from the control plane to execute locally.
#[derive(Debug, Deserialize)]
pub struct PendingCommand {
    pub id: Uuid,
    pub device_id: Uuid,
    pub command_type: String,
    pub params: serde_json::Value,
}

/// Result of executing a command, sent back to the control plane.
#[derive(Debug, Serialize)]
pub struct CommandResult {
    pub command_id: Uuid,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ControlPlaneClient {
    pub fn new(base_url: &str, agent_id: &str, tls_skip_verify: bool) -> anyhow::Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(30));

        if tls_skip_verify {
            tracing::warn!("TLS certificate verification is DISABLED — do not use in production");
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http = builder.build()?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            agent_id: agent_id.to_string(),
            http,
        })
    }

    /// Register a discovered device with the control plane.
    pub async fn register_device(&self, req: &RegisterDeviceRequest) -> anyhow::Result<Uuid> {
        let url = format!("{}/api/v1/agent/devices", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(req)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Register device failed ({status}): {body}");
        }

        let body: serde_json::Value = resp.json().await?;
        let id = body["device_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| anyhow::anyhow!("Missing device_id in response"))?;

        Ok(id)
    }

    /// Deregister a device (e.g. when it disappears from discovery).
    pub async fn deregister_device(&self, device_id: Uuid) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/agent/devices/{device_id}", self.base_url);
        let resp = self.http.delete(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("Deregister device {device_id} failed ({status}): {body}");
        }
        Ok(())
    }

    /// Send a heartbeat for a device.
    pub async fn send_heartbeat(&self, heartbeat: &HeartbeatPayload) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/v1/devices/{}/heartbeat",
            self.base_url, heartbeat.device_id
        );
        let resp = self.http.post(&url).json(heartbeat).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            tracing::warn!(
                device_id = %heartbeat.device_id,
                "Heartbeat failed ({status})"
            );
        }
        Ok(())
    }

    /// Poll the control plane for pending commands for this agent.
    pub async fn poll_commands(&self) -> anyhow::Result<Vec<PendingCommand>> {
        let url = format!(
            "{}/api/v1/agent/{}/commands",
            self.base_url, self.agent_id
        );
        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let body: serde_json::Value = resp.json().await?;
        let commands: Vec<PendingCommand> = serde_json::from_value(
            body.get("commands").cloned().unwrap_or(serde_json::json!([])),
        )?;

        Ok(commands)
    }

    /// Report the result of executing a command back to the control plane.
    pub async fn report_command_result(&self, result: &CommandResult) -> anyhow::Result<()> {
        let url = format!(
            "{}/api/v1/agent/commands/{}/result",
            self.base_url, result.command_id
        );
        let resp = self.http.post(&url).json(result).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            tracing::warn!(
                command_id = %result.command_id,
                "Report command result failed ({status})"
            );
        }
        Ok(())
    }

    /// Check if the control plane is reachable.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        match self.http.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}
