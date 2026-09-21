use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::process::Command;
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

use ef_db::repo::build::{BuildArtifactRepo, BuildJobRepo};
use ef_db::repo::device::DeviceRepo;
use ef_scheduler::{RunnerFilter, Scheduler};

#[derive(Debug, Deserialize)]
pub struct SubmitIosBuildRequest {
    pub source_ref: String,
    pub project_path: String,
    pub scheme: String,
    #[serde(default = "default_configuration")]
    pub configuration: String,
    pub target_device_id: Uuid,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub required_runner_capabilities: Vec<String>,
    #[serde(default)]
    pub required_runner_labels: Vec<String>,
    #[serde(default)]
    pub lease_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub ssh_user: Option<String>,
    #[serde(default)]
    pub repository_url: Option<String>,
    #[serde(default)]
    pub git_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitIotDeployRequest {
    pub source_ref: String,
    pub project_path: String,
    pub target_device_id: Uuid,
    #[serde(default)]
    pub board_profile: Option<String>,
    #[serde(default)]
    pub deploy_command: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub required_runner_capabilities: Vec<String>,
    #[serde(default)]
    pub required_runner_labels: Vec<String>,
    #[serde(default)]
    pub lease_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub ssh_user: Option<String>,
    #[serde(default)]
    pub repository_url: Option<String>,
    #[serde(default)]
    pub git_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitQuantumProgramRequest {
    pub source_ref: String,
    pub project_path: String,
    #[serde(default)]
    pub run_command: Option<String>,
    #[serde(default)]
    pub backend_kind: Option<String>,
    #[serde(default)]
    pub backend_name: Option<String>,
    #[serde(default)]
    pub veeros_host_hint: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub required_runner_capabilities: Vec<String>,
    #[serde(default)]
    pub required_runner_labels: Vec<String>,
    #[serde(default)]
    pub lease_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub ssh_user: Option<String>,
    #[serde(default)]
    pub repository_url: Option<String>,
    #[serde(default)]
    pub git_ref: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListBuildsQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

fn normalize_opt_filter(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn default_configuration() -> String {
    "Debug".to_string()
}

fn shell_quote(s: &str) -> String {
    let escaped = s.replace('\'', "'\"'\"'");
    format!("'{}'", escaped)
}

fn is_terminal_status(status: &str) -> bool {
    matches!(status, "succeeded" | "failed" | "cancelled")
}

async fn ssh_exec(host: &str, port: i32, user: &str, script: &str) -> std::io::Result<std::process::Output> {
    let target = format!("{}@{}", user, host);
    Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=accept-new",
            "-o",
            "ConnectTimeout=15",
            "-o",
            "BatchMode=yes",
            "-p",
            &port.to_string(),
            &target,
            "sh",
            "-lc",
            script,
        ])
        .output()
        .await
}

async fn run_ios_build_background(
    pool: sqlx::PgPool,
    build_id: Uuid,
    runner_id: Uuid,
    runner_host: String,
    runner_port: i32,
    lease_id: Uuid,
    ssh_user: String,
    project_path: String,
    scheme: String,
    configuration: String,
    target_udid: String,
) {
    let scheduler = Scheduler::new(pool.clone());
    let remote_log_path = format!("/tmp/edgefabric-build-{}.log", build_id);
    let marker = format!("edgefabric-build-{}", build_id);

    let _ = BuildJobRepo::set_running(&pool, build_id).await;

    let remote_script = format!(
        "set -o pipefail; cd {} && (echo {} && xcodebuild -scheme {} -configuration {} -destination {} build) 2>&1 | tee {}",
        shell_quote(project_path.trim()),
        shell_quote(&marker),
        shell_quote(scheme.trim()),
        shell_quote(configuration.trim()),
        shell_quote(&format!("id={target_udid}")),
        shell_quote(&remote_log_path),
    );

    let output = ssh_exec(&runner_host, runner_port, &ssh_user, &remote_script).await;

    let release_result = scheduler.release_runner_lease(lease_id).await;
    if let Err(e) = release_result {
        tracing::warn!(lease_id = %lease_id, error = %e, "Failed to release runner lease");
    }

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let exit_code = out.status.code().unwrap_or(-1);
            let status = if out.status.success() { "succeeded" } else { "failed" };
            let error_msg = if out.status.success() {
                None
            } else {
                Some(format!("xcodebuild failed with exit code {exit_code}"))
            };

            let runtime = json!({
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "runner_id": runner_id,
                "runner_host": runner_host,
                "runner_port": runner_port,
                "lease_id": lease_id,
                "ssh_user": ssh_user,
                "remote_log_path": remote_log_path,
                "target_udid": target_udid,
            });

            let _ = BuildJobRepo::set_terminal(
                &pool,
                build_id,
                status,
                &runtime,
                error_msg.as_deref(),
                None,
            )
            .await;
        }
        Err(e) => {
            let _ = BuildJobRepo::set_terminal(
                &pool,
                build_id,
                "failed",
                &json!({
                    "runner_id": runner_id,
                    "runner_host": runner_host,
                    "runner_port": runner_port,
                    "lease_id": lease_id,
                    "ssh_user": ssh_user,
                    "remote_log_path": remote_log_path,
                    "spawn_error": e.to_string(),
                }),
                Some(&format!("Failed to execute remote build: {e}")),
                None,
            )
            .await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_remote_job_background(
    pool: sqlx::PgPool,
    build_id: Uuid,
    build_type: &'static str,
    runner_id: Uuid,
    runner_host: String,
    runner_port: i32,
    lease_id: Uuid,
    ssh_user: String,
    project_path: String,
    run_command: String,
    extra_runtime: Value,
) {
    let scheduler = Scheduler::new(pool.clone());
    let remote_log_path = format!("/tmp/edgefabric-{build_type}-{}.log", build_id);
    let marker = format!("edgefabric-{build_type}-{}", build_id);

    let _ = BuildJobRepo::set_running(&pool, build_id).await;

    let remote_script = format!(
        "set -o pipefail; cd {} && (echo {} && {}) 2>&1 | tee {}",
        shell_quote(project_path.trim()),
        shell_quote(&marker),
        run_command,
        shell_quote(&remote_log_path),
    );

    let output = ssh_exec(&runner_host, runner_port, &ssh_user, &remote_script).await;

    let release_result = scheduler.release_runner_lease(lease_id).await;
    if let Err(e) = release_result {
        tracing::warn!(lease_id = %lease_id, error = %e, "Failed to release runner lease");
    }

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            let exit_code = out.status.code().unwrap_or(-1);
            let status = if out.status.success() { "succeeded" } else { "failed" };
            let error_msg = if out.status.success() {
                None
            } else {
                Some(format!("{build_type} remote command failed with exit code {exit_code}"))
            };

            let mut runtime = json!({
                "exit_code": exit_code,
                "stdout": stdout,
                "stderr": stderr,
                "runner_id": runner_id,
                "runner_host": runner_host,
                "runner_port": runner_port,
                "lease_id": lease_id,
                "ssh_user": ssh_user,
                "remote_log_path": remote_log_path,
                "build_type": build_type,
            });

            if let Some(obj) = runtime.as_object_mut() {
                if let Some(extra) = extra_runtime.as_object() {
                    for (k, v) in extra {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }

            let _ = BuildJobRepo::set_terminal(
                &pool,
                build_id,
                status,
                &runtime,
                error_msg.as_deref(),
                None,
            )
            .await;
        }
        Err(e) => {
            let _ = BuildJobRepo::set_terminal(
                &pool,
                build_id,
                "failed",
                &json!({
                    "runner_id": runner_id,
                    "runner_host": runner_host,
                    "runner_port": runner_port,
                    "lease_id": lease_id,
                    "ssh_user": ssh_user,
                    "remote_log_path": remote_log_path,
                    "spawn_error": e.to_string(),
                    "build_type": build_type,
                }),
                Some(&format!("Failed to execute remote {build_type} command: {e}")),
                None,
            )
            .await;
        }
    }
}

pub async fn submit_ios_build(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<SubmitIosBuildRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    if body.source_ref.trim().is_empty() {
        return Err(ApiError::bad_request("source_ref must not be empty"));
    }
    if body.project_path.trim().is_empty() {
        return Err(ApiError::bad_request("project_path must not be empty"));
    }
    if body.scheme.trim().is_empty() {
        return Err(ApiError::bad_request("scheme must not be empty"));
    }

    let target_device = DeviceRepo::find_by_id(&state.pool, body.target_device_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch target iOS device");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| {
            ApiError::bad_request(format!("target_device_id {} not found", body.target_device_id))
        })?;

    if target_device.os != "ios" {
        return Err(ApiError::bad_request(format!(
            "target_device_id {} is not an iOS device",
            target_device.id
        )));
    }

    let target_udid = target_device.serial.clone().ok_or_else(|| {
        ApiError::bad_request(format!(
            "target_device_id {} does not have a usable serial/UDID",
            target_device.id
        ))
    })?;

    let config_json = json!({
        "source_ref": body.source_ref,
        "project_path": body.project_path,
        "scheme": body.scheme,
        "configuration": body.configuration,
        "target_device_id": body.target_device_id,
        "target_udid": target_udid,
    });

    let mut build = BuildJobRepo::create_ios(
        &state.pool,
        auth.tenant_id,
        &auth.tenant_name,
        &body.source_ref,
        body.repository_url.as_deref(),
        body.git_ref.as_deref(),
        &config_json,
        &json!({}),
        Some(body.target_device_id),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to create build job");
        ApiError::internal("Failed to create build job")
    })?;

    let mut required_caps = body.required_runner_capabilities.clone();
    for c in ["cicd_runner", "xcode", "artifact_upload", "ssh"] {
        if !required_caps.iter().any(|x| x == c) {
            required_caps.push(c.to_string());
        }
    }
    required_caps.sort();
    required_caps.dedup();

    let scheduler = Scheduler::new(state.pool.clone());
    let filter = RunnerFilter {
        region: body.region.clone(),
        required_capabilities: required_caps,
        required_labels: body.required_runner_labels.clone(),
        lease_ttl_seconds: body.lease_ttl_seconds.unwrap_or(1800),
        runner_class: None,
    };

    let (runner, lease) = match scheduler
        .allocate_runner(auth.tenant_id, &filter, Some(build.id))
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let _ = BuildJobRepo::set_terminal(
                &state.pool,
                build.id,
                "failed",
                &json!({ "allocation_error": e.to_string() }),
                Some(&format!("Runner allocation failed: {e}")),
                None,
            )
            .await;
            return Err(ApiError::not_found("No matching runner available"));
        }
    };

    BuildJobRepo::set_provisioning(
        &state.pool,
        build.id,
        runner.id,
        &json!({
            "runner_id": runner.id,
            "lease_id": lease.id,
            "runner_host": runner.host,
            "runner_port": runner.port,
        }),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to update build provisioning state");
        ApiError::internal("Failed to update build state")
    })?;

    let ssh_user = body
        .ssh_user
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("builder");

    let pool = state.pool.clone();
    let build_id = build.id;
    let runner_id = runner.id;
    let runner_host = runner.host.clone();
    let runner_port = runner.port;
    let lease_id = lease.id;
    let target_udid_owned = target_udid.clone();
    let project_path = body.project_path.clone();
    let scheme = body.scheme.clone();
    let configuration = body.configuration.clone();
    let ssh_user_owned = ssh_user.to_string();

    tokio::spawn(async move {
        run_ios_build_background(
            pool,
            build_id,
            runner_id,
            runner_host,
            runner_port,
            lease_id,
            ssh_user_owned,
            project_path,
            scheme,
            configuration,
            target_udid_owned,
        )
        .await;
    });

    build = BuildJobRepo::find_by_id(&state.pool, build.id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to refetch build job");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found("Build job not found after enqueue"))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "build": build,
            "runner": runner,
            "lease_id": lease.id,
        })),
    ))
}

