use axum::{
    Json,
    extract::{
        Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt, future::join_all};
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
    #[serde(default)]
    pub event_definitions: Vec<Value>,
}

fn default_controller() -> String { "esp32-devkit".into() }
fn default_region() -> String { "local".into() }
fn default_sim_rate() -> i32 { 500 }
fn default_provider() -> String { "veer-vm".into() }
fn default_available_only() -> bool { true }
fn default_auto_start() -> bool { true }

fn normalize_backend_provider(provider: &str) -> Option<&'static str> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "veer-vm" | "veeros-vm" | "veerm-vm" => Some("veer-vm"),
        _ => None,
    }
}

#[derive(Deserialize)]
pub struct UpdateVirtualIotConfigRequest {
    pub controller_type: Option<String>,
    pub enabled_sensors: Option<Vec<String>>,
    pub sim_rate_ms: Option<i32>,
    pub event_definitions: Option<Vec<Value>>,
}

#[derive(Deserialize)]
pub struct CreateVirtualIotBackendRequest {
    pub name: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_controller")]
    pub controller_type: String,
    #[serde(default)]
    pub controller_types: Vec<String>,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_auto_start")]
    pub auto_start: bool,
    /// If set, provisioning (create/start) is performed through the remote
    /// ef-agent instead of the local VeerOsDriver, and the binding is
    /// persisted so subsequent lifecycle ops are proxied as well.
    #[serde(default)]
    pub agent_id: Option<Uuid>,
}

#[derive(Deserialize)]
pub struct UpdateVirtualIotBackendRequest {
    pub name: Option<String>,
    pub region: Option<String>,
    pub controller_types: Option<Vec<String>>,
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
    /// Optional free-form provenance tag (e.g., "manual", "event:Fever spike").
    /// When omitted, the backend picks a sensible default.
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Deserialize)]
pub struct ActuatorWriteRequest {
    /// GPIO pin number (0..=29 on the simulated ESP32-C6).
    pub pin: u32,
    /// Digital level to drive: 0 (low) or 1 (high).
    pub level: u32,
}

