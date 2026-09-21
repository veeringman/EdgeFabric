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

fn normalize_runner_class(raw: &str) -> String {
    match raw.trim() {
        "" | "mlx-cluster" | "ios" | "macos" | "macos-runner" | "linux" | "linux-runner"
        | "windows" | "windows-runner" => "generic".to_string(),
        other => other.to_string(),
    }
}

fn normalize_host_os(raw: &str) -> String {
    let s = raw.trim().to_ascii_lowercase();
    if s.contains("darwin") || s.contains("mac") {
        "macos".to_string()
    } else if s.contains("win") || s.contains("mingw") || s.contains("cygwin") || s.contains("msys")
    {
        "windows".to_string()
    } else {
        "linux".to_string()
    }
}

fn merge_caps(dst: &mut Vec<String>, extra: &[&str]) {
    for cap in extra {
        if !dst.iter().any(|c| c == cap) {
            dst.push((*cap).to_string());
        }
    }
    dst.sort();
    dst.dedup();
}

fn seed_macos_host_capabilities(caps: &mut Vec<String>) {
    merge_caps(
        caps,
        &[
            "ssh",
            "cicd_runner",
            "macos_runner",
            "artifact_upload",
            "workspace_bootstrap",
        ],
    );
}

fn seed_linux_host_capabilities(caps: &mut Vec<String>) {
    merge_caps(
        caps,
        &[
            "ssh",
            "cicd_runner",
            "linux_runner",
            "artifact_upload",
            "workspace_bootstrap",
        ],
    );
}

fn seed_windows_host_capabilities(caps: &mut Vec<String>) {
    merge_caps(
        caps,
        &[
            "ssh",
            "cicd_runner",
            "windows_runner",
            "artifact_upload",
            "workspace_bootstrap",
        ],
    );
}

fn seed_host_capabilities_for_os(host_os: &str, caps: &mut Vec<String>) {
    match host_os {
        "macos" => seed_macos_host_capabilities(caps),
        "windows" => seed_windows_host_capabilities(caps),
        _ => seed_linux_host_capabilities(caps),
    }
}

fn classify_uname(os_name: &str) -> Option<&'static str> {
    let s = os_name.to_ascii_lowercase();
    if s.contains("darwin") {
        Some("macos")
    } else if s.contains("linux") {
        Some("linux")
    } else if s.contains("mingw")
        || s.contains("cygwin")
        || s.contains("msys")
        || s.contains("windows")
        || s.contains("microsoft")
    {
        Some("windows")
    } else {
        None
    }
}

