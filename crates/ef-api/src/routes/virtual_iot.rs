use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

use ef_db::repo::device::DeviceRepo;
use ef_db::repo::virtual_iot::VirtualIotConfigRepo;

#[derive(Deserialize)]
pub struct CreateVirtualIotRequest {
    pub name: String,
    #[serde(default = "default_controller")]
    pub controller_type: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_sim_rate")]
    pub sim_rate_ms: i32,
    #[serde(default)]
    pub enabled_sensors: Vec<String>,
}

fn default_controller() -> String { "esp32-devkit".into() }
fn default_region() -> String { "local".into() }
fn default_sim_rate() -> i32 { 500 }

#[derive(Deserialize)]
pub struct UpdateVirtualIotConfigRequest {
    pub controller_type: Option<String>,
    pub enabled_sensors: Option<Vec<String>>,
    pub sim_rate_ms: Option<i32>,
}

/// POST /api/v1/devices/virtual-iot
pub async fn create_virtual_iot(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CreateVirtualIotRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, name = %body.name, "Creating virtual IoT device");

    // Create the device record with os=virtual_iot, kind=virtual
    let device = DeviceRepo::create(
        &state.pool,
        &body.name,
        None,
        "virtual_iot",
        &body.controller_type,  // store controller type as os_version for display
        "virtual",
        &body.region,
        &[],
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to create virtual IoT device: {e}");
        ApiError::internal("Failed to create device")
    })?;

    let _ = DeviceRepo::update_heartbeat(&state.pool, device.id).await;

    // Create the virtual IoT config record
    let sensors_json = serde_json::to_value(&body.enabled_sensors).unwrap_or_default();
    let config = VirtualIotConfigRepo::create(
        &state.pool,
        device.id,
        &body.controller_type,
        &sensors_json,
        body.sim_rate_ms,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to create virtual IoT config: {e}");
        ApiError::internal("Failed to create virtual IoT config")
    })?;

    Ok((StatusCode::CREATED, Json(json!({
        "device": device,
        "virtual_iot_config": config,
    }))))
}

/// GET /api/v1/devices/:id/virtual-iot/config
pub async fn get_virtual_iot_config(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Getting virtual IoT config");

    let config = VirtualIotConfigRepo::find_by_device_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    Ok(Json(json!({ "config": config })))
}

/// PATCH /api/v1/devices/:id/virtual-iot/config
pub async fn update_virtual_iot_config(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<UpdateVirtualIotConfigRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Updating virtual IoT config");

    // Fetch existing config
    let existing = VirtualIotConfigRepo::find_by_device_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    let controller_type = body.controller_type.as_deref().unwrap_or(&existing.controller_type);
    let sensors_json = match &body.enabled_sensors {
        Some(sensors) => serde_json::to_value(sensors).unwrap_or_default(),
        None => existing.enabled_sensors.clone(),
    };
    let sim_rate_ms = body.sim_rate_ms.unwrap_or(existing.sim_rate_ms);

    let updated = VirtualIotConfigRepo::update(
        &state.pool,
        id,
        controller_type,
        &sensors_json,
        sim_rate_ms,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to update virtual IoT config: {e}");
        ApiError::internal("Failed to update config")
    })?
    .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    Ok(Json(json!({ "config": updated })))
}
