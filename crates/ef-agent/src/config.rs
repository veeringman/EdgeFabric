//! Agent-specific configuration loader.
//!
//! The agent reuses `AppConfig::load()` from ef-core (TOML + env vars)
//! but adds agent-specific validation and a dedicated `AgentRuntime` config
//! that captures only the fields the agent binary needs.

use ef_core::config::{AgentConfig, LoggingConfig, NatsConfig, StreamingConfig};
use ef_core::telemetry::TelemetryConfig;

/// Runtime configuration for the edge agent, extracted from the full `AppConfig`.
#[derive(Debug, Clone)]
pub struct AgentRuntime {
    pub agent: AgentConfig,
    pub nats: NatsConfig,
    pub streaming: StreamingConfig,
    pub logging: LoggingConfig,
    pub telemetry: TelemetryConfig,
}

impl AgentRuntime {
    /// Load and validate agent configuration.
    pub fn load() -> anyhow::Result<Self> {
        let cfg = ef_core::config::AppConfig::load()
            .map_err(|e| anyhow::anyhow!("Config load failed: {e}"))?;

        // Validate
        if cfg.agent.control_plane_url.is_empty() {
            anyhow::bail!("agent.control_plane_url must not be empty");
        }
        if cfg.agent.heartbeat_interval_seconds == 0 {
            anyhow::bail!("agent.heartbeat_interval_seconds must be > 0");
        }
        if cfg.agent.device_scan_interval_seconds == 0 {
            anyhow::bail!("agent.device_scan_interval_seconds must be > 0");
        }

        Ok(Self {
            agent: cfg.agent,
            nats: cfg.nats,
            streaming: cfg.streaming,
            logging: cfg.logging,
            telemetry: cfg.telemetry,
        })
    }
}
