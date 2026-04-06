//! TestService gRPC implementation.

use ef_db::repo::job::{JobRepo, TestCaseRepo};
use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::proto::test::test_service_server::TestService;
use crate::proto::test::{
    CancelJobRequest, CancelJobResponse, JobStatus as ProtoJobStatus, JobStatusRequest,
    RunTestRequest, RunTestResponse, TestResult,
};

pub struct TestServiceImpl {
    pool: PgPool,
}

impl TestServiceImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[tonic::async_trait]
impl TestService for TestServiceImpl {
    async fn run_test(
        &self,
        request: Request<RunTestRequest>,
    ) -> Result<Response<RunTestResponse>, Status> {
        let req = request.into_inner();
        tracing::info!(
            parallelism = req.parallelism,
            timeout_minutes = req.timeout_minutes,
            "gRPC RunTest"
        );

        let parallelism = if req.parallelism > 0 { req.parallelism } else { 1 };
        let tenant_id = Uuid::nil(); // auth interceptor should inject real tenant

        // Create a job record
        let job = JobRepo::create(
            &self.pool,
            tenant_id,
            parallelism,
            "grpc-submitted",
            None, // no webhook
        )
        .await
        .map_err(|e| Status::internal(format!("Job creation failed: {e}")))?;

        // Parse test names from the test_suite bytes (newline-separated names)
        let suite_text = String::from_utf8_lossy(&req.test_suite);
        let test_names: Vec<String> = suite_text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .collect();

        if test_names.is_empty() {
            return Err(Status::invalid_argument("test_suite contains no test names"));
        }

        let max_retries = 1;
        TestCaseRepo::create_batch(&self.pool, job.id, &test_names, max_retries)
            .await
            .map_err(|e| Status::internal(format!("Test case creation failed: {e}")))?;

        // Mark the job as queued
        let _ = JobRepo::set_status(&self.pool, job.id, "queued").await;

        Ok(Response::new(RunTestResponse {
            job_id: job.id.to_string(),
            status: "queued".to_string(),
            devices_allocated: 0,
        }))
    }

    async fn get_job_status(
        &self,
        request: Request<JobStatusRequest>,
    ) -> Result<Response<ProtoJobStatus>, Status> {
        let req = request.into_inner();
        let id = Uuid::parse_str(&req.job_id)
            .map_err(|_| Status::invalid_argument("Invalid job_id UUID"))?;

        let job = JobRepo::find_by_id(&self.pool, id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Job {id} not found")))?;

        let (total, passed, failed, _skipped) = TestCaseRepo::aggregate(&self.pool, job.id)
            .await
            .map_err(|e| Status::internal(format!("Aggregate error: {e}")))?;

        let running = total - passed - failed - _skipped;
        let pending = _skipped; // tests not yet started

        Ok(Response::new(ProtoJobStatus {
            job_id: job.id.to_string(),
            status: job.status.clone(),
            total_tests: total,
            passed,
            failed,
            running,
            pending,
        }))
    }

    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> Result<Response<CancelJobResponse>, Status> {
        let req = request.into_inner();
        let id = Uuid::parse_str(&req.job_id)
            .map_err(|_| Status::invalid_argument("Invalid job_id UUID"))?;

        let updated = JobRepo::set_status(&self.pool, id, "cancelled")
            .await
            .map_err(|e| Status::internal(format!("Cancel error: {e}")))?;

        if !updated {
            return Err(Status::not_found(format!("Job {id} not found")));
        }

        tracing::info!(%id, "Job cancelled via gRPC");
        Ok(Response::new(CancelJobResponse { success: true }))
    }

    type StreamResultsStream = ReceiverStream<Result<TestResult, Status>>;

    async fn stream_results(
        &self,
        request: Request<JobStatusRequest>,
    ) -> Result<Response<Self::StreamResultsStream>, Status> {
        let req = request.into_inner();
        let job_id = Uuid::parse_str(&req.job_id)
            .map_err(|_| Status::invalid_argument("Invalid job_id UUID"))?;

        // Verify job exists
        let _ = JobRepo::find_by_id(&self.pool, job_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {e}")))?
            .ok_or_else(|| Status::not_found(format!("Job {job_id} not found")))?;

        let pool = self.pool.clone();
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut seen = std::collections::HashSet::new();
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));

            loop {
                interval.tick().await;

                let cases = match TestCaseRepo::list_by_job(&pool, job_id).await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("StreamResults DB error: {e}");
                        continue;
                    }
                };

                let mut all_done = true;
                for tc in &cases {
                    if tc.status == "pending" || tc.status == "running" {
                        all_done = false;
                        continue;
                    }
                    // Send completed test results we haven't sent yet
                    if seen.insert(tc.id) {
                        let result = TestResult {
                            test_name: tc.name.clone(),
                            device_id: tc.device_id.map(|d| d.to_string()).unwrap_or_default(),
                            status: tc.status.clone(),
                            duration_ms: tc.duration_ms.unwrap_or(0) as i64,
                            error_message: tc.error.clone().unwrap_or_default(),
                        };
                        if tx.send(Ok(result)).await.is_err() {
                            return; // client disconnected
                        }
                    }
                }

                if all_done && !cases.is_empty() {
                    return; // all done
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
