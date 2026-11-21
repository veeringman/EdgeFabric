//! Tenant-scoped immutable hardware profile registry and validation API.
use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use ef_core::audit::{AuditAction, AuditLogger};
use ef_core::hardware_profiles::{HardwareProfile, builtin_hardware_profiles};
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

#[derive(Debug, Deserialize)]
pub struct CloneProfileRequest { pub profile_id: String }

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
    let builtins = builtin_hardware_profiles().into_iter()
        .filter(|profile| query.profile_id.as_deref().is_none_or(|id| profile.id == id))
        .collect::<Vec<_>>();
    let built_in_profile_count = builtins.len();
    Ok(Json(json!({
        "profiles": rows,
        "built_in_profiles": builtins,
        "count": count,
        "tenant_profile_count": count,
        "built_in_profile_count": built_in_profile_count,
        "catalog": "edgefabric",
        "catalog_version": 4
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
    let profile_record_id = row.id.to_string();
    if let Err(error) = AuditLogger::log(
        &state.pool,
        Some(auth.tenant_id),
        &auth.tenant_name,
        AuditAction::Custom("hardware_profile_submitted".into()),
        "infrastructure_hardware_profile",
        Some(&profile_record_id),
        json!({
            "profile_id": row.profile_id,
            "version": row.version,
            "qualification_claim": row.qualification,
            "review_status": row.review_status
        }),
        None,
    ).await {
        tracing::error!(tenant_id = %auth.tenant_id, profile_id = %row.id, error = %error, "Hardware profile submission audit write failed");
    }
    Ok(Json(json!({
        "profile": row,
        "submitted": true,
        "qualification_approved": false,
        "review_workflow": "pending_external_authorized_review"
    })))
}

/// POST /api/v1/infrastructure/hardware-profiles/clone
/// Copies a read-only built-in reference into the caller's tenant as an immutable draft-review record.
pub async fn clone_builtin(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(body): Json<CloneProfileRequest>,
) -> ApiResult<Json<Value>> {
    let profile = builtin_hardware_profiles().into_iter()
        .find(|profile| profile.id == body.profile_id)
        .ok_or_else(|| ApiError::not_found("Built-in hardware profile not found"))?;
    let issues = profile.validate();
    if !issues.is_empty() {
        return Err(ApiError::internal(format!("Built-in hardware profile failed validation: {}", issues.join("; "))));
    }
    let version = i32::try_from(profile.version)
        .map_err(|_| ApiError::internal("Built-in profile version exceeds the supported integer range"))?;
    let document = serde_json::to_value(&profile)
        .map_err(|error| ApiError::internal(format!("Invalid built-in profile document: {error}")))?;
    let qualification = qualification_name(&profile);
    let row = InfrastructureHardwareProfileRepo::create(
        &state.pool, auth.tenant_id, &profile.id, version, qualification, &document
    ).await.map_err(|error| {
        if let sqlx::Error::Database(db_error) = &error {
            if db_error.code().as_deref() == Some("23505") {
                return ApiError::bad_request("This built-in profile version already exists in the tenant");
            }
        }
        tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Built-in hardware profile cloning failed");
        ApiError::internal("Failed to clone built-in hardware profile")
    })?;
    Ok(Json(json!({
        "profile": row,
        "cloned_from": "edgefabric_builtin_catalog",
        "immutable": true,
        "review_status": "draft",
        "qualification_approved": false
    })))
}