#[derive(Deserialize)]
pub struct ScriptRunRequest {
    /// Shell script source. Multi-line supported; each line executed by the
    /// VeerOS shell one at a time.
    pub source: String,
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
    let event_definitions_json = serde_json::to_value(&body.event_definitions).unwrap_or_else(|_| json!([]));
    let sensor_state_json = default_sensor_state(&body.enabled_sensors);
    let config = VirtualIotConfigRepo::create(
        &state.pool,
        device.id,
        &body.controller_type,
        &sensors_json,
        None,
        &json!({}),
        body.sim_rate_ms,
        &event_definitions_json,
        &sensor_state_json,
        &json!([]),
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

    let mut backends = VirtualIotBackendRepo::list(
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

    let runtime_statuses = join_all(backends.iter().map(|backend| {
        let veeros = state.veeros.clone();
        let pool = state.pool.clone();
        let vm_name = backend.vm_name.clone();
        let agent_id = backend.agent_id;
        let agent_base_url = backend.agent_base_url.clone();
        async move {
            let Some(name) = vm_name else { return String::from("unknown") };
            if let Some(aid) = agent_id {
                match crate::veeros_proxy::resolve_agent_base_url(&pool, aid, agent_base_url.as_deref()).await {
                    Ok(base) => crate::veeros_proxy::vm_status(&base, &name)
                        .await
                        .unwrap_or_else(|_| String::from("unknown")),
                    Err(_) => String::from("unknown"),
                }
            } else {
                VeerOsDriver::vm_status(&veeros, &name)
                    .await
                    .unwrap_or_else(|_| String::from("unknown"))
            }
        }
    }))
    .await;

    for (backend, runtime_status) in backends.iter_mut().zip(runtime_statuses) {
        if let Some(config) = backend.config.as_object_mut() {
            config.insert("runtime_status".into(), Value::String(runtime_status));
        }
    }

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

    let provider = normalize_backend_provider(&body.provider)
        .ok_or_else(|| ApiError::bad_request(format!(
            "Unsupported backend provider: {} (supported: veer-vm, veerm-vm, veeros-vm)",
            body.provider
        )))?;

    let seed = Uuid::new_v4().simple().to_string();
    let vm_name = VeerOsDriver::sanitize_vm_name(&body.name, &seed[..8]);
    let hostname = vm_name.replace('_', "-");

    // Resolve the agent base URL up-front when binding to an agent, so we
    // can short-circuit before any DB writes if the agent is unreachable.
    let agent_base_url = if let Some(aid) = body.agent_id {
        Some(
            crate::veeros_proxy::resolve_agent_base_url(&state.pool, aid, None)
                .await
                .map_err(|e| ApiError::bad_request(format!("Agent {aid} not usable: {e}")))?,
        )
    } else {
        None
    };

    let create_output = if let Some(base) = agent_base_url.as_deref() {
        crate::veeros_proxy::create_vm(
            base,
            &vm_name,
            &hostname,
            &state.veeros.default_target,
            state.veeros.default_memory_mb,
            &state.veeros.default_network_mode,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Agent create_vm failed: {e}")))?
    } else {
        VeerOsDriver::create_vm(
            &state.veeros,
            &vm_name,
            &hostname,
            &state.veeros.default_target,
            state.veeros.default_memory_mb,
            &state.veeros.default_network_mode,
        )
        .await
        .map_err(ApiError::from_domain)?
    };

    let mut status = "available".to_string();
    if body.auto_start {
        if let Some(base) = agent_base_url.as_deref() {
            crate::veeros_proxy::start_vm(base, &vm_name)
                .await
                .map_err(|e| ApiError::internal(format!("Agent start_vm failed: {e}")))?;
        } else {
            VeerOsDriver::start_vm(&state.veeros, &vm_name)
                .await
                .map_err(ApiError::from_domain)?;
        }
        status = "running".to_string();
    }

    // shell_info: when the VM lives on a remote agent, fetch the real host /
    // port / connect_command from the agent; the agent reports its own
    // localhost, so we rewrite the host to the agent's reachable hostname so
    // the recorded value is meaningful as diagnostic metadata. The actual
    // shell WS is proxied to the agent regardless of these fields.
    let shell = if let Some(base) = agent_base_url.as_deref() {
        match crate::veeros_proxy::shell_info(base, &vm_name).await {
            Ok((agent_host, port, cmd)) => {
                let public_host = extract_host(base).unwrap_or(agent_host);
                VeerOsShellInfo {
                    vm_name: vm_name.clone(),
                    host: public_host,
                    port,
                    connect_command: cmd,
                    sensor_feed_path: None,
                }
            }
            Err(e) => {
                tracing::warn!(vm_name = %vm_name, "agent shell_info failed: {e}");
                VeerOsShellInfo {
                    vm_name: vm_name.clone(),
                    host: extract_host(base).unwrap_or_else(|| "agent".to_string()),
                    port: 0,
                    connect_command: String::new(),
                    sensor_feed_path: None,
                }
            }
        }
    } else {
        VeerOsDriver::shell_info(&state.veeros, &vm_name)
            .await
            .map_err(ApiError::from_domain)?
    };
    let backend_config = backend_json(
        provider,
        &shell,
        &state.veeros.default_network_mode,
        &status,
    );
    let capabilities = vec![
        format!("provider:{provider}"),
        "runtime:folded-veerm-vm".to_string(),
        "microcm:secure".to_string(),
        format!("target:{}", state.veeros.default_target),
        "shell:veeros".to_string(),
    ];

    let backend = VirtualIotBackendRepo::create(
        &state.pool,
        &body.name,
        provider,
        &body.controller_type,
        &{
            let mut types = body.controller_types.clone();
            if types.is_empty() {
                types.push(body.controller_type.clone());
            } else if !types.contains(&body.controller_type) {
                types.insert(0, body.controller_type.clone());
            }
            types
        },
        &body.region,
        if body.auto_start { "available" } else { "maintenance" },
        Some(&vm_name),
        Some(&shell.host),
        Some(shell.port as i32),
        &backend_config,
        &capabilities,
        body.agent_id,
        agent_base_url.as_deref(),
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
    let event_definitions_json = match &body.event_definitions {
        Some(events) => serde_json::to_value(events).unwrap_or_else(|_| json!([])),
        None => existing.event_definitions.clone(),
    };

    let updated = VirtualIotConfigRepo::update(
        &state.pool,
        id,
        controller_type,
        &sensors_json,
        existing.backend_id,
        &existing.backend,
        sim_rate_ms,
        &event_definitions_json,
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

    if !backend.controller_types.contains(&config.controller_type) && backend.controller_type != config.controller_type {
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
    let backend = live_shell_backend(&state, id).await?;
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
    let config = VirtualIotConfigRepo::find_by_device_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    let mut sensor_state = config.sensor_state.as_object().cloned().unwrap_or_default();
    sensor_state.insert(body.sensor.clone(), Value::String(body.value.clone()));

    let mut sensor_log = config.sensor_log.as_array().cloned().unwrap_or_default();
    let source = body.source.clone().unwrap_or_else(|| {
        if config.backend_id.is_some() { "ui+vm".to_string() } else { "ui-simulated".to_string() }
    });
    sensor_log.push(json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "sensor": body.sensor,
        "value": body.value,
        "source": source,
    }));
    if sensor_log.len() > 200 {
        sensor_log.drain(0..sensor_log.len().saturating_sub(200));
    }

    let updated = VirtualIotConfigRepo::update_sensor_runtime(
        &state.pool,
        id,
        &Value::Object(sensor_state),
        &Value::Array(sensor_log.clone()),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to persist Virtual IoT sensor runtime state: {e}");
        ApiError::internal("Failed to persist Virtual IoT sensor runtime state")
    })?
    .ok_or_else(|| ApiError::not_found(format!("Virtual IoT config for device {id} not found")))?;

    let output = if let Ok(backend) = attached_backend(&state, id).await {
        if let Some(vm_name) = backend.vm_name.clone() {
            if let Some(base) = maybe_agent_base(&state, &backend).await? {
                Some(
                    crate::veeros_proxy::sensor_inject(&base, &vm_name, &body.sensor, &body.value)
                        .await
                        .map_err(|e| ApiError::internal(format!("Agent sensor_inject failed: {e}")))?,
                )
            } else {
                Some(
                    VeerOsDriver::sensor_inject(&state.veeros, &vm_name, &body.sensor, &body.value)
                        .await
                        .map_err(ApiError::from_domain)?,
                )
            }
        } else {
            None
        }
    } else {
        None
    };

    let _ = DeviceRepo::update_heartbeat(&state.pool, id).await;

    // Publish the sensor frame to all live-viewer WebSocket subscribers.
    let inject_source = body.source.clone().unwrap_or_else(|| {
        if updated.backend_id.is_some() { "ui-inject+vm".to_string() } else { "ui-inject".to_string() }
    });
    crate::sensor_bus::publish(
        &state.sensor_bus,
        id,
        crate::sensor_bus::SensorFrame {
            sensor: body.sensor.clone(),
            value: body.value.clone(),
            source: inject_source,
            ts: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(Json(json!({
        "device_id": id,
        "backend_id": updated.backend_id,
        "sensor": body.sensor,
        "value": body.value,
        "stdout": output.as_ref().map(|value| value.stdout.clone()).unwrap_or_default(),
        "sensor_state": updated.sensor_state,
        "sensor_log": updated.sensor_log,
    })))
}

/// POST /api/v1/devices/{id}/virtual-iot/actuators
///
/// Drive a GPIO pin on the attached VeerOS VM. Writes `gpio write <pin> <level>\n`
/// to the sensor FIFO; the VeerOS shell executes it and emits
/// `[actuator] gpio.<pin>=<level>` which is parsed by the shell-tail bridge and
/// re-broadcast on the sensor stream WebSocket as a `SensorFrame` with
/// `sensor: "gpio_<pin>"`.
pub async fn write_virtual_iot_actuator(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<ActuatorWriteRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(
        tenant_id = %auth.tenant_id,
        device_id = %id,
        pin = body.pin,
        level = body.level,
        "Writing virtual IoT actuator (gpio)"
    );

    let backend = attached_backend(&state, id).await.map_err(|_| {
        ApiError::bad_request(format!(
            "Device {id} has no running virtual-IoT backend attached"
        ))
    })?;
    let vm_name = backend.vm_name.clone().ok_or_else(|| {
        ApiError::bad_request(format!(
            "Attached backend for device {id} has no VM name"
        ))
    })?;

    let output = if let Some(base) = maybe_agent_base(&state, &backend).await? {
        // Remote agent path is not yet implemented; fall through for now.
        let _ = base;
        VeerOsDriver::actuator_gpio_write(&state.veeros, &vm_name, body.pin, body.level)
            .await
            .map_err(ApiError::from_domain)?
    } else {
        VeerOsDriver::actuator_gpio_write(&state.veeros, &vm_name, body.pin, body.level)
            .await
            .map_err(ApiError::from_domain)?
    };

    let _ = DeviceRepo::update_heartbeat(&state.pool, id).await;

    // Publish the actuator change directly onto the sensor bus so the Live
    // Viewer lights up immediately, regardless of whether anyone is tailing
    // the shell WS. The VM-echoed `[actuator]` line acts as a cross-check.
    crate::sensor_bus::publish(
        &state.sensor_bus,
        id,
        crate::sensor_bus::SensorFrame {
            sensor: format!("gpio_{}", body.pin),
            value: body.level.to_string(),
            source: "ui-actuator".to_string(),
            ts: chrono::Utc::now().to_rfc3339(),
        },
    );

    Ok(Json(json!({
        "device_id": id,
        "vm_name": vm_name,
        "pin": body.pin,
        "level": body.level,
        "stdout": output.stdout,
    })))
}

/// POST /api/v1/devices/{id}/virtual-iot/scripts/run
///
/// Upload a VeerOS shell script and execute it on the attached VM. The script
/// is streamed line-by-line into the sensor FIFO; the shell consumes each
/// line, its stdout flows back through the live log, and any `[actuator]`
/// sentinels produced by the script light up sensor tiles in the UI.
pub async fn run_virtual_iot_script(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<ScriptRunRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(
        tenant_id = %auth.tenant_id,
        device_id = %id,
        bytes = body.source.len(),
        "Running virtual IoT script"
    );

    if body.source.trim().is_empty() {
        return Err(ApiError::bad_request("Script source is empty"));
    }

    let backend = attached_backend(&state, id).await.map_err(|_| {
        ApiError::bad_request(format!(
            "Device {id} has no running virtual-IoT backend attached"
        ))
    })?;
    let vm_name = backend.vm_name.clone().ok_or_else(|| {
        ApiError::bad_request(format!(
            "Attached backend for device {id} has no VM name"
        ))
    })?;

    let output = VeerOsDriver::script_run(&state.veeros, &vm_name, &body.source)
        .await
        .map_err(ApiError::from_domain)?;

    let _ = DeviceRepo::update_heartbeat(&state.pool, id).await;

    Ok(Json(json!({
        "device_id": id,
        "vm_name": vm_name,
        "bytes": body.source.len(),
        "lines": body.source.lines().count(),
        "stdout": output.stdout,
    })))
}

// ── Sensor Stream WebSocket ────────────────────────────────────────────────

/// GET /api/v1/devices/{id}/virtual-iot/sensors/stream
///
/// Upgrades to WebSocket. Pushes `{"type":"sensor_frame","sensor":"...","value":"...","source":"...","ts":"..."}`
/// JSON text frames to all connected clients whenever any sensor is injected
/// (UI, scenario runner) or when the noise-model ticker fires.
pub async fn virtual_iot_sensor_stream_ws(
    ws: WebSocketUpgrade,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_sensor_stream(socket, id, state))
}

async fn handle_sensor_stream(socket: WebSocket, device_id: Uuid, state: AppState) {
    let mut rx = crate::sensor_bus::subscribe(&state.sensor_bus, device_id);
    let (mut sender, mut receiver) = socket.split();

    // Forward bus frames → WS client; drop on client disconnect.
    let mut forward = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(frame) => {
                    let msg = match serde_json::to_string(&serde_json::json!({
                        "type": "sensor_frame",
                        "sensor": frame.sensor,
                        "value": frame.value,
                        "source": frame.source,
                        "ts": frame.ts,
                    })) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    if sender.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(device_id = %device_id, skipped = n, "Sensor stream lagged — client too slow");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Drain incoming client messages (ping/close frames) so the connection stays alive.
    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            _ = &mut forward => break,
        }
    }
}

fn default_sensor_state(enabled_sensors: &[String]) -> Value {
    let map = enabled_sensors
        .iter()
        .map(|sensor| (sensor.clone(), Value::String(default_sensor_preview(sensor))))
        .collect::<serde_json::Map<_, _>>();
    Value::Object(map)
}

fn default_sensor_preview(sensor: &str) -> String {
    match sensor {
        "temperature" => "24.6 C".into(),
        "humidity" => "58 %".into(),
        "pressure" => "1012 hPa".into(),
        "air_quality_co2" => "612 ppm".into(),
        "air_quality_tvoc" => "0.41 ppm".into(),
        "accel_xyz" => "0.04, -0.02, 0.98 g".into(),
        "gyro_xyz" => "0.1, 0.0, -0.1 dps".into(),
        "magnetometer" => "12, -8, 31 uT".into(),
        "ambient_light" => "326 lux".into(),
        "uv_index" => "2.1".into(),
        "proximity" => "18 cm".into(),
        "gps" => "12.9716, 77.5946".into(),
        "battery_voltage" => "3.92 V".into(),
        "current" => "118 mA".into(),
        "gpio_states" => "GPIO4=1 GPIO5=0".into(),
        "adc_raw" => "1842".into(),
        "sound_level" => "43 dB".into(),
        "wifi_rssi" => "-58 dBm".into(),
        "ble_scan" => "[beacon-a]".into(),
        _ => "--".into(),
    }
}

pub async fn virtual_iot_shell_ws(
    ws: WebSocketUpgrade,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_virtual_iot_shell(socket, id, state))
}

async fn handle_virtual_iot_shell(socket: WebSocket, device_id: Uuid, state: AppState) {
    let backend = match live_shell_backend(&state, device_id).await {
        Ok(backend) => backend,
        Err(err) => {
            tracing::warn!(device_id = %device_id, "live_shell_backend failed: {:?}", err);
            return;
        }
    };

    if let Some(runtime_error) = backend_runtime_error(&backend) {
        tracing::warn!(
            device_id = %device_id,
            vm_name = ?backend.vm_name,
            agent_id = ?backend.agent_id,
            runtime_error = %runtime_error,
            "virtual IoT shell blocked by runtime_error"
        );
        let (mut ws_tx, _ws_rx) = socket.split();
        let _ = ws_tx.send(Message::Text(json!({
            "type": "error",
            "message": runtime_error,
        }).to_string().into())).await;
        return;
    }

    // When the backend was attached to a remote ef-agent, proxy the shell WS
    // to the agent's `/ws/veeros/{vm_name}/shell` endpoint. The agent runs the
    // provider-reported shell command on its own host. Otherwise, fall back to
    // spawning the provider-reported command locally.
    if let Some(agent_id) = backend.agent_id {
        proxy_shell_to_agent(socket, device_id, agent_id, backend, state).await;
    } else {
        local_shell_session(socket, device_id, backend, state).await;
    }
}

/// Relay the dashboard ↔ agent shell WebSocket. The agent owns `veer-connect`
/// and the VM's port, so this host only has to copy bytes each way.
async fn proxy_shell_to_agent(
    socket: WebSocket,
    device_id: Uuid,
    agent_id: Uuid,
    backend: VirtualIotBackendRow,
    state: AppState,
) {
    use tokio_tungstenite::tungstenite::Message as TsMsg;

    let (mut ws_tx, mut ws_rx) = socket.split();

    let vm_name = match backend.vm_name.clone() {
        Some(n) if !n.is_empty() => n,
        _ => {
            let _ = ws_tx.send(Message::Text(json!({
                "type": "error",
                "message": "Backend has no vm_name; cannot open shell",
            }).to_string().into())).await;
            return;
        }
    };

    let base = match crate::veeros_proxy::resolve_agent_base_url(
        &state.pool,
        agent_id,
        backend.agent_base_url.as_deref(),
    ).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(%device_id, %agent_id, "resolve agent base url failed: {e}");
            let _ = ws_tx.send(Message::Text(json!({
                "type": "error",
                "message": format!("Agent unreachable: {e}"),
            }).to_string().into())).await;
            return;
        }
    };

