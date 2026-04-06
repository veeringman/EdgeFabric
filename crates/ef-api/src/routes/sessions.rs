use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

use ef_core::session::SessionStatus;
use ef_db::repo::device::DeviceRepo;
use ef_db::repo::session::SessionRepo;
use ef_db::repo::tenant::TenantRepo;

// ── Request bodies ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub device_id: Uuid,
    #[serde(default = "default_duration_minutes")]
    pub duration_minutes: i64,
}

fn default_duration_minutes() -> i64 {
    30
}

#[derive(Deserialize)]
pub struct TransitionRequest {
    pub status: String,
}

// ── POST /api/v1/sessions ─────────────────────────────────────────

pub async fn create_session(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    tracing::info!(
        tenant_id = %auth.tenant_id,
        device_id = %body.device_id,
        "Creating session"
    );

    // P5.7 — Check tenant session limit
    let tenant = TenantRepo::find_by_id(&state.pool, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found("Tenant not found"))?;

    let active_count =
        SessionRepo::count_active_by_tenant(&state.pool, auth.tenant_id)
            .await
            .map_err(|e| {
                tracing::error!("DB error: {e}");
                ApiError::internal("Database error")
            })?;

    if active_count >= tenant.max_sessions as i64 {
        return Err(ApiError::bad_request(format!(
            "Session limit exceeded: {active_count}/{} active sessions",
            tenant.max_sessions
        )));
    }

    // Verify device exists and is available
    let device = DeviceRepo::find_by_id(&state.pool, body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Device {} not found", body.device_id)))?;

    if device.status != "available" {
        return Err(ApiError::bad_request(format!(
            "Device {} is not available (status: {})",
            body.device_id, device.status
        )));
    }

    // Lock the device
    DeviceRepo::set_status(&state.pool, body.device_id, "busy")
        .await
        .map_err(|e| {
            tracing::error!("Failed to lock device: {e}");
            ApiError::internal("Failed to lock device")
        })?;

    let expires_at = Utc::now() + Duration::minutes(body.duration_minutes);

    let session = SessionRepo::create(
        &state.pool,
        body.device_id,
        auth.tenant_id,
        expires_at,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to create session: {e}");
        ApiError::internal("Failed to create session")
    })?;

    // Automatically transition Requested → Queued → Allocating → Active
    let _ = SessionRepo::set_status(&state.pool, session.id, "active").await;

    // Re-fetch to get final state
    let session = SessionRepo::find_by_id(&state.pool, session.id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .unwrap();

    Ok((StatusCode::CREATED, Json(json!({ "session": session }))))
}

// ── GET /api/v1/sessions ──────────────────────────────────────────

pub async fn list_sessions(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, "Listing sessions");

    let sessions =
        SessionRepo::list_by_tenant(&state.pool, auth.tenant_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list sessions: {e}");
                ApiError::internal("Failed to list sessions")
            })?;

    let total = sessions.len();
    Ok(Json(json!({ "sessions": sessions, "total": total })))
}

// ── GET /api/v1/sessions/:id ─────────────────────────────────────

pub async fn get_session(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, session_id = %id, "Getting session");

    let session = SessionRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Session {id} not found")))?;

    // Ensure tenant can only see their own sessions
    if session.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Session {id} not found")));
    }

    Ok(Json(json!({ "session": session })))
}

// ── PUT /api/v1/sessions/:id/status ──────────────────────────────

pub async fn transition_session(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<TransitionRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let target = SessionStatus::from_str_lowercase(&body.status)
        .ok_or_else(|| ApiError::bad_request(format!("Invalid session status: {}", body.status)))?;

    let session = SessionRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Session {id} not found")))?;

    if session.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Session {id} not found")));
    }

    let current = SessionStatus::from_str_lowercase(&session.status)
        .ok_or_else(|| ApiError::internal("Session has invalid status in DB"))?;

    // P5.1 — Enforce state machine transitions
    if !current.can_transition_to(target) {
        return Err(ApiError::bad_request(format!(
            "Invalid transition: {} → {}",
            current, target
        )));
    }

    let updated = SessionRepo::transition(
        &state.pool,
        id,
        current.as_str(),
        target.as_str(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to transition session: {e}");
        ApiError::internal("Failed to transition session")
    })?
    .ok_or_else(|| {
        ApiError::bad_request("Transition failed: session status may have changed concurrently")
    })?;

    tracing::info!(session_id = %id, from = %current, to = %target, "Session transitioned");

    Ok(Json(json!({ "session": updated })))
}

// ── POST /api/v1/sessions/:id/end ────────────────────────────────

pub async fn end_session(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    tracing::info!(tenant_id = %auth.tenant_id, session_id = %id, "Ending session");

    let session = SessionRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .ok_or_else(|| ApiError::not_found(format!("Session {id} not found")))?;

    if session.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Session {id} not found")));
    }

    let current = SessionStatus::from_str_lowercase(&session.status)
        .ok_or_else(|| ApiError::internal("Session has invalid status in DB"))?;

    if current.is_terminal() {
        return Err(ApiError::bad_request(format!(
            "Session is already in terminal state: {current}"
        )));
    }

    // End the session
    SessionRepo::end_session(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to end session: {e}");
            ApiError::internal("Failed to end session")
        })?;

    // P5.5 — Release the device back to available
    DeviceRepo::set_status(&state.pool, session.device_id, "available")
        .await
        .map_err(|e| {
            tracing::error!("Failed to release device: {e}");
            ApiError::internal("Failed to release device")
        })?;

    tracing::info!(
        session_id = %id,
        device_id = %session.device_id,
        "Session ended, device released"
    );

    let ended = SessionRepo::find_by_id(&state.pool, id)
        .await
        .map_err(|e| {
            tracing::error!("DB error: {e}");
            ApiError::internal("Database error")
        })?
        .unwrap();

    Ok(Json(json!({ "session": ended })))
}
