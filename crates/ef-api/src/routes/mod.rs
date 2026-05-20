pub mod devices;
pub mod jobs;
pub mod sessions;
pub mod streaming;
pub mod agent;
pub mod virtual_iot;
pub mod quantum;
pub mod discovery;
pub mod runners;
pub mod builds;
pub mod integrations;
pub mod starter_projects;

use axum::{Json, Router, extract::State, http::StatusCode, routing::{get, post, put}};
use serde::Deserialize;
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::auth::{api_key, jwt};
use crate::middleware::auth_extractor::AppState;
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
        // VeerOS shell terminal (xterm.js over WS)
        .route("/veeros/shell/ws", get(streaming::ws_veeros_shell_handler))
        // Device log streaming (WebSocket logcat)
        .route("/devices/{id}/logs", get(streaming::ws_logs_handler))
        // Agent endpoints (device registration, heartbeat, command polling)
        .route("/agent/register", post(agent::register_agent))
        .route("/agent/devices", post(agent::register_device))
        .route("/agent/devices/{id}", axum::routing::delete(agent::deregister_device))
        .route("/agent/{agent_id}/commands", get(agent::poll_commands))
        .route("/agents", get(agent::list_agents))
        .route("/agents/{agent_id}", get(agent::get_agent))
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
        .route("/devices/{id}/virtual-iot/sensors/stream", get(virtual_iot::virtual_iot_sensor_stream_ws))
        .route("/devices/{id}/virtual-iot/actuators", post(virtual_iot::write_virtual_iot_actuator))
        .route("/devices/{id}/virtual-iot/scripts/run", post(virtual_iot::run_virtual_iot_script))
        .route("/devices/{id}/virtual-iot/shell", get(virtual_iot::get_virtual_iot_shell_info))
        .route("/devices/{id}/virtual-iot/shell/ws", get(virtual_iot::virtual_iot_shell_ws))
        // Global synthetic sensor simulation toggle (disabled by default).
        .route("/virtual-iot/simulation", get(virtual_iot::get_simulation_state))
        .route("/virtual-iot/simulation/start", post(virtual_iot::start_simulation))
        .route("/virtual-iot/simulation/stop", post(virtual_iot::stop_simulation))
        // Quantum
        .route("/quantum/backends", get(quantum::list_backends))
        .route("/quantum/agents/{agent_id}/backends", get(quantum::agent_quantum_backends))
        .route("/quantum/agents/{agent_id}/simulate", post(quantum::simulate_on_agent))
        .route("/quantum/agents/{agent_id}/ibm/run", post(quantum::ibm_run_on_agent))
        .route("/quantum/agents/{agent_id}/vms/{vm_name}/quantum", get(quantum::vm_quantum_info))
        // Discovery
        .route("/discovery/hosts", get(discovery::list_hosts))
        .route("/discovery/hosts/{id}", get(discovery::get_host).delete(discovery::delete_host))
        // Runner management
        .route("/runners", get(runners::list_runners))
        .route("/runners/register", post(runners::register_runner))
        .route("/runners/allocate", post(runners::allocate_runner_lease))
        .route("/runners/{id}", get(runners::get_runner))
        .route("/runners/{id}/drain", post(runners::drain_runner))
        .route("/runners/{id}/probe", post(runners::probe_runner))
        .route("/runners/leases/{id}/release", post(runners::release_runner_lease))
        // iOS build execution
        .route("/builds", get(builds::list_builds))
        .route("/builds/ios", post(builds::submit_ios_build))
        .route("/builds/iot/deploy", post(builds::submit_iot_deploy))
        .route("/builds/quantum/run", post(builds::submit_quantum_program))
        .route("/builds/{id}", get(builds::get_build))
        .route("/builds/{id}/logs", get(builds::get_build_logs))
        .route("/builds/{id}/artifacts", get(builds::list_build_artifacts))
        .route("/builds/{id}/cancel", post(builds::cancel_build))
        // iOS starter project generation/download
        .route("/starter-projects/ios/defaults", get(starter_projects::ios_starter_defaults))
        .route("/starter-projects/ios", post(starter_projects::create_ios_starter_project))
        .route("/starter-projects/ios/archive/{module_name}", get(starter_projects::download_ios_starter_archive))
        .route("/starter-projects/workspace/archive", post(starter_projects::export_workspace_archive))
        .route("/starter-projects/workspace/import", post(starter_projects::import_workspace_archive))
        .route("/starter-projects/workspace/save", post(starter_projects::save_workspace_project))
        .route("/starter-projects/workspace/projects", get(starter_projects::list_workspace_projects))
        .route(
            "/starter-projects/workspace/projects/{name}",
            get(starter_projects::get_workspace_project)
                .delete(starter_projects::delete_workspace_project),
        )
        // External integrations (MQTT + cloud sinks)
        .route("/integrations/mqtt/subscriptions", get(integrations::list_mqtt_subscriptions).post(integrations::create_mqtt_subscription))
        .route("/integrations/mqtt/subscriptions/{id}", axum::routing::delete(integrations::delete_mqtt_subscription))
        .route("/integrations/mqtt/publishers", get(integrations::list_mqtt_publishers).post(integrations::create_mqtt_publisher))
        .route("/integrations/mqtt/publishers/{id}", axum::routing::delete(integrations::delete_mqtt_publisher))
        .route("/integrations/cloud/sinks", get(integrations::list_cloud_sinks).post(integrations::create_cloud_sink))
        .route("/integrations/cloud/sinks/{id}", axum::routing::delete(integrations::delete_cloud_sink))
        .route("/integrations/delivery-attempts", get(integrations::list_delivery_attempts))
        .route("/integrations/sensors/events", post(integrations::ingest_sensor_event))
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
