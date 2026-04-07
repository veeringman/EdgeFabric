use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::AppState;
use crate::middleware::error_response::ApiError;

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
