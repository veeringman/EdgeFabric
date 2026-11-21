//! Tenant-scoped, versioned declarative infrastructure manifest APIs.
use axum::{Json, extract::{Path, State}, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use ef_db::repo::infrastructure_manifest::InfrastructureManifestRepo;
use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

#[derive(Debug, Deserialize)]
pub struct CreateManifestRequest {
    pub name: String,
    pub version: i32,
    #[serde(default)]
    pub description: String,
    pub document: Value,
}

/// POST /api/v1/infrastructure/manifests
/// Creates an immutable manifest version. A new revision is a new version.
pub async fn create(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(body): Json<CreateManifestRequest>,
) -> ApiResult<Json<Value>> {
    let name = body.name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err(ApiError::bad_request("Manifest name must contain 1 to 200 characters"));
    }
    if body.version < 1 {
        return Err(ApiError::bad_request("Manifest version must be a positive integer"));
    }
    if !body.document.is_object() {
        return Err(ApiError::bad_request("Manifest document must be a JSON object"));
    }
    if body.document.get("declarations").and_then(Value::as_array).is_none() {
        return Err(ApiError::bad_request("Manifest document must contain a declarations array"));
    }

    let row = InfrastructureManifestRepo::create(
        &state.pool, auth.tenant_id, name, body.version, &body.description, &body.document,
    ).await.map_err(|error| {
        if let sqlx::Error::Database(db_error) = &error {
            if db_error.code().as_deref() == Some("23505") {
                return ApiError::bad_request("A manifest with this name and version already exists");
            }
        }
        tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Infrastructure manifest creation failed");
        ApiError::internal("Failed to create infrastructure manifest")
    })?;

    Ok(Json(json!({ "manifest": row, "read_only": true })))
}

/// GET /api/v1/infrastructure/manifests
pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
) -> ApiResult<Json<Value>> {
    let manifests = InfrastructureManifestRepo::list(&state.pool, auth.tenant_id)
        .await
        .map_err(|error| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Infrastructure manifest listing failed");
            ApiError::internal("Failed to list infrastructure manifests")
        })?;
    Ok(Json(json!({ "manifests": manifests, "count": manifests.len() })))
}

/// GET /api/v1/infrastructure/manifests/:id
pub async fn get(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let manifest = InfrastructureManifestRepo::get(&state.pool, auth.tenant_id, id)
        .await
        .map_err(|error| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Infrastructure manifest lookup failed");
            ApiError::internal("Failed to load infrastructure manifest")
        })?
        .ok_or_else(|| ApiError::not_found("Infrastructure manifest not found"))?;
    Ok(Json(json!({ "manifest": manifest })))
}
