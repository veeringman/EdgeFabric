//! Core orchestration engine — job lifecycle, parallel dispatch, result aggregation.

use ef_core::device::DeviceFilter;
use ef_db::repo::device::DeviceRepo;
use ef_db::repo::job::{ArtifactRepo, JobRepo, TestCaseRepo};
use ef_scheduler::Scheduler;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::runner::RunConfig;
use crate::webhook;

/// The orchestration engine that drives test jobs through their lifecycle.
pub struct Engine {
    pool: PgPool,
    scheduler: Scheduler,
    nats: Option<async_nats::Client>,
    /// Base directory for storing artifacts on disk.
    artifact_dir: String,
}

impl Engine {
    pub fn new(
        pool: PgPool,
        nats: Option<async_nats::Client>,
        artifact_dir: String,
    ) -> Self {
        let scheduler = Scheduler::new(pool.clone());
        Self {
            pool,
            scheduler,
            nats,
            artifact_dir,
        }
    }

    /// Submit a new job: create DB row + test cases, then return job ID.
    pub async fn submit_job(
        &self,
        tenant_id: Uuid,
        test_suite: &str,
        test_names: &[String],
        parallelism: i32,
        max_retries: i32,
        webhook_url: Option<&str>,
    ) -> Result<Uuid, sqlx::Error> {
        let job = JobRepo::create(
            &self.pool,
            tenant_id,
            parallelism,
            test_suite,
            webhook_url,
        )
        .await?;

        TestCaseRepo::create_batch(&self.pool, job.id, test_names, max_retries).await?;

        JobRepo::update_progress(&self.pool, job.id, test_names.len() as i32, 0, 0).await?;

        tracing::info!(job_id = %job.id, tests = test_names.len(), parallelism, "Job submitted");

        self.publish_event("job.submitted", job.id, tenant_id, "queued")
            .await;

        Ok(job.id)
    }

    /// Execute a job: allocate devices, run tests in parallel, collect results.
    ///
    /// This is the main orchestration loop. It should be called from a background task.
    pub async fn execute_job(&self, job_id: Uuid) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let job = JobRepo::find_by_id(&self.pool, job_id)
            .await?
            .ok_or("Job not found")?;

        // Mark job as running
        JobRepo::mark_started(&self.pool, job_id).await?;
        self.publish_event("job.started", job_id, job.tenant_id, "running")
            .await;

        let parallelism = job.parallelism.max(1) as usize;
        let semaphore = Arc::new(Semaphore::new(parallelism));
        let pool = self.pool.clone();

        // Process test cases until none are pending
        loop {
            let pending = TestCaseRepo::list_pending(&pool, job_id, parallelism as i64).await?;
            if pending.is_empty() {
                break;
            }

            let mut handles = Vec::new();

            for tc in pending {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let pool = pool.clone();
                let scheduler = Scheduler::new(pool.clone());
                let run_config = RunConfig {
                    artifact_dir: self.artifact_dir.clone(),
                    test_suite: job.test_suite.clone(),
                };

                handles.push(tokio::spawn(async move {
                    let _permit = permit; // held until task completes

                    // Allocate a device
                    let device = match scheduler.allocate(&DeviceFilter::default()).await {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::warn!(test = %tc.name, "No device available: {e}");
                            let _ = TestCaseRepo::mark_failed(
                                &pool,
                                tc.id,
                                0,
                                &format!("No device: {e}"),
                            )
                            .await;
                            return;
                        }
                    };

                    // Mark test as running on this device
                    let _ = TestCaseRepo::mark_running(&pool, tc.id, device.id).await;

                    // Execute the test via the platform-specific runner
                    let start = std::time::Instant::now();
                    let result = crate::runner::run_test_case(&tc.name, &device, &run_config).await;
                    let duration_ms = start.elapsed().as_millis() as i32;

                    match result {
                        Ok(test_result) => {
                            let _ = TestCaseRepo::mark_passed(&pool, tc.id, duration_ms).await;
                            tracing::info!(test = %tc.name, device = %device.name, duration_ms, "PASS");

                            // Record artifacts produced by the runner
                            for artifact in &test_result.artifact_paths {
                                let _ = ArtifactRepo::create(
                                    &pool,
                                    tc.job_id,
                                    Some(tc.id),
                                    artifact.kind,
                                    &artifact.path,
                                    artifact.size_bytes as i64,
                                )
                                .await;
                            }
                        }
                        Err(err) => {
                            let _ = TestCaseRepo::mark_failed(
                                &pool,
                                tc.id,
                                duration_ms,
                                &err.to_string(),
                            )
                            .await;
                            tracing::warn!(test = %tc.name, device = %device.name, duration_ms, "FAIL: {err}");
                        }
                    }

                    // Release the device
                    let _ = DeviceRepo::set_status(&pool, device.id, "available").await;
                }));
            }

