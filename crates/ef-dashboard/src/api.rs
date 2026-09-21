use gloo_net::http::Request;
use gloo_storage::Storage;
use serde::{Deserialize, Serialize, de::{DeserializeOwned, Deserializer}};

/// Build API base URL from the browser's current origin.
///
/// The dashboard dev server proxies `/api` to the backend, so using same-origin
/// avoids localhost-vs-LAN-IP drift and keeps HTTPS/WSS aligned automatically.
fn base_url() -> String {
    let origin = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "http://127.0.0.1:9080".to_string());
    format!("{origin}/api/v1")
}

pub fn ws_base_url() -> String {
    let origin = web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "http://127.0.0.1:9080".to_string());
    let ws_origin = origin
        .replace("https://", "wss://")
        .replace("http://", "ws://");
    format!("{ws_origin}/api/v1")
}

fn token() -> Option<String> {
    gloo_storage::LocalStorage::get::<String>("ef_token").ok()
}

fn auth_header() -> String {
    format!("Bearer {}", token().unwrap_or_default())
}

/// Redirect to /login when the server says our token is invalid/expired.
fn handle_unauthorized() {
    let _ = gloo_storage::LocalStorage::delete("ef_token");
    let _ = gloo_storage::LocalStorage::delete("ef_tenant");
    if let Some(w) = web_sys::window() {
        let _ = w.location().set_href("/login");
    }
}

/// Generic JSON GET
pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let resp = Request::get(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    resp.json::<T>().await.map_err(|e| e.to_string())
}

