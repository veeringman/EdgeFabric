//! Read-only catalog and validation endpoints for versioned hardware profiles.
use axum::{Json, extract::Path, http::StatusCode};
use ef_core::hardware_profiles::{HardwareProfile, generic_gpu_node_profile};
use serde::Deserialize;
use serde_json::{Value, json};
use crate::middleware::error_response::ApiError;
type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;
#[derive(Debug, Deserialize)]
pub struct ValidateProfileRequest { pub profile: HardwareProfile }
/// GET /api/v1/infrastructure/hardware-profiles
pub async fn list() -> Json<Value> {
    let profiles = vec![generic_gpu_node_profile()];
    Json(json!({
        "profiles": profiles, "count": profiles.len(),
        "catalog": "edgefabric.builtin", "catalog_version": 1,
        "limitations": [
            "Built-in catalog currently contains generic topology templates only.",
            "Vendor-specific Rubin and Rubin Ultra profiles are not asserted until authoritative source documents are attached and reviewed."
        ]
    }))
}
/// GET /api/v1/infrastructure/hardware-profiles/:id
pub async fn get(Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let profile = generic_gpu_node_profile();
    if id != profile.id { return Err(ApiError::not_found("Hardware profile not found")); }
    Ok(Json(json!({ "profile": profile })))
}
/// POST /api/v1/infrastructure/hardware-profiles/validate
/// Validates a profile payload without persisting or qualifying it.
pub async fn validate(Json(body): Json<ValidateProfileRequest>) -> ApiResult<Json<Value>> {
    let issues = body.profile.validate();
    Ok(Json(json!({ "valid": issues.is_empty(), "issues": issues, "persisted": false, "qualification_granted": false })))
}