            // Wait for this batch to finish
            for h in handles {
                let _ = h.await;
            }

            // Update running progress
            let (total, passed, failed, _skipped) =
                TestCaseRepo::aggregate(&pool, job_id).await?;
            JobRepo::update_progress(&pool, job_id, total, passed, failed).await?;
        }

        // Final aggregation
        let (total, passed, failed, skipped) =
            TestCaseRepo::aggregate(&self.pool, job_id).await?;

        let final_status = if failed > 0 { "failed" } else { "completed" };
        JobRepo::mark_completed(&self.pool, job_id, final_status, total, passed, failed).await?;

        tracing::info!(
            job_id = %job_id,
            status = final_status,
            total, passed, failed, skipped,
            "Job finished"
        );

        self.publish_event(&format!("job.{final_status}"), job_id, job.tenant_id, final_status)
            .await;

        // P6.8 — Fire webhook if configured
        if let Some(ref url) = job.webhook_url {
            let result = crate::JobResult {
                job_id,
                status: final_status.to_string(),
                total,
                passed,
                failed,
                skipped,
                duration_ms: job
                    .created_at
                    .signed_duration_since(chrono::Utc::now())
                    .num_milliseconds()
                    .unsigned_abs() as i64,
            };
            tokio::spawn(webhook::send(url.clone(), result));
        }

        Ok(())
    }

    /// Cancel a running or queued job.
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<bool, sqlx::Error> {
        let updated = JobRepo::set_status(&self.pool, job_id, "cancelled").await?;
        if updated {
            if let Some(job) = JobRepo::find_by_id(&self.pool, job_id).await? {
                self.publish_event("job.cancelled", job_id, job.tenant_id, "cancelled")
                    .await;
            }
        }
        Ok(updated)
    }

    async fn publish_event(&self, event_type: &str, job_id: Uuid, tenant_id: Uuid, status: &str) {
        let Some(ref nc) = self.nats else { return };
        let event = serde_json::json!({
            "event": event_type,
            "job_id": job_id,
            "tenant_id": tenant_id,
            "status": status,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let _ = nc
            .publish(format!("ef.jobs.{job_id}"), payload.clone().into())
            .await;
        let _ = nc
            .publish("ef.jobs.events".to_string(), payload.into())
            .await;
    }
}

/// Spawn a background job processor that polls for queued jobs and executes them.
pub fn spawn_job_processor(
    pool: PgPool,
    nats: Option<async_nats::Client>,
    artifact_dir: String,
    poll_interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let engine = Engine::new(pool.clone(), nats, artifact_dir);
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(poll_interval_secs));
        loop {
            tick.tick().await;
            match JobRepo::list_queued(&pool, 5).await {
                Ok(jobs) => {
                    for job in jobs {
                        tracing::info!(job_id = %job.id, "Picking up queued job");
                        if let Err(e) = engine.execute_job(job.id).await {
                            tracing::error!(job_id = %job.id, "Job execution failed: {e}");
                            let _ = JobRepo::set_status(&pool, job.id, "failed").await;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to poll job queue: {e}");
                }
            }
        }
    })
}
