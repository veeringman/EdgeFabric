use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;
use ef_db::repo::device::{DeviceRepo, DeviceRow};
use ef_db::repo::runner::RunnerRepo;
use ef_scheduler::{RunnerFilter, Scheduler};

#[derive(Debug, Deserialize)]
pub struct RegisterRunnerRequest {
    pub name: String,
    pub host: Option<String>,
    pub port: Option<i32>,
    #[serde(default = "default_runner_class")]
    pub runner_class: String,
    #[serde(default = "default_host_os")]
    pub host_os: String,
    #[serde(default)]
    pub architecture: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: i32,
    pub device_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct DrainRunnerRequest {
    #[serde(default = "default_true")]
    pub drained: bool,
}

#[derive(Debug, Deserialize)]
pub struct ProbeRunnerRequest {
    pub ssh_user: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AllocateRunnerRequest {
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub required_labels: Vec<String>,
    #[serde(default)]
    pub lease_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub build_job_id: Option<Uuid>,
    #[serde(default)]
    pub target_device_id: Option<Uuid>,
}

fn default_port() -> i32 { 22 }
fn default_runner_class() -> String { "generic".to_string() }
fn default_host_os() -> String { "linux".to_string() }
fn default_region() -> String { "default".to_string() }
fn default_max_concurrency() -> i32 { 1 }
fn default_true() -> bool { true }

fn parse_host_port_from_serial(serial: &str) -> Option<(String, i32)> {
    let s = serial.trim();
    if s.is_empty() {
        return None;
    }

    if let Some((h, p)) = s.rsplit_once(':') {
        if !h.is_empty() {
            if let Ok(port) = p.parse::<i32>() {
                if (1..=65535).contains(&port) {
                    return Some((h.to_string(), port));
                }
            }
        }
    }

    Some((s.to_string(), 22))
}

fn derive_host_port_from_device(device: &DeviceRow) -> Option<(String, i32)> {
    if let Some(serial) = &device.serial {
        if let Some(hp) = parse_host_port_from_serial(serial) {
            return Some(hp);
        }
    }

    if let Some(ip) = &device.ip_address {
        let h = ip.trim();
        if !h.is_empty() {
            return Some((h.to_string(), 22));
        }
    }

    None
}

fn shorten(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    format!("{}...", &s[..max_len])
}

async fn ssh_exec_with_port(host: &str, port: i32, user: &str, script: &str) -> Value {
    let target = format!("{user}@{host}");
    let port_s = port.to_string();
    match timeout(
        Duration::from_secs(10),
        Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=accept-new",
                "-o",
                "ConnectTimeout=8",
                "-o",
                "BatchMode=yes",
                "-p",
                &port_s,
                &target,
                "sh",
                "-lc",
                script,
            ])
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => {
            let code = output.status.code().unwrap_or(-1);
            json!({
                "ok": code == 0,
                "exit_code": code,
                "stdout": shorten(&String::from_utf8_lossy(&output.stdout), 200),
                "stderr": shorten(&String::from_utf8_lossy(&output.stderr), 200),
            })
        }
        Ok(Err(e)) => json!({
            "ok": false,
            "exit_code": -1,
            "stdout": "",
            "stderr": shorten(&e.to_string(), 200),
        }),
        Err(_) => json!({
            "ok": false,
            "exit_code": -1,
            "stdout": "",
            "stderr": "ssh probe timeout",
        }),
    }
}

pub async fn register_runner(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<RegisterRunnerRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    if body.name.trim().is_empty() {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    if body.max_concurrency <= 0 {
        return Err(ApiError::bad_request("max_concurrency must be > 0"));
    }

    let linked_device = if let Some(device_id) = body.device_id {
        DeviceRepo::find_by_id(&state.pool, device_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to fetch linked device");
                ApiError::internal("Database error")
            })?
            .ok_or_else(|| ApiError::bad_request(format!("device_id {device_id} not found")))
            .map(Some)?
    } else {
        None
    };

    let provided_host = body
        .host
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(ToString::to_string);

    let mut port = body.port.unwrap_or(default_port());
    let host = if let Some(h) = provided_host {
        h
    } else if let Some(device) = linked_device.as_ref() {
        let (h, p) = derive_host_port_from_device(device).ok_or_else(|| {
            ApiError::bad_request(
                "Could not derive host from linked device; provide host explicitly or ensure device serial/ip is set",
            )
        })?;
        if body.port.is_none() {
            port = p;
        }
        h
    } else {
        return Err(ApiError::bad_request(
            "host must be provided or device_id must reference a discovered device",
        ));
    };

    if port <= 0 || port > 65535 {
        return Err(ApiError::bad_request("port must be in 1..65535"));
    }

    let mut merged_capabilities = body.capabilities.clone();
    if let Some(device) = linked_device.as_ref() {
        for cap in &device.capabilities {
            if !merged_capabilities.iter().any(|c| c == cap) {
                merged_capabilities.push(cap.clone());
            }
        }
        if !merged_capabilities.iter().any(|c| c == "ssh") {
            merged_capabilities.push("ssh".to_string());
        }
    }
    merged_capabilities.sort();
    merged_capabilities.dedup();

    let runner = RunnerRepo::create(
        &state.pool,
        auth.tenant_id,
        body.device_id,
        body.name.trim(),
        body.runner_class.trim(),
        &host,
        port,
        body.host_os.trim(),
        body.architecture.trim(),
        body.region.trim(),
        &body.labels,
        &merged_capabilities,
        body.max_concurrency,
    )
    .await
    .map_err(|e| {
        tracing::error!(tenant_id = %auth.tenant_id, error = %e, "Failed to register runner");
        ApiError::internal("Failed to register runner")
    })?;

    Ok((StatusCode::CREATED, Json(json!({ "runner": runner }))))
}

