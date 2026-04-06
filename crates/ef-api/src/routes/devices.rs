use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

use ef_db::repo::device::DeviceRepo;

#[derive(Deserialize)]
pub struct CreateDeviceRequest {
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
}

fn default_kind() -> String { "real".into() }
fn default_region() -> String { "default".into() }

#[derive(Deserialize)]
pub struct UpdateDeviceRequest {
    pub name: String,
    pub serial: Option<String>,
    pub os_version: String,
    pub region: String,
    pub capabilities: Vec<String>,
}

#[derive(Deserialize)]
pub struct DeviceListQuery {
    pub os: Option<String>,
    pub status: Option<String>,
    pub region: Option<String>,
}

/// GET /api/v1/devices
pub async fn list_devices(
    auth: AuthContext,
    Query(q): Query<DeviceListQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, "Listing devices");

    let devices = if q.os.is_some() || q.region.is_some() {
        DeviceRepo::list_available(&state.pool, q.os.as_deref(), q.region.as_deref())
            .await
    } else {
        DeviceRepo::list(&state.pool).await
    }
    .map_err(|e| {
        tracing::error!("Failed to list devices: {e}");
        ApiError::internal("Failed to list devices")
    })?;

    let total = devices.len();
    Ok(Json(json!({ "devices": devices, "total": total })))
}

/// GET /api/v1/devices/:id
pub async fn get_device(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Getting device");

    let device = DeviceRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Device {id} not found")))?;

    Ok(Json(json!({ "device": device })))
}

/// POST /api/v1/devices
pub async fn create_device(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CreateDeviceRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, name = %body.name, "Creating device");

    let device = DeviceRepo::create(
        &state.pool,
        &body.name,
        body.serial.as_deref(),
        &body.os,
        &body.os_version,
        &body.kind,
        &body.region,
        &body.capabilities,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to create device: {e}");
        ApiError::internal("Failed to create device")
    })?;

    Ok((StatusCode::CREATED, Json(json!({ "device": device }))))
}

/// PUT /api/v1/devices/:id
pub async fn update_device(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<UpdateDeviceRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Updating device");

    let device = DeviceRepo::update(
        &state.pool,
        id,
        &body.name,
        body.serial.as_deref(),
        &body.os_version,
        &body.region,
        &body.capabilities,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to update device: {e}");
        ApiError::internal("Failed to update device")
    })?
    .ok_or_else(|| ApiError::not_found(format!("Device {id} not found")))?;

    Ok(Json(json!({ "device": device })))
}

/// DELETE /api/v1/devices/:id
pub async fn delete_device(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Deleting device");

    let deleted = DeviceRepo::delete(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete device: {e}");
            ApiError::internal("Failed to delete device")
        })?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("Device {id} not found")))
    }
}

/// PUT /api/v1/devices/:id/status
#[derive(Deserialize)]
pub struct SetStatusRequest {
    pub status: String,
}

pub async fn set_device_status(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<SetStatusRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, status = %body.status, "Setting device status");

    let updated = DeviceRepo::set_status(&state.pool, id, &body.status)
        .await
        .map_err(|e| {
            tracing::error!("Failed to set status: {e}");
            ApiError::internal("Failed to set device status")
        })?;

    if updated {
        Ok(Json(json!({ "device_id": id, "status": body.status })))
    } else {
        Err(ApiError::not_found(format!("Device {id} not found")))
    }
}

/// POST /api/v1/devices/:id/heartbeat
pub async fn device_heartbeat(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let updated = DeviceRepo::update_heartbeat(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Heartbeat DB error: {e}");
            ApiError::internal("Heartbeat update failed")
        })?;

    if updated {
        // Also ensure device is marked available if it was offline
        let _ = DeviceRepo::set_status(&state.pool, id, "available").await;
        Ok(Json(json!({ "device_id": id, "heartbeat": "ok" })))
    } else {
        Err(ApiError::not_found(format!("Device {id} not found")))
    }
}

/// GET /api/v1/devices/:id/health
pub async fn device_health(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Checking device health");

    let device = DeviceRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Device {id} not found")))?;

    let healthy = device.last_heartbeat.map_or(false, |hb| {
        (chrono::Utc::now() - hb).num_seconds() < 60
    });

    Ok(Json(json!({
        "device_id": id,
        "status": device.status,
        "last_heartbeat": device.last_heartbeat,
        "healthy": healthy
    })))
}
