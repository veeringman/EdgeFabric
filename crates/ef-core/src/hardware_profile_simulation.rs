//! Deterministic structural simulation expansion from immutable hardware profiles.
//! Only declared component quantities and containment are materialized. Unknown
//! geometry, cabling and connectivity are deliberately not synthesized.
use crate::hardware_profiles::HardwareProfile;
use crate::infrastructure::{
    DiscoveredResource, InfrastructureGraph, ResourceRelationship, SimulationInstance,
    SimulationKind, validate_graph, validate_simulation,
};
use chrono::Utc;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

pub fn instantiate_hardware_profile(
    profile: &HardwareProfile,
    identity: &str,
    scenario_id: &str,
    seed: u64,
) -> Result<SimulationInstance, Vec<String>> {
    let mut issues = profile.validate();
    if identity.trim().is_empty() || identity.len() > 200 {
        issues.push("Simulation identity must contain 1 to 200 characters".into());
    }
    if scenario_id.trim().is_empty() || scenario_id.len() > 200 {
        issues.push("Scenario ID must contain 1 to 200 characters".into());
    }
    if profile.components.is_empty() {
        issues.push("Hardware profile has no component inventory to instantiate".into());
    }
    let mut component_ids = std::collections::HashSet::new();
    for component in &profile.components {
        if !component_ids.insert(component.id.as_str()) {
            issues.push(format!("Duplicate profile component ID '{}'", component.id));
        }
        if let Some(parent) = &component.parent_component_id {
            if parent == &component.id {
                issues.push(format!("Component '{}' cannot contain itself", component.id));
            } else if !profile.components.iter().any(|candidate| &candidate.id == parent) {
                issues.push(format!("Component '{}' references missing parent '{}'", component.id, parent));
            }
        }
    }
    let expanded_count = profile.components.iter().fold(0usize, |total, component| {
        total.saturating_add(usize::try_from(component.quantity.unwrap_or(1)).unwrap_or(10_001))
    });
    if expanded_count > 9_999 {
        issues.push("Profile expansion exceeds the 10000-resource simulation limit".into());
    }
    if !issues.is_empty() { return Err(issues); }

    let now = Utc::now();
    let root_identity = format!("profile:{}/v{}/system", profile.id, profile.version);
    let mut resources = vec![DiscoveredResource {
        identity: root_identity.clone(),
        kind: "hardware_profile_system".into(),
        attributes: BTreeMap::from([
            ("name".into(), Value::String(profile.display_name.clone())),
            ("profile_id".into(), Value::String(profile.id.clone())),
            ("profile_version".into(), json!(profile.version)),
            ("vendor".into(), json!(profile.vendor)),
            ("qualification".into(), json!(profile.qualification)),
            ("profile_facts".into(), profile.facts.clone()),
            ("profile_sources".into(), json!(profile.sources)),
            ("supported_components".into(), json!(profile.supported_components)),
            ("geometry_assets".into(), json!(profile.assets.iter().map(|asset| json!({
                "format": asset.format,
                "uri": asset.uri,
                "sha256": asset.sha256,
                "license": asset.license,
                "coordinate_system": asset.coordinate_system,
                "units": asset.units
            })).collect::<Vec<_>>())),
            ("inventory_class".into(), json!("simulated")),
        ]),
        source: "hardware_profile".into(),
        observed_at: now,
        verified: false,
    }];
    let mut relationships = Vec::new();
    let mut geometry = BTreeMap::new();
    if let Some(asset) = profile.assets.iter().find(|asset| asset.units.as_ref().is_some_and(|units| !units.trim().is_empty())) {
        geometry.insert(root_identity.clone(), crate::infrastructure::GeometryAsset {
            asset_uri: asset.uri.clone(),
            sha256: asset.sha256.clone(),
            format: asset.format.clone(),
            units: asset.units.clone().unwrap_or_default(),
        });
    }
    let mut instances_by_component: HashMap<String, Vec<String>> = HashMap::new();
    let mut pending = profile.components.iter().collect::<Vec<_>>();
    while !pending.is_empty() {
        let Some(position) = pending.iter().position(|component| {
            component.parent_component_id.as_ref()
                .is_none_or(|parent| instances_by_component.contains_key(parent))
        }) else {
            return Err(vec!["Profile component containment contains a cycle".into()]);
        };
        let component = pending.remove(position);
        let parent_instances = component.parent_component_id.as_ref()
            .and_then(|parent| instances_by_component.get(parent).cloned())
            .unwrap_or_else(|| vec![root_identity.clone()]);
        let known_quantity = component.quantity.is_some();
        let count = component.quantity.unwrap_or(1) as usize;
        let mut created = Vec::with_capacity(count);
        for index in 0..count {
            let resource_identity = if known_quantity {
                format!("{root_identity}/component/{}/{}", component.id, index + 1)
            } else {
                format!("{root_identity}/component/{}/unspecified", component.id)
            };
            let geometry_assets = component.assets.iter().map(|asset| json!({
                "format": asset.format,
                "uri": asset.uri,
                "sha256": asset.sha256,
                "license": asset.license,
                "coordinate_system": asset.coordinate_system,
                "units": asset.units
            })).collect::<Vec<_>>();
            let mut attributes = BTreeMap::from([
                ("name".into(), Value::String(component.model.clone().unwrap_or_else(|| component.id.clone()))),
                ("geometry_assets".into(), json!(geometry_assets)),
                ("profile_component_id".into(), Value::String(component.id.clone())),
                ("profile_component_index".into(), json!(index + 1)),
                ("quantity_known".into(), Value::Bool(known_quantity)),
                ("facts".into(), component.facts.clone()),
                ("component_sources".into(), json!(component.sources)),
                ("physical_geometry_available".into(), Value::Bool(false)),
            ]);
            if let Some(quantity) = component.quantity {
                attributes.insert("profile_component_quantity".into(), json!(quantity));
            }
            if let Some(vendor) = &component.vendor {
                attributes.insert("vendor".into(), Value::String(vendor.clone()));
            }
            if let Some(model) = &component.model {
                attributes.insert("model".into(), Value::String(model.clone()));
            }
            if let Some(parent) = &component.parent_component_id {
                attributes.insert("parent_component_id".into(), Value::String(parent.clone()));
            }
            if let Some(asset) = component.assets.iter().find(|asset| asset.units.as_ref().is_some_and(|units| !units.trim().is_empty())) {
                geometry.insert(resource_identity.clone(), crate::infrastructure::GeometryAsset {
                    asset_uri: asset.uri.clone(),
                    sha256: asset.sha256.clone(),
                    format: asset.format.clone(),
                    units: asset.units.clone().unwrap_or_default(),
                });
            }
            resources.push(DiscoveredResource {
                identity: resource_identity.clone(),
                kind: component.class.clone(),
                attributes,
                source: "hardware_profile".into(),
                observed_at: now,
                verified: false,
            });
            let parent_identity = &parent_instances[index % parent_instances.len()];
            relationships.push(ResourceRelationship {
                identity: format!("contains:{parent_identity}:{resource_identity}"),
                source_identity: parent_identity.clone(),
                target_identity: resource_identity.clone(),
                kind: "contains".into(),
                attributes: BTreeMap::from([
                    ("declared_by_profile_component".into(), Value::String(component.id.clone())),
                ]),
                source: "hardware_profile".into(),
                observed_at: now,
                verified: false,
            });
            created.push(resource_identity);
        }
        instances_by_component.insert(component.id.clone(), created);
    }
    let graph = InfrastructureGraph {
        observed_at: now,
        // This is complete only for the profile's declared component inventory,
        // not for every physical device, connection or facility dependency.
        coverage_complete: false,
        resources,
        relationships,
    };
    let instance = SimulationInstance {
        identity: identity.into(),
        kind: SimulationKind::TopologySimulation,
        scenario_id: scenario_id.into(),
        engine: "edgefabric.hardware_profile_template".into(),
        engine_version: "1.0.0".into(),
        seed,
        created_at: now,
        source_manifest_revision: None,
        graph,
        pcie_topology: None,
        gpu_peer_fabric: None,
        network_fabric: None,
        storage_topology: None,
        cxl_topology: None,
        rack_power_cooling: None,
        geometry,
        poses: BTreeMap::new(),
        bounds: BTreeMap::new(),
        parameters: BTreeMap::from([
            ("hardware_profile".into(), json!({
                "profile_id": profile.id,
                "version": profile.version,
                "qualification": profile.qualification,
                "source_count": profile.sources.len(),
                "component_count": profile.components.len()
            })),
            ("structural_model_only".into(), Value::Bool(true)),
            ("geometry_and_cabling_inferred".into(), Value::Bool(false)),
            ("physical_inventory_mutated".into(), Value::Bool(false)),
        ]),
    };
    let mut validation = validate_simulation(&instance);
    validation.extend(validate_graph(&instance.graph).into_iter().map(|issue| {
        format!("{}: {}", issue.relationship_identity, issue.detail)
    }));
    if validation.is_empty() { Ok(instance) } else { Err(validation) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_profiles::builtin_hardware_profiles;

    #[test]
    fn rubin_nvl72_expands_declared_trays_and_components() {
        let profile = builtin_hardware_profiles().into_iter()
            .find(|profile| profile.id == "nvidia.vera-rubin-nvl72").unwrap();
        let instance = instantiate_hardware_profile(&profile, "rubin-nvl72", "qualification", 7).unwrap();
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "gpu").count(), 72);
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "compute_tray").count(), 18);
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "nvlink_switch_tray").count(), 9);
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "network_interface").count(), 144);
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "dpu").count(), 18);
        let first_tray = "profile:nvidia.vera-rubin-nvl72/v1/system/component/compute-tray/1";
        assert_eq!(instance.graph.relationships.iter().filter(|edge| {
            edge.source_identity == first_tray && edge.target_identity.contains("/component/rubin-gpu/")
        }).count(), 4);
        assert_eq!(instance.graph.relationships.iter().filter(|edge| {
            edge.source_identity == first_tray && edge.target_identity.contains("/component/connectx9-supernic/")
        }).count(), 8);
        assert!(!instance.graph.coverage_complete);
        assert!(instance.geometry.is_empty());
    }

    #[test]
    fn component_geometry_assets_are_preserved_as_references() {
        let mut profile = builtin_hardware_profiles().into_iter()
            .find(|profile| profile.id == "nvidia.vera-rubin-nvl72").unwrap();
        profile.components[0].assets.push(crate::hardware_profiles::HardwareProfileAsset {
            format: "glb".into(),
            uri: "artifact://tenant-assets/rubin-gpu.glb".into(),
            sha256: "a".repeat(64),
            license: "licensed-for-internal-visualization".into(),
            coordinate_system: Some("right_handed".into()),
            units: Some("mm".into()),
        });
        let instance = instantiate_hardware_profile(&profile, "rubin-assets", "qualification", 11).unwrap();
        let gpu = instance.graph.resources.iter()
            .find(|resource| resource.kind == "gpu").unwrap();
        assert_eq!(gpu.attributes["geometry_assets"][0]["format"], "glb");
        assert_eq!(instance.geometry.len(), 72);
        let geometry = instance.geometry.values().next().unwrap();
        assert_eq!(geometry.format, "glb");
        assert_eq!(geometry.units, "mm");
    }

    #[test]
    fn ultra_nvl576_expands_eight_racks_and_576_gpus() {
        let profile = builtin_hardware_profiles().into_iter()
            .find(|profile| profile.id == "nvidia.vera-rubin-ultra-nvl576").unwrap();
        let instance = instantiate_hardware_profile(&profile, "rubin-ultra", "qualification", 9).unwrap();
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "gpu").count(), 576);
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "rack").count(), 8);
        let first_rack = "profile:nvidia.vera-rubin-ultra-nvl576/v1/system/component/mgx-nvl-rack/1";
        assert_eq!(instance.graph.relationships.iter().filter(|edge| {
            edge.source_identity == first_rack && edge.target_identity.contains("/component/rubin-ultra-gpu/")
        }).count(), 72);
    }
}
