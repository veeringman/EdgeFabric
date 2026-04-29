use gloo_net::http::Request;
use gloo_storage::Storage;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

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
    let Some(window) = web_sys::window() else {
        return "ws://127.0.0.1:6060/api/v1".to_string();
    };

    let hostname = window
        .location()
        .hostname()
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let scheme = window
        .location()
        .protocol()
        .ok()
        .map(|p| if p == "https:" { "wss" } else { "ws" })
        .unwrap_or("ws");

    // Direct API WS is more stable than Trunk's WS proxy for terminal streams.
    format!("{scheme}://{hostname}:6060/api/v1")
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

#[derive(Deserialize, Clone, Debug, Default)]
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
    pub capabilities: serde_json::Value,
    pub last_heartbeat: Option<String>,
    pub created_at: Option<String>,
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
    pub streaming_url: Option<String>,
    pub status: String,
    pub capabilities: Vec<String>,
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
