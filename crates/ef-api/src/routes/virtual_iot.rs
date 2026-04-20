use axum::{
    Json,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, process::Command};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

use ef_control::command::veeros::{VeerOsDriver, VeerOsShellInfo};
use ef_db::repo::device::DeviceRepo;
use ef_db::repo::virtual_iot_backend::{VirtualIotBackendRepo, VirtualIotBackendRow};
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
fn default_provider() -> String { "veeros-vm".into() }
fn default_available_only() -> bool { true }
fn default_auto_start() -> bool { true }

#[derive(Deserialize)]
pub struct UpdateVirtualIotConfigRequest {
    pub controller_type: Option<String>,
    pub enabled_sensors: Option<Vec<String>>,
    pub sim_rate_ms: Option<i32>,
}

#[derive(Deserialize)]
pub struct CreateVirtualIotBackendRequest {
    pub name: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_controller")]
    pub controller_type: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
}

#[derive(Deserialize)]
pub struct ListVirtualIotBackendsQuery {
    pub controller_type: Option<String>,
    pub region: Option<String>,
    #[serde(default = "default_available_only")]
    pub available_only: bool,
}

#[derive(Deserialize)]
pub struct AttachVirtualIotBackendRequest {
    pub backend_id: Uuid,
}

#[derive(Deserialize)]
pub struct SensorInjectRequest {
    pub sensor: String,
    pub value: String,
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

    let sensors_json = serde_json::to_value(&body.enabled_sensors).unwrap_or_default();
    let config = VirtualIotConfigRepo::create(
        &state.pool,
        device.id,
        &body.controller_type,
        &sensors_json,
        None,
        &json!({}),
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

pub async fn list_virtual_iot_backends(
    auth: AuthContext,
    Query(query): Query<ListVirtualIotBackendsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, "Listing virtual IoT backends");

    let backends = VirtualIotBackendRepo::list(
        &state.pool,
        query.controller_type.as_deref(),
        query.region.as_deref(),
        query.available_only,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to list virtual IoT backends: {e}");
        ApiError::internal("Failed to list virtual IoT backends")
    })?;

    Ok(Json(json!({
        "backends": backends,
        "total": backends.len(),
    })))
}

