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
    if instance.storage_topology.as_ref().is_some_and(|topology| topology.links.len() > 20_000) {
        issues.push("A storage topology may contain at most 20000 links".into());
    }
    if instance.cxl_topology.as_ref().is_some_and(|topology| topology.devices.len() > 10_000 || topology.links.len() > 20_000) {
        issues.push("A CXL topology may contain at most 10000 memory devices and 20000 links".into());
    }
    if instance.rack_power_cooling.as_ref().is_some_and(|topology| topology.racks.len() > 10_000
        || topology.racks.iter().any(|rack| rack.host_identities.len() > 256
            || rack.pdu_identities.len() > 8 || rack.ups_identities.len() > 16
            || rack.placements.len() > 256)) {
        issues.push("Rack power/cooling topology exceeds rack, host, PDU, UPS or placement limits".into());
    }
    if instance.geometry.len() > 10_000 || instance.poses.len() > 10_000 || instance.bounds.len() > 10_000 {
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
        "storage_link_count": instance.storage_topology.as_ref().map_or(0, |topology| topology.links.len()),
        "cxl_memory_device_count": instance.cxl_topology.as_ref().map_or(0, |topology| topology.devices.len()),
        "cxl_link_count": instance.cxl_topology.as_ref().map_or(0, |topology| topology.links.len()),
        "rack_count": instance.rack_power_cooling.as_ref().map_or(0, |topology| topology.racks.len()),
        "pdu_count": instance.rack_power_cooling.as_ref().map_or(0, |topology| topology.racks.iter().map(|rack| rack.pdu_identities.len()).sum::<usize>()),
        "ups_count": instance.rack_power_cooling.as_ref().map_or(0, |topology| topology.racks.iter().map(|rack| rack.ups_identities.len()).sum::<usize>()),
        "rack_unit_placement_count": instance.rack_power_cooling.as_ref().map_or(0, |topology| topology.racks.iter().map(|rack| rack.placements.len()).sum::<usize>()),
        "spatial_pose_count": instance.poses.len(),
        "spatial_bounds_count": instance.bounds.len(),
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


/// GET /api/v1/infrastructure/simulations/:id/scene
/// Returns a renderer-neutral scene projection derived from the stored resource graph.
pub async fn get_scene(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = InfrastructureSimulationRepo::get(&state.pool, auth.tenant_id, id)
        .await
        .map_err(|error| {
            tracing::error!(tenant_id = %auth.tenant_id, error = %error, "Simulation scene lookup failed");
            ApiError::internal("Failed to load simulation scene")
        })?
        .ok_or_else(|| ApiError::not_found("Simulation not found"))?;

    let instance: SimulationInstance = serde_json::from_value(row.document)
        .map_err(|error| {
            tracing::error!(simulation_id = %id, error = %error, "Stored simulation document is invalid");
            ApiError::internal("Stored simulation document cannot be projected")
        })?;

    Ok(Json(project_simulation_scene(id, &instance)))
}

/// Build a stable, renderer-neutral scene document. Geometry references are
/// optional; consumers should render bounds as primitive placeholders instead.
fn project_simulation_scene(id: Uuid, instance: &SimulationInstance) -> Value {
    let nodes: Vec<Value> = instance.graph.resources.iter().map(|resource| {
        let identity = &resource.identity;
        let geometry = instance.geometry.get(identity);
        let pose = instance.poses.get(identity);
        let bounds = instance.bounds.get(identity);
        json!({
            "id": identity,
            "kind": resource.kind,
            "attributes": resource.attributes,
            "provenance": {
                "source": resource.source,
                "observed_at": resource.observed_at,
                "verified": resource.verified,
                "inventory_class": "simulated"
            },
            "transform": pose,
            "bounds_mm": bounds,
            "geometry": geometry,
            "render_hint": if geometry.is_some() { "asset" } else if bounds.is_some() { "bounds_primitive" } else { "unresolved" }
        })
    }).collect();

    let edges: Vec<Value> = instance.graph.relationships.iter().map(|edge| json!({
        "id": edge.identity,
        "source": edge.source_identity,
        "target": edge.target_identity,
        "kind": edge.kind,
        "attributes": edge.attributes,
        "provenance": {
            "source": edge.source,
            "observed_at": edge.observed_at,
            "verified": edge.verified,
            "inventory_class": "simulated"
        }
    })).collect();

    let overlays = json!({
        "pcie": instance.pcie_topology,
        "gpu_peer_fabric": instance.gpu_peer_fabric,
        "network": instance.network_fabric,
        "storage": instance.storage_topology,
        "cxl": instance.cxl_topology,
        "rack_power_cooling": instance.rack_power_cooling
    });

    json!({
        "scene_id": format!("simulation:{id}"),
        "simulation_id": id,
        "identity": instance.identity,
        "scenario_id": instance.scenario_id,
        "simulation_kind": instance.kind,
        "engine": { "name": instance.engine, "version": instance.engine_version },
        "coordinate_units": "mm",
        "coordinate_convention": "right_handed",
        "inventory_class": "simulated",
        "execution_started": false,
        "coverage_complete": instance.graph.coverage_complete,
        "nodes": nodes,
        "edges": edges,
        "topology_overlays": overlays,
        "scene_metadata": {
            "node_count": nodes.len(),
            "edge_count": edges.len(),
            "asset_node_count": instance.geometry.len(),
            "bounds_node_count": instance.bounds.len(),
            "transformed_node_count": instance.poses.len(),
            "primitive_fallback_count": nodes.iter().filter(|node| node["render_hint"] == "bounds_primitive").count(),
            "unresolved_geometry_count": nodes.iter().filter(|node| node["render_hint"] == "unresolved").count()
        },
        "fidelity": {
            "geometry": "asset_references_when_available_otherwise_bounds_primitives",
            "power": "declared_design_envelope_not_live_telemetry",
            "thermal": "declared_capacity_only_no_cfd",
            "performance": "topology_only_no_measured_or_predicted_throughput"
        }
    })
}

#[cfg(test)]
mod scene_projection_tests {
    use super::*;
    use chrono::Utc;
    use ef_core::infrastructure::{DiscoveredResource, InfrastructureGraph, SimulationKind};
    use std::collections::BTreeMap;

    #[test]
    fn scene_projection_uses_graph_and_explicit_primitive_fallbacks() {
        let instance = SimulationInstance {
            identity: "scene-test".into(),
            kind: SimulationKind::SpatialVisualization,
            scenario_id: "rack-a".into(),
            engine: "test".into(),
            engine_version: "1".into(),
            seed: 7,
            created_at: Utc::now(),
            source_manifest_revision: None,
            graph: InfrastructureGraph {
                observed_at: Utc::now(),
                coverage_complete: true,
                resources: vec![DiscoveredResource {
                    identity: "rack-1".into(),
                    kind: "rack".into(),
                    attributes: BTreeMap::new(),
                    source: "test".into(),
                    observed_at: Utc::now(),
                    verified: false,
                }],
                relationships: vec![],
            },
            pcie_topology: None,
            gpu_peer_fabric: None,
            network_fabric: None,
            storage_topology: None,
            cxl_topology: None,
            rack_power_cooling: None,
            geometry: BTreeMap::new(),
            poses: BTreeMap::new(),
            bounds: BTreeMap::new(),
            parameters: BTreeMap::new(),
        };
        let scene = project_simulation_scene(Uuid::nil(), &instance);
        assert_eq!(scene["nodes"][0]["id"], "rack-1");
        assert_eq!(scene["nodes"][0]["render_hint"], "unresolved");
        assert_eq!(scene["inventory_class"], "simulated");
        assert_eq!(scene["execution_started"], false);
        assert_eq!(scene["scene_metadata"]["node_count"], 1);
    }
}
