use serde::Deserialize;

/// Top-level application configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub nats: NatsConfig,
    pub auth: AuthConfig,
    pub streaming: StreamingConfig,
    pub agent: AgentConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub telemetry: crate::telemetry::TelemetryConfig,
    #[serde(default)]
    pub veeros: VeerOsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub grpc_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NatsConfig {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub token_expiry_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamingConfig {
    pub signaling_port: u16,
    pub turn_server: String,
    pub turn_username: String,
    pub turn_password: String,
    pub stun_servers: Vec<String>,
    pub max_resolution_width: u32,
    pub max_resolution_height: u32,
    pub max_fps: u32,
    pub target_bitrate_kbps: u32,
    pub hardware_accel: bool,
    pub recording_enabled: bool,
    pub recording_dir: String,
    pub latency_target_ms: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub control_plane_url: String,
    pub heartbeat_interval_seconds: u64,
    pub device_scan_interval_seconds: u64,
    pub agent_id: String,
    pub region: String,
    pub auto_update_enabled: bool,
    pub auto_update_url: String,
    pub reconnect_interval_seconds: u64,
    pub max_reconnect_attempts: u32,
    /// Skip TLS certificate verification (IP addresses, self-signed certs, test setups).
    #[serde(default)]
    pub tls_skip_verify: bool,
    /// Port for the agent's local streaming HTTP/WS server (default 7070).
    #[serde(default)]
    pub streaming_port: Option<u16>,
    /// The agent's externally reachable IP/hostname (auto-detected if absent).
    #[serde(default)]
    pub advertise_address: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VeerOsConfig {
    pub root_dir: String,
    pub vm_script_path: String,
    pub veer_connect_path: String,
    pub default_target: String,
    pub default_memory_mb: u32,
    pub default_network_mode: String,
    pub default_region: String,
}

impl Default for VeerOsConfig {
    fn default() -> Self {
        Self {
            root_dir: "/home/vijay/rnd/VeerOS".into(),
            vm_script_path: "/home/vijay/rnd/VeerOS/scripts/veeros-vm".into(),
            veer_connect_path: "/home/vijay/rnd/VeerOS/target/release/veer-connect".into(),
            default_target: "esp32c6".into(),
            default_memory_mb: 128,
            default_network_mode: "nat".into(),
            default_region: "local".into(),
        }
    }
}

impl AppConfig {
    /// Load configuration from TOML files + environment variable overrides.
    ///
    /// Merge order (later overrides earlier):
    /// 1. `config/default.toml`
    /// 2. `config/{run_mode}.toml` (e.g. `development`, `production`)
    /// 3. Environment variables prefixed with `EF_` (double underscore = nesting separator)
    pub fn load() -> Result<Self, config::ConfigError> {
        let run_mode = std::env::var("EF_RUN_MODE").unwrap_or_else(|_| "development".into());

        let cfg = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(config::File::with_name(&format!("config/{run_mode}")).required(false))
            .add_source(
                config::Environment::with_prefix("EF")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        cfg.try_deserialize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserialize_from_toml() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 8080
grpc_port = 50051

[database]
url = "postgres://localhost/test"
max_connections = 5

[redis]
url = "redis://localhost:6379"

[nats]
url = "nats://localhost:4222"

[auth]
jwt_secret = "test-secret"
token_expiry_seconds = 3600

[streaming]
signaling_port = 8443
turn_server = "turn:localhost:3478"
turn_username = "user"
turn_password = "pass"
stun_servers = ["stun:stun.l.google.com:19302"]
max_resolution_width = 1920
max_resolution_height = 1080
max_fps = 30
target_bitrate_kbps = 2000
hardware_accel = false
recording_enabled = false
recording_dir = "/tmp/rec"
latency_target_ms = 200

[agent]
control_plane_url = "https://localhost:50051"
heartbeat_interval_seconds = 10
device_scan_interval_seconds = 30
agent_id = "test-agent"
region = "test"
auto_update_enabled = false
auto_update_url = "https://example.com"
reconnect_interval_seconds = 5
max_reconnect_attempts = 3

[logging]
level = "debug"
format = "pretty"

[veeros]
root_dir = "/home/vijay/rnd/VeerOS"
vm_script_path = "/home/vijay/rnd/VeerOS/scripts/veeros-vm"
veer_connect_path = "/home/vijay/rnd/VeerOS/target/release/veer-connect"
default_target = "esp32c6"
default_memory_mb = 128
default_network_mode = "nat"
default_region = "local"
"#;
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.database.max_connections, 5);
        assert_eq!(cfg.auth.jwt_secret, "test-secret");
        assert_eq!(cfg.agent.region, "test");
        assert_eq!(cfg.logging.level, "debug");
    assert_eq!(cfg.veeros.default_target, "esp32c6");
    }

    #[test]
    fn telemetry_config_defaults() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 8080
grpc_port = 50051

[database]
url = "postgres://localhost/test"
max_connections = 5

[redis]
url = "redis://localhost:6379"

[nats]
url = "nats://localhost:4222"

[auth]
jwt_secret = "s"
token_expiry_seconds = 60

[streaming]
signaling_port = 8443
turn_server = "turn:localhost:3478"
turn_username = "u"
turn_password = "p"
stun_servers = []
max_resolution_width = 1920
max_resolution_height = 1080
max_fps = 30
target_bitrate_kbps = 2000
hardware_accel = false
recording_enabled = false
recording_dir = "/tmp"
latency_target_ms = 200

[agent]
control_plane_url = "https://localhost:50051"
heartbeat_interval_seconds = 10
device_scan_interval_seconds = 30
agent_id = "a"
region = "r"
auto_update_enabled = false
auto_update_url = "https://example.com"
reconnect_interval_seconds = 5
max_reconnect_attempts = 0

[logging]
level = "info"
format = "json"

[veeros]
root_dir = "/tmp/veeros"
vm_script_path = "/tmp/veeros/scripts/veeros-vm"
veer_connect_path = "/tmp/veeros/target/release/veer-connect"
default_target = "esp32c6"
default_memory_mb = 128
default_network_mode = "nat"
default_region = "local"
"#;
        // telemetry section omitted — should use defaults
        let cfg: AppConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.telemetry.otlp_endpoint, "");
    }
}