    // http(s)://host:port  →  ws(s)://host:port/ws/veeros/{vm_name}/shell
    let ws_base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        format!("ws://{base}")
    };
    let agent_url = format!("{}/ws/veeros/{}/shell", ws_base.trim_end_matches('/'), vm_name);

    tracing::info!(%device_id, %agent_id, %agent_url, "proxying VeerOS shell WS to agent");

    let (agent_ws, _resp) = match tokio_tungstenite::connect_async(&agent_url).await {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(%device_id, %agent_url, "agent shell WS connect failed: {e}");
            let _ = ws_tx.send(Message::Text(json!({
                "type": "error",
                "message": format!("Agent shell unreachable: {e}"),
            }).to_string().into())).await;
            return;
        }
    };

    // Surface the `shell_ready` banner the browser expects so the xterm page
    // moves out of the "Connecting…" state immediately. The agent may also
    // emit its own banner; the dashboard filters any leading shell_ready JSON.
    let _ = ws_tx.send(Message::Text(json!({
        "type": "shell_ready",
        "device_id": device_id,
        "host": backend.shell_host,
        "port": backend.shell_port,
        "via": "agent",
        "agent_id": agent_id,
    }).to_string().into())).await;

    let (mut agent_tx, mut agent_rx) = agent_ws.split();

    // agent → dashboard
    let a2d = tokio::spawn(async move {
        while let Some(msg) = agent_rx.next().await {
            let relay = match msg {
                Ok(TsMsg::Text(t)) => Some(Message::Text(t.to_string().into())),
                Ok(TsMsg::Binary(b)) => Some(Message::Binary(b.into())),
                Ok(TsMsg::Close(_)) | Err(_) => break,
                _ => None,
            };
            if let Some(m) = relay {
                if ws_tx.send(m).await.is_err() {
                    break;
                }
            }
        }
    });

    // dashboard → agent
    let d2a = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            let relay = match msg {
                Message::Text(t) => Some(TsMsg::Text(t.to_string().into())),
                Message::Binary(b) => Some(TsMsg::Binary(b.into())),
                Message::Close(_) => break,
                _ => None,
            };
            if let Some(m) = relay {
                if agent_tx.send(m).await.is_err() {
                    break;
                }
            }
        }
        let _ = agent_tx.close().await;
    });

    tokio::select! {
        _ = a2d => {},
        _ = d2a => {},
    }

    tracing::info!(%device_id, "agent shell proxy ended");
}

