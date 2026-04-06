use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

use ef_db::repo::job::{ArtifactRepo, JobRepo, TestCaseRepo};

// ── Request bodies ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SubmitJobRequest {
    pub test_suite: String,
    pub test_names: Vec<String>,
    #[serde(default = "default_parallelism")]
    pub parallelism: i32,
    #[serde(default = "default_retries")]
    pub max_retries: i32,
    pub webhook_url: Option<String>,
}

fn default_parallelism() -> i32 { 1 }
fn default_retries() -> i32 { 2 }

// ── POST /api/v1/jobs ─────────────────────────────────────────────

pub async fn submit_job(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<SubmitJobRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    tracing::info!(
        tenant_id = %auth.tenant_id,
        suite = %body.test_suite,
        tests = body.test_names.len(),
        "Submitting job"
    );

    if body.test_names.is_empty() {
        return Err(ApiError::bad_request("test_names must not be empty"));
    }

    let job = JobRepo::create(
        &state.pool,
        auth.tenant_id,
        body.parallelism,
        &body.test_suite,
        body.webhook_url.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to create job: {e}");
        ApiError::internal("Failed to create job")
    })?;

    TestCaseRepo::create_batch(&state.pool, job.id, &body.test_names, body.max_retries)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create test cases: {e}");
            ApiError::internal("Failed to create test cases")
        })?;

    JobRepo::update_progress(&state.pool, job.id, body.test_names.len() as i32, 0, 0)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update job progress: {e}");
            ApiError::internal("Failed to update job")
        })?;

    // Re-fetch to get the updated row
    let job = JobRepo::find_by_id(&state.pool, job.id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .unwrap();

    Ok((StatusCode::CREATED, Json(json!({ "job": job }))))
}

// ── GET /api/v1/jobs ──────────────────────────────────────────────

pub async fn list_jobs(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let jobs = JobRepo::list_by_tenant(&state.pool, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list jobs: {e}");
            ApiError::internal("Failed to list jobs")
        })?;

    let total = jobs.len();
    Ok(Json(json!({ "jobs": jobs, "total": total })))
}

// ── GET /api/v1/jobs/:id ──────────────────────────────────────────

pub async fn get_job(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let job = JobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;

    if job.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Job {id} not found")));
    }

    let test_cases = TestCaseRepo::list_by_job(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?;

    let artifacts = ArtifactRepo::list_by_job(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?;

    Ok(Json(json!({
        "job": job,
        "test_cases": test_cases,
        "artifacts": artifacts,
    })))
}

// ── GET /api/v1/jobs/:id/progress ─────────────────────────────────

pub async fn get_job_progress(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let job = JobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;

    if job.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Job {id} not found")));
    }

    let (total, passed, failed, skipped) = TestCaseRepo::aggregate(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?;

    let pending = total - passed - failed - skipped;

    Ok(Json(json!({
        "job_id": id,
        "status": job.status,
        "total": total,
        "passed": passed,
        "failed": failed,
        "skipped": skipped,
        "pending": pending,
        "started_at": job.started_at,
        "completed_at": job.completed_at,
    })))
}

// ── POST /api/v1/jobs/:id/cancel ──────────────────────────────────

pub async fn cancel_job(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let job = JobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;

    if job.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Job {id} not found")));
    }

    if job.status == "completed" || job.status == "failed" || job.status == "cancelled" {
        return Err(ApiError::bad_request(format!(
            "Job is already in terminal state: {}",
            job.status
        )));
    }

    JobRepo::set_status(&state.pool, id, "cancelled")
        .await
        .map_err(|e| {
            tracing::error!("Failed to cancel job: {e}");
            ApiError::internal("Failed to cancel job")
        })?;

    tracing::info!(job_id = %id, "Job cancelled");

    Ok(Json(json!({ "job_id": id, "status": "cancelled" })))
}

// ── GET /api/v1/jobs/:id/artifacts ────────────────────────────────

pub async fn list_artifacts(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let job = JobRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;

    if job.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Job {id} not found")));
    }

    let artifacts = ArtifactRepo::list_by_job(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?;

    Ok(Json(json!({ "artifacts": artifacts, "total": artifacts.len() })))
}
