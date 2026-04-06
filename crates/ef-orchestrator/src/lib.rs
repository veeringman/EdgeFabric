//! # ef-orchestrator
//!
//! Test job orchestration: enqueue, dispatch, parallel execution, retry, result aggregation.

use ef_core::error::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestJob {
    pub id: Uuid,
    pub status: JobStatus,
    pub parallelism: u32,
    pub total_tests: u32,
    pub passed: u32,
    pub failed: u32,
}

pub struct Orchestrator {
    // Will hold references to scheduler, control plane, and state store.
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {}
    }

    /// Submit a new test job for execution.
    pub async fn submit(&self, _parallelism: u32) -> Result<TestJob> {
        let job = TestJob {
            id: Uuid::new_v4(),
            status: JobStatus::Queued,
            parallelism: _parallelism,
            total_tests: 0,
            passed: 0,
            failed: 0,
        };
        tracing::info!(job_id = %job.id, "Test job queued");
        Ok(job)
    }
}
