//! Prototype gRPC control-plane client — runs in parallel with the existing
//! HTTP `ControlPlaneClient`.
//!
//! Opt-in via config: set `agent.grpc_control_plane_url` (e.g.
//! `http://127.0.0.1:50051`). When set, the agent:
//!   1. Calls `RegisterAgent` once.
//!   2. Opens a `SubscribeCommands` server-streaming RPC and logs pushed commands.
//!   3. Spawns a `PushHeartbeats` client-streaming RPC that re-uses one
//!      connection for the full agent lifetime.
//!
//! This is a prototype — it does not replace the HTTP path yet. The goal is
//! to validate the wire protocol and connection semantics end-to-end.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::sync::watch;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

pub mod proto {
    tonic::include_proto!("edgefabric.agent.v1");
}

use proto::agent_service_client::AgentServiceClient;
use proto::{AgentRegistration, Heartbeat, SubscribeRequest};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Connect to the control plane over gRPC and spawn heartbeat + command
/// subscription tasks. Returns a handle for optional shutdown coordination.
pub async fn spawn(
    grpc_url: String,
    agent_id: String,
    region: String,
    hostname: String,
    version: String,
    streaming_url: Option<String>,
    heartbeat_interval_secs: u64,
    mut shutdown: watch::Receiver<bool>,
) -> Result<tokio::task::JoinHandle<()>> {
    let endpoint = tonic::transport::Endpoint::from_shared(grpc_url.clone())?
        .connect_timeout(Duration::from_secs(5))
        .keep_alive_while_idle(true)
        .http2_keep_alive_interval(Duration::from_secs(15));

    let channel = endpoint.connect().await?;
    tracing::info!(grpc_url, "gRPC control-plane channel connected (prototype)");

    let mut client = AgentServiceClient::new(channel);

    // 1. Register ----------------------------------------------------------
    let reg = AgentRegistration {
        agent_id: agent_id.clone(),
        region: region.clone(),
        version,
        hostname,
        streaming_url,
    };
    match client.register_agent(reg).await {
        Ok(resp) => {
            let ack = resp.into_inner();
            tracing::info!(
                accepted = ack.accepted,
                server_time_ms = ack.server_time_unix_ms,
                "gRPC RegisterAgent ack"
            );
        }
        Err(e) => tracing::warn!("gRPC RegisterAgent failed: {e}"),
    }

    // 2. SubscribeCommands (server streaming) ------------------------------
    let mut cmd_client = client.clone();
    let cmd_agent_id = agent_id.clone();
    tokio::spawn(async move {
        let req = SubscribeRequest {
            agent_id: cmd_agent_id.clone(),
            device_ids: vec![],
        };
        match cmd_client.subscribe_commands(req).await {
            Ok(resp) => {
                let mut stream = resp.into_inner();
                tracing::info!(agent_id = %cmd_agent_id, "gRPC subscribed to commands");
                while let Some(msg) = stream.next().await {
                    match msg {
                        Ok(cmd) => tracing::info!(
                            command_id = %cmd.command_id,
                            device_id = %cmd.device_id,
                            cmd_type = %cmd.command_type,
                            "gRPC command received (prototype — not executed)"
                        ),
                        Err(e) => {
                            tracing::warn!("gRPC command stream error: {e}");
                            break;
                        }
                    }
                }
                tracing::info!("gRPC command stream ended");
            }
            Err(e) => tracing::warn!("gRPC SubscribeCommands failed: {e}"),
        }
    });

    // 3. PushHeartbeats (client streaming) ---------------------------------
    let handle = tokio::spawn(async move {
        let (tx, rx) = tokio::sync::mpsc::channel::<Heartbeat>(16);
        let stream = ReceiverStream::new(rx);

        // Background sender: one heartbeat per tick until shutdown.
        let send_agent_id = agent_id.clone();
        let sender = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(heartbeat_interval_secs));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let hb = Heartbeat {
                            agent_id: send_agent_id.clone(),
                            device_id: String::new(), // agent-level heartbeat
                            status: "available".into(),
                            cpu_usage: None,
                            memory_usage: None,
                            timestamp_unix_ms: now_ms(),
                        };
                        if tx.send(hb).await.is_err() {
                            break;
                        }
                    }
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() { break; }
                    }
                }
            }
        });

        match client.push_heartbeats(stream).await {
            Ok(resp) => {
                let ack = resp.into_inner();
                tracing::info!(
                    accepted = ack.accepted,
                    rejected = ack.rejected,
                    "gRPC PushHeartbeats stream closed"
                );
            }
            Err(e) => tracing::warn!("gRPC PushHeartbeats failed: {e}"),
        }
        sender.abort();
    });

    Ok(handle)
}