/// Generic JSON POST
pub async fn post<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, String> {
    let resp = Request::post(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    resp.json::<T>().await.map_err(|e| e.to_string())
}

/// Generic JSON PUT
pub async fn put<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, String> {
    let resp = Request::put(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    resp.json::<T>().await.map_err(|e| e.to_string())
}

/// DELETE request (no body)
pub async fn delete(path: &str) -> Result<(), String> {
    let resp = Request::delete(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    Ok(())
}

/// Generic JSON PATCH
pub async fn patch<B: Serialize, T: DeserializeOwned>(path: &str, body: &B) -> Result<T, String> {
    let resp = Request::patch(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    resp.json::<T>().await.map_err(|e| e.to_string())
}

/// POST with no meaningful body — for action endpoints like start/stop
pub async fn post_empty(path: &str) -> Result<(), String> {
    let resp = Request::post(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body("{}")
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    Ok(())
}

/// Login — exchange API key for JWT (unauthed endpoint)
pub async fn login(api_key: &str) -> Result<LoginResponse, String> {
    let resp = Request::post(&format!("{}/auth/login", base_url()))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&LoginRequest { api_key: api_key.to_string() }).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err("Invalid API key".to_string());
    }

    resp.json::<LoginResponse>().await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct LoginRequest {
    api_key: String,
}

/// Login with username/password — OAuth2 via AuthBuddy (placeholder endpoint)
pub async fn login_password(username: &str, password: &str) -> Result<LoginResponse, String> {
    let resp = Request::post(&format!("{}/auth/login", base_url()))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&PasswordLoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        }).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.ok() {
        return Err("Invalid username or password".to_string());
    }

    resp.json::<LoginResponse>().await.map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct PasswordLoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LoginResponse {
    pub token: String,
    pub tenant_id: String,
    pub expires_in: u64,
}

// ---------- Response models ----------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DeviceListResponse {
    pub devices: Vec<Device>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub serial: Option<String>,
    pub os: String,
    pub os_version: String,
    pub kind: String,
    pub status: String,
    pub region: String,
    pub agent_id: Option<String>,
    pub capabilities: serde_json::Value,
    pub last_heartbeat: Option<String>,
    pub created_at: Option<String>,
}

// -- Agent types --

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct AgentListResponse {
    pub agents: Vec<Agent>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub region: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub streaming_url: Option<String>,
    pub last_heartbeat: Option<String>,
}

// -- Virtual IoT types --

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VirtualIotConfigResponse {
    pub config: VirtualIotConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct VirtualIotEventStep {
    pub sensor: String,
    pub value: String,
    #[serde(default)]
    pub end_value: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i32>,
    #[serde(default)]
    pub action: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct VirtualIotEventDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub steps: Vec<VirtualIotEventStep>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
pub struct VirtualIotSensorLogEntry {
    pub ts: String,
    pub sensor: String,
    pub value: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VirtualIotConfig {
    pub device_id: String,
    pub controller_type: String,
    pub enabled_sensors: serde_json::Value,
    pub backend_id: Option<String>,
    pub backend: serde_json::Value,
    pub sim_rate_ms: i32,
    #[serde(default)]
    pub event_definitions: Vec<VirtualIotEventDefinition>,
    #[serde(default)]
    pub sensor_state: serde_json::Value,
    #[serde(default)]
    pub sensor_log: Vec<VirtualIotSensorLogEntry>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct VirtualIotBackendListResponse {
    pub backends: Vec<VirtualIotBackend>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VirtualIotBackendEnvelope {
    pub backend: VirtualIotBackend,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct VirtualIotBackend {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub controller_type: String,
    #[serde(default)]
    pub controller_types: Vec<String>,
    pub region: String,
    pub status: String,
    pub vm_name: Option<String>,
    pub shell_host: Option<String>,
    pub shell_port: Option<i32>,
    pub config: serde_json::Value,
    pub capabilities: Vec<String>,
    pub attached_device_id: Option<String>,
    pub agent_id: Option<String>,
    pub agent_base_url: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateVirtualIotBackendRequest {
    pub name: String,
    pub provider: String,
    pub controller_type: String,
    pub controller_types: Vec<String>,
    pub region: String,
    pub auto_start: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct UpdateVirtualIotBackendRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_types: Option<Vec<String>>,
}

#[derive(Serialize, Clone, Debug)]
pub struct AttachVirtualIotBackendRequest {
    pub backend_id: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SensorInjectRequest {
    pub sensor: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ActuatorWriteRequest {
    pub pin: u32,
    pub level: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct ScriptRunRequest {
    pub source: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct VirtualIotShellResponse {
    pub backend: VirtualIotBackend,
    pub shell: serde_json::Value,
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateVirtualIotRequest {
    pub name: String,
    pub controller_type: String,
    pub region: String,
    pub sim_rate_ms: i32,
    pub enabled_sensors: Vec<String>,
    pub event_definitions: Vec<VirtualIotEventDefinition>,
}

#[derive(Serialize, Clone, Debug)]
pub struct UpdateVirtualIotConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_sensors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sim_rate_ms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_definitions: Option<Vec<VirtualIotEventDefinition>>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct SessionListResponse {
    pub sessions: Vec<Session>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Session {
    pub id: String,
    pub device_id: String,
    pub tenant_id: String,
    pub status: String,
    pub expires_at: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct JobListResponse {
    pub jobs: Vec<Job>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Job {
    pub id: String,
    pub tenant_id: String,
    pub test_suite: String,
    pub status: String,
    pub priority: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct JobDetail {
    pub job: JobWithCases,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct JobWithCases {
    pub id: String,
    pub tenant_id: String,
    pub test_suite: String,
    pub status: String,
    pub priority: Option<String>,
    pub created_at: Option<String>,
    #[serde(default)]
    pub test_cases: Vec<TestCase>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct TestCase {
    pub id: String,
    pub name: String,
    pub status: String,
    pub attempt: i32,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Artifact {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub size_bytes: Option<i64>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct JobProgress {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub running: usize,
    pub queued: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct BuildListResponse {
    pub builds: Vec<BuildJob>,
    pub total: usize,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    #[serde(default)]
    pub status_counts: BuildStatusCounts,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct BuildStatusCounts {
    pub queued: i64,
    pub provisioning: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub cancelled: i64,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct BuildJob {
    pub id: String,
    pub tenant_id: String,
    pub runner_id: Option<String>,
    pub device_id: Option<String>,
    pub build_type: String,
    pub status: String,
    pub source_ref: String,
    pub repository_url: Option<String>,
    pub git_ref: Option<String>,
    pub error_message: Option<String>,
    pub queued_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    #[serde(default)]
    pub runtime: serde_json::Value,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct BuildDetailResponse {
    pub build: BuildJob,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct BuildLogsResponse {
    pub build_id: String,
    pub status: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct BuildArtifact {
    pub id: String,
    pub artifact_type: String,
    pub file_name: String,
    pub storage_path: String,
    pub size_bytes: i64,
    pub checksum_sha256: Option<String>,
    pub retention_until: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct BuildArtifactsResponse {
    pub build_id: String,
    pub artifacts: Vec<BuildArtifact>,
    pub total: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct SubmitIosBuildRequest {
    pub source_ref: String,
    pub project_path: String,
    pub scheme: String,
    pub configuration: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target_device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub required_runner_capabilities: Vec<String>,
    #[serde(default)]
    pub required_runner_labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_ttl_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SubmitIotDeployRequest {
    pub source_ref: String,
    pub project_path: String,
    pub target_device_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deploy_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub required_runner_capabilities: Vec<String>,
    #[serde(default)]
    pub required_runner_labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_ttl_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SubmitQuantumProgramRequest {
    pub source_ref: String,
    pub project_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veeros_host_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub required_runner_capabilities: Vec<String>,
    #[serde(default)]
    pub required_runner_labels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lease_ttl_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateIosStarterProjectRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct CreateIosStarterProjectResponse {
    pub module_name: String,
    pub project_dir: String,
    pub archive_path: Option<String>,
    pub download_path: Option<String>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct IosStarterDefaultsResponse {
    pub username: String,
    pub suggested_app_name: String,
    pub suggested_organization_id: String,
    pub xcode_developer_dir: Option<String>,
    pub xcode_version: Option<String>,
    pub default_team_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WorkspaceProjectFile {
    pub path: String,
    pub content: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct WorkspaceArchiveExportRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    pub files: Vec<WorkspaceProjectFile>,
}

#[derive(Serialize, Clone, Debug)]
pub struct WorkspaceArchiveImportRequest {
    pub archive_base64: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct WorkspaceArchiveImportResponse {
    pub files: Vec<WorkspaceProjectFile>,
    pub skipped_binary_files: usize,
}

pub async fn list_builds(
    limit: i64,
    offset: i64,
    status: Option<&str>,
    device: Option<&str>,
    source: Option<&str>,
) -> Result<BuildListResponse, String> {
    let mut path = format!("/builds?limit={limit}&offset={offset}");
    if let Some(status) = status {
        path.push_str("&status=");
        let encoded = js_sys::encode_uri_component(status);
        path.push_str(&encoded.as_string().unwrap_or_default());
    }
    if let Some(device) = device {
        path.push_str("&device=");
        let encoded = js_sys::encode_uri_component(device);
        path.push_str(&encoded.as_string().unwrap_or_default());
    }
    if let Some(source) = source {
        path.push_str("&source=");
        let encoded = js_sys::encode_uri_component(source);
        path.push_str(&encoded.as_string().unwrap_or_default());
    }
    get::<BuildListResponse>(&path).await
}

pub async fn get_build(id: &str) -> Result<BuildDetailResponse, String> {
    get::<BuildDetailResponse>(&format!("/builds/{id}")).await
}

pub async fn get_build_logs(id: &str) -> Result<BuildLogsResponse, String> {
    get::<BuildLogsResponse>(&format!("/builds/{id}/logs")).await
}

pub async fn get_build_artifacts(id: &str) -> Result<BuildArtifactsResponse, String> {
    get::<BuildArtifactsResponse>(&format!("/builds/{id}/artifacts")).await
}

pub async fn submit_ios_build(req: &SubmitIosBuildRequest) -> Result<serde_json::Value, String> {
    post::<SubmitIosBuildRequest, serde_json::Value>("/builds/ios", req).await
}

pub async fn submit_iot_deploy(req: &SubmitIotDeployRequest) -> Result<serde_json::Value, String> {
    post::<SubmitIotDeployRequest, serde_json::Value>("/builds/iot/deploy", req).await
}

pub async fn submit_quantum_program(req: &SubmitQuantumProgramRequest) -> Result<serde_json::Value, String> {
    post::<SubmitQuantumProgramRequest, serde_json::Value>("/builds/quantum/run", req).await
}

pub async fn get_ios_starter_defaults() -> Result<IosStarterDefaultsResponse, String> {
    get::<IosStarterDefaultsResponse>("/starter-projects/ios/defaults").await
}

pub async fn create_ios_starter_project(
    req: &CreateIosStarterProjectRequest,
) -> Result<CreateIosStarterProjectResponse, String> {
    post::<CreateIosStarterProjectRequest, CreateIosStarterProjectResponse>("/starter-projects/ios", req).await
}

pub async fn download_ios_starter_archive(module_name: &str) -> Result<Vec<u8>, String> {
    let encoded = js_sys::encode_uri_component(module_name)
        .as_string()
        .unwrap_or_default();
    let path = format!("/starter-projects/ios/archive/{encoded}");
    let resp = Request::get(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    resp.binary().await.map_err(|e| e.to_string())
}

pub async fn export_workspace_archive(
    req: &WorkspaceArchiveExportRequest,
) -> Result<Vec<u8>, String> {
    let resp = Request::post(&format!("{}/starter-projects/workspace/archive", base_url()))
        .header("Authorization", &auth_header())
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status() == 401 {
        handle_unauthorized();
        return Err("Session expired".into());
    }

    if !resp.ok() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    resp.binary().await.map_err(|e| e.to_string())
}

pub async fn import_workspace_archive(
    req: &WorkspaceArchiveImportRequest,
) -> Result<WorkspaceArchiveImportResponse, String> {
    post::<WorkspaceArchiveImportRequest, WorkspaceArchiveImportResponse>("/starter-projects/workspace/import", req).await
}

// ─── Saved workspace projects (server-side persistence) ──────────────────────

#[derive(Serialize, Clone, Debug)]
pub struct SaveWorkspaceProjectRequest {
    pub name: String,
    pub files: Vec<WorkspaceProjectFile>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct SaveWorkspaceProjectResponse {
    pub id: String,
    pub name: String,
    pub file_count: i32,
    pub updated_at: String,
    pub created_at: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WorkspaceProjectSummary {
    pub id: String,
    pub name: String,
    pub file_count: i32,
    pub updated_at: String,
    pub created_at: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ListWorkspaceProjectsResponse {
    #[serde(default)]
    pub projects: Vec<WorkspaceProjectSummary>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct WorkspaceProjectDetail {
    pub id: String,
    pub name: String,
    pub files: Vec<WorkspaceProjectFile>,
    pub file_count: i32,
    pub updated_at: String,
    pub created_at: String,
}

pub async fn save_workspace_project(
    req: &SaveWorkspaceProjectRequest,
) -> Result<SaveWorkspaceProjectResponse, String> {
    post::<SaveWorkspaceProjectRequest, SaveWorkspaceProjectResponse>(
        "/starter-projects/workspace/save",
        req,
    )
    .await
}

pub async fn list_workspace_projects() -> Result<ListWorkspaceProjectsResponse, String> {
    get::<ListWorkspaceProjectsResponse>("/starter-projects/workspace/projects").await
}

pub async fn get_workspace_project(name: &str) -> Result<WorkspaceProjectDetail, String> {
    let encoded = encode_path_segment(name);
    get::<WorkspaceProjectDetail>(&format!(
        "/starter-projects/workspace/projects/{encoded}"
    ))
    .await
}

pub async fn delete_workspace_project(name: &str) -> Result<(), String> {
    let encoded = encode_path_segment(name);
    delete(&format!("/starter-projects/workspace/projects/{encoded}")).await
}

fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' || ch == '~' {
            out.push(ch);
        } else {
            let mut buf = [0u8; 4];
            for b in ch.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

pub async fn cancel_build(id: &str) -> Result<(), String> {
    post_empty(&format!("/builds/{id}/cancel")).await
}

#[derive(Deserialize, Clone, Debug)]
pub struct DeviceHealth {
    pub device_id: String,
    pub cpu_usage: Option<f64>,
    pub memory_usage: Option<f64>,
    pub battery_level: Option<f64>,
    pub temperature: Option<f64>,
    pub last_heartbeat: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct HealthResponse {
    pub status: String,
}

// ─── Quantum API types ────────────────────────────────────────────────────────

#[derive(Deserialize, Clone, Debug)]
pub struct QBackend {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(rename = "type")]
    pub backend_type: String,
    pub qubits: Option<u32>,
    pub description: String,
    pub available: bool,
    #[serde(default)]
    pub quantum_volume: Option<u32>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct QBackendsResponse {
    pub backends: Vec<QBackend>,
}

#[derive(Serialize, Clone, Debug)]
pub struct QSimulateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qasm: Option<String>,
    pub shots: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct QHistogramBin {
    pub state: String,
    pub count: u32,
    pub probability: f64,
}

/// Accept both histogram payload shapes:
/// - array: [{ state, count, probability }, ...]
/// - map:   { "00": 70, "11": 58 }
fn deserialize_histogram_bins<'de, D>(deserializer: D) -> Result<Vec<QHistogramBin>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;

    match value {
        serde_json::Value::Array(items) => {
            let mut bins = Vec::new();
            for item in items {
                if let serde_json::Value::Object(obj) = item {
                    let state = obj
                        .get("state")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let count = obj
                        .get("count")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0) as u32;
                    let probability = obj
                        .get("probability")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);

                    if !state.is_empty() {
                        bins.push(QHistogramBin { state, count, probability });
                    }
                }
            }
            Ok(bins)
        }
        serde_json::Value::Object(map) => {
            let total: u64 = map.values().filter_map(serde_json::Value::as_u64).sum();
            let denom = if total == 0 { 1.0 } else { total as f64 };

            let mut bins: Vec<QHistogramBin> = map
                .into_iter()
                .filter_map(|(state, v)| {
                    let count = v.as_u64()? as u32;
                    Some(QHistogramBin {
                        state,
                        count,
                        probability: (count as f64) / denom,
                    })
                })
                .collect();

            // Keep display stable and easier to read in UI.
            bins.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.state.cmp(&b.state)));
            Ok(bins)
        }
        _ => Ok(Vec::new()),
    }
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct QSimulateResponse {
    #[serde(default, deserialize_with = "deserialize_histogram_bins")]
    pub histogram: Vec<QHistogramBin>,
    pub shots: Option<u32>,
    pub backend: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct QIbmRunRequest {
    pub token: String,
    pub instance: String,
    pub backend: String,
    pub qasm: String,
    pub shots: u32,
    pub histogram: bool,
    pub endpoint_staging: bool,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AgentInfo {
    pub id: String,
    pub name: Option<String>,
    pub hostname: Option<String>,
    pub status: Option<String>,
    pub streaming_url: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct AgentsListResponse {
    pub agents: Vec<AgentInfo>,
}

/// Fetch the static quantum backends catalog (no agent required).
pub async fn fetch_quantum_backends() -> Result<QBackendsResponse, String> {
    get::<QBackendsResponse>("/quantum/backends").await
}

/// Fetch quantum backends available on a specific agent.
pub async fn fetch_agent_quantum_backends(agent_id: &str) -> Result<QBackendsResponse, String> {
    get::<QBackendsResponse>(&format!("/quantum/agents/{agent_id}/backends")).await
}

/// Run a simulation on a specific agent.
pub async fn quantum_simulate(agent_id: &str, req: &QSimulateRequest) -> Result<QSimulateResponse, String> {
    post::<QSimulateRequest, QSimulateResponse>(&format!("/quantum/agents/{agent_id}/simulate"), req).await
}

/// Submit a circuit to IBM Quantum via a specific agent.
pub async fn quantum_ibm_run(agent_id: &str, req: &QIbmRunRequest) -> Result<serde_json::Value, String> {
    post::<QIbmRunRequest, serde_json::Value>(&format!("/quantum/agents/{agent_id}/ibm/run"), req).await
}

/// List all registered agents.
pub async fn list_agents() -> Result<AgentsListResponse, String> {
    get::<AgentsListResponse>("/agents").await
}

// ─── Discovery API types ────────────────────────────────────────────────────

#[derive(Deserialize, Clone, Debug)]
pub struct DiscoveredHost {
    pub id: String,
    pub agent_id: String,
    pub host_id: String,
    pub hostname: String,
    pub ip_address: String,
    pub port: i32,
    pub os_version: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub quantum_capable: bool,
    pub quantum_qubits: Option<i32>,
    #[serde(default)]
    pub quantum_backends: Vec<String>,
    pub verified: bool,
    pub last_seen_at: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct DiscoveryHostsResponse {
    pub hosts: Vec<DiscoveredHost>,
    pub count: usize,
}

/// Convert a discovered VeerOS host into the common `Device` shape so it can
/// be merged into any page that already handles `Device` lists.
pub fn veeros_host_to_device(h: &DiscoveredHost) -> Device {
    use serde_json::json;
    let status = {
        // last_seen_at is an ISO-8601 string; treat hosts seen within 5 min as online
        let online = js_sys::Date::new_0().get_time() / 1000.0
            - chrono_ts_to_secs(&h.last_seen_at) as f64;
        if online < 300.0 { "online" } else { "offline" }
    };
    let mut caps = h.capabilities.clone();
    // Encode shell routing metadata in capabilities so any page can build an in-browser shell link.
    caps.push(format!("veeros_ip:{}", h.ip_address));
    caps.push(format!("veeros_port:{}", h.port));
    caps.push(format!("veeros_agent_id:{}", h.agent_id));
    if h.verified { caps.push("veeros_verified".into()); }

    Device {
        id: h.id.clone(),
        name: h.hostname.clone(),
        serial: Some(format!("{}:{}", h.ip_address, h.port)),
        os: "veeros".into(),
        os_version: h.os_version.clone().unwrap_or_else(|| "VeerOS".into()),
        kind: "veeros_host".into(),
        status: status.into(),
        region: "VeerOS".into(),
        agent_id: Some(h.agent_id.clone()),
        capabilities: json!(caps),
        last_heartbeat: Some(h.last_seen_at.clone()),
        created_at: None,
    }
}

/// Parse an ISO-8601 timestamp string to Unix seconds (WASM-compatible).
fn chrono_ts_to_secs(ts: &str) -> i64 {
    // Attempt rough parse: "2026-05-04T12:34:56Z" → seconds since epoch
    js_sys::Date::parse(ts) as i64 / 1000
}

/// Fetch regular devices AND discovered VeerOS hosts and return a merged list.
pub async fn fetch_all_devices() -> DeviceListResponse {
    let reg = get::<DeviceListResponse>("/devices").await.unwrap_or_default();
    let disc = fetch_discovered_hosts(false).await;
    let mut devices = reg.devices;
    if let Ok(d) = disc {
        // Avoid duplicates: skip any VeerOS host whose serial (ip:port) already
        // appears in the registered device list.
        let existing_serials: std::collections::HashSet<String> = devices
            .iter()
            .filter_map(|d| d.serial.clone())
            .collect();
        for h in &d.hosts {
            let serial = format!("{}:{}", h.ip_address, h.port);
            if !existing_serials.contains(&serial) {
                devices.push(veeros_host_to_device(h));
            }
        }
    }
    let total = devices.len();
    DeviceListResponse { devices, total }
}

/// List all discovered VeerOS hosts. Pass `quantum_only=true` to filter.
pub async fn fetch_discovered_hosts(quantum_only: bool) -> Result<DiscoveryHostsResponse, String> {
    let path = if quantum_only {
        "/discovery/hosts?quantum=true"
    } else {
        "/discovery/hosts"
    };
    get::<DiscoveryHostsResponse>(path).await
}

/// Fetch all registered agents with their capabilities.
pub async fn fetch_all_agents() -> AgentListResponse {
    get::<AgentListResponse>("/agents").await.unwrap_or_default()
}

/// Fetch a single agent by ID.
pub async fn fetch_agent(agent_id: &str) -> Result<Agent, String> {
    get::<Agent>(&format!("/agents/{agent_id}")).await
}

// ─── External Integrations API ─────────────────────────────────────────────

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MqttSubscriptionsResponse {
    pub subscriptions: Vec<MqttSubscription>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct MqttSubscription {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub broker_host: String,
    pub broker_port: u16,
    pub topic_filter: String,
    pub client_id: String,
    pub username: Option<String>,
    pub use_tls: bool,
    pub reconnect_secs: u64,
    pub created_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateMqttSubscriptionRequest {
    pub name: String,
    pub broker_host: String,
    pub broker_port: u16,
    pub topic_filter: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub use_tls: bool,
    pub reconnect_secs: u64,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MqttSubscriptionEnvelope {
    pub subscription: MqttSubscription,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MqttPublishersResponse {
    pub publishers: Vec<MqttPublisher>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct MqttPublisher {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub broker_host: String,
    pub broker_port: u16,
    pub topic_root: String,
    pub client_id: String,
    pub username: Option<String>,
    pub use_tls: bool,
    pub created_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateMqttPublisherRequest {
    pub name: String,
    pub broker_host: String,
    pub broker_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub use_tls: bool,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MqttPublisherEnvelope {
    pub publisher: MqttPublisher,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct CloudSinksResponse {
    pub sinks: Vec<CloudSink>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudSink {
    AwsS3PresignedPut {
        id: String,
        tenant_id: String,
        name: String,
        presigned_put_url: String,
        created_at: String,
    },
    AwsIotCoreHttps {
        id: String,
        tenant_id: String,
        name: String,
        publish_url: String,
        created_at: String,
    },
    AzureIotHub {
        id: String,
        tenant_id: String,
        name: String,
        host_name: String,
        device_id: String,
        api_version: String,
        created_at: String,
    },
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateCloudSinkRequest {
    AwsS3PresignedPut {
        name: String,
        presigned_put_url: String,
    },
    AwsIotCoreHttps {
        name: String,
        publish_url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        authorization_header: Option<String>,
    },
    AzureIotHub {
        name: String,
        host_name: String,
        device_id: String,
        sas_token: String,
        api_version: String,
    },
}

#[derive(Deserialize, Clone, Debug)]
pub struct CloudSinkEnvelope {
    pub sink: CloudSink,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct DeliveryAttemptsResponse {
    pub attempts: Vec<DeliveryAttempt>,
    pub total: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct DeliveryAttempt {
    pub id: String,
    pub endpoint_id: String,
    pub tenant_id: Option<String>,
    pub device_id: Option<String>,
    pub sensor: Option<String>,
    pub status: String,
    pub attempt: i32,
    pub error_message: Option<String>,
    pub created_at: String,
}

pub async fn list_mqtt_subscriptions() -> Result<MqttSubscriptionsResponse, String> {
    get::<MqttSubscriptionsResponse>("/integrations/mqtt/subscriptions").await
}

pub async fn create_mqtt_subscription(
    req: &CreateMqttSubscriptionRequest,
) -> Result<MqttSubscriptionEnvelope, String> {
    post::<CreateMqttSubscriptionRequest, MqttSubscriptionEnvelope>(
        "/integrations/mqtt/subscriptions",
        req,
    )
    .await
}

pub async fn delete_mqtt_subscription(id: &str) -> Result<(), String> {
    delete(&format!("/integrations/mqtt/subscriptions/{id}")).await
}

pub async fn list_mqtt_publishers() -> Result<MqttPublishersResponse, String> {
    get::<MqttPublishersResponse>("/integrations/mqtt/publishers").await
}

pub async fn create_mqtt_publisher(
    req: &CreateMqttPublisherRequest,
) -> Result<MqttPublisherEnvelope, String> {
    post::<CreateMqttPublisherRequest, MqttPublisherEnvelope>(
        "/integrations/mqtt/publishers",
        req,
    )
    .await
}

pub async fn delete_mqtt_publisher(id: &str) -> Result<(), String> {
    delete(&format!("/integrations/mqtt/publishers/{id}")).await
}

pub async fn list_cloud_sinks() -> Result<CloudSinksResponse, String> {
    get::<CloudSinksResponse>("/integrations/cloud/sinks").await
}

pub async fn create_cloud_sink(req: &CreateCloudSinkRequest) -> Result<CloudSinkEnvelope, String> {
    post::<CreateCloudSinkRequest, CloudSinkEnvelope>("/integrations/cloud/sinks", req).await
}

pub async fn delete_cloud_sink(id: &str) -> Result<(), String> {
    delete(&format!("/integrations/cloud/sinks/{id}")).await
}

pub async fn list_delivery_attempts(
    status: Option<&str>,
    limit: i64,
) -> Result<DeliveryAttemptsResponse, String> {
    let mut path = format!("/integrations/delivery-attempts?limit={limit}");
    if let Some(status) = status {
        path.push_str("&status=");
        let encoded = js_sys::encode_uri_component(status);
        path.push_str(&encoded.as_string().unwrap_or_default());
    }
    get::<DeliveryAttemptsResponse>(&path).await
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlNodeListResponse {
    #[serde(default)]
    pub nodes: Vec<MlNode>,
    #[serde(default)]
    pub total: usize,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlNode {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub runner_id: String,
    #[serde(default)]
    pub chip: String,
    pub cpu_cores: Option<i32>,
    pub gpu_cores: Option<i32>,
    pub ane_cores: Option<i32>,
    pub unified_memory_gb: Option<i32>,
    #[serde(default)]
    pub link_mbps: i32,
    #[serde(default)]
    pub mlx_version: String,
    pub free_memory_gb: Option<f64>,
    #[serde(default)]
    pub worker_host: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub status: String,
    pub last_heartbeat_at: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlClusterListResponse {
    #[serde(default)]
    pub clusters: Vec<MlClusterEnvelope>,
    #[serde(default)]
    pub total: usize,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlClusterEnvelope {
    pub cluster: MlCluster,
    #[serde(default)]
    pub members: Vec<MlClusterMember>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlCluster {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub rank0_url: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlClusterMember {
    #[serde(default)]
    pub runner_id: String,
    #[serde(default)]
    pub rank: i32,
    #[serde(default)]
    pub dataplane_host: String,
    #[serde(default)]
    pub dataplane_port: i32,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlJobListResponse {
    #[serde(default)]
    pub jobs: Vec<MlJob>,
    #[serde(default)]
    pub total: usize,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct MlJob {
    #[serde(default)]
    pub id: String,
    pub cluster_id: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub quantization: String,
    #[serde(default)]
    pub context_length: i32,
    #[serde(default)]
    pub metrics: serde_json::Value,
    pub error_message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateMlClusterRequest {
    pub name: String,
    pub role: String,
    pub size: usize,
    pub interconnect_cidrs: Vec<String>,
    pub required_capabilities: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateMlJobRequest {
    pub cluster_id: String,
    pub kind: String,
    pub model_id: String,
    pub quantization: String,
    pub context_length: i32,
    pub n_layers: i32,
    pub hidden_size: i32,
}

#[derive(Serialize, Clone, Debug)]
pub struct MlChatRequest {
    pub model: String,
    pub messages: Vec<MlChatMessage>,
    pub max_tokens: u32,
    pub cluster_id: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct MlChatMessage {
    pub role: String,
    pub content: String,
}

pub async fn list_ml_nodes() -> Result<MlNodeListResponse, String> {
    get("/ml/nodes").await
}

pub async fn list_ml_clusters() -> Result<MlClusterListResponse, String> {
    get("/ml/clusters").await
}

pub async fn list_ml_jobs() -> Result<MlJobListResponse, String> {
    get("/ml/jobs").await
}

pub async fn create_ml_cluster(req: &CreateMlClusterRequest) -> Result<serde_json::Value, String> {
    post("/ml/clusters", req).await
}

pub async fn create_ml_job(req: &CreateMlJobRequest) -> Result<serde_json::Value, String> {
    post("/ml/jobs", req).await
}

pub async fn ml_chat(req: &MlChatRequest) -> Result<serde_json::Value, String> {
    post("/ml/v1/chat/completions", req).await
}

pub async fn cancel_ml_job(id: &str) -> Result<serde_json::Value, String> {
    post(&format!("/ml/jobs/{id}/cancel"), &serde_json::json!({})).await
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ResourceListResponse {
    #[serde(default)]
    pub resources: Vec<ResourceItem>,
}

#[derive(Deserialize, Clone, Debug, Default)]
pub struct ResourceItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub install_hint: String,
    #[serde(default)]
    pub available: bool,
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub download_path: String,
}

pub async fn list_resources() -> Result<ResourceListResponse, String> {
    match get("/resources").await {
        Ok(resp) => Ok(resp),
        Err(_) => Ok(fallback_resources()),
    }
}

fn fallback_resources() -> ResourceListResponse {
    ResourceListResponse {
        resources: vec![
            ResourceItem {
                id: "host-macos-arm64".into(),
                name: "Host worker — macOS Apple Silicon".into(),
                kind: "host-package".into(),
                platform: "macos".into(),
                arch: "aarch64".into(),
                filename: "edgefabric-host-aarch64-apple-darwin.tar.gz".into(),
                summary: "Install on a Mac mini or MacBook: OpenSSH, VeerSetu overlay, and ef host join.".into(),
                install_hint: "tar xzf edgefabric-host-aarch64-apple-darwin.tar.gz && export EF_API_KEY=… && ./install.sh".into(),
                available: true,
                size_bytes: None,
                download_path: "/downloads/edgefabric-host-aarch64-apple-darwin.tar.gz".into(),
            },
            ResourceItem {
                id: "host-linux-x64".into(),
                name: "Host worker — Linux x86_64".into(),
                kind: "host-package".into(),
                platform: "linux".into(),
                arch: "x86_64".into(),
                filename: "edgefabric-host-x86_64-unknown-linux-gnu.tar.gz".into(),
                summary: "Same generic SSH worker for Linux servers.".into(),
                install_hint: "Unpack and run ./install.sh with EF_API_KEY.".into(),
                available: true,
                size_bytes: None,
                download_path: "/downloads/edgefabric-host-x86_64-unknown-linux-gnu.tar.gz".into(),
            },
            ResourceItem {
                id: "host-linux-arm64".into(),
                name: "Host worker — Linux ARM64".into(),
                kind: "host-package".into(),
                platform: "linux".into(),
                arch: "aarch64".into(),
                filename: "edgefabric-host-aarch64-unknown-linux-gnu.tar.gz".into(),
                summary: "ARM Linux servers and cloud instances.".into(),
                install_hint: "Unpack and run ./install.sh with EF_API_KEY.".into(),
                available: false,
                size_bytes: None,
                download_path: "/api/v1/resources/files/edgefabric-host-aarch64-unknown-linux-gnu.tar.gz".into(),
            },
            ResourceItem {
                id: "host-windows-x64".into(),
                name: "Host worker — Windows x64".into(),
                kind: "host-package".into(),
                platform: "windows".into(),
                arch: "x86_64".into(),
                filename: "edgefabric-host-x86_64-pc-windows-msvc.zip".into(),
                summary: "Windows OpenSSH Server plus ef host join.".into(),
                install_hint: "Expand the zip, then ef host join --api-key …".into(),
                available: false,
                size_bytes: None,
                download_path: "/api/v1/resources/files/edgefabric-host-x86_64-pc-windows-msvc.zip".into(),
            },
        ],
    }
}
