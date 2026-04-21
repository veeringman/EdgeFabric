pub mod devices;
pub mod jobs;
pub mod sessions;
pub mod streaming;
pub mod agent;
pub mod virtual_iot;

use axum::{Json, Router, extract::State, http::StatusCode, routing::{get, post, put}};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::auth::{api_key, jwt};
use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

pub fn api_router() -> Router<AppState> {
    Router::new()
        // Device CRUD
        .route("/devices", get(devices::list_devices).post(devices::create_device))
        .route("/devices/{id}", get(devices::get_device).put(devices::update_device).delete(devices::delete_device))
        .route("/devices/{id}/status", put(devices::set_device_status))
        .route("/devices/{id}/preference", put(devices::set_device_preference))
        .route("/devices/{id}/heartbeat", post(devices::device_heartbeat))
        .route("/devices/{id}/health", get(devices::device_health))
        // Session CRUD
        .route("/sessions", get(sessions::list_sessions).post(sessions::create_session))
        .route("/sessions/{id}", get(sessions::get_session))
        .route("/sessions/{id}/status", put(sessions::transition_session))
        .route("/sessions/{id}/end", post(sessions::end_session))
        // Job / Test Orchestration
        .route("/jobs", get(jobs::list_jobs).post(jobs::submit_job))
        .route("/jobs/{id}", get(jobs::get_job))
        .route("/jobs/{id}/progress", get(jobs::get_job_progress))
        .route("/jobs/{id}/cancel", post(jobs::cancel_job))
        .route("/jobs/{id}/artifacts", get(jobs::list_artifacts))
        // Streaming (WebSocket screenshot stream)
        .route("/stream/{session_id}", get(streaming::ws_stream_handler))
        // Also allow streaming by device_id directly
        .route("/devices/{id}/stream", get(streaming::ws_stream_handler))
        // Device log streaming (WebSocket logcat)
        .route("/devices/{id}/logs", get(streaming::ws_logs_handler))
        // Agent endpoints (device registration, heartbeat, command polling)
        .route("/agent/register", post(agent::register_agent))
        .route("/agent/devices", post(agent::register_device))
        .route("/agent/devices/{id}", axum::routing::delete(agent::deregister_device))
        .route("/agent/{agent_id}/commands", get(agent::poll_commands))
        .route("/agents", get(agent::list_agents))
        // Virtual IoT endpoints
        .route("/devices/virtual-iot", post(virtual_iot::create_virtual_iot))
        .route("/devices/virtual-iot/backends", get(virtual_iot::list_virtual_iot_backends).post(virtual_iot::create_virtual_iot_backend))
        .route("/devices/virtual-iot/backends/{id}", axum::routing::patch(virtual_iot::update_virtual_iot_backend).delete(virtual_iot::delete_virtual_iot_backend))
        .route("/devices/virtual-iot/backends/{id}/start", post(virtual_iot::start_virtual_iot_backend))
        .route("/devices/virtual-iot/backends/{id}/stop", post(virtual_iot::stop_virtual_iot_backend))
        .route("/devices/{id}/virtual-iot/config", get(virtual_iot::get_virtual_iot_config).patch(virtual_iot::update_virtual_iot_config))
        .route("/devices/{id}/virtual-iot/backend/attach", post(virtual_iot::attach_virtual_iot_backend))
        .route("/devices/{id}/virtual-iot/backend/detach", post(virtual_iot::detach_virtual_iot_backend))
        .route("/devices/{id}/virtual-iot/sensors", post(virtual_iot::inject_virtual_iot_sensor))
        .route("/devices/{id}/virtual-iot/shell", get(virtual_iot::get_virtual_iot_shell_info))
        .route("/devices/{id}/virtual-iot/shell/ws", get(virtual_iot::virtual_iot_shell_ws))
}

#[utoipa::path(get, path = "/health", tag = "health",
    responses((status = 200, description = "Service is healthy", body = Value))
)]
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

// -- Auth endpoints (public) --

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub api_key: String,
}

/// Exchange an API key for a JWT token.
#[utoipa::path(post, path = "/api/v1/auth/login", tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "JWT token issued", body = Value),
        (status = 401, description = "Invalid API key", body = ApiError),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let tenant = api_key::validate_api_key(&state.pool, &body.api_key)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
        .ok_or_else(|| ApiError::unauthorized("Invalid API key"))?;

    let token = jwt::generate_token(tenant.id, &tenant.name, &state.jwt_secret, 3600)
        .map_err(|e| ApiError::internal(format!("Token generation failed: {e}")))?;

    Ok(Json(json!({
        "token": token,
        "tenant_id": tenant.id,
        "expires_in": 3600
    })))
}