/// Legacy single-host path: spawn `veer-connect` locally and pipe it over the
/// dashboard's WebSocket. Used when the backend has no `agent_id`.
async fn local_shell_session(
    socket: WebSocket,
    device_id: Uuid,
    backend: VirtualIotBackendRow,
    state: AppState,
) {
    if let Some(fifo_path) = backend
        .config
        .get("sensor_feed_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        // For folded VMs we always stream the fold log directly via
        // `tail -n 0 -F <log_path>` so reconnects don't replay historical
        // output. Any stored `connect_command` (e.g. a stale `fold logs -f`)
        // is intentionally ignored here — it points to the wrong binary on
        // most systems (/usr/bin/fold is GNU coreutils, not VeerOS fold).
        let vm_name = backend.vm_name.clone().unwrap_or_default();
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let log_path = format!("{home}/.local/state/veeros/fold/{vm_name}.log");
        let connect_command = format!("tail -n 0 -F {}", log_path);
        local_folded_shell_session(
            socket,
            device_id,
            &connect_command,
            fifo_path,
            state.sensor_bus.clone(),
        )
        .await;
        return;
    }

    let host = backend.shell_host.clone().unwrap_or_else(|| "localhost".into());
    let port = backend.shell_port.unwrap_or_default().to_string();
    let mut child = match spawn_backend_shell_command(&state, &backend) {
        Ok(child) => child,
        Err(e) => {
            tracing::error!(device_id = %device_id, "Failed to spawn backend shell command: {e}");
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

    // Funnel stdout + stderr chunks through an mpsc channel so the WebSocket sink
    // is driven by a single writer task. Holding a Mutex across `.send().await`
    // (the previous design) caused stdout to stall behind stderr under even
    // slight back-pressure, which made the terminal appear to hang after a few
    // commands.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();
    let out_tx_stdout = out_tx.clone();
    let out_tx_stderr = out_tx.clone();
    drop(out_tx);

    let writer_task = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
        let _ = ws_tx.close().await;
    });

    let stdout_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match child_stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    if out_tx_stdout.send(Message::Text(chunk.into())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // Forward veer-connect's stderr progress ([vsc] …) and errors to the client so
    // handshake failures and prompts are visible in the browser terminal.
    let stderr_task = tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match child_stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    // Wrap stderr in ANSI dim so it's visually distinct from the shell.
                    let decorated = format!("\x1b[2m{}\x1b[0m", chunk);
                    if out_tx_stderr.send(Message::Text(decorated.into())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Text(text) => {
                if child_stdin.write_all(text.as_bytes()).await.is_err() {
                    break;
                }
                let _ = child_stdin.flush().await;
            }
            Message::Binary(data) => {
                if child_stdin.write_all(&data).await.is_err() {
                    break;
                }
                let _ = child_stdin.flush().await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let _ = child.kill().await;
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    let _ = writer_task.await;
}

/// Parse a single line of VeerOS shell output and, if it matches the
/// `[actuator] <kind>.<key>=<value>` sentinel format, publish a SensorFrame
/// on the per-device bus. Silently ignores non-matching lines.
///
/// Supported kinds today: `gpio`. Example line:
///   `[actuator] gpio.15=1`
/// becomes frame `{ sensor: "gpio_15", value: "1", source: "fold-vm" }`.
fn parse_and_publish_actuator(
    line: &[u8],
    device_id: Uuid,
    sensor_bus: &crate::sensor_bus::SensorBus,
) {
    const PREFIX: &[u8] = b"[actuator] ";
    // Fast reject: every line scanned; must not allocate on the hot path.
    if line.len() < PREFIX.len() + 3 {
        return;
    }
    // Allow arbitrary leading whitespace produced by the serial console.
    let trimmed = match line.iter().position(|b| !b.is_ascii_whitespace()) {
        Some(i) => &line[i..],
        None => return,
    };
    if !trimmed.starts_with(PREFIX) {
        return;
    }
    let rest = &trimmed[PREFIX.len()..];
    let s = match core::str::from_utf8(rest) {
        Ok(s) => s.trim(),
        Err(_) => return,
    };
    // Expected shape: `<kind>.<key>=<value>`
    let (lhs, value) = match s.split_once('=') {
        Some(pair) => pair,
        None => return,
    };
    let (kind, key) = match lhs.split_once('.') {
        Some(pair) => pair,
        None => return,
    };
    let kind = kind.trim();
    let key = key.trim();
    let value = value.trim();
    if kind.is_empty() || key.is_empty() || value.is_empty() {
        return;
    }
    // Namespace the sensor key per-kind so the dashboard can route widgets.
    // gpio -> `gpio_15`; future: pwm -> `pwm_2`, dac -> `dac_0`, etc.
    let sensor_name = match kind {
        "gpio" => format!("gpio_{}", key),
        _ => format!("{}_{}", kind, key),
    };
    crate::sensor_bus::publish(
        sensor_bus,
        device_id,
        crate::sensor_bus::SensorFrame {
            sensor: sensor_name,
            value: value.to_string(),
            source: "fold-vm".to_string(),
            ts: chrono::Utc::now().to_rfc3339(),
        },
    );
}

async fn local_folded_shell_session(
    socket: WebSocket,
    device_id: Uuid,
    logs_command: &str,
    fifo_path: &str,
    sensor_bus: crate::sensor_bus::SensorBus,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let _ = ws_tx.send(Message::Text(json!({
        "type": "shell_ready",
        "device_id": device_id,
        "host": "localhost",
        "port": "folded-console",
        "hint": "Using folded console fallback until DHCP/TCP shell is available"
    }).to_string().into())).await;

    let _ = ws_tx
        .send(Message::Text(
            "\r\n[EdgeFabric] Connected to VeerOS folded runtime (root shell, no login).\r\n".into(),
        ))
        .await;

    let mut parts = logs_command.split_whitespace();
    let program = match parts.next() {
        Some(program) => program.to_string(),
        None => {
            let _ = ws_tx.send(Message::Text("error: empty logs command\n".into())).await;
            return;
        }
    };
    let args: Vec<String> = parts.map(|part| part.to_string()).collect();

    let mut child = match Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = ws_tx.send(Message::Text(format!("spawn failed: {error}\n").into())).await;
            return;
        }
    };

    let mut stdout = child.stdout.take();
    let fifo_path = fifo_path.to_string();
    let fifo_path_for_stdout = fifo_path.clone();

    let (fifo_ready_tx, mut fifo_ready_rx) = tokio::sync::mpsc::unbounded_channel::<Result<(), String>>();

    let fifo_writer = tokio::spawn(async move {
        use std::os::unix::fs::OpenOptionsExt;

        let mut attempts = 0u8;
        let mut file = loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo_path)
            {
                Ok(file) => break Some(tokio::fs::File::from_std(file)),
                Err(_) if attempts < 10 => {
                    attempts = attempts.saturating_add(1);
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                }
                Err(_) => break None,
            }
        };

        let Some(mut file) = file.take() else {
            let _ = fifo_ready_tx.send(Err(format!(
                "cannot open sensor FIFO {} for shell input (VM may still be booting)",
                fifo_path
            )));
            return;
        };
        let _ = fifo_ready_tx.send(Ok(()));
        // Send a single newline so the VM redraws a fresh prompt line for
        // this session. Using "\n" (not "\r") avoids appending a blank
        // command to the shell's history. We do NOT write anything else on
        // connect — the VeerOS shell has no login gate; it's already root.
        let _ = tokio::io::AsyncWriteExt::write_all(&mut file, b"\n").await;

        while let Some(Ok(msg)) = ws_rx.next().await {
            let bytes = match msg {
                Message::Text(text) => text.as_bytes().to_vec(),
                Message::Binary(binary) => binary.to_vec(),
                Message::Close(_) => break,
                _ => continue,
            };
            if tokio::io::AsyncWriteExt::write_all(&mut file, &bytes).await.is_err() {
                break;
            }
        }
    });

    let _ = fifo_path_for_stdout; // legacy: previously used for forced-logout hack
    let stdout_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        // Line accumulator: `[actuator]` sentinel lines emitted by the VeerOS
        // shell (feature `gpio-sim`) are parsed out and republished on the
        // per-device SensorBus so the dashboard Live Viewer reflects GPIO
        // state changes in real time. Format: `[actuator] gpio.<pin>=<level>`.
        let mut line_buf: Vec<u8> = Vec::with_capacity(512);
        if let Some(out) = stdout.as_mut() {
            loop {
                match out.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        // Feed incoming bytes into the line accumulator; dispatch
                        // one scan per newline boundary.
                        for &byte in &buf[..n] {
                            if byte == b'\n' {
                                parse_and_publish_actuator(&line_buf, device_id, &sensor_bus);
                                line_buf.clear();
                            } else if byte != b'\r' {
                                // Guard against runaway growth on a stuck stream.
                                if line_buf.len() < 4096 {
                                    line_buf.push(byte);
                                }
                            }
                        }
                        if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        ws_tx
    });

    if let Some(fifo_result) = fifo_ready_rx.recv().await {
        if let Err(msg) = fifo_result {
            if let Ok(mut tx) = stdout_task.await {
                let _ = tx
                    .send(Message::Text(format!("\r\n[EdgeFabric] {msg}\r\n").into()))
                    .await;
                let _ = tx.send(Message::Close(None)).await;
            }
            let _ = child.kill().await;
            return;
        }
    }

    let _ = child.wait().await;
    fifo_writer.abort();
    if let Ok(mut ws_tx) = stdout_task.await {
        let _ = ws_tx.send(Message::Close(None)).await;
    }
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

