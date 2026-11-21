//! Tenant-scoped immutable hardware profile registry and validation API.
use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use ef_core::hardware_profiles::{HardwareProfile, generic_gpu_node_profile};
use ef_db::repo::infrastructure_hardware_profile::InfrastructureHardwareProfileRepo;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

#[derive(Debug, Deserialize)]
pub struct ValidateProfileRequest { pub profile: HardwareProfile }

#[derive(Debug, Deserialize)]
pub struct ProfileQuery { pub profile_id: Option<String> }

fn qualification_name(profile: &HardwareProfile) -> &'static str {
    match profile.qualification {
        ef_core::hardware_profiles::HardwareProfileQualification::Draft => "draft",
        ef_core::hardware_profiles::HardwareProfileQualification::TemplateOnly => "template_only",
        ef_core::hardware_profiles::HardwareProfileQualification::VendorDocumented => "vendor_documented",
        ef_core::hardware_profiles::HardwareProfileQualification::VendorValidated => "vendor_validated",
        ef_core::hardware_profiles::HardwareProfileQualification::FieldVerified => "field_verified",
    }
}

/// GET /api/v1/infrastructure/hardware-profiles
/// Lists built-in templates and the caller's tenant-owned immutable profile versions.
pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(query): Query<ProfileQuery>,
) -> ApiResult<Json<Value>> {
    let rows = InfrastructureHardwareProfileRepo::list(
        &state.pool, auth.tenant_id, query.profile_id.as_deref()
    ).await.map_err(|error| {
        tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Hardware profile listing failed");
        ApiError::internal("Failed to list hardware profiles")
    })?;
    let count = rows.len();
    let builtins = if query.profile_id.as_deref().is_none_or(|id| id == "edgefabric.generic-gpu-node") {
        vec![generic_gpu_node_profile()]
    } else { Vec::new() };
    Ok(Json(json!({
        "profiles": rows,
        "built_in_profiles": builtins,
        "count": count,
        "catalog": "edgefabric",
        "catalog_version": 2
    })))
}

/// GET /api/v1/infrastructure/hardware-profiles/{id}
/// The UUID identifies a tenant-owned profile version.
pub async fn get(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let profile = InfrastructureHardwareProfileRepo::get(&state.pool, auth.tenant_id, id)
        .await.map_err(|error| {
            tracing::error!(tenant_id = %auth.tenant_id, profile_id = %id, error = %error, "Hardware profile lookup failed");
            ApiError::internal("Failed to load hardware profile")
        })?.ok_or_else(|| ApiError::not_found("Hardware profile not found"))?;
    Ok(Json(json!({ "profile": profile })))
}

/// POST /api/v1/infrastructure/hardware-profiles/validate
/// Validates without persistence or qualification approval.
pub async fn validate(
    Json(body): Json<ValidateProfileRequest>,
) -> ApiResult<Json<Value>> {
    let issues = body.profile.validate();
    Ok(Json(json!({
        "valid": issues.is_empty(),
        "issues": issues,
        "persisted": false,
        "qualification_granted": false
    })))
}

/// POST /api/v1/infrastructure/hardware-profiles
/// Stores an immutable tenant-owned profile version in draft review state.
pub async fn create(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(body): Json<ValidateProfileRequest>,
) -> ApiResult<Json<Value>> {
    let issues = body.profile.validate();
    if !issues.is_empty() {
        return Err(ApiError::bad_request(format!("Hardware profile validation failed: {}", issues.join("; "))));
    }
    let version = i32::try_from(body.profile.version)
        .map_err(|_| ApiError::bad_request("Profile version exceeds the supported integer range"))?;
    let document = serde_json::to_value(&body.profile)
        .map_err(|error| ApiError::bad_request(format!("Invalid hardware profile document: {error}")))?;
    let qualification = qualification_name(&body.profile);
    let row = InfrastructureHardwareProfileRepo::create(
        &state.pool, auth.tenant_id, &body.profile.id, version, qualification, &document
    ).await.map_err(|error| {
        if let sqlx::Error::Database(db_error) = &error {
            if db_error.code().as_deref() == Some("23505") {
                return ApiError::bad_request("This profile ID and version already exist in the tenant; create a new immutable version");
            }
        }
        tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Hardware profile persistence failed");
        ApiError::internal("Failed to store hardware profile")
    })?;
    Ok(Json(json!({
        "profile": row,
        "immutable": true,
        "review_required": true,
        "qualification_approved": false
    })))
}

/// POST /api/v1/infrastructure/hardware-profiles/{id}/submit
/// Submits a tenant-owned draft for review; this does not approve or qualify it.
pub async fn submit(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = InfrastructureHardwareProfileRepo::submit(&state.pool, auth.tenant_id, id)
        .await.map_err(|error| {
            tracing::error!(tenant_id = %auth.tenant_id, profile_id = %id, error = %error, "Hardware profile submission failed");
            ApiError::internal("Failed to submit hardware profile")
        })?.ok_or_else(|| ApiError::bad_request("Profile was not found in this tenant or is not in draft status"))?;
    Ok(Json(json!({
        "profile": row,
        "submitted": true,
        "qualification_approved": false,
        "review_workflow": "pending_external_authorized_review"
    })))
}
