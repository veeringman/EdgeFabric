//! Read-only Infrastructure Fabric views over existing EdgeFabric runner inventory.
//!
//! This first vertical slice deliberately reuses registered runners and their
//! existing probe snapshots. It does not launch a second discovery process.

use axum::{Json, extract::State, http::StatusCode};
use chrono::Utc;
use ef_core::infrastructure::{
    DiscoveredResource, DiscoverySnapshot, ResourceDeclaration, reconcile,
};
use ef_db::repo::runner::{RunnerRepo, RunnerRow};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

#[derive(Debug, Deserialize)]
pub struct ReconcileRequest {
    /// Complete declared host set for the authenticated tenant/scope.
    pub declarations: Vec<ResourceDeclaration>,
}

/// GET /api/v1/infrastructure/inventory
///
/// Returns a normalized, provenance-attributed view of the authenticated
/// tenant's existing registered runners. Runner probes remain the discovery
/// source; this endpoint is read-only.
pub async fn inventory(
    State(state): State<AppState>,
    auth: AuthContext,
) -> ApiResult<Json<Value>> {
    let runners = RunnerRepo::list_by_tenant(&state.pool, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %e, "Infrastructure inventory query failed");
            ApiError::internal("Failed to load infrastructure inventory")
        })?;

    let resources: Vec<DiscoveredResource> = runners.iter().map(runner_resource).collect();
    Ok(Json(json!({
        "resources": resources,
        "count": resources.len(),
        "scope": { "tenant_id": auth.tenant_id },
        "source": "edgefabric.runner_inventory",
        "read_only": true
    })))
}

/// POST /api/v1/infrastructure/reconcile
///
/// Compares a caller-supplied, validated declaration set with the tenant's
/// existing runner inventory. No discovery or infrastructure mutation occurs.
pub async fn reconcile_inventory(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(body): Json<ReconcileRequest>,
) -> ApiResult<Json<Value>> {
    if body.declarations.len() > 10_000 {
        return Err(ApiError::bad_request("A reconciliation request may contain at most 10000 declarations"));
    }

    let runners = RunnerRepo::list_by_tenant(&state.pool, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %e, "Infrastructure reconciliation query failed");
            ApiError::internal("Failed to load infrastructure inventory")
        })?;

    let resources: Vec<DiscoveredResource> = runners.iter().map(runner_resource).collect();
    // This endpoint covers registered runner inventory only. It is not proof
    // that the full physical site/network has been discovered.
    let snapshot = DiscoverySnapshot {
        observed_at: Utc::now(),
        coverage_complete: false,
        resources,
    };
    let result = reconcile(&body.declarations, &snapshot);

    Ok(Json(json!({
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

fn runner_resource(runner: &RunnerRow) -> DiscoveredResource {
    let mut attributes = BTreeMap::new();
    attributes.insert("name".into(), json!(runner.name));
    attributes.insert("runner_class".into(), json!(runner.runner_class));
    attributes.insert("host_os".into(), json!(runner.host_os));
    attributes.insert("architecture".into(), json!(runner.architecture));
    attributes.insert("region".into(), json!(runner.region));
    attributes.insert("status".into(), json!(runner.status));
    attributes.insert("capabilities".into(), json!(runner.capabilities));
    attributes.insert("labels".into(), json!(runner.labels));
    attributes.insert("max_concurrency".into(), json!(runner.max_concurrency));
    attributes.insert("drained".into(), json!(runner.drained));

    let hardware = runner.last_probe_result.get("hardware").cloned().unwrap_or(Value::Null);
    if let Some(object) = hardware.as_object() {
        for (key, value) in object {
            attributes.insert(format!("hardware.{key}"), value.clone());
        }
    }

    let probe_verified = runner.last_probe_at.is_some()
        && runner.last_probe_result.get("reachable").and_then(Value::as_bool) == Some(true)
        && runner.last_probe_result.get("ssh_authenticated_as").and_then(Value::as_str).is_some();

    DiscoveredResource {
        identity: format!("runner:{}", runner.id),
        kind: "host".into(),
        attributes,
        source: if runner.last_probe_at.is_some() {
            "edgefabric.runner_probe".into()
        } else {
            "edgefabric.runner_registration".into()
        },
        observed_at: runner.last_probe_at.clone().unwrap_or_else(|| runner.updated_at.clone()),
        verified: probe_verified,
    }
}
