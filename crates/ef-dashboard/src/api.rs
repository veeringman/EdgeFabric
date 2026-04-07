use gloo_net::http::Request;
use gloo_storage::Storage;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

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

/// Generic JSON GET
pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let resp = Request::get(&format!("{}{path}", base_url()))
        .header("Authorization", &auth_header())
        .send()
        .await
        .map_err(|e| e.to_string())?;

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