fn parse_host_port_from_serial(serial: &str) -> Option<(String, i32)> {
    let s = serial.trim();
    if s.is_empty() {
        return None;
    }

    let s = if let Some((_, rest)) = s.rsplit_once('@') {
        rest
    } else {
        s
    };

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
    let runner_class = normalize_runner_class(&body.runner_class);
    let host_os = normalize_host_os(&body.host_os);
    seed_host_capabilities_for_os(&host_os, &mut merged_capabilities);
    if body
        .capabilities
        .iter()
        .any(|c| c == "mlx" || c == "llm_infer")
        || body.runner_class.trim() == "mlx-cluster"
    {
        merge_caps(
            &mut merged_capabilities,
            &["ssh", "mlx", "llm_infer", "pipeline_pp"],
        );
        if host_os == "macos" {
            merge_caps(&mut merged_capabilities, &["ane"]);
        }
    }

    let existing = if let Some(device_id) = body.device_id {
        RunnerRepo::find_by_device_id(&state.pool, device_id)
            .await
            .map_err(|_| ApiError::internal("Database error"))?
    } else {
        None
    };
    let existing = match existing {
        Some(row) => Some(row),
        None => RunnerRepo::find_by_host_port(&state.pool, auth.tenant_id, &host, port)
            .await
            .map_err(|_| ApiError::internal("Database error"))?,
    };

    let runner = if let Some(existing) = existing {
        RunnerRepo::update_registration(
            &state.pool,
            existing.id,
            body.device_id,
            body.name.trim(),
            &runner_class,
            &host,
            port,
            &host_os,
            body.architecture.trim(),
            body.region.trim(),
            &body.labels,
            &merged_capabilities,
            body.max_concurrency,
        )
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %e, "Failed to update runner");
            ApiError::internal("Failed to register runner")
        })?
        .unwrap_or(existing)
    } else {
        RunnerRepo::create(
            &state.pool,
            auth.tenant_id,
            body.device_id,
            body.name.trim(),
            &runner_class,
            &host,
            port,
            &host_os,
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
        })?
    };

    let mut runner = runner;
    if runner.device_id.is_none() {
        let serial = format!("{}:{}", host, port);
        if let Ok(dev) = DeviceRepo::upsert(
            &state.pool,
            body.name.trim(),
            Some(serial.as_str()),
            "ssh",
            &host_os,
            "real",
            body.region.trim(),
            &merged_capabilities,
        )
        .await
        {
            if let Ok(Some(linked)) = RunnerRepo::link_device(&state.pool, runner.id, dev.id).await {
                runner = linked;
            }
        }
    }

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
            "ubuntu".to_string(),
            "ec2-user".to_string(),
            "Administrator".to_string(),
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
        let os_out = ssh_exec_with_port(
            &runner.host,
            runner.port,
            &user,
            "uname -s 2>/dev/null || cmd.exe /c ver",
        )
        .await;
        let os_name = os_out
            .get("stdout")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if let Some(os) = classify_uname(&os_name) {
            detected_host_os = os.to_string();
        }

        let unix_checks = [
            ("ssh", "command -v ssh >/dev/null 2>&1"),
            ("cicd_runner", "command -v sh >/dev/null 2>&1"),
            ("linux_runner", "uname -s | grep -qi linux"),
            ("linux_deploy", "command -v systemctl >/dev/null 2>&1 || command -v scp >/dev/null 2>&1"),
            ("docker", "command -v docker >/dev/null 2>&1"),
            ("cuda", "command -v nvidia-smi >/dev/null 2>&1"),
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
            ("artifact_upload", "command -v scp >/dev/null 2>&1 || command -v git >/dev/null 2>&1"),
            ("workspace_bootstrap", "command -v git >/dev/null 2>&1"),
            ("mlx", "command -v python3 >/dev/null 2>&1 && python3 -c 'import mlx' >/dev/null 2>&1"),
            ("ane", "uname -s | grep -qi darwin && sysctl -n hw.optional.arm64 2>/dev/null | grep -q 1"),
            ("llm_infer", "command -v ef-ml-worker >/dev/null 2>&1 || command -v ollama >/dev/null 2>&1 || (command -v python3 >/dev/null 2>&1 && python3 -c 'import mlx' >/dev/null 2>&1)"),
            ("llm_train", "command -v python3 >/dev/null 2>&1 && (python3 -c 'import mlx' >/dev/null 2>&1 || python3 -c 'import torch' >/dev/null 2>&1)"),
            ("pipeline_pp", "command -v ef-ml-worker >/dev/null 2>&1 || command -v python3 >/dev/null 2>&1"),
            ("macos_runner", "uname -s | grep -qi darwin"),
            ("macos_deploy", "uname -s | grep -qi darwin && command -v xcodebuild >/dev/null 2>&1"),
        ];
        let windows_checks = [
            ("ssh", "where.exe ssh"),
            ("cicd_runner", "where.exe powershell"),
            ("windows_runner", "cmd.exe /c ver"),
            ("windows_deploy", "where.exe dotnet"),
            ("msbuild", "where.exe msbuild"),
            ("powershell", "where.exe powershell"),
            ("docker", "where.exe docker"),
            ("cuda", "where.exe nvidia-smi"),
            ("wsl", "where.exe wsl"),
            ("artifact_upload", "where.exe git"),
            ("workspace_bootstrap", "where.exe git"),
            ("llm_infer", "where.exe ollama"),
        ];

        let checks: &[(&str, &str)] = if detected_host_os == "windows" {
            &windows_checks
        } else {
            &unix_checks
        };

        for (capability, cmd) in checks {
            let out = ssh_exec_with_port(&runner.host, runner.port, &user, cmd).await;
            let ok = out
                .get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if ok && !detected_capabilities.iter().any(|c| c == capability) {
                detected_capabilities.push((*capability).to_string());
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

    let mut hardware = json!({});
    if let Some(user) = ssh_authenticated_as.clone() {
        let hw_script = r#"python3 - <<'PY'
import json, os, platform, re, shutil, subprocess
def sh(cmd):
    try:
        return subprocess.check_output(cmd, shell=True, stderr=subprocess.DEVNULL, text=True, timeout=8).strip()
    except Exception:
        return ""
sysname = platform.system().lower()
chip = platform.processor() or platform.machine()
cpu = os.cpu_count()
mem_gb = None
gpu = None
ane = None
link = 1000
if sysname == "darwin":
    chip = sh("sysctl -n machdep.cpu.brand_string") or chip
    mem = sh("sysctl -n hw.memsize")
    try:
        mem_gb = round(int(mem) / (1024**3)) if mem else None
    except ValueError:
        mem_gb = None
    cpu = sh("sysctl -n hw.ncpu") or cpu
    low = (chip or "").lower()
    if "m6" in low: gpu, ane = 12, 32
    elif "m5" in low: gpu, ane = 10, 16
    elif "m4" in low: gpu, ane = 10, 16
    elif "apple" in low or platform.machine() == "arm64": gpu, ane = 8, 16
    media = sh("ifconfig -a")
    m = re.search(r"media:.*\((\d+)(?:G|g)ase-T", media)
    if m:
        try: link = int(m.group(1)) * 1000
        except ValueError: pass
    m2 = re.search(r"(2500|1000|10000)baseT", media, re.I)
    if m2:
        try: link = int(m2.group(1))
        except ValueError: pass
elif sysname == "linux":
    mem = sh("awk '/MemTotal/ {print $2}' /proc/meminfo")
    try:
        mem_gb = round(int(mem) / (1024**2)) if mem else None
    except ValueError:
        mem_gb = None
    cpu = sh("nproc") or cpu
    chip = sh("lscpu | awk -F: '/Model name/ {print $2; exit}'") or chip
    nvs = sh("nvidia-smi --query-gpu=count --format=csv,noheader 2>/dev/null | head -1")
    try:
        gpu = int(nvs.strip()) if nvs.strip() else None
    except ValueError:
        gpu = None
    etht = sh("cat /sys/class/net/*/speed 2>/dev/null | sort -n | tail -1")
    try:
        if etht: link = int(etht)
    except ValueError:
        pass
else:
    mem = sh("powershell -NoProfile -Command \"[math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory/1GB)\"")
    try:
        mem_gb = int(mem) if mem else None
    except ValueError:
        mem_gb = None
    chip = sh("powershell -NoProfile -Command \"(Get-CimInstance Win32_Processor).Name\"") or chip
    nvs = sh("nvidia-smi --query-gpu=count --format=csv,noheader")
    try:
        gpu = int(nvs.strip().splitlines()[0]) if nvs.strip() else None
    except Exception:
        gpu = None
mlx_ver = ""
try:
    import mlx
    mlx_ver = getattr(mlx, "__version__", "present")
except Exception:
    pass
print(json.dumps({
    "chip": (chip or "unknown").strip(),
    "cpu_cores": int(cpu) if str(cpu).isdigit() else None,
    "gpu_cores": gpu,
    "ane_cores": ane,
    "unified_memory_gb": mem_gb,
    "link_mbps": link,
    "mlx_version": mlx_ver,
    "worker_present": shutil.which("ef-ml-worker") is not None,
    "arch": platform.machine(),
    "os": platform.system(),
}))
PY"#;
        let hw_out = ssh_exec_with_port(&runner.host, runner.port, &user, hw_script).await;
        if let Some(stdout) = hw_out.get("stdout").and_then(|v| v.as_str()) {
            if let Ok(parsed) = serde_json::from_str::<Value>(stdout.trim()) {
                hardware = parsed;
            }
        }
        seed_host_capabilities_for_os(&detected_host_os, &mut detected_capabilities);
        if detected_host_os == "macos" && detected_capabilities.iter().any(|c| c == "xcode") {
            merge_caps(&mut detected_capabilities, &["macos_deploy"]);
        }
        if detected_host_os == "linux" && detected_capabilities.iter().any(|c| c == "iot_deploy" || c == "docker") {
            merge_caps(&mut detected_capabilities, &["linux_deploy"]);
        }
        if detected_host_os == "windows"
            && detected_capabilities
                .iter()
                .any(|c| c == "msbuild" || c == "dotnet" || c == "windows_deploy")
        {
            merge_caps(&mut detected_capabilities, &["windows_deploy"]);
        }
        if detected_capabilities
            .iter()
            .any(|c| c == "mlx" || c == "llm_infer")
            || runner.runner_class == "mlx-cluster"
        {
            merge_caps(
                &mut detected_capabilities,
                &["llm_infer", "pipeline_pp"],
            );
            if detected_host_os == "macos" {
                merge_caps(&mut detected_capabilities, &["mlx", "ane"]);
            }
        }
    }

    let probe_result = json!({
        "target": target,
        "reachable": reachable,
        "probe_type": "tcp_connect+ssh_host_probe",
        "requested_user": requested_user,
        "ssh_authenticated_as": ssh_authenticated_as,
        "auth_attempts": auth_attempts,
        "checks": tool_checks,
        "detected_host_os": detected_host_os,
        "detected_capabilities": detected_capabilities,
        "hardware": hardware,
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

    if let Some(hw) = probe_result.get("hardware") {
        if detected_capabilities
            .iter()
            .any(|c| {
                c == "mlx"
                    || c == "llm_infer"
                    || c == "macos_runner"
                    || c == "linux_runner"
                    || c == "windows_runner"
                    || c == "cuda"
            })
        {
            let chip = hw.get("chip").and_then(|v| v.as_str()).unwrap_or("");
            let cpu = hw.get("cpu_cores").and_then(|v| v.as_i64()).map(|n| n as i32);
            let gpu = hw.get("gpu_cores").and_then(|v| v.as_i64()).map(|n| n as i32);
            let ane = hw.get("ane_cores").and_then(|v| v.as_i64()).map(|n| n as i32);
            let mem = hw
                .get("unified_memory_gb")
                .and_then(|v| v.as_i64())
                .map(|n| n as i32);
            let link = hw
                .get("link_mbps")
                .and_then(|v| v.as_i64())
                .map(|n| n as i32)
                .unwrap_or(2500);
            let mlx_ver = hw.get("mlx_version").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(tenant_id) = updated.tenant_id {
                let _ = ef_db::repo::ml::MlRepo::upsert_node(
                    &state.pool,
                    tenant_id,
                    id,
                    chip,
                    cpu,
                    gpu,
                    ane,
                    mem,
                    link,
                    mlx_ver,
                    &updated.labels,
                    status,
                )
                .await;
            }
        }
    }

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
        runner_class: None,
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

pub(crate) async fn ensure_ssh_host_runner(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    device: &DeviceRow,
) -> Result<Option<ef_db::repo::runner::RunnerRow>, sqlx::Error> {
    if device.os != "ssh" && device.os != "veeros" {
        return Ok(None);
    }
    if let Some(existing) = RunnerRepo::find_by_device_id(pool, device.id).await? {
        return Ok(Some(existing));
    }
    let Some((host, port)) = derive_host_port_from_device(device) else {
        return Ok(None);
    };
    if let Some(existing) = RunnerRepo::find_by_host_port(pool, tenant_id, &host, port).await? {
        if existing.device_id.is_none() {
            return RunnerRepo::link_device(pool, existing.id, device.id).await;
        }
        return Ok(Some(existing));
    }
    let os_ver = device.os_version.to_ascii_lowercase();
    let host_os = normalize_host_os(&os_ver);
    let mut caps = device.capabilities.clone();
    seed_host_capabilities_for_os(&host_os, &mut caps);
    let concurrency = if host_os == "linux" { 2 } else { 2 };
    let runner = RunnerRepo::create(
        pool,
        tenant_id,
        Some(device.id),
        &device.name,
        "generic",
        &host,
        port,
        &host_os,
        "",
        &device.region,
        &[],
        &caps,
        concurrency,
    )
    .await?;
    Ok(Some(runner))
}
