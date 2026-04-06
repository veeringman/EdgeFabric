//! # ef-orchestrator
//!
//! Test job orchestration: enqueue, dispatch, parallel execution, retry, result aggregation.

pub mod engine;
pub mod runner;
pub mod webhook;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Job status (mirrors DB enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Summary of a completed job's results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: Uuid,
    pub status: String,
    pub total: i32,
    pub passed: i32,
    pub failed: i32,
    pub skipped: i32,
    pub duration_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_status_as_str() {
        assert_eq!(JobStatus::Queued.as_str(), "queued");
        assert_eq!(JobStatus::Running.as_str(), "running");
        assert_eq!(JobStatus::Completed.as_str(), "completed");
        assert_eq!(JobStatus::Failed.as_str(), "failed");
        assert_eq!(JobStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn job_status_terminal() {
        assert!(!JobStatus::Queued.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
    }

    #[test]
    fn job_status_serde_roundtrip() {
        for status in [
            JobStatus::Queued,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: JobStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn job_result_serialize() {
        let result = JobResult {
            job_id: Uuid::nil(),
            status: "completed".into(),
            total: 10,
            passed: 8,
            failed: 1,
            skipped: 1,
            duration_ms: 5000,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["total"], 10);
        assert_eq!(json["passed"], 8);
        assert_eq!(json["status"], "completed");
    }
}
