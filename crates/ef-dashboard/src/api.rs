use gloo_net::http::Request;
use gloo_storage::Storage;
use serde::{Deserialize, Serialize, de::{DeserializeOwned, Deserializer}};

/// Build API base URL from the browser's current hostname (port 6060).
fn base_url() -> String {
    let host = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());
    format!("http://{host}:6060/api/v1")
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
}

#[derive(Serialize, Clone, Debug)]
pub struct CreateVirtualIotBackendRequest {
    pub name: String,
    pub provider: String,
    pub controller_type: String,
    pub controller_types: Vec<String>,
    pub region: String,
    pub auto_start: bool,
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