async fn live_shell_backend(
    state: &AppState,
    device_id: Uuid,
) -> Result<VirtualIotBackendRow, (StatusCode, Json<ApiError>)> {
    let backend = attached_backend(state, device_id).await?;
    hydrate_live_shell_backend(state, backend).await
}

async fn hydrate_live_shell_backend(
    state: &AppState,
    mut backend: VirtualIotBackendRow,
) -> Result<VirtualIotBackendRow, (StatusCode, Json<ApiError>)> {
    let Some(vm_name) = backend.vm_name.clone() else {
        return Ok(backend);
    };

    let agent_base = maybe_agent_base(state, &backend).await?;
    let runtime_status = if let Some(base) = agent_base.as_deref() {
        crate::veeros_proxy::vm_status(base, &vm_name)
            .await
            .unwrap_or_else(|_| String::from("unknown"))
    } else {
        VeerOsDriver::vm_status(&state.veeros, &vm_name)
            .await
            .unwrap_or_else(|_| String::from("unknown"))
    };

    let shell_result = if let Some(base) = agent_base.as_deref() {
        crate::veeros_proxy::shell_info(base, &vm_name)
            .await
            .map(|(host, port, connect_command)| VeerOsShellInfo {
                vm_name: vm_name.clone(),
                host: extract_host(base).unwrap_or(host),
                port,
                connect_command,
                sensor_feed_path: None, // agent handles this internally
            })
            .map_err(|e| format!("Agent shell_info failed: {e}"))
    } else {
        VeerOsDriver::shell_info(&state.veeros, &vm_name)
            .await
            .map_err(|e| e.to_string())
    };

    if let Some(config) = backend.config.as_object_mut() {
        config.insert("runtime_status".into(), Value::String(runtime_status));
        config.remove("runtime_error");
    }

    match shell_result {
        Ok(shell) => {
            backend.shell_host = Some(shell.host.clone());
            backend.shell_port = Some(i32::from(shell.port));
            if let Some(config) = backend.config.as_object_mut() {
                config.insert("shell_host".into(), Value::String(shell.host.clone()));
                config.insert("shell_port".into(), Value::from(shell.port));
                config.insert("connect_command".into(), Value::String(shell.connect_command.clone()));
                if let Some(sensor_feed_path) = shell.sensor_feed_path.clone() {
                    config.insert("sensor_feed_path".into(), Value::String(sensor_feed_path));
                } else {
                    config.remove("sensor_feed_path");
                }
            }
        }
        Err(error) => {
            tracing::warn!(
                backend_id = %backend.id,
                vm_name = %vm_name,
                agent_id = ?backend.agent_id,
                shell_error = %error,
                "virtual IoT shell_info failed"
            );
            if let Some(config) = backend.config.as_object_mut() {
                config.insert("runtime_error".into(), Value::String(error));
            }
        }
    }

    Ok(backend)
}

