use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::AppState;
use crate::middleware::error_response::ApiError;

use ef_db::repo::agent::AgentRepo;
use ef_db::repo::device::DeviceRepo;

/// Agent device registration request (upsert by serial).
#[derive(Deserialize)]
pub struct AgentRegisterDeviceRequest {
    pub name: String,
    pub serial: Option<String>,
    pub os: String,
    pub os_version: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub agent_id: String,
    pub ip_address: Option<String>,
    /// The agent's streaming WebSocket base URL (e.g. http://192.168.29.6:7070).
    pub streaming_url: Option<String>,
    /// Capabilities the agent itself advertises (veeros, runner, ...).
    #[serde(default)]
    pub agent_capabilities: Vec<String>,
}

fn default_kind() -> String { "real".into() }
fn default_region() -> String { "default".into() }

/// POST /api/v1/agent/devices
///
/// Called by edge agents to register (or re-register) a discovered device.
/// Upserts by serial so re-scans don't create duplicates.
pub async fn register_device(
    State(state): State<AppState>,
    Json(body): Json<AgentRegisterDeviceRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    let serial = body.serial.as_deref().unwrap_or(&body.name);

    tracing::info!(
        agent_id = %body.agent_id,
        serial,
        name = %body.name,
        os = %body.os,
        "Agent registering device"
    );

    // Parse agent_id as UUID (agents should use a UUID, fall back to a
    // deterministic UUID from the string if it's not a valid UUID).
    let agent_uuid = Uuid::parse_str(&body.agent_id).unwrap_or_else(|_| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        body.agent_id.hash(&mut hasher);
        let h = hasher.finish().to_be_bytes();
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&h);
        bytes[8..16].copy_from_slice(&h);
        Uuid::from_bytes(bytes)
    });

    // Upsert the agent record (creates agents table row with streaming_url)
    if let Err(e) = AgentRepo::upsert(
        &state.pool,
        agent_uuid,
        &body.agent_id,
        &body.region,
        body.streaming_url.as_deref(),
        &body.agent_capabilities,
    ).await {
        tracing::warn!(agent_id = %body.agent_id, "Failed to upsert agent: {e}");
    }

    let device = DeviceRepo::upsert_by_serial(
        &state.pool,
        &body.name,
        serial,
        &body.os,
        &body.os_version,
        &body.kind,
        &body.region,
        &body.capabilities,
        agent_uuid,
        body.ip_address.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to upsert device: {e}");
        ApiError::internal("Failed to register device")
    })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "device_id": device.id, "status": "registered" })),
    ))
}

/// DELETE /api/v1/agent/devices/:id
///
/// Called by an agent when a device disappears from discovery.
pub async fn deregister_device(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(device_id = %id, "Agent deregistering device");

    let updated = DeviceRepo::set_status(&state.pool, id, "offline")
        .await
        .map_err(|e| {
            tracing::error!("Failed to set offline: {e}");
            ApiError::internal("Failed to deregister device")
        })?;

    if updated {
        Ok(Json(json!({ "device_id": id, "status": "offline" })))
    } else {
        Err(ApiError::not_found(format!("Device {id} not found")))
    }
}

/// GET /api/v1/agent/:agent_id/commands
///
/// Agents poll this to check for pending commands.
pub async fn poll_commands(
    Path(agent_id): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::debug!(agent_id, "Agent polling for commands");

    // TODO: implement command queue — for now return empty list
    Ok(Json(json!({ "commands": [] })))
}

/// Agent registration request — called once at agent startup so the
/// control plane knows about the agent and its capabilities even if
/// no devices have been discovered yet.
#[derive(Deserialize)]
pub struct AgentRegisterRequest {
    pub agent_id: String,
    #[serde(default = "default_region")]
    pub region: String,
    pub streaming_url: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// POST /api/v1/agent/register
///
/// Standalone agent registration — upserts the agents table without
/// requiring a device. Used by ef-agent at startup.
pub async fn register_agent(
    State(state): State<AppState>,
    Json(body): Json<AgentRegisterRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(
        agent_id = %body.agent_id,
        region = %body.region,
        capabilities = ?body.capabilities,
        "Agent registering"
    );

    let agent_uuid = Uuid::parse_str(&body.agent_id).unwrap_or_else(|_| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        body.agent_id.hash(&mut hasher);
        let h = hasher.finish().to_be_bytes();
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&h);
        bytes[8..16].copy_from_slice(&h);
        Uuid::from_bytes(bytes)
    });

    let agent = AgentRepo::upsert(
        &state.pool,
        agent_uuid,
        &body.agent_id,
        &body.region,
        body.streaming_url.as_deref(),
        &body.capabilities,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to upsert agent: {e}");
        ApiError::internal("Failed to register agent")
    })?;

    Ok(Json(json!({ "agent": agent })))
}

/// GET /api/v1/agents
///
/// List all registered agents, optionally filtered by capability/region.
#[derive(Deserialize)]
pub struct ListAgentsQuery {
    pub capability: Option<String>,
    pub region: Option<String>,
    #[serde(default)]
    pub only_online: bool,
}

pub async fn list_agents(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListAgentsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let agents = AgentRepo::list(
        &state.pool,
        query.capability.as_deref(),
        query.region.as_deref(),
        query.only_online,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list agents: {e}");
        ApiError::internal("Failed to list agents")
    })?;

    Ok(Json(json!({ "agents": agents, "total": agents.len() })))
}
