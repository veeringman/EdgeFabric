//! SessionService gRPC implementation.

use ef_core::device::{DeviceFilter, DeviceKind, Os};
use ef_db::repo::device::DeviceRepo;
use ef_db::repo::session::SessionRepo;
use ef_scheduler::Scheduler;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::proto::session::session_service_server::SessionService;
use crate::proto::session::{
    CommandRequest, CommandResponse, CreateSessionRequest, EndSessionRequest, EndSessionResponse,
    GetSessionRequest, LogEntry, LogStreamRequest, Session as ProtoSession,
};

fn row_to_proto(s: &ef_db::repo::session::SessionRow) -> ProtoSession {
    ProtoSession {
        id: s.id.to_string(),
        device_id: s.device_id.to_string(),
        status: s.status.clone(),
        stream_url: String::new(), // populated when streaming is active
        created_at: s.created_at.timestamp(),
        expires_at: s.expires_at.timestamp(),
    }
}

fn parse_os(s: &str) -> Option<Os> {
    match s {
        "android" => Some(Os::Android),
        "ios" => Some(Os::Ios),
        "iot" => Some(Os::Iot),
        _ => None,
    }
}

fn parse_kind(s: &str) -> Option<DeviceKind> {
    match s {
        "real" => Some(DeviceKind::Real),
        "virtual" => Some(DeviceKind::Virtual),
        _ => None,
    }
}

pub struct SessionServiceImpl {
    pool: PgPool,
    scheduler: Scheduler,
}

impl SessionServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        let scheduler = Scheduler::new(pool.clone());
        Self { pool, scheduler }
    }
}