fn backend_runtime_error(backend: &VirtualIotBackendRow) -> Option<String> {
    backend
        .config
        .get("runtime_error")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|msg| !msg.is_empty())
        .map(str::to_string)
}

fn backend_json(provider: &str, shell: &VeerOsShellInfo, network_mode: &str, runtime_status: &str) -> Value {
    json!({
        "provider": provider,
        "runtime": "folded-veerm-vm",
        "runtime_family": "microcm",
        "runtime_security": "secure",
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

fn spawn_backend_shell_command(
    state: &AppState,
    backend: &VirtualIotBackendRow,
) -> Result<tokio::process::Child, std::io::Error> {
    let connect_command = backend
        .config
        .get("connect_command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let host = backend.shell_host.clone().unwrap_or_else(|| "localhost".into());
            let port = backend.shell_port.unwrap_or_default().to_string();
            format!("{} shell {} {}", resolve_veer_connect_path(state), host, port)
        });

    let mut parts = connect_command.split_whitespace();
    let program = parts.next().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "backend connect_command is empty")
    })?;

    Command::new(program)
        .current_dir(&state.veeros.root_dir)
        .env("TERM", "xterm-256color")
        .args(parts)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
}

/// Extract just the host portion from an `http(s)://host:port/...` URL.
/// Used to rewrite a VM's shell host to the agent's reachable hostname.
fn extract_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let rest = rest.split('/').next().unwrap_or(rest);
    let host = rest.split(':').next().unwrap_or(rest).trim();
    if host.is_empty() { None } else { Some(host.to_string()) }
}