fn default_iot_command(board_profile: Option<&str>) -> String {
    match board_profile
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "esp32c6" => {
            "(command -v idf.py >/dev/null 2>&1 && idf.py build flash) || (command -v pio >/dev/null 2>&1 && pio run -t upload)".to_string()
        }
        "rpi" | "raspberrypi" | "raspberry_pi" => {
            "(command -v cmake >/dev/null 2>&1 && cmake -S . -B build && cmake --build build) || (command -v make >/dev/null 2>&1 && make)".to_string()
        }
        _ => {
            "(command -v pio >/dev/null 2>&1 && pio run -t upload) || (command -v make >/dev/null 2>&1 && make)".to_string()
        }
    }
}

pub async fn submit_iot_deploy(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<SubmitIotDeployRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    if body.source_ref.trim().is_empty() {
        return Err(ApiError::bad_request("source_ref must not be empty"));
    }
    if body.project_path.trim().is_empty() {
        return Err(ApiError::bad_request("project_path must not be empty"));
    }

    let target_device = DeviceRepo::find_by_id(&state.pool, body.target_device_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch target IoT device");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| {
            ApiError::bad_request(format!("target_device_id {} not found", body.target_device_id))
        })?;

    if target_device.os != "iot" && target_device.os != "virtual_iot" {
        return Err(ApiError::bad_request(format!(
            "target_device_id {} must be an IoT or Virtual IoT device",
            target_device.id
        )));
    }

    if target_device.status != "available" && target_device.status != "busy" {
        return Err(ApiError::bad_request(format!(
            "target_device_id {} is not active (status={})",
            target_device.id, target_device.status
        )));
    }

    let config_json = json!({
        "source_ref": body.source_ref,
        "project_path": body.project_path,
        "target_device_id": body.target_device_id,
        "target_device_os": target_device.os,
        "board_profile": body.board_profile,
        "deploy_command": body.deploy_command,
    });

    let mut build = BuildJobRepo::create_job(
        &state.pool,
        auth.tenant_id,
        "iot",
        &auth.tenant_name,
        &body.source_ref,
        body.repository_url.as_deref(),
        body.git_ref.as_deref(),
        &config_json,
        &json!({}),
        Some(body.target_device_id),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to create IoT deploy job");
        ApiError::internal("Failed to create IoT deploy job")
    })?;

    let mut required_caps = body.required_runner_capabilities.clone();
    for c in ["ssh", "cicd_runner", "artifact_upload", "iot_toolchain", "linux_runner"] {
        if !required_caps.iter().any(|x| x == c) {
            required_caps.push(c.to_string());
        }
    }

    if let Some(profile) = body.board_profile.as_deref() {
        if profile.eq_ignore_ascii_case("esp32c6") && !required_caps.iter().any(|x| x == "esp32") {
            required_caps.push("esp32".to_string());
        }
        if profile.eq_ignore_ascii_case("rpi") && !required_caps.iter().any(|x| x == "rpi") {
            required_caps.push("rpi".to_string());
        }
    }

    required_caps.sort();
    required_caps.dedup();

    let scheduler = Scheduler::new(state.pool.clone());
    let filter = RunnerFilter {
        region: body.region.clone(),
        required_capabilities: required_caps,
        required_labels: body.required_runner_labels.clone(),
        lease_ttl_seconds: body.lease_ttl_seconds.unwrap_or(1800),
        runner_class: None,
    };

    let (runner, lease) = match scheduler
        .allocate_runner(auth.tenant_id, &filter, Some(build.id))
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let _ = BuildJobRepo::set_terminal(
                &state.pool,
                build.id,
                "failed",
                &json!({ "allocation_error": e.to_string() }),
                Some(&format!("Runner allocation failed: {e}")),
                None,
            )
            .await;
            return Err(ApiError::not_found("No matching Linux IoT runner available"));
        }
    };

    BuildJobRepo::set_provisioning(
        &state.pool,
        build.id,
        runner.id,
        &json!({
            "runner_id": runner.id,
            "lease_id": lease.id,
            "runner_host": runner.host,
            "runner_port": runner.port,
            "target_device_id": body.target_device_id,
            "target_device_os": target_device.os,
        }),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to update IoT deploy provisioning state");
        ApiError::internal("Failed to update build state")
    })?;

    let ssh_user = body
        .ssh_user
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("builder")
        .to_string();

    let run_command = body
        .deploy_command
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| default_iot_command(body.board_profile.as_deref()));

    let pool = state.pool.clone();
    let build_id = build.id;
    let runner_id = runner.id;
    let runner_host = runner.host.clone();
    let runner_port = runner.port;
    let lease_id = lease.id;
    let project_path = body.project_path.clone();
    let extra_runtime = json!({
        "target_device_id": body.target_device_id,
        "target_device_os": target_device.os,
        "board_profile": body.board_profile,
        "run_command": run_command,
    });

    tokio::spawn(async move {
        run_remote_job_background(
            pool,
            build_id,
            "iot",
            runner_id,
            runner_host,
            runner_port,
            lease_id,
            ssh_user,
            project_path,
            run_command,
            extra_runtime,
        )
        .await;
    });

    build = BuildJobRepo::find_by_id(&state.pool, build.id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to refetch IoT deploy job");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found("Build job not found after enqueue"))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "build": build,
            "runner": runner,
            "lease_id": lease.id,
        })),
    ))
}

