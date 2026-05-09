//! # ef-scheduler
//!
//! Device allocation and scheduling engine.
//! Matches incoming requests to available devices by OS, version, type, and region.
//! Uses database-level locking to prevent double-allocation.

pub mod priority;

use ef_core::device::DeviceFilter;
use ef_core::error::{EdgeFabricError, Result};
use ef_db::repo::device::{DeviceRepo, DeviceRow};
use ef_db::repo::runner::{RunnerLeaseRow, RunnerRepo, RunnerRow};
use sqlx::PgPool;
use uuid::Uuid;

pub struct Scheduler {
    pool: PgPool,
}

#[derive(Debug, Clone, Default)]
pub struct RunnerFilter {
    pub region: Option<String>,
    pub required_capabilities: Vec<String>,
    pub required_labels: Vec<String>,
    pub lease_ttl_seconds: i64,
}

impl Scheduler {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Find and allocate a device matching the given filter.
    ///
    /// Algorithm:
    /// 1. Query available devices matching the filter criteria.
    /// 2. Score candidates by priority (least-recently-used first for fair scheduling).
    /// 3. Atomically set status to 'busy' using a conditional UPDATE (optimistic lock).
    /// 4. Return the allocated device, or error if nothing matched.
    pub async fn allocate(&self, filter: &DeviceFilter) -> Result<DeviceRow> {
        let os_str = filter.os.map(|o| format!("{o:?}").to_lowercase());
        let candidates = DeviceRepo::list_available(
            &self.pool,
            os_str.as_deref(),
            filter.region.as_deref(),
        )
        .await?;

        if candidates.is_empty() {
            return Err(EdgeFabricError::NoMatchingDevices);
        }

        // Filter by os_version and kind if specified
        let mut filtered: Vec<&DeviceRow> = candidates.iter().collect();

        if let Some(ref ver) = filter.os_version {
            filtered.retain(|d| d.os_version == *ver);
        }
        if let Some(kind) = filter.kind {
            let kind_str = format!("{kind:?}").to_lowercase();
            filtered.retain(|d| d.kind == kind_str);
        }
        if let Some(ref caps) = filter.capabilities {
            filtered.retain(|d| caps.iter().all(|c| d.capabilities.contains(c)));
        }

        if filtered.is_empty() {
            return Err(EdgeFabricError::NoMatchingDevices);
        }

        // Priority: least-recently-used (sort by last_heartbeat ascending, NULL first)
        filtered.sort_by(|a, b| a.last_heartbeat.cmp(&b.last_heartbeat));

        // Try to atomically claim the best candidate
        for candidate in &filtered {
            let locked = self.try_lock_device(candidate.id).await?;
            if locked {
                tracing::info!(device_id = %candidate.id, name = %candidate.name, "Device allocated");
                // Return fresh copy from DB
                return DeviceRepo::find_by_id(&self.pool, candidate.id)
                    .await?
                    .ok_or(EdgeFabricError::NoMatchingDevices);
            }
        }

        Err(EdgeFabricError::NoMatchingDevices)
    }

    /// Atomically lock a device: UPDATE status='busy' WHERE id=? AND status='available'.
    /// Returns true if the row was updated (we got the lock).
    async fn try_lock_device(&self, device_id: Uuid) -> std::result::Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE devices SET status = 'busy'::device_status
               WHERE id = $1 AND status = 'available'::device_status"#,
        )
        .bind(device_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Release a previously allocated device back to the pool and trigger reset.
    pub async fn release(&self, device_id: Uuid) -> Result<()> {
        let updated = DeviceRepo::set_status(&self.pool, device_id, "available").await?;
        if updated {
            tracing::info!(%device_id, "Device released and marked available");
            // TODO: P4.8 — trigger device reset/wipe here
        } else {
            tracing::warn!(%device_id, "Release: device not found or status unchanged");
        }
        Ok(())
    }

    /// Allocate a runner using lease-based concurrency checks.
    pub async fn allocate_runner(
        &self,
        tenant_id: Uuid,
        filter: &RunnerFilter,
        build_job_id: Option<Uuid>,
    ) -> Result<(RunnerRow, RunnerLeaseRow)> {
        match RunnerRepo::acquire_lease(
            &self.pool,
            tenant_id,
            &filter.required_capabilities,
            &filter.required_labels,
            filter.region.as_deref(),
            build_job_id,
            filter.lease_ttl_seconds,
        )
        .await?
        {
            Some((runner, lease)) => {
                tracing::info!(runner_id = %runner.id, lease_id = %lease.id, "Runner allocated with lease");
                Ok((runner, lease))
            }
            None => Err(EdgeFabricError::NoMatchingDevices),
        }
    }

    pub async fn release_runner_lease(&self, lease_id: Uuid) -> Result<()> {
        let released = RunnerRepo::release_lease(&self.pool, lease_id).await?;
        if released.is_some() {
            tracing::info!(lease_id = %lease_id, "Runner lease released");
        } else {
            tracing::warn!(lease_id = %lease_id, "Runner lease release skipped (not found or already non-active)");
        }
        Ok(())
    }
}