pub async fn list_runners(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let runners = RunnerRepo::list_by_tenant(&state.pool, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %e, "Failed to list runners");
            ApiError::internal("Failed to list runners")
        })?;

    Ok(Json(json!({ "runners": runners, "total": runners.len() })))
}

pub async fn get_runner(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let runner = RunnerRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch runner");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Runner {id} not found")))?;

    if runner.tenant_id != Some(auth.tenant_id) {
        return Err(ApiError::not_found(format!("Runner {id} not found")));
    }

    Ok(Json(json!({ "runner": runner })))
}

pub async fn drain_runner(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<DrainRunnerRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let existing = RunnerRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch runner");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Runner {id} not found")))?;

    if existing.tenant_id != Some(auth.tenant_id) {
        return Err(ApiError::not_found(format!("Runner {id} not found")));
    }

    let runner = RunnerRepo::set_drained(&state.pool, id, body.drained)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update runner drained state");
            ApiError::internal("Failed to update runner")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Runner {id} not found")))?;

    Ok(Json(json!({
        "runner": runner,
        "message": if body.drained { "Runner drained" } else { "Runner resumed" }
    })))
}

pub async fn probe_runner(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    body: Option<Json<ProbeRunnerRequest>>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let runner = RunnerRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch runner");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Runner {id} not found")))?;

    if runner.tenant_id != Some(auth.tenant_id) {
        return Err(ApiError::not_found(format!("Runner {id} not found")));
    }

    let target = format!("{}:{}", runner.host, runner.port);
    let reachable = timeout(Duration::from_secs(3), TcpStream::connect(&target))
        .await
        .map(|res| res.is_ok())
        .unwrap_or(false);

    let requested_user = body
        .as_ref()
        .and_then(|b| b.ssh_user.as_ref())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let users: Vec<String> = if let Some(u) = requested_user.clone() {
        vec![u]
    } else {
        vec![
            "builder".to_string(),
            "runner".to_string(),
            "admin".to_string(),
            "root".to_string(),
            "user".to_string(),
        ]
    };

    let mut auth_attempts = Vec::new();
    let mut ssh_authenticated_as: Option<String> = None;
    let mut tool_checks = Vec::new();
    let mut detected_capabilities = runner.capabilities.clone();
    let mut detected_host_os = runner.host_os.clone();

    if reachable {
        for u in &users {
            let out = ssh_exec_with_port(&runner.host, runner.port, u, "echo edgefabric_probe_ok").await;
            let ok = out
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
                && out
                    .get("stdout")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .contains("edgefabric_probe_ok");

            auth_attempts.push(json!({
                "user": u,
                "ok": ok,
                "stderr": out.get("stderr").cloned().unwrap_or(json!("")),
            }));

            if ok {
                ssh_authenticated_as = Some(u.clone());
                break;
            }
        }
    }

    if let Some(user) = ssh_authenticated_as.clone() {
        let os_out = ssh_exec_with_port(&runner.host, runner.port, &user, "uname -s").await;
        let os_name = os_out
            .get("stdout")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        if os_name.contains("darwin") {
            detected_host_os = "macos".to_string();
        } else if os_name.contains("linux") {
            detected_host_os = "linux".to_string();
        }

        let checks = [
            ("ssh", "command -v ssh >/dev/null 2>&1"),
            ("cicd_runner", "command -v sh >/dev/null 2>&1"),
            ("linux_runner", "uname -s | grep -qi linux"),
            ("xcode", "command -v xcodebuild >/dev/null 2>&1 && xcodebuild -version | head -n 2"),
            ("simctl", "command -v xcrun >/dev/null 2>&1 && xcrun simctl help >/dev/null 2>&1"),
            ("fastlane", "command -v fastlane >/dev/null 2>&1 && fastlane --version | head -n 1"),
            ("codesign", "security find-identity -v -p codesigning 2>/dev/null | head -n 5"),
            ("cocoapods", "command -v pod >/dev/null 2>&1 && pod --version | head -n 1"),
            ("swiftpm", "command -v swift >/dev/null 2>&1 && swift --version | head -n 1"),
            ("iot_toolchain", "command -v pio >/dev/null 2>&1 || command -v idf.py >/dev/null 2>&1 || command -v cmake >/dev/null 2>&1"),
            ("esp32", "command -v idf.py >/dev/null 2>&1 || command -v pio >/dev/null 2>&1 || command -v esptool.py >/dev/null 2>&1"),
            ("rpi", "command -v cmake >/dev/null 2>&1 || command -v make >/dev/null 2>&1"),
            ("iot_deploy", "command -v scp >/dev/null 2>&1 || command -v rsync >/dev/null 2>&1"),
            ("quantum_sdk", "command -v python3 >/dev/null 2>&1 && python3 -c 'import qiskit' >/dev/null 2>&1"),
            ("quantum_simulator", "command -v python3 >/dev/null 2>&1 && python3 -c 'import qiskit_aer' >/dev/null 2>&1"),
            ("veeros_host", "command -v veerctl >/dev/null 2>&1 || command -v virsh >/dev/null 2>&1 || command -v qemu-system-x86_64 >/dev/null 2>&1"),
            ("artifact_upload", "command -v scp >/dev/null 2>&1"),
            ("workspace_bootstrap", "command -v git >/dev/null 2>&1"),
        ];

        for (capability, cmd) in checks {
            let out = ssh_exec_with_port(&runner.host, runner.port, &user, cmd).await;
            let ok = out
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if ok && !detected_capabilities.iter().any(|c| c == capability) {
                detected_capabilities.push(capability.to_string());
            }

            tool_checks.push(json!({
                "capability": capability,
                "status": if ok { "pass" } else { "fail" },
                "evidence": {
                    "stdout": out.get("stdout").cloned().unwrap_or(json!("")),
                    "stderr": out.get("stderr").cloned().unwrap_or(json!("")),
                    "exit_code": out.get("exit_code").cloned().unwrap_or(json!(-1)),
                }
            }));
        }
    }

    detected_capabilities.sort();
    detected_capabilities.dedup();

    let probe_result = json!({
        "target": target,
        "reachable": reachable,
        "probe_type": "tcp_connect+ssh_toolchain",
        "requested_user": requested_user,
        "ssh_authenticated_as": ssh_authenticated_as,
        "auth_attempts": auth_attempts,
        "checks": tool_checks,
        "detected_host_os": detected_host_os,
        "detected_capabilities": detected_capabilities,
    });

    let status = if runner.drained {
        "maintenance"
    } else if reachable && probe_result.get("ssh_authenticated_as").and_then(|v| v.as_str()).is_some() {
        "online"
    } else {
        "offline"
    };

    let updated = RunnerRepo::update_probe_snapshot(
        &state.pool,
        id,
        status,
        &detected_host_os,
        &detected_capabilities,
        &probe_result,
    )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update probe result");
            ApiError::internal("Failed to update probe result")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Runner {id} not found")))?;

    Ok(Json(json!({
        "runner": updated,
        "probe": probe_result,
    })))
}