pub async fn create_virtual_iot_backend(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CreateVirtualIotBackendRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, name = %body.name, "Creating virtual IoT backend");

    if body.provider != "veeros-vm" {
        return Err(ApiError::bad_request(format!("Unsupported backend provider: {}", body.provider)));
    }

    let seed = Uuid::new_v4().simple().to_string();
    let vm_name = VeerOsDriver::sanitize_vm_name(&body.name, &seed[..8]);
    let hostname = vm_name.replace('_', "-");

    let create_output = VeerOsDriver::create_vm(
        &state.veeros,
        &vm_name,
        &hostname,
        &state.veeros.default_target,
        state.veeros.default_memory_mb,
        &state.veeros.default_network_mode,
    )
    .await
    .map_err(ApiError::from_domain)?;

    let mut status = "available".to_string();
    if body.auto_start {
        VeerOsDriver::start_vm(&state.veeros, &vm_name)
            .await
            .map_err(ApiError::from_domain)?;
        status = "running".to_string();
    }

    let shell = VeerOsDriver::shell_info(&state.veeros, &vm_name)
        .await
        .map_err(ApiError::from_domain)?;
    let backend_config = backend_json(&shell, &state.veeros.default_network_mode, &status);
    let capabilities = vec![
        format!("provider:{}", body.provider),
        format!("target:{}", state.veeros.default_target),
        "shell:veeros".to_string(),
    ];

    let backend = VirtualIotBackendRepo::create(
        &state.pool,
        &body.name,
        &body.provider,
        &body.controller_type,
        &body.region,
        if body.auto_start { "available" } else { "maintenance" },
        Some(&vm_name),
        Some(&shell.host),
        Some(shell.port as i32),
        &backend_config,
        &capabilities,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to persist virtual IoT backend: {e}");
        ApiError::internal("Failed to persist virtual IoT backend")
    })?;

    Ok((StatusCode::CREATED, Json(json!({
        "backend": backend,
        "provision_stdout": create_output.stdout,
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
        existing.backend_id,
        &existing.backend,
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

pub async fn attach_virtual_iot_backend(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<AttachVirtualIotBackendRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, backend_id = %body.backend_id, "Attaching virtual IoT backend");

    let config = VirtualIotConfigRepo::find_by_device_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    let backend = VirtualIotBackendRepo::find_by_id(&state.pool, body.backend_id)
        .await
        .map_err(|e| {
            tracing::error!("Backend DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT backend {} not found", body.backend_id)))?;

    if backend.controller_type != config.controller_type {
        return Err(ApiError::bad_request(format!(
            "Backend {} is not compatible with controller {}",
            backend.name, config.controller_type
        )));
    }

    if backend.attached_device_id.is_some() && backend.attached_device_id != Some(id) {
        return Err(ApiError::bad_request(format!("Backend {} is already attached", backend.name)));
    }

    let backend_payload = backend_payload(&backend);
    let updated = VirtualIotConfigRepo::update_backend(&state.pool, id, Some(backend.id), &backend_payload)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update backend attachment: {e}");
            ApiError::internal("Failed to update backend attachment")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    let backend = VirtualIotBackendRepo::update_status_and_attachment(&state.pool, backend.id, "busy", Some(id))
        .await
        .map_err(|e| {
            tracing::error!("Failed to update backend status: {e}");
            ApiError::internal("Failed to update backend status")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT backend {} not found", body.backend_id)))?;

    let _ = DeviceRepo::set_status(&state.pool, id, if backend.config.get("runtime_status").and_then(Value::as_str) == Some("running") { "available" } else { "maintenance" }).await;
    let _ = DeviceRepo::update_heartbeat(&state.pool, id).await;

    Ok(Json(json!({
        "config": updated,
        "backend": backend,
    })))
}

pub async fn detach_virtual_iot_backend(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Detaching virtual IoT backend");

    let config = VirtualIotConfigRepo::find_by_device_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    if let Some(backend_id) = config.backend_id {
        let _ = VirtualIotBackendRepo::update_status_and_attachment(&state.pool, backend_id, "available", None).await;
    }

    let updated = VirtualIotConfigRepo::update_backend(&state.pool, id, None, &json!({}))
        .await
        .map_err(|e| {
            tracing::error!("Failed to detach backend: {e}");
            ApiError::internal("Failed to detach backend")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    let _ = DeviceRepo::set_status(&state.pool, id, "maintenance").await;

    Ok(Json(json!({ "config": updated })))
}

pub async fn get_virtual_iot_shell_info(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, "Getting virtual IoT shell info");
    let backend = attached_backend(&state, id).await?;
    Ok(Json(json!({
        "backend": backend,
        "shell": shell_descriptor(&backend, id),
    })))
}

pub async fn inject_virtual_iot_sensor(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<SensorInjectRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, device_id = %id, sensor = %body.sensor, "Injecting sensor into attached backend");
    let backend = attached_backend(&state, id).await?;
    let vm_name = backend.vm_name.clone().ok_or_else(|| ApiError::bad_request("Attached backend has no VM binding"))?;

    let output = VeerOsDriver::sensor_inject(&state.veeros, &vm_name, &body.sensor, &body.value)
        .await
        .map_err(ApiError::from_domain)?;

    Ok(Json(json!({
        "device_id": id,
        "backend_id": backend.id,
        "sensor": body.sensor,
        "value": body.value,
        "stdout": output.stdout,
    })))
}

pub async fn virtual_iot_shell_ws(
    ws: WebSocketUpgrade,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_virtual_iot_shell(socket, id, state))
}

async fn handle_virtual_iot_shell(socket: WebSocket, device_id: Uuid, state: AppState) {
    let backend = match attached_backend(&state, device_id).await {
        Ok(backend) => backend,
        Err(_) => return,
    };

    let host = backend.shell_host.clone().unwrap_or_else(|| "localhost".into());
    let port = backend.shell_port.unwrap_or_default().to_string();
    let veer_connect_path = resolve_veer_connect_path(&state);
    let mut child = match Command::new(&veer_connect_path)
        .current_dir(&state.veeros.root_dir)
        .env("TERM", "xterm-256color")
        .args(["shell", &host, &port])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            tracing::error!(device_id = %device_id, "Failed to spawn veer-connect: {e}");
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();
    let mut child_stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => return,
    };
    let mut child_stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => return,
    };
    let mut child_stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => return,
    };

    let _ = ws_tx.send(Message::Text(json!({
        "type": "shell_ready",
        "device_id": device_id,
        "host": host,
        "port": port,
        "hint": "Login with root/toor or user/veeros"
    }).to_string().into())).await;

    let stdout_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match child_stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if ws_tx.send(Message::Text(String::from_utf8_lossy(&buf[..n]).to_string().into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match child_stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });

    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                if child_stdin.write_all(text.as_bytes()).await.is_err() {
                    break;
                }
            }
            Message::Binary(data) => {
                if child_stdin.write_all(&data).await.is_err() {
                    break;
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = child.kill().await;
    let _ = stdout_task.await;
    let _ = stderr_task.await;
}

async fn attached_backend(
    state: &AppState,
    device_id: Uuid,
) -> Result<VirtualIotBackendRow, (StatusCode, Json<ApiError>)> {
    let config = VirtualIotConfigRepo::find_by_device_id(&state.pool, device_id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {device_id} not found")))?;

    let backend_id = config.backend_id.ok_or_else(|| ApiError::bad_request("No backend attached"))?;
    VirtualIotBackendRepo::find_by_id(&state.pool, backend_id)
        .await
        .map_err(|e| {
            tracing::error!("Backend DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT backend {backend_id} not found")))
}

fn backend_json(shell: &VeerOsShellInfo, network_mode: &str, runtime_status: &str) -> Value {
    json!({
        "provider": "veeros-vm",
        "vm_name": shell.vm_name,
        "shell_host": shell.host,
        "shell_port": shell.port,
        "connect_command": shell.connect_command,
        "network_mode": network_mode,
        "runtime_status": runtime_status,
    })
}

fn backend_payload(backend: &VirtualIotBackendRow) -> Value {
    json!({
        "id": backend.id,
        "name": backend.name,
        "provider": backend.provider,
        "controller_type": backend.controller_type,
        "region": backend.region,
        "status": backend.status,
        "vm_name": backend.vm_name,
        "shell_host": backend.shell_host,
        "shell_port": backend.shell_port,
        "config": backend.config,
    })
}

fn shell_descriptor(backend: &VirtualIotBackendRow, device_id: Uuid) -> Value {
    json!({
        "backend_id": backend.id,
        "host": backend.shell_host,
        "port": backend.shell_port,
        "connect_command": backend.config.get("connect_command").cloned().unwrap_or(Value::Null),
        "ws_path": format!("/api/v1/devices/{device_id}/virtual-iot/shell/ws"),
    })
}

fn resolve_veer_connect_path(state: &AppState) -> String {
    let configured = std::path::Path::new(&state.veeros.veer_connect_path);
    if configured.exists() {
        state.veeros.veer_connect_path.clone()
    } else {
        format!("{}/target/debug/veer-connect", state.veeros.root_dir)
    }
}