/// If the backend is bound to a remote ef-agent, resolve its base URL.
/// Returns `Ok(Some(url))` when proxying should be used, `Ok(None)` when
/// the caller should fall back to the local `VeerOsDriver`.
async fn maybe_agent_base(
    state: &AppState,
    backend: &VirtualIotBackendRow,
) -> Result<Option<String>, (StatusCode, Json<ApiError>)> {
    let Some(agent_id) = backend.agent_id else { return Ok(None) };
    let base = crate::veeros_proxy::resolve_agent_base_url(
        &state.pool,
        agent_id,
        backend.agent_base_url.as_deref(),
    )
    .await
    .map_err(|e| ApiError::internal(format!("Agent {agent_id} not reachable: {e}")))?;
    Ok(Some(base))
}

/// DELETE /api/v1/devices/virtual-iot/backends/:id
pub async fn delete_virtual_iot_backend(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, backend_id = %id, "Deleting virtual IoT backend");

    let backend = VirtualIotBackendRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Backend {id} not found")))?;

    if let Some(vm_name) = &backend.vm_name {
        if let Some(base) = maybe_agent_base(&state, &backend).await? {
            let _ = crate::veeros_proxy::stop_vm(&base, vm_name).await;
            let _ = crate::veeros_proxy::destroy_vm(&base, vm_name).await;
        } else {
            let _ = VeerOsDriver::stop_vm(&state.veeros, vm_name).await;
            let _ = VeerOsDriver::destroy_vm(&state.veeros, vm_name).await;
        }
    }

    let deleted = VirtualIotBackendRepo::delete(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete backend: {e}");
            ApiError::internal("Failed to delete backend")
        })?;

    if !deleted {
        return Err(ApiError::not_found(format!("Backend {id} not found")));
    }

    Ok(Json(json!({ "deleted": true, "id": id })))
}

