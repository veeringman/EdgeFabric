//! Tenant-scoped simulation scenario registry. This API validates and stores
//! reproducible simulation inputs; it does not execute a simulator.
use axum::{Json, extract::{Path, Query, State}, http::StatusCode};
use ef_core::infrastructure::{GpuClusterTemplate, SimulationInstance, instantiate_gpu_cluster_template, validate_simulation};
use ef_db::repo::infrastructure_simulation::InfrastructureSimulationRepo;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

#[derive(Debug, Deserialize)]
pub struct SimulationQuery {
    pub scenario_id: Option<String>,
}

fn validate_instance(instance: &SimulationInstance) -> Vec<String> {
    let mut issues = validate_simulation(instance);
    if instance.identity.len() > 200 {
        issues.push("Simulation identity must be at most 200 characters".into());
    }
    if instance.scenario_id.len() > 200 {
        issues.push("Scenario ID must be at most 200 characters".into());
    }
    if instance.engine.len() > 100 || instance.engine_version.len() > 100 {
        issues.push("Engine and engine version must be at most 100 characters".into());
    }
    if instance.graph.resources.len() > 10_000 {
        issues.push("A simulation graph may contain at most 10000 resources".into());
    }
    if instance.graph.relationships.len() > 20_000 {
        issues.push("A simulation graph may contain at most 20000 relationships".into());
    }
    if instance.pcie_topology.as_ref().is_some_and(|topology| topology.devices.len() > 10_000 || topology.links.len() > 20_000) {
        issues.push("A PCIe topology may contain at most 10000 devices and 20000 links".into());
    }
    if instance.gpu_peer_fabric.as_ref().is_some_and(|fabric| fabric.links.len() > 20_000) {
        issues.push("A GPU peer fabric may contain at most 20000 links".into());
    }
    if instance.network_fabric.as_ref().is_some_and(|fabric| fabric.links.len() > 20_000) {
        issues.push("A network fabric may contain at most 20000 links".into());
    }
    if instance.geometry.len() > 10_000 || instance.poses.len() > 10_000 {
        issues.push("A simulation may contain at most 10000 geometry or pose entries".into());
    }
    issues
}

/// POST /api/v1/infrastructure/simulations/validate
pub async fn validate(
    Json(instance): Json<SimulationInstance>,
) -> ApiResult<Json<Value>> {
    let issues = validate_instance(&instance);
    Ok(Json(json!({
        "valid": issues.is_empty(),
        "issues": issues,
        "persisted": false,
        "executed": false,
        "inventory_class": "simulated"
    })))
}

/// POST /api/v1/infrastructure/simulations
/// Stores an immutable, tenant-owned simulation input document.
pub async fn create(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(instance): Json<SimulationInstance>,
) -> ApiResult<Json<Value>> {
    let issues = validate_instance(&instance);
    if !issues.is_empty() {
        return Err(ApiError::bad_request(format!(
            "Simulation validation failed: {}", issues.join("; ")
        )));
    }
    let document = serde_json::to_value(&instance)
        .map_err(|error| ApiError::bad_request(format!("Invalid simulation document: {error}")))?;
    let seed = i64::try_from(instance.seed)
        .map_err(|_| ApiError::bad_request("Simulation seed must be within the signed 64-bit range"))?;
    let kind = serde_json::to_value(instance.kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into());

    let row = InfrastructureSimulationRepo::create(
        &state.pool,
        auth.tenant_id,
        &instance.identity,
        &kind,
        &instance.scenario_id,
        &instance.engine,
        &instance.engine_version,
        seed,
        instance.source_manifest_revision.as_deref(),
        &document,
    )
    .await
    .map_err(|error| {
        if let sqlx::Error::Database(db_error) = &error {
            if db_error.code().as_deref() == Some("23505") {
                return ApiError::bad_request("A simulation with this identity already exists in this tenant");
            }
        }
        tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Simulation persistence failed");
        ApiError::internal("Failed to store simulation")
    })?;

    Ok(Json(json!({
        "simulation": row,
        "inventory_class": "simulated",
        "physical_inventory_mutated": false,
        "execution_started": false
    })))
}

/// POST /api/v1/infrastructure/simulations/templates/gpu-cluster
/// Deterministically expands and persists a virtual multi-host GPU cluster.
pub async fn create_gpu_cluster(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(template): Json<GpuClusterTemplate>,
) -> ApiResult<Json<Value>> {
    let instance = instantiate_gpu_cluster_template(&template)
        .map_err(|issues| ApiError::bad_request(format!(
            "GPU cluster template validation failed: {}", issues.join("; ")
        )))?;
    let document = serde_json::to_value(&instance)
        .map_err(|error| ApiError::bad_request(format!("Invalid simulation document: {error}")))?;
    let seed = i64::try_from(instance.seed)
        .map_err(|_| ApiError::bad_request("Simulation seed must be within the signed 64-bit range"))?;
    let row = InfrastructureSimulationRepo::create(
        &state.pool,
        auth.tenant_id,
        &instance.identity,
        "topology_simulation",
        &instance.scenario_id,
        &instance.engine,
        &instance.engine_version,
        seed,
        instance.source_manifest_revision.as_deref(),
        &document,
    )
    .await
    .map_err(|error| {
        if let sqlx::Error::Database(db_error) = &error {
            if db_error.code().as_deref() == Some("23505") {
                return ApiError::bad_request("A simulation with this identity already exists in this tenant");
            }
        }
        tracing::error!(tenant_id = %auth.tenant_id, error = %error, "GPU cluster simulation persistence failed");
        ApiError::internal("Failed to store GPU cluster simulation")
    })?;

    Ok(Json(json!({
        "simulation": row,
        "inventory_class": "simulated",
        "resource_count": instance.graph.resources.len(),
        "relationship_count": instance.graph.relationships.len(),
        "pcie_device_count": instance.pcie_topology.as_ref().map_or(0, |topology| topology.devices.len()),
        "pcie_link_count": instance.pcie_topology.as_ref().map_or(0, |topology| topology.links.len()),
        "gpu_peer_fabric_kind": instance.gpu_peer_fabric.as_ref().map(|fabric| fabric.kind),
        "gpu_peer_link_count": instance.gpu_peer_fabric.as_ref().map_or(0, |fabric| fabric.links.len()),
        "network_fabric_kind": instance.network_fabric.as_ref().map(|fabric| fabric.kind),
        "network_fabric_link_count": instance.network_fabric.as_ref().map_or(0, |fabric| fabric.links.len()),
        "deterministic_topology": true,
        "physical_inventory_mutated": false,
        "execution_started": false
    })))
}

/// GET /api/v1/infrastructure/simulations
pub async fn list(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(query): Query<SimulationQuery>,
) -> ApiResult<Json<Value>> {
    let simulations = InfrastructureSimulationRepo::list(
        &state.pool, auth.tenant_id, query.scenario_id.as_deref()
    )
    .await
    .map_err(|error| {
        tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Simulation listing failed");
        ApiError::internal("Failed to list simulations")
    })?;
    Ok(Json(json!({ "simulations": simulations, "count": simulations.len() })))
}

/// GET /api/v1/infrastructure/simulations/:id
pub async fn get(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let simulation = InfrastructureSimulationRepo::get(&state.pool, auth.tenant_id, id)
        .await
        .map_err(|error| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Simulation lookup failed");
            ApiError::internal("Failed to load simulation")
        })?
        .ok_or_else(|| ApiError::not_found("Simulation not found"))?;
    Ok(Json(json!({
        "simulation": simulation,
        "inventory_class": "simulated",
        "execution_started": false
    })))
}