pub async fn submit_quantum_program(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<SubmitQuantumProgramRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    if body.source_ref.trim().is_empty() {
        return Err(ApiError::bad_request("source_ref must not be empty"));
    }
    if body.project_path.trim().is_empty() {
        return Err(ApiError::bad_request("project_path must not be empty"));
    }

    let backend_kind = body
        .backend_kind
        .clone()
        .unwrap_or_else(|| "virtual".to_string())
        .to_ascii_lowercase();

    if backend_kind != "virtual" && backend_kind != "real" {
        return Err(ApiError::bad_request("backend_kind must be 'virtual' or 'real'"));
    }

    let run_command = body
        .run_command
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            "(command -v python3 >/dev/null 2>&1 && python3 main.py) || (command -v qiskit >/dev/null 2>&1 && qiskit --help)".to_string()
        });

    let config_json = json!({
        "source_ref": body.source_ref,
        "project_path": body.project_path,
        "run_command": run_command,
        "backend_kind": backend_kind,
        "backend_name": body.backend_name,
        "veeros_host_hint": body.veeros_host_hint,
    });

    let mut build = BuildJobRepo::create_job(
        &state.pool,
        auth.tenant_id,
        "quantum",
        &auth.tenant_name,
        &body.source_ref,
        body.repository_url.as_deref(),
        body.git_ref.as_deref(),
        &config_json,
        &json!({}),
        None,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to create quantum job");
        ApiError::internal("Failed to create quantum job")
    })?;

    let mut required_caps = body.required_runner_capabilities.clone();
    for c in ["ssh", "cicd_runner", "quantum_sdk", "veeros_host", "linux_runner"] {
        if !required_caps.iter().any(|x| x == c) {
            required_caps.push(c.to_string());
        }
    }
    let quantum_backend_cap = if backend_kind == "real" {
        "quantum_hardware"
    } else {
        "quantum_simulator"
    };
    if !required_caps.iter().any(|x| x == quantum_backend_cap) {
        required_caps.push(quantum_backend_cap.to_string());
    }

    required_caps.sort();
    required_caps.dedup();

    let scheduler = Scheduler::new(state.pool.clone());
    let filter = RunnerFilter {
        region: body.region.clone(),
        required_capabilities: required_caps,
        required_labels: body.required_runner_labels.clone(),
        lease_ttl_seconds: body.lease_ttl_seconds.unwrap_or(1800),
        runner_class: None,
    };

    let (runner, lease) = match scheduler
        .allocate_runner(auth.tenant_id, &filter, Some(build.id))
        .await
    {
        Ok(v) => v,
        Err(e) => {
            let _ = BuildJobRepo::set_terminal(
                &state.pool,
                build.id,
                "failed",
                &json!({ "allocation_error": e.to_string() }),
                Some(&format!("Runner allocation failed: {e}")),
                None,
            )
            .await;
            return Err(ApiError::not_found("No matching VeerOS/Quantum runner available"));
        }
    };

    BuildJobRepo::set_provisioning(
        &state.pool,
        build.id,
        runner.id,
        &json!({
            "runner_id": runner.id,
            "lease_id": lease.id,
            "runner_host": runner.host,
            "runner_port": runner.port,
            "backend_kind": backend_kind,
            "backend_name": body.backend_name,
            "veeros_host_hint": body.veeros_host_hint,
        }),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to update quantum provisioning state");
        ApiError::internal("Failed to update build state")
    })?;

    let ssh_user = body
        .ssh_user
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("builder")
        .to_string();

    let pool = state.pool.clone();
    let build_id = build.id;
    let runner_id = runner.id;
    let runner_host = runner.host.clone();
    let runner_port = runner.port;
    let lease_id = lease.id;
    let project_path = body.project_path.clone();
    let extra_runtime = json!({
        "backend_kind": backend_kind,
        "backend_name": body.backend_name,
        "veeros_host_hint": body.veeros_host_hint,
        "run_command": run_command,
    });

    tokio::spawn(async move {
        run_remote_job_background(
            pool,
            build_id,
            "quantum",
            runner_id,
            runner_host,
            runner_port,
            lease_id,
            ssh_user,
            project_path,
            run_command,
            extra_runtime,
        )
        .await;
    });

    build = BuildJobRepo::find_by_id(&state.pool, build.id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to refetch quantum job");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found("Build job not found after enqueue"))?;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "build": build,
            "runner": runner,
            "lease_id": lease.id,
        })),
    ))
}

