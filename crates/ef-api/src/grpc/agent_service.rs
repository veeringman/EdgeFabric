//! AgentService gRPC implementation — prototype of the binary control-plane
//! channel replacing HTTP heartbeat polling + command fetch.
//!
//! The service is intentionally lightweight:
//! * `PushHeartbeats` consumes a client-streaming RPC, updates in-memory
//!   last-seen state, and returns an ack on stream end.
//! * `SubscribeCommands` is a server-streaming RPC. The agent opens it once
//!   and the server holds an `mpsc::Sender<Command>` per agent_id so any
//!   other part of ef-api can push a command by calling
//!   `AgentHub::enqueue_command(agent_id, cmd)`.
//! * `ReportCommandResult` and `RegisterAgent` are unary acks for now.
//!
//! Nothing here touches the DB — this is a prototype focused on the wire
//! protocol. Persisting heartbeats / results can be wired in once the
//! channel semantics are validated.

use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use futures::Stream;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};

use crate::proto::agent::agent_service_server::AgentService;
use crate::proto::agent::{
    AgentRegistration, Command, Heartbeat, HeartbeatAck, RegisterAck, ReportAck, CommandResult,
    SubscribeRequest,
};

/// Per-agent state kept on the API side for the lifetime of a connected agent.
#[derive(Debug)]
struct AgentEntry {
    last_heartbeat_unix_ms: i64,
    command_tx: Option<mpsc::Sender<Result<Command, Status>>>,
}

/// Shared handle so other parts of ef-api can push commands into a live agent
/// stream without having to know about gRPC types directly.
#[derive(Clone, Default)]
pub struct AgentHub {
    agents: Arc<DashMap<String, AgentEntry>>,
}

impl AgentHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a command to a connected agent. Returns false if the agent is not
    /// currently subscribed (caller can fall back to HTTP / persist-and-retry).
    pub async fn enqueue_command(&self, agent_id: &str, cmd: Command) -> bool {
        if let Some(entry) = self.agents.get(agent_id) {
            if let Some(tx) = &entry.command_tx {
                return tx.send(Ok(cmd)).await.is_ok();
            }
        }
        false
    }

    pub fn connected_agent_ids(&self) -> Vec<String> {
        self.agents.iter().map(|e| e.key().clone()).collect()
    }

    fn touch(&self, agent_id: &str, ts_ms: i64) {
        self.agents
            .entry(agent_id.to_string())
            .and_modify(|e| e.last_heartbeat_unix_ms = ts_ms)
            .or_insert(AgentEntry {
                last_heartbeat_unix_ms: ts_ms,
                command_tx: None,
            });
    }

    fn register_subscriber(
        &self,
        agent_id: &str,
        tx: mpsc::Sender<Result<Command, Status>>,
    ) {
        self.agents
            .entry(agent_id.to_string())
            .and_modify(|e| e.command_tx = Some(tx.clone()))
            .or_insert(AgentEntry {
                last_heartbeat_unix_ms: now_ms(),
                command_tx: Some(tx),
            });
    }

    fn drop_subscriber(&self, agent_id: &str) {
        if let Some(mut e) = self.agents.get_mut(agent_id) {
            e.command_tx = None;
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub struct AgentServiceImpl {
    hub: AgentHub,
}

impl AgentServiceImpl {
    pub fn new(hub: AgentHub) -> Self {
        Self { hub }
    }
}

type CommandStream = Pin<Box<dyn Stream<Item = Result<Command, Status>> + Send + 'static>>;

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    async fn register_agent(
        &self,
        request: Request<AgentRegistration>,
    ) -> Result<Response<RegisterAck>, Status> {
        let reg = request.into_inner();
        tracing::info!(
            agent_id = %reg.agent_id,
            region = %reg.region,
            version = %reg.version,
            hostname = %reg.hostname,
            "gRPC agent register"
        );
        self.hub.touch(&reg.agent_id, now_ms());
        Ok(Response::new(RegisterAck {
            accepted: true,
            // Placeholder token — a real impl would mint a short-lived HMAC or
            // JWT bound to the channel identity.
            session_token: format!("grpc-proto-{}", reg.agent_id),
            server_time_unix_ms: now_ms(),
        }))
    }

    async fn push_heartbeats(
        &self,
        request: Request<Streaming<Heartbeat>>,
    ) -> Result<Response<HeartbeatAck>, Status> {
        let mut stream = request.into_inner();
        let mut accepted = 0i32;
        let mut rejected = 0i32;

        while let Some(item) = stream.next().await {
            match item {
                Ok(hb) => {
                    self.hub.touch(&hb.agent_id, hb.timestamp_unix_ms.max(now_ms()));
                    accepted += 1;
                    tracing::debug!(
                        agent_id = %hb.agent_id,
                        device_id = %hb.device_id,
                        status = %hb.status,
                        "grpc heartbeat"
                    );
                }
                Err(e) => {
                    rejected += 1;
                    tracing::warn!("grpc heartbeat stream error: {e}");
                }
            }
        }

        Ok(Response::new(HeartbeatAck { accepted, rejected }))
    }

    type SubscribeCommandsStream = CommandStream;

    async fn subscribe_commands(
        &self,
        request: Request<SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeCommandsStream>, Status> {
        let req = request.into_inner();
        let agent_id = req.agent_id.clone();
        if agent_id.is_empty() {
            return Err(Status::invalid_argument("agent_id is required"));
        }

        let (tx, rx) = mpsc::channel::<Result<Command, Status>>(32);
        self.hub.register_subscriber(&agent_id, tx);
        tracing::info!(%agent_id, "gRPC agent subscribed to commands");

        // Wrap receiver and attach a drop-guard so the hub entry is cleared
        // when the agent disconnects.
        let hub = self.hub.clone();
        let stream = ReceiverStream::new(rx);
        let guarded = GuardedStream {
            inner: stream,
            hub,
            agent_id,
        };

        Ok(Response::new(Box::pin(guarded) as Self::SubscribeCommandsStream))
    }

    async fn report_command_result(
        &self,
        request: Request<CommandResult>,
    ) -> Result<Response<ReportAck>, Status> {
        let r = request.into_inner();
        tracing::info!(
            command_id = %r.command_id,
            success = r.success,
            exit_code = r.exit_code,
            "grpc command result"
        );
        Ok(Response::new(ReportAck { accepted: true }))
    }
}

/// Wraps the command `ReceiverStream` so that when the agent disconnects
/// (stream is dropped) we clear the hub's sender entry.
struct GuardedStream {
    inner: ReceiverStream<Result<Command, Status>>,
    hub: AgentHub,
    agent_id: String,
}

impl Stream for GuardedStream {
    type Item = Result<Command, Status>;
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl Drop for GuardedStream {
    fn drop(&mut self) {
        tracing::info!(agent_id = %self.agent_id, "gRPC agent command stream closed");
        self.hub.drop_subscriber(&self.agent_id);
    }
}