/// POST /api/v1/devices/virtual-iot/backends/:id/start
pub async fn start_virtual_iot_backend(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, backend_id = %id, "Starting virtual IoT backend");

    let backend = VirtualIotBackendRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Backend {id} not found")))?;

    let vm_name = backend.vm_name.as_deref()
        .ok_or_else(|| ApiError::bad_request("Backend has no VM binding"))?;

    if let Some(base) = maybe_agent_base(&state, &backend).await? {
        crate::veeros_proxy::start_vm(&base, vm_name)
            .await
            .map_err(|e| ApiError::internal(format!("Agent start_vm failed: {e}")))?;
    } else {
        VeerOsDriver::start_vm(&state.veeros, vm_name)
            .await
            .map_err(ApiError::from_domain)?;
    }

    let next_status = if backend.attached_device_id.is_some() { "busy" } else { "available" };

    let updated = VirtualIotBackendRepo::update_status_and_attachment(&state.pool, id, next_status, backend.attached_device_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update backend status: {e}");
            ApiError::internal("Failed to update backend status")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Backend {id} not found")))?;

    Ok(Json(json!({ "backend": updated })))
}

/// POST /api/v1/devices/virtual-iot/backends/:id/stop
pub async fn stop_virtual_iot_backend(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, backend_id = %id, "Stopping virtual IoT backend");

    let backend = VirtualIotBackendRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Backend {id} not found")))?;

    let vm_name = backend.vm_name.as_deref()
        .ok_or_else(|| ApiError::bad_request("Backend has no VM binding"))?;

    if let Some(base) = maybe_agent_base(&state, &backend).await? {
        crate::veeros_proxy::stop_vm(&base, vm_name)
            .await
            .map_err(|e| ApiError::internal(format!("Agent stop_vm failed: {e}")))?;
    } else {
        VeerOsDriver::stop_vm(&state.veeros, vm_name)
            .await
            .map_err(ApiError::from_domain)?;
    }

    let next_status = if backend.attached_device_id.is_some() { "busy" } else { "available" };

    let updated = VirtualIotBackendRepo::update_status_and_attachment(&state.pool, id, next_status, backend.attached_device_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update backend status: {e}");
            ApiError::internal("Failed to update backend status")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Backend {id} not found")))?;

    Ok(Json(json!({ "backend": updated })))
}

/// PATCH /api/v1/devices/virtual-iot/backends/:id
pub async fn update_virtual_iot_backend(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<UpdateVirtualIotBackendRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, backend_id = %id, "Updating virtual IoT backend");

    let updated = VirtualIotBackendRepo::update(
        &state.pool,
        id,
        body.name.as_deref(),
        body.region.as_deref(),
        body.controller_types.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to update backend: {e}");
        ApiError::internal("Failed to update backend")
    })?
    .ok_or_else(|| ApiError::not_found(format!("Backend {id} not found")))?;

    Ok(Json(json!({ "backend": updated })))
}

// ── Synthetic sensor simulation toggle ───────────────────────────────────────
//
// The periodic noise-model sensor ticker is disabled by default so the
// dashboard stays quiet. Users opt in on demand via these endpoints.
//
//   GET  /api/v1/virtual-iot/simulation           → { "enabled": bool }
//   POST /api/v1/virtual-iot/simulation/start     → enable the ticker
//   POST /api/v1/virtual-iot/simulation/stop      → disable the ticker
//
// All handlers require an authenticated caller (any tenant).

/// GET /api/v1/virtual-iot/simulation
pub async fn get_simulation_state(
    _auth: AuthContext,
) -> Json<Value> {
    Json(json!({
        "enabled": crate::virtual_iot_runtime::simulation_enabled(),
    }))
}

/// POST /api/v1/virtual-iot/simulation/start
pub async fn start_simulation(
    auth: AuthContext,
) -> Json<Value> {
    let was = crate::virtual_iot_runtime::set_simulation_enabled(true);
    tracing::info!(tenant_id = %auth.tenant_id, was_enabled = was, "Synthetic sensor simulation ENABLED");
    Json(json!({ "enabled": true, "previously_enabled": was }))
}

/// POST /api/v1/virtual-iot/simulation/stop
pub async fn stop_simulation(
    auth: AuthContext,
) -> Json<Value> {
    let was = crate::virtual_iot_runtime::set_simulation_enabled(false);
    tracing::info!(tenant_id = %auth.tenant_id, was_enabled = was, "Synthetic sensor simulation DISABLED");
    Json(json!({ "enabled": false, "previously_enabled": was }))
}