pub async fn get_build(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let build = BuildJobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch build");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Build {id} not found")))?;

    if build.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Build {id} not found")));
    }

    Ok(Json(json!({ "build": build })))
}

pub async fn list_builds(
    auth: AuthContext,
    Query(query): Query<ListBuildsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let status = normalize_opt_filter(query.status);
    let device_q = normalize_opt_filter(query.device);
    let source_q = normalize_opt_filter(query.source);

    let builds = BuildJobRepo::list_by_tenant(
        &state.pool,
        auth.tenant_id,
        limit,
        offset,
        status.as_deref(),
        device_q.as_deref(),
        source_q.as_deref(),
    )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list builds");
            ApiError::internal("Database error")
        })?;

    let total = BuildJobRepo::count_by_tenant(
        &state.pool,
        auth.tenant_id,
        status.as_deref(),
        device_q.as_deref(),
        source_q.as_deref(),
    )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to count builds");
            ApiError::internal("Database error")
        })?;

    let status_counts = BuildJobRepo::count_status_facets_by_tenant(
        &state.pool,
        auth.tenant_id,
        device_q.as_deref(),
        source_q.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to count build status facets");
        ApiError::internal("Database error")
    })?;

    Ok(Json(json!({
        "builds": builds,
        "total": total,
        "limit": limit,
        "offset": offset,
        "status": status,
        "device": device_q,
        "source": source_q,
        "status_counts": status_counts,
    })))
}

