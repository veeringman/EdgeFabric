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
    let expanded_count = profile.components.iter()
        .map(|component| component.quantity.unwrap_or(1) as usize)
        .sum::<usize>();
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
            ("inventory_class".into(), json!("simulated")),
        ]),
        source: "hardware_profile".into(),
        observed_at: now,
        verified: false,
    }];
    let mut relationships = Vec::new();
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
            let mut attributes = BTreeMap::from([
                ("name".into(), Value::String(component.model.clone().unwrap_or_else(|| component.id.clone()))),
                ("profile_component_id".into(), Value::String(component.id.clone())),
                ("profile_component_index".into(), json!(index + 1)),
                ("quantity_known".into(), Value::Bool(known_quantity)),
                ("facts".into(), component.facts.clone()),
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
        geometry: BTreeMap::new(),
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
        assert!(!instance.graph.coverage_complete);
        assert!(instance.geometry.is_empty());
    }

    #[test]
    fn ultra_nvl576_expands_eight_racks_and_576_gpus() {
        let profile = builtin_hardware_profiles().into_iter()
            .find(|profile| profile.id == "nvidia.vera-rubin-ultra-nvl576").unwrap();
        let instance = instantiate_hardware_profile(&profile, "rubin-ultra", "qualification", 9).unwrap();
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "gpu").count(), 576);
        assert_eq!(instance.graph.resources.iter().filter(|resource| resource.kind == "rack").count(), 8);
    }
}
