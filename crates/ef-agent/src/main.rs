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
mod veeros_discovery;

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

    // ── Agent-local streaming HTTP/WS server — starts immediately ─
    // This runs independently of the control-plane connection so the
    // VeerOS and sensor APIs are available even when offline.
    let veeros_cfg = cfg.veeros.clone();
    let host_table = veeros_discovery::new_host_table();
    let host_table_srv = host_table.clone();
    tokio::spawn(async move {
        if let Err(e) = streaming_server::start_streaming_server(streaming_port, veeros_cfg, host_table_srv).await {
            tracing::error!("Streaming server failed: {e}");
        }
    });

    // ── VeerOS host advertisement discovery listener ───────────────
    let host_table_listener = host_table.clone();
    tokio::spawn(async move {
        if let Err(e) = veeros_discovery::start_listener(host_table_listener).await {
            tracing::warn!("VeerOS discovery listener failed (UDP multicast may be unavailable): {e}");
        }
    });

    tracing::info!(streaming_port, "Streaming server started");

    // ── P8.6 Streaming forwarder is available on demand ──────────
    let _streaming = streaming::StreamingForwarder::new(
        cfg.streaming.clone(),
        Arc::clone(&registry),
    );

    // ── P8.2 + P8.8 Connect to control plane (background, retries forever) ─
    // Heartbeat, discovery, and executor are launched once connected.
    // The streaming server above continues to operate regardless.
    let cp_url = cfg.agent.control_plane_url.clone();
    let agent_id = cfg.agent.agent_id.clone();
    let region = cfg.agent.region.clone();
    let reconnect_interval = cfg.agent.reconnect_interval_seconds;
    let tls_skip = cfg.agent.tls_skip_verify;
    let hb_interval = cfg.agent.heartbeat_interval_seconds;
    let scan_interval = cfg.agent.device_scan_interval_seconds;
    let auto_update_url = cfg.agent.auto_update_url.clone();
    let auto_update_enabled = cfg.agent.auto_update_enabled;
    let capabilities = detect_capabilities(&cfg);
    let streaming_url_bg = streaming_url.clone();
    let registry_bg = Arc::clone(&registry);
    let shutdown_rx_bg = shutdown_rx.clone();

    tokio::spawn(async move {
        // Retry forever until control plane is reachable
        let cp_client = Arc::new(
            connect_with_retry(&cp_url, &agent_id, reconnect_interval, 0, tls_skip)
                .await
                .expect("connect_with_retry with max_attempts=0 never returns Err"),
        );

        tracing::info!("Control plane connected — launching subsystems");

        if let Err(e) = cp_client
            .register_agent(&region, Some(&streaming_url_bg), &capabilities)
            .await
        {
            tracing::warn!("Agent register failed (will retry on next heartbeat cycle): {e}");
        }

        let discovery_handle = discovery::spawn_discovery_loop(
            Arc::clone(&cp_client),
            Arc::clone(&registry_bg),
            agent_id.clone(),
            region.clone(),
            scan_interval,
            Some(streaming_url_bg),
            shutdown_rx_bg.clone(),
        );

        let heartbeat_handle = heartbeat::spawn_heartbeat_reporter(
            Arc::clone(&cp_client),
            Arc::clone(&registry_bg),
            agent_id.clone(),
            hb_interval,
            shutdown_rx_bg.clone(),
        );

        let executor_handle = executor::spawn_command_executor(
            Arc::clone(&cp_client),
            Arc::clone(&registry_bg),
            hb_interval,
            shutdown_rx_bg.clone(),
        );

        let auto_updater = updater::AutoUpdater::new(&auto_update_url, auto_update_enabled);
        let updater_handle = auto_updater.spawn_update_checker(3600, shutdown_rx_bg.clone());

        tracing::info!("All control-plane subsystems active");

        let _ = tokio::join!(discovery_handle, heartbeat_handle, executor_handle, updater_handle);
    });

    tracing::info!("Agent running — streaming ready, connecting to control plane in background");

    // ── P8.8 Wait for shutdown signal ─────────────────────────────
    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received — stopping subsystems");

    // Signal all background tasks to stop
    let _ = shutdown_tx.send(true);

    // Brief wait for graceful shutdown
    let shutdown_timeout = Duration::from_secs(10);
    let _ = tokio::time::timeout(shutdown_timeout, tokio::time::sleep(shutdown_timeout)).await;

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
    let mut caps: Vec<String> = vec!["android".into(), "ios".into(), "iot".into(), "ssh".into()];

    // ── VeerOS capability (if VM script exists) ────────────────────────
    if std::path::Path::new(&cfg.veeros.vm_script_path).exists() {
        caps.push("veeros".into());
    }

    // ── Build toolchain detection ─────────────────────────────────────
    if command_exists("xcodebuild") {
        caps.push("xcode".into());
    }
    if command_exists("cargo") {
        caps.push("cargo".into());
    }
    if command_exists("gradle") {
        caps.push("gradle".into());
    }
    if command_exists("cmake") {
        caps.push("cmake".into());
    }
    if command_exists("make") {
        caps.push("make".into());
    }

    // ── SDK & Runtime detection ───────────────────────────────────────
    if command_exists("docker") || command_exists("podman") {
        caps.push("containerization".into());
    }
    if command_exists("swift") {
        caps.push("swift".into());
    }
    if command_exists("python3") || command_exists("python") {
        caps.push("python".into());
    }
    if command_exists("node") {
        caps.push("node".into());
    }
    if command_exists("go") {
        caps.push("go".into());
    }
    if command_exists("java") {
        caps.push("java".into());
    }

    // ── CI/CD & Runner capabilities ───────────────────────────────────
    if command_exists("git") {
        caps.push("git".into());
    }
    if command_exists("curl") {
        caps.push("curl".into());
    }

    tracing::info!(capabilities = ?caps, "Agent capabilities detected");
    caps
}

/// Check if a command exists in PATH.
fn command_exists(cmd: &str) -> bool {
    let output = std::process::Command::new("which")
        .arg(cmd)
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}
