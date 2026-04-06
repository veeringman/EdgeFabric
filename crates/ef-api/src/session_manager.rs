//! Session lifecycle management — timeout enforcement, cleanup, NATS events, isolation.

use ef_db::repo::device::DeviceRepo;
use ef_db::repo::session::SessionRepo;
use sqlx::PgPool;
use tokio::time::{Duration, interval};

/// Spawn a background task that periodically expires timed-out sessions.
///
/// Any session past its `expires_at` that is still in a non-terminal state
/// gets transitioned to `'failed'` and its device is released back to `'available'`.
pub fn spawn_session_reaper(
    pool: PgPool,
    nats: Option<async_nats::Client>,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(check_interval_secs));
        loop {
            tick.tick().await;
            if let Err(e) = reap_expired_sessions(&pool, nats.as_ref()).await {
                tracing::error!("Session reaper error: {e}");
            }
        }
    })
}

async fn reap_expired_sessions(
    pool: &PgPool,
    nats: Option<&async_nats::Client>,
) -> Result<(), sqlx::Error> {
    let expired = SessionRepo::list_expired(pool).await?;
    if expired.is_empty() {
        return Ok(());
    }

    tracing::warn!(count = expired.len(), "Reaping expired sessions");

    for session in &expired {
        // Force-terminate the session
        let _ = SessionRepo::end_session(pool, session.id).await;
        // Update status to 'failed' since it expired
        let _ = SessionRepo::set_status(pool, session.id, "failed").await;

        // Release the device
        let _ = DeviceRepo::set_status(pool, session.device_id, "available").await;

        tracing::info!(
            session_id = %session.id,
            device_id = %session.device_id,
            "Expired session reaped, device released"
        );

        // P5.6 — Publish session event to NATS
        if let Some(nc) = nats {
            let event = serde_json::json!({
                "event": "session.expired",
                "session_id": session.id,
                "device_id": session.device_id,
                "tenant_id": session.tenant_id,
                "previous_status": session.status,
            });
            let payload = serde_json::to_vec(&event).unwrap_or_default();
            if let Err(e) = nc.publish("ef.sessions.events", payload.into()).await {
                tracing::warn!("Failed to publish session event: {e}");
            }
        }
    }

    Ok(())
}

/// Publish a session lifecycle event to NATS.
pub async fn publish_session_event(
    nats: &async_nats::Client,
    event_type: &str,
    session_id: uuid::Uuid,
    device_id: uuid::Uuid,
    tenant_id: uuid::Uuid,
    status: &str,
) {
    let event = serde_json::json!({
        "event": event_type,
        "session_id": session_id,
        "device_id": device_id,
        "tenant_id": tenant_id,
        "status": status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let payload = serde_json::to_vec(&event).unwrap_or_default();

    // Publish to both specific and wildcard subjects
    let subjects = [
        format!("ef.sessions.{session_id}"),
        "ef.sessions.events".to_string(),
        format!("ef.tenants.{tenant_id}.sessions"),
    ];

    for subject in subjects {
        if let Err(e) = nats.publish(subject, payload.clone().into()).await {
            tracing::warn!("Failed to publish session event: {e}");
        }
    }
}

/// Session isolation enforcement tags.
///
/// Each active session gets a unique isolation context that is passed to the
/// command engine to enforce device-level separation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionIsolation {
    pub session_id: uuid::Uuid,
    pub device_id: uuid::Uuid,
    /// Unique network namespace or VLAN tag (format: `ef-{short_session_id}`).
    pub namespace: String,
    /// Temp directory scoped to this session on the target device.
    pub scratch_dir: String,
}

impl SessionIsolation {
    pub fn new(session_id: uuid::Uuid, device_id: uuid::Uuid) -> Self {
        let short_id = &session_id.to_string()[..8];
        Self {
            session_id,
            device_id,
            namespace: format!("ef-{short_id}"),
            scratch_dir: format!("/tmp/ef-session-{short_id}"),
        }
    }
}

/// Set up isolation for a session on an Android device (via ADB).
pub async fn setup_android_isolation(serial: &str, isolation: &SessionIsolation) -> Result<(), ef_core::error::EdgeFabricError> {
    use ef_control::command::adb::AdbDriver;

    // Create isolated scratch directory
    AdbDriver::shell(serial, &["mkdir", "-p", &isolation.scratch_dir]).await?;

    // Clear logcat for a clean slate
    AdbDriver::logcat_clear(serial).await?;

    tracing::info!(session_id = %isolation.session_id, serial, "Android isolation set up");
    Ok(())
}

/// Tear down isolation — clean up the scratch directory.
pub async fn teardown_android_isolation(serial: &str, isolation: &SessionIsolation) -> Result<(), ef_core::error::EdgeFabricError> {
    use ef_control::command::adb::AdbDriver;

    AdbDriver::shell(serial, &["rm", "-rf", &isolation.scratch_dir]).await?;

    tracing::info!(session_id = %isolation.session_id, serial, "Android isolation torn down");
    Ok(())
}