#[tonic::async_trait]
impl SessionService for SessionServiceImpl {
    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<ProtoSession>, Status> {
        let req = request.into_inner();
        tracing::info!(?req, "gRPC CreateSession");

        let filter = DeviceFilter {
            os: req.os.as_deref().and_then(parse_os),
            os_version: req.version.clone(),
            kind: req.kind.as_deref().and_then(parse_kind),
            region: None,
            capabilities: None,
        };

        // Allocate a device
        let device = self
            .scheduler
            .allocate(&filter)
            .await
            .map_err(|e| Status::resource_exhausted(format!("Device allocation failed: {e}")))?;

        let timeout = if req.timeout_minutes > 0 {
            req.timeout_minutes
        } else {
            30
        };
        let expires_at = chrono::Utc::now() + chrono::Duration::minutes(timeout as i64);

        // Use a nil tenant_id for gRPC — auth interceptor should inject real tenant
        let tenant_id = Uuid::nil();

        let row = SessionRepo::create(&self.pool, device.id, tenant_id, expires_at)
            .await
            .map_err(|e| Status::internal(format!("Session creation failed: {e}")))?;

        tracing::info!(session_id = %row.id, device_id = %device.id, "Session created via gRPC");
        Ok(Response::new(row_to_proto(&row)))
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<ProtoSession>, Status> {
        let req = request.into_inner();
        let id = Uuid::parse_str(&req.session_id)
            .map_err(|_| Status::invalid_argument("Invalid session_id UUID"))?;

        let row = SessionRepo::find_by_id(&self.pool, id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Session {id} not found")))?;

        Ok(Response::new(row_to_proto(&row)))
    }

    async fn end_session(
        &self,
        request: Request<EndSessionRequest>,
    ) -> Result<Response<EndSessionResponse>, Status> {
        let req = request.into_inner();
        let id = Uuid::parse_str(&req.session_id)
            .map_err(|_| Status::invalid_argument("Invalid session_id UUID"))?;

        // End the session
        let ended = SessionRepo::end_session(&self.pool, id)
            .await
            .map_err(|e| Status::internal(format!("End session error: {e}")))?;

        if !ended {
            return Err(Status::not_found(format!("Session {id} not found or already ended")));
        }

        // Release the device
        if let Some(row) = SessionRepo::find_by_id(&self.pool, id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
        {
            let _ = self.scheduler.release(row.device_id).await;
        }

        tracing::info!(%id, "Session ended via gRPC");
        Ok(Response::new(EndSessionResponse { success: true }))
    }

    async fn send_command(
        &self,
        request: Request<CommandRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let req = request.into_inner();
        let session_id = Uuid::parse_str(&req.session_id)
            .map_err(|_| Status::invalid_argument("Invalid session_id UUID"))?;

        // Look up the session to find the device
        let session = SessionRepo::find_by_id(&self.pool, session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Session {session_id} not found")))?;

        if session.status != "active" && session.status != "streaming" {
            return Err(Status::failed_precondition(format!(
                "Session is in '{}' state, expected 'active' or 'streaming'",
                session.status
            )));
        }

        // Load device info for CommandEngine
        let device_row = DeviceRepo::find_by_id(&self.pool, session.device_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found("Device not found for session"))?;

        let device = ef_core::device::Device {
            id: device_row.id,
            name: device_row.name.clone(),
            serial: device_row.serial.clone(),
            os: parse_os(&device_row.os).unwrap_or(Os::Android),
            os_version: device_row.os_version.clone(),
            kind: parse_kind(&device_row.kind).unwrap_or(DeviceKind::Real),
            status: ef_core::device::DeviceStatus::Busy,
            region: device_row.region.clone(),
            capabilities: device_row.capabilities.clone(),
            registered_at: device_row.registered_at,
            last_heartbeat: device_row.last_heartbeat,
        };

        let params = &req.params;
        match req.command_type.as_str() {
            "tap" => {
                let x: u32 = params.get("x").and_then(|v| v.parse().ok()).unwrap_or(0);
                let y: u32 = params.get("y").and_then(|v| v.parse().ok()).unwrap_or(0);
                ef_control::command::CommandEngine::tap(&device, x, y)
                    .await
                    .map_err(|e| Status::internal(format!("Command error: {e}")))?;
                Ok(Response::new(CommandResponse {
                    success: true,
                    data: Vec::new(),
                    error: String::new(),
                }))
            }
            "swipe" => {
                let x1: u32 = params.get("x1").and_then(|v| v.parse().ok()).unwrap_or(0);
                let y1: u32 = params.get("y1").and_then(|v| v.parse().ok()).unwrap_or(0);
                let x2: u32 = params.get("x2").and_then(|v| v.parse().ok()).unwrap_or(0);
                let y2: u32 = params.get("y2").and_then(|v| v.parse().ok()).unwrap_or(0);
                let dur: u32 = params.get("duration_ms").and_then(|v| v.parse().ok()).unwrap_or(300);
                ef_control::command::CommandEngine::swipe(&device, x1, y1, x2, y2, dur)
                    .await
                    .map_err(|e| Status::internal(format!("Command error: {e}")))?;
                Ok(Response::new(CommandResponse {
                    success: true,
                    data: Vec::new(),
                    error: String::new(),
                }))
            }
            "key" => {
                let keycode = params.get("keycode").map(|s| s.as_str()).unwrap_or("KEYCODE_HOME");
                ef_control::command::CommandEngine::key(&device, keycode)
                    .await
                    .map_err(|e| Status::internal(format!("Command error: {e}")))?;
                Ok(Response::new(CommandResponse {
                    success: true,
                    data: Vec::new(),
                    error: String::new(),
                }))
            }
            "text" => {
                let text = params.get("text").map(|s| s.as_str()).unwrap_or("");
                ef_control::command::CommandEngine::text(&device, text)
                    .await
                    .map_err(|e| Status::internal(format!("Command error: {e}")))?;
                Ok(Response::new(CommandResponse {
                    success: true,
                    data: Vec::new(),
                    error: String::new(),
                }))
            }
            "screenshot" => {
                let bytes = ef_control::command::CommandEngine::screenshot(&device)
                    .await
                    .map_err(|e| Status::internal(format!("Screenshot error: {e}")))?;
                Ok(Response::new(CommandResponse {
                    success: true,
                    data: bytes,
                    error: String::new(),
                }))
            }
            "install" => {
                let path = params.get("app_path").map(|s| s.as_str()).unwrap_or("");
                let out = ef_control::command::CommandEngine::install_app(&device, path)
                    .await
                    .map_err(|e| Status::internal(format!("Install error: {e}")))?;
                Ok(Response::new(CommandResponse {
                    success: out.exit_code == 0,
                    data: out.stdout.into_bytes(),
                    error: out.stderr,
                }))
            }
            other => Err(Status::invalid_argument(format!(
                "Unknown command_type: {other}"
            ))),
        }
    }

    type StreamLogsStream = ReceiverStream<Result<LogEntry, Status>>;

    async fn stream_logs(
        &self,
        request: Request<LogStreamRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let req = request.into_inner();
        let session_id = Uuid::parse_str(&req.session_id)
            .map_err(|_| Status::invalid_argument("Invalid session_id UUID"))?;

        let session = SessionRepo::find_by_id(&self.pool, session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Session {session_id} not found")))?;

        let device_row = DeviceRepo::find_by_id(&self.pool, session.device_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found("Device not found for session"))?;

        let device = ef_core::device::Device {
            id: device_row.id,
            name: device_row.name.clone(),
            serial: device_row.serial.clone(),
            os: parse_os(&device_row.os).unwrap_or(Os::Android),
            os_version: device_row.os_version.clone(),
            kind: parse_kind(&device_row.kind).unwrap_or(DeviceKind::Real),
            status: ef_core::device::DeviceStatus::Busy,
            region: device_row.region.clone(),
            capabilities: device_row.capabilities.clone(),
            registered_at: device_row.registered_at,
            last_heartbeat: device_row.last_heartbeat,
        };

        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                interval.tick().await;

                let log_text = match ef_control::command::CommandEngine::logs(&device, 50).await {
                    Ok(text) => text,
                    Err(e) => {
                        tracing::warn!("Log stream error: {e}");
                        continue;
                    }
                };

                for line in log_text.lines() {
                    let entry = LogEntry {
                        timestamp: chrono::Utc::now().timestamp(),
                        level: "INFO".to_string(),
                        message: line.to_string(),
                        source: device.name.clone(),
                    };
                    if tx.send(Ok(entry)).await.is_err() {
                        return; // client disconnected
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