pub async fn get_build_logs(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let build = BuildJobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch build");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Build {id} not found")))?;

    if build.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Build {id} not found")));
    }

    let stdout = build
        .runtime
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let stderr = build
        .runtime
        .get("stderr")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if !is_terminal_status(&build.status) {
        let host = build.runtime.get("runner_host").and_then(|v| v.as_str());
        let port = build.runtime.get("runner_port").and_then(|v| v.as_i64());
        let user = build.runtime.get("ssh_user").and_then(|v| v.as_str());
        let remote_log = build.runtime.get("remote_log_path").and_then(|v| v.as_str());

        if let (Some(host), Some(port), Some(user), Some(remote_log)) = (host, port, user, remote_log) {
            let script = format!("tail -n 200 {}", shell_quote(remote_log));
            match ssh_exec(host, port as i32, user, &script).await {
                Ok(out) => {
                    let live = String::from_utf8_lossy(&out.stdout).into_owned();
                    return Ok(Json(json!({
                        "build_id": id,
                        "status": build.status,
                        "mode": "live_tail",
                        "stdout": live,
                        "stderr": String::from_utf8_lossy(&out.stderr),
                    })));
                }
                Err(e) => {
                    tracing::warn!(build_id = %id, error = %e, "Failed to fetch live remote logs");
                }
            }
        }
    }

    Ok(Json(json!({
        "build_id": id,
        "status": build.status,
        "mode": "stored_runtime",
        "stdout": stdout,
        "stderr": stderr,
    })))
}

