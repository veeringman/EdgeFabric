//! Tenant-scoped, versioned declarative infrastructure manifest APIs.
use axum::{Json, extract::{Path, State}, http::StatusCode};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use ef_core::infrastructure::{DiscoverySnapshot, ResourceDeclaration, reconcile, validate_declarations};
use ef_db::repo::{infrastructure_manifest::InfrastructureManifestRepo, runner::RunnerRepo};
use crate::routes::infrastructure::runner_resource;
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
    let declarations = body.document.get("declarations")
        .cloned()
        .filter(Value::is_array)
        .ok_or_else(|| ApiError::bad_request("Manifest document must contain a declarations array"))?;
    let declarations: Vec<ResourceDeclaration> = serde_json::from_value(declarations)
        .map_err(|error| ApiError::bad_request(format!("Invalid manifest declarations: {error}")))?;
    let issues = validate_declarations(&declarations);
    if !issues.is_empty() {
        return Err(ApiError::bad_request(format!("Manifest validation failed: {}", issues.join("; "))));
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

/// POST /api/v1/infrastructure/manifests/validate
/// Validates a manifest document without persisting it.
pub async fn validate(
    Json(body): Json<CreateManifestRequest>,
) -> ApiResult<Json<Value>> {
    let mut issues = Vec::new();
    if body.name.trim().is_empty() || body.name.len() > 200 {
        issues.push("Manifest name must contain 1 to 200 characters".to_string());
    }
    if body.version < 1 {
        issues.push("Manifest version must be a positive integer".to_string());
    }
    let declarations = body.document.get("declarations")
        .cloned()
        .filter(Value::is_array)
        .ok_or_else(|| ApiError::bad_request("Manifest document must contain a declarations array"))?;
    let declarations: Vec<ResourceDeclaration> = serde_json::from_value(declarations)
        .map_err(|error| ApiError::bad_request(format!("Invalid manifest declarations: {error}")))?;
    issues.extend(validate_declarations(&declarations));
    Ok(Json(json!({
        "valid": issues.is_empty(),
        "issues": issues,
        "declaration_count": declarations.len(),
        "persisted": false
    })))
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

/// POST /api/v1/infrastructure/manifests/:id/reconcile
/// Reconciles a stored manifest against the current registered-runner snapshot.
pub async fn reconcile_manifest(
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

    let declarations_value = manifest.document.get("declarations")
        .cloned()
        .ok_or_else(|| ApiError::bad_request("Manifest document has no declarations array"))?;
    let declarations: Vec<ResourceDeclaration> = serde_json::from_value(declarations_value)
        .map_err(|error| ApiError::bad_request(format!("Invalid manifest declarations: {error}")))?;

    let runners = RunnerRepo::list_by_tenant(&state.pool, auth.tenant_id)
        .await
        .map_err(|error| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Infrastructure reconciliation query failed");
            ApiError::internal("Failed to load infrastructure inventory")
        })?;
    let resources = runners.iter().map(runner_resource).collect();
    let snapshot = DiscoverySnapshot {
        observed_at: Utc::now(),
        coverage_complete: false,
        resources,
    };
    let result = reconcile(&declarations, &snapshot);

    Ok(Json(json!({
        "manifest": {
            "id": manifest.id,
            "name": manifest.name,
            "version": manifest.version
        },
        "result": result,
        "scope": { "tenant_id": auth.tenant_id },
        "source": "edgefabric.runner_inventory",
        "read_only": true,
        "limitations": [
            "Only registered runners are included in this initial inventory slice.",
            "Unregistered physical resources cannot be classified as missing until complete discovery coverage is available."
        ]
    })))
}
