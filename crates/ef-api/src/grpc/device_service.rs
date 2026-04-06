//! DeviceService gRPC implementation.

use ef_core::device::{DeviceFilter, DeviceKind, Os};
use ef_db::repo::device::DeviceRepo;
use ef_scheduler::Scheduler;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::proto::device::device_service_server::DeviceService;
use crate::proto::device::{
    AllocateRequest, AllocateResponse, Device as ProtoDevice, Empty, GetDeviceRequest,
    HealthEvent, ListDevicesRequest, ListDevicesResponse, ReleaseRequest, ReleaseResponse,
};

fn row_to_proto(d: &ef_db::repo::device::DeviceRow) -> ProtoDevice {
    ProtoDevice {
        id: d.id.to_string(),
        name: d.name.clone(),
        os: d.os.clone(),
        os_version: d.os_version.clone(),
        kind: d.kind.clone(),
        status: d.status.clone(),
        region: d.region.clone(),
        capabilities: d.capabilities.clone(),
        serial: d.serial.clone(),
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

pub struct DeviceServiceImpl {
    pool: PgPool,
    scheduler: Scheduler,
}

impl DeviceServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        let scheduler = Scheduler::new(pool.clone());
        Self { pool, scheduler }
    }
}

#[tonic::async_trait]
impl DeviceService for DeviceServiceImpl {
    async fn list_devices(
        &self,
        request: Request<ListDevicesRequest>,
    ) -> Result<Response<ListDevicesResponse>, Status> {
        let req = request.into_inner();
        tracing::debug!(?req, "gRPC ListDevices");

        let mut rows = DeviceRepo::list(&self.pool)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?;

        // Apply filters in-memory
        if let Some(ref os) = req.os {
            rows.retain(|d| d.os == *os);
        }
        if let Some(ref status) = req.status {
            rows.retain(|d| d.status == *status);
        }
        if let Some(ref version) = req.version {
            rows.retain(|d| d.os_version == *version);
        }
        if let Some(ref kind) = req.kind {
            rows.retain(|d| d.kind == *kind);
        }
        if let Some(ref region) = req.region {
            rows.retain(|d| d.region == *region);
        }

        let total = rows.len() as i32;
        let devices: Vec<ProtoDevice> = rows.iter().map(row_to_proto).collect();
        Ok(Response::new(ListDevicesResponse { devices, total }))
    }

    async fn get_device(
        &self,
        request: Request<GetDeviceRequest>,
    ) -> Result<Response<ProtoDevice>, Status> {
        let req = request.into_inner();
        let id = Uuid::parse_str(&req.device_id)
            .map_err(|_| Status::invalid_argument("Invalid device_id UUID"))?;

        let row = DeviceRepo::find_by_id(&self.pool, id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Device {id} not found")))?;

        Ok(Response::new(row_to_proto(&row)))
    }

    async fn allocate_device(
        &self,
        request: Request<AllocateRequest>,
    ) -> Result<Response<AllocateResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(?req, "gRPC AllocateDevice");

        let filter = DeviceFilter {
            os: req.os.as_deref().and_then(parse_os),
            os_version: req.version.clone(),
            kind: req.kind.as_deref().and_then(parse_kind),
            region: req.region.clone(),
            capabilities: None,
        };

        let device = self
            .scheduler
            .allocate(&filter)
            .await
            .map_err(|e| Status::resource_exhausted(format!("Allocation failed: {e}")))?;

        Ok(Response::new(AllocateResponse {
            device_id: device.id.to_string(),
            session_id: String::new(), // session created separately
        }))
    }

    async fn release_device(
        &self,
        request: Request<ReleaseRequest>,
    ) -> Result<Response<ReleaseResponse>, Status> {
        let req = request.into_inner();
        let device_id = Uuid::parse_str(&req.device_id)
            .map_err(|_| Status::invalid_argument("Invalid device_id UUID"))?;

        self.scheduler
            .release(device_id)
            .await
            .map_err(|e| Status::internal(format!("Release error: {e}")))?;

        tracing::info!(%device_id, "Device released via gRPC");
        Ok(Response::new(ReleaseResponse { success: true }))
    }

    type StreamHealthStream = ReceiverStream<Result<HealthEvent, Status>>;

    async fn stream_health(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<Self::StreamHealthStream>, Status> {
        let pool = self.pool.clone();
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;

                let devices = match DeviceRepo::list(&pool).await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::warn!("Health stream DB error: {e}");
                        continue;
                    }
                };

                for d in &devices {
                    let event = HealthEvent {
                        device_id: d.id.to_string(),
                        status: d.status.clone(),
                        cpu_usage: 0.0,
                        memory_usage: 0.0,
                        timestamp: chrono::Utc::now().timestamp(),
                    };
                    if tx.send(Ok(event)).await.is_err() {
                        return; // client disconnected
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
