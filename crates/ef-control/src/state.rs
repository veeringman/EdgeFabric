use ef_core::device::DeviceStatus;
use std::collections::HashMap;
use tokio::time::{Duration, interval};
use uuid::Uuid;

/// Tracks device heartbeat timestamps in-memory for fast staleness checks.
/// The authoritative state lives in PostgreSQL; this is a cache for health monitoring.
pub struct HealthMonitor {
    /// device_id → last heartbeat unix timestamp
    heartbeats: HashMap<Uuid, i64>,
    /// Seconds after which a device with no heartbeat is considered offline.
    stale_threshold_secs: i64,
}

impl HealthMonitor {
    pub fn new(stale_threshold_secs: i64) -> Self {
        Self {
            heartbeats: HashMap::new(),
            stale_threshold_secs,
        }
    }

    /// Record a heartbeat for a device.
    pub fn record_heartbeat(&mut self, device_id: Uuid) {
        let now = chrono::Utc::now().timestamp();
        self.heartbeats.insert(device_id, now);
    }

    /// Check if a device is considered healthy (heartbeat within threshold).
    pub fn is_healthy(&self, device_id: &Uuid) -> bool {
        let now = chrono::Utc::now().timestamp();
        self.heartbeats
            .get(device_id)
            .map_or(false, |ts| (now - *ts) < self.stale_threshold_secs)
    }

    /// Return IDs of devices whose heartbeat is stale.
    pub fn stale_devices(&self) -> Vec<Uuid> {
        let now = chrono::Utc::now().timestamp();
        self.heartbeats
            .iter()
            .filter(|(_, ts)| (now - **ts) >= self.stale_threshold_secs)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Remove a device from tracking (e.g., after deletion).
    pub fn remove(&mut self, device_id: &Uuid) {
        self.heartbeats.remove(device_id);
    }
}

/// Spawn a background task that periodically marks stale devices as offline in the DB.
pub fn spawn_stale_device_checker(
    pool: sqlx::PgPool,
    stale_threshold_secs: i64,
    check_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(check_interval_secs));
        loop {
            tick.tick().await;
            let cutoff = chrono::Utc::now() - chrono::Duration::seconds(stale_threshold_secs);
            let result = sqlx::query(
                r#"UPDATE devices SET status = 'offline'::device_status
                   WHERE status IN ('available'::device_status, 'busy'::device_status)
                     AND (last_heartbeat IS NULL OR last_heartbeat < $1)"#,
            )
            .bind(cutoff)
            .execute(&pool)
            .await;

            match result {
                Ok(r) if r.rows_affected() > 0 => {
                    tracing::warn!(count = r.rows_affected(), "Marked stale devices offline");
                }
                Err(e) => {
                    tracing::error!("Stale device check failed: {e}");
                }
                _ => {}
            }
        }
    })
}