pub async fn allocate_runner_lease(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<AllocateRunnerRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    let target_device = if let Some(device_id) = body.target_device_id {
        let device = DeviceRepo::find_by_id(&state.pool, device_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to fetch target iOS device");
                ApiError::internal("Database error")
            })?
            .ok_or_else(|| ApiError::bad_request(format!("target_device_id {device_id} not found")))?;

        if device.os != "ios" {
            return Err(ApiError::bad_request(format!(
                "target_device_id {} is not an iOS device",
                device.id
            )));
        }

        // Target must be active in dashboard inventory for deployment routing.
        if device.status != "available" && device.status != "busy" {
            return Err(ApiError::bad_request(format!(
                "target_device_id {} is not active (status={})",
                device.id, device.status
            )));
        }

        Some(device)
    } else {
        None
    };

    let scheduler = Scheduler::new(state.pool.clone());
    let filter = RunnerFilter {
        region: body.region,
        required_capabilities: body.required_capabilities,
        required_labels: body.required_labels,
        lease_ttl_seconds: body.lease_ttl_seconds.unwrap_or(600),
    };

    let (runner, lease) = scheduler
        .allocate_runner(auth.tenant_id, &filter, body.build_job_id)
        .await
        .map_err(|e| {
            tracing::warn!(tenant_id = %auth.tenant_id, error = %e, "No matching runner available");
            ApiError::not_found("No matching runner available")
        })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "runner": runner,
            "lease": lease,
            "target_device": target_device,
        })),
    ))
}

pub async fn release_runner_lease(
    auth: AuthContext,
    Path(lease_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let scheduler = Scheduler::new(state.pool.clone());
    scheduler.release_runner_lease(lease_id).await.map_err(|e| {
        tracing::error!(tenant_id = %auth.tenant_id, lease_id = %lease_id, error = %e, "Failed to release runner lease");
        ApiError::internal("Failed to release runner lease")
    })?;

    Ok(Json(json!({
        "lease_id": lease_id,
        "status": "released"
    })))
}
