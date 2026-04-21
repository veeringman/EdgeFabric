use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

mod client;
mod config;
mod discovery;
mod executor;
mod heartbeat;
mod streaming;
mod streaming_server;
mod updater;
mod veeros_api;

use client::ControlPlaneClient;
use config::AgentRuntime;
use discovery::DeviceRegistry;

#[tokio::main]
async fn main() -> Result<()> {
    // ── P8.1 Load config ──────────────────────────────────────────
    let cfg = AgentRuntime::load()?;

    // ── P12.1 + P12.4 Unified telemetry init ─────────────────────
    let _telemetry = ef_core::telemetry::init(
        &cfg.logging.level,
        &cfg.logging.format,
        &cfg.telemetry,
    );

    tracing::info!(
        agent_id = %cfg.agent.agent_id,
        region = %cfg.agent.region,
        control_plane = %cfg.agent.control_plane_url,
        "Starting EdgeFabric Agent"
    );

    // ── Shutdown signal ───────────────────────────────────────────
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // ── P8.2 + P8.8 Connect to control plane (with reconnection) ─
    let cp_client = Arc::new(
        connect_with_retry(
            &cfg.agent.control_plane_url,
            &cfg.agent.agent_id,
            cfg.agent.reconnect_interval_seconds,
            cfg.agent.max_reconnect_attempts,
            cfg.agent.tls_skip_verify,
        )
        .await?,
    );

    // ── Shared device registry ────────────────────────────────────
    let registry = Arc::new(DeviceRegistry::new());

    // ── Build streaming URL for this agent ─────────────────────────
    let streaming_port = cfg.agent.streaming_port.unwrap_or(7070);
    let streaming_url = {
        let addr = cfg.agent.advertise_address.clone()
            .or_else(|| detect_local_ip())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        format!("http://{addr}:{streaming_port}")
    };
    tracing::info!(%streaming_url, "Agent streaming URL");

    // ── Detect agent capabilities ──────────────────────────────────
    let capabilities = detect_capabilities(&cfg);
    tracing::info!(capabilities = ?capabilities, "Agent capabilities");

    // ── Register this agent with the control plane ────────────────
    if let Err(e) = cp_client
        .register_agent(&cfg.agent.region, Some(&streaming_url), &capabilities)
        .await
    {
        tracing::warn!("Agent register failed (will retry on next heartbeat cycle): {e}");
    }

    // ── P8.3 Device discovery loop ────────────────────────────────
    let discovery_handle = discovery::spawn_discovery_loop(
        Arc::clone(&cp_client),
        Arc::clone(&registry),
        cfg.agent.agent_id.clone(),
        cfg.agent.region.clone(),
        cfg.agent.device_scan_interval_seconds,
        Some(streaming_url),
        shutdown_rx.clone(),
    );

    // ── P8.4 Heartbeat reporter ───────────────────────────────────
    let heartbeat_handle = heartbeat::spawn_heartbeat_reporter(
        Arc::clone(&cp_client),
        Arc::clone(&registry),
        cfg.agent.agent_id.clone(),
        cfg.agent.heartbeat_interval_seconds,
        shutdown_rx.clone(),
    );

    // ── P8.5 Command executor ─────────────────────────────────────
    let executor_handle = executor::spawn_command_executor(
        Arc::clone(&cp_client),
        Arc::clone(&registry),
        cfg.agent.heartbeat_interval_seconds, // poll at same rate as heartbeats
        shutdown_rx.clone(),
    );

    // ── P8.7 Auto-update ─────────────────────────────────────────
    let auto_updater = updater::AutoUpdater::new(
        &cfg.agent.auto_update_url,
        cfg.agent.auto_update_enabled,
    );
    let updater_handle = auto_updater.spawn_update_checker(
        3600, // check every hour
        shutdown_rx.clone(),
    );

    // ── P8.6 Streaming forwarder is available on demand ──────────
    let _streaming = streaming::StreamingForwarder::new(
        cfg.streaming.clone(),
        Arc::clone(&registry),
    );

    // ── Agent-local streaming HTTP/WS server ──────────────────────
    let streaming_port = cfg.agent.streaming_port.unwrap_or(7070);
    let veeros_cfg = cfg.veeros.clone();
    tokio::spawn(async move {
        if let Err(e) = streaming_server::start_streaming_server(streaming_port, veeros_cfg).await {
            tracing::error!("Streaming server failed: {e}");
        }
    });

    tracing::info!(streaming_port, "Agent running — all subsystems active");

    // ── P8.8 Wait for shutdown signal ─────────────────────────────
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received — stopping subsystems");

    // Signal all background tasks to stop
    let _ = shutdown_tx.send(true);

    // Wait for graceful shutdown (with timeout)
    let shutdown_timeout = Duration::from_secs(10);
    let _ = tokio::time::timeout(shutdown_timeout, async {
        let _ = discovery_handle.await;
        let _ = heartbeat_handle.await;
        let _ = executor_handle.await;
        let _ = updater_handle.await;
    })
    .await;

    tracing::info!("EdgeFabric Agent shut down gracefully");
    Ok(())
}

/// Connect to the control plane with exponential backoff.
/// `max_attempts = 0` means retry forever.
async fn connect_with_retry(
    url: &str,
    agent_id: &str,
    base_interval_secs: u64,
    max_attempts: u32,
    tls_skip_verify: bool,
) -> Result<ControlPlaneClient> {
    let client = ControlPlaneClient::new(url, agent_id, tls_skip_verify)?;

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        if client.health_check().await {
            tracing::info!("Connected to control plane at {url}");
            return Ok(client);
        }

        if max_attempts > 0 && attempt >= max_attempts {
            anyhow::bail!(
                "Failed to connect to control plane after {attempt} attempts"
            );
        }

        let backoff = Duration::from_secs(
            base_interval_secs * 2u64.pow((attempt - 1).min(5)),
        );
        tracing::warn!(
            attempt,
            backoff_secs = backoff.as_secs(),
            "Control plane unreachable — retrying"
        );
        tokio::time::sleep(backoff).await;
    }
}

/// Detect a LAN-reachable IP address for this host.
fn detect_local_ip() -> Option<String> {
    // Connect to a non-routable address to find the default outgoing interface IP.
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("10.255.255.255:1").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

/// Detect which high-level capabilities this agent host can serve.
/// Platform-level device capabilities (android, ios, iot) are always
/// advertised — discovery will prove them out. Richer capabilities are
/// probed for locally-installed tooling.
fn detect_capabilities(cfg: &config::AgentRuntime) -> Vec<String> {
    let mut caps: Vec<String> = vec!["android".into(), "ios".into(), "iot".into()];

    if std::path::Path::new(&cfg.veeros.vm_script_path).exists() {
        caps.push("veeros".into());
    }

    // Future: detect runner (cargo/gradle/xcodebuild), dockerized capabilities, etc.
    caps
}