pub async fn list_build_artifacts(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let build = BuildJobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch build");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Build {id} not found")))?;

    if build.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Build {id} not found")));
    }

    let artifacts = BuildArtifactRepo::list_by_build_job(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list build artifacts");
            ApiError::internal("Database error")
        })?;

    Ok(Json(json!({
        "build_id": id,
        "artifacts": artifacts,
        "total": artifacts.len(),
    })))
}

pub async fn cancel_build(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let build = BuildJobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to fetch build");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Build {id} not found")))?;

    if build.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Build {id} not found")));
    }
    if is_terminal_status(&build.status) {
        return Err(ApiError::bad_request(format!(
            "Build {} is already terminal ({})",
            build.id, build.status
        )));
    }

    let marker = format!("edgefabric-build-{}", id);
    let host = build.runtime.get("runner_host").and_then(|v| v.as_str());
    let port = build.runtime.get("runner_port").and_then(|v| v.as_i64());
    let user = build.runtime.get("ssh_user").and_then(|v| v.as_str()).unwrap_or("builder");

    let mut cancel_note = "cancel requested".to_string();
    if let (Some(host), Some(port)) = (host, port) {
        let kill_script = format!("pkill -f {} || true", shell_quote(&marker));
        match ssh_exec(host, port as i32, user, &kill_script).await {
            Ok(out) => {
                cancel_note = format!(
                    "cancel requested; remote kill attempted (exit={})",
                    out.status.code().unwrap_or(-1)
                );
            }
            Err(e) => {
                cancel_note = format!("cancel requested; remote kill failed: {e}");
            }
        }
    }

    if let Some(lease_s) = build.runtime.get("lease_id").and_then(|v| v.as_str()) {
        if let Ok(lease_id) = Uuid::parse_str(lease_s) {
            let scheduler = Scheduler::new(state.pool.clone());
            let _ = scheduler.release_runner_lease(lease_id).await;
        }
    }

    BuildJobRepo::set_terminal(
        &state.pool,
        id,
        "cancelled",
        &json!({ "cancel_note": cancel_note }),
        Some("Cancelled by user"),
        None,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to cancel build");
        ApiError::internal("Failed to cancel build")
    })?;

    Ok(Json(json!({
        "build_id": id,
        "status": "cancelled",
        "message": cancel_note,
    })))
}
