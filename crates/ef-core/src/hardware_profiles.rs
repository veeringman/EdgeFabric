//! Versioned hardware-profile metadata and qualification boundaries.
//! Unknown facts remain unknown; vendor specifications and geometry are never inferred.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareProfileQualification { Draft, TemplateOnly, VendorDocumented, VendorValidated, FieldVerified }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareProfileSource {
    pub title: String,
    pub uri: String,
    pub publisher: String,
    pub revision: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareProfileAsset {
    pub format: String,
    pub uri: String,
    pub sha256: String,
    pub license: String,
    pub coordinate_system: Option<String>,
    pub units: Option<String>,
}

fn default_component_facts() -> serde_json::Value {
    serde_json::json!({})
}

/// A first-class component entry inside a system or rack profile.
/// Quantity is omitted when the authoritative source does not specify it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareProfileComponent {
    pub id: String,
    pub class: String,
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub quantity: Option<u64>,
    /// Parent component class ID; absent means directly contained by the profile root.
    #[serde(default)]
    pub parent_component_id: Option<String>,
    #[serde(default = "default_component_facts")]
    pub facts: serde_json::Value,
    #[serde(default)]
    pub sources: Vec<HardwareProfileSource>,
    #[serde(default)]
    pub assets: Vec<HardwareProfileAsset>,
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub id: String,
    pub version: u32,
    pub vendor: Option<String>,
    pub product_family: Option<String>,
    pub model: Option<String>,
    pub display_name: String,
    pub qualification: HardwareProfileQualification,
    /// Machine-readable sourced facts; unknown values are omitted.
    pub facts: serde_json::Value,
    #[serde(default)]
    pub supported_components: Vec<String>,
    #[serde(default)]
    pub components: Vec<HardwareProfileComponent>,
    #[serde(default)]
    pub assets: Vec<HardwareProfileAsset>,
    #[serde(default)]
    pub sources: Vec<HardwareProfileSource>,
    pub notes: Vec<String>,
}

impl HardwareProfile {
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.id.trim().is_empty() || self.id.len() > 160 { issues.push("Profile ID must contain 1 to 160 characters".into()); }
        if self.version == 0 { issues.push("Profile version must be a positive integer".into()); }
        if self.display_name.trim().is_empty() || self.display_name.len() > 200 { issues.push("Profile display name must contain 1 to 200 characters".into()); }
        if !self.facts.is_object() { issues.push("Profile facts must be a JSON object; omit unknown facts".into()); }
        if let Some(constraints) = self.facts.get("constraints") {
            if let Some(constraints) = constraints.as_object() {
                for (key, rule) in constraints {
                    match rule.as_object() {
                        Some(rule) if !rule.is_empty() => {
                            for (operator, expected) in rule {
                                if !matches!(operator.as_str(), "min" | "max" | "equals") || expected.as_f64().is_none() {
                                    issues.push(format!("Constraint '{key}' must use numeric min, max or equals operators"));
                                }
                            }
                        }
                        _ => issues.push(format!("Constraint '{key}' must be a non-empty object")),
                    }
                }
            } else {
                issues.push("Profile facts.constraints must be an object".into());
            }
        }
        if self.sources.len() > 100 || self.assets.len() > 100 || self.components.len() > 256 { issues.push("A profile may contain at most 100 sources, 100 assets and 256 components".into()); }
        let mut component_ids = std::collections::HashSet::new();
        for component in &self.components {
            if component.id.trim().is_empty() || component.class.trim().is_empty() {
                issues.push("Each profile component requires a stable ID and class".into());
            }
            if !component_ids.insert(component.id.as_str()) {
                issues.push(format!("Duplicate profile component ID '{}'", component.id));
            }
            if component.quantity == Some(0) {
                issues.push(format!("Component '{}' quantity must be positive when supplied", component.id));
            }
            if let Some(parent) = &component.parent_component_id {
                if parent == &component.id || !self.components.iter().any(|candidate| &candidate.id == parent) {
                    issues.push(format!("Component '{}' has an invalid parent component reference", component.id));
                }
            }
            if !component.facts.is_object() {
                issues.push(format!("Component '{}' facts must be a JSON object", component.id));
            }
        }
        let expanded_component_count = self.components.iter().try_fold(1_u64, |total, component| {
            total.checked_add(component.quantity.unwrap_or(1))
        });
        if expanded_component_count.is_none_or(|count| count > 10_000) {
            issues.push("Expanded profile component inventory may contain at most 10000 resources".into());
        }
        for component in &self.components {
            let mut visited = std::collections::HashSet::new();
            let mut current = Some(component.id.as_str());
            while let Some(component_id) = current {
                if !visited.insert(component_id) {
                    issues.push(format!("Component containment cycle includes '{}'", component.id));
                    break;
                }
                current = self.components.iter()
                    .find(|candidate| candidate.id == component_id)
                    .and_then(|candidate| candidate.parent_component_id.as_deref());
            }
        }
        for source in &self.sources {
            if source.title.trim().is_empty() || source.uri.trim().is_empty() || source.publisher.trim().is_empty() {
                issues.push("Each source requires a title, URI and publisher".into());
            }
            if source.sha256.as_ref().is_some_and(|digest| digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())) {
                issues.push("Source SHA-256 must be 64 hexadecimal characters when supplied".into());
            }
        }
        for component in &self.components {
            if component.sources.len() > 100 || component.assets.len() > 100 {
                issues.push(format!("Component '{}' may contain at most 100 sources and 100 assets", component.id));
            }
            for source in &component.sources {
                if source.title.trim().is_empty() || source.uri.trim().is_empty() || source.publisher.trim().is_empty() {
                    issues.push(format!("Component '{}' has a source without title, URI or publisher", component.id));
                }
                if source.sha256.as_ref().is_some_and(|digest| digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())) {
                    issues.push(format!("Component '{}' has an invalid source SHA-256", component.id));
                }
            }
            for asset in &component.assets {
                if asset.uri.trim().is_empty() || asset.license.trim().is_empty()
                    || asset.sha256.len() != 64
                    || !asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    issues.push(format!("Component '{}' has an invalid or unlicensed geometry asset", component.id));
                }
            }
        }
        for asset in &self.assets {
            if asset.uri.trim().is_empty() || asset.sha256.trim().is_empty() || asset.license.trim().is_empty() {
                issues.push("Each geometry asset requires a URI, SHA-256 and license".into());
            }
            if asset.sha256.len() != 64 || !asset.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                issues.push("Geometry asset SHA-256 must be 64 hexadecimal characters".into());
            }
        }
        if matches!(self.qualification, HardwareProfileQualification::VendorDocumented | HardwareProfileQualification::VendorValidated | HardwareProfileQualification::FieldVerified) && self.sources.is_empty() {
            issues.push("A qualified profile must include at least one provenance source".into());
        }
        if matches!(self.qualification, HardwareProfileQualification::VendorValidated | HardwareProfileQualification::FieldVerified) && self.assets.iter().any(|asset| asset.sha256.len() != 64) {
            issues.push("Qualified geometry assets must include a 64-character SHA-256 digest".into());
        }
        issues
    }
}

/// Explicitly generic topology template; contains no vendor or product assertions.
pub fn generic_gpu_node_profile() -> HardwareProfile {
    HardwareProfile {
        id: "edgefabric.generic-gpu-node".into(),
        version: 1,
        vendor: None,
        product_family: None,
        model: None,
        display_name: "Generic GPU node (topology template)".into(),
        qualification: HardwareProfileQualification::TemplateOnly,
        facts: serde_json::json!({}),
        supported_components: vec!["host".into(), "gpu".into(), "pcie".into(), "network".into(), "storage".into(), "rack".into(), "power_cooling".into()],
        components: Vec::new(),
        assets: Vec::new(),
        sources: Vec::new(),
        notes: vec![
            "Topology template only; not a vendor-qualified system profile.".into(),
            "No chassis dimensions, slot maps, GPU counts, power limits or cable maps are asserted.".into(),
            "Use synthetic cluster simulation until authoritative profile data is attached and reviewed.".into(),
        ],
    }
}

/// Read-only vendor-documented reference profiles built from public NVIDIA material.
/// These are architecture references, not vendor-validated system configurations.
pub fn builtin_hardware_profiles() -> Vec<HardwareProfile> {
    let source_hgx = HardwareProfileSource {
        title: "NVIDIA HGX Platform specifications".into(),
        uri: "https://www.nvidia.com/en-us/data-center/hgx/".into(),
        publisher: "NVIDIA".into(),
        revision: Some("Accessed 2026-11-21; preliminary specifications".into()),
        sha256: None,
    };
    let source_pod = HardwareProfileSource {
        title: "NVIDIA Vera Rubin POD: Seven Chips, Five Rack-Scale Systems, One AI Supercomputer".into(),
        uri: "https://developer.nvidia.com/blog/nvidia-vera-rubin-pod-seven-chips-five-rack-scale-systems-one-ai-supercomputer".into(),
        publisher: "NVIDIA Developer".into(),
        revision: Some("Published 2026-03-16".into()),
        sha256: None,
    };
    vec![
        generic_gpu_node_profile(),
        HardwareProfile {
            id: "nvidia.rubin-gpu".into(),
            version: 1,
            vendor: Some("NVIDIA".into()),
            product_family: Some("Rubin".into()),
            model: Some("Rubin GPU".into()),
            display_name: "NVIDIA Rubin GPU (public reference)".into(),
            qualification: HardwareProfileQualification::VendorDocumented,
            facts: serde_json::json!({
                "gpu": {
                    "memory_capacity_gb": 288,
                    "memory_type": "HBM4",
                    "memory_bandwidth_tb_s": 22.0,
                    "nvlink_generation": 6,
                    "nvlink_bandwidth_tb_s": 3.6,
                    "nvfp4_inference_sparse_pflops": 50.0,
                    "nvfp4_training_pflops": 35.0
                },
                "specification_status": "preliminary_subject_to_change"
            }),
            supported_components: vec!["gpu".into(), "memory".into(), "nvlink".into()],
            components: vec![HardwareProfileComponent { id: "rubin-gpu".into(), class: "gpu".into(), vendor: Some("NVIDIA".into()), model: Some("Rubin GPU".into()), quantity: Some(1), parent_component_id: None, facts: serde_json::json!({"memory_capacity_gb":288,"memory_type":"HBM4","memory_bandwidth_tb_s":22.0,"nvlink_generation":6,"nvlink_bandwidth_tb_s":3.6}), sources: vec![source_hgx.clone()], assets: Vec::new() }],
            assets: Vec::new(),
            sources: vec![source_hgx.clone()],
            notes: vec![
                "Public NVIDIA HGX page lists 288 GB HBM4, 22 TB/s memory bandwidth and sixth-generation NVLink.".into(),
                "Performance values are source-defined peak specifications; do not treat them as measured workload results.".into(),
                "No package dimensions, board layout, power envelope, cooling requirements or physical CAD are included.".into(),
            ],
        },
        HardwareProfile {
            id: "nvidia.hgx-rubin-nvl8".into(),
            version: 1,
            vendor: Some("NVIDIA".into()),
            product_family: Some("HGX Rubin".into()),
            model: Some("HGX Rubin NVL8".into()),
            display_name: "NVIDIA HGX Rubin NVL8 (public reference)".into(),
            qualification: HardwareProfileQualification::VendorDocumented,
            facts: serde_json::json!({
                "system": {
                    "gpu_count": 8,
                    "gpu_memory_capacity_tb": 2.3,
                    "gpu_memory_bandwidth_tb_s": 176.0,
                    "nvlink_generation": 6,
                    "nvlink_switch_bandwidth_tb_s": 28.8,
                    "networking_bandwidth_tb_s": 1.6
                },
                "cpu_options": {
                    "vera_cpu": {
                        "core_count": 88,
                        "memory_capacity_tb": 1.5,
                        "memory_type": "LPDDR5X",
                        "memory_bandwidth_tb_s": 1.2
                    },
                    "x86_baseboard": "OEM_defined"
                },
                "specification_status": "preliminary_subject_to_change",
                "constraints": {
                    "gpus_per_host": { "equals": 8 }
                }
            }),
            supported_components: vec!["gpu".into(), "host".into(), "cpu".into(), "nvlink".into(), "network".into(), "memory".into()],
            components: vec![
                HardwareProfileComponent { id: "rubin-gpu".into(), class: "gpu".into(), vendor: Some("NVIDIA".into()), model: Some("Rubin SXM".into()), quantity: Some(8), parent_component_id: Some("hgx-baseboard".into()), facts: serde_json::json!({"memory_capacity_tb_system":2.3,"memory_bandwidth_tb_s_system":176.0}), sources: vec![source_hgx.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "hgx-baseboard".into(), class: "baseboard".into(), vendor: Some("NVIDIA".into()), model: Some("HGX Rubin NVL8".into()), quantity: Some(1), parent_component_id: None, facts: serde_json::json!({"form_factor":"8x Rubin SXM"}), sources: vec![source_hgx.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "host-cpu".into(), class: "cpu".into(), vendor: None, model: Some("Vera CPU or OEM x86 CPU".into()), quantity: None, parent_component_id: Some("hgx-baseboard".into()), facts: serde_json::json!({"variant":"Vera single-socket or OEM x86 baseboard"}), sources: vec![source_hgx.clone()], assets: Vec::new() }
            ],
            assets: Vec::new(),
            sources: vec![source_hgx.clone()],
            notes: vec![
                "NVIDIA documents an eight-Rubin-SXM HGX baseboard and either Vera CPU or x86 CPU baseboard variants.".into(),
                "OEM-defined CPU and memory details are intentionally not filled for x86 variants.".into(),
                "No chassis dimensions, PCIe slot map, port inventory, rack power or cooling capacity is asserted.".into(),
            ],
        },
        HardwareProfile {
            id: "nvidia.vera-rubin-nvl72".into(),
            version: 1,
            vendor: Some("NVIDIA".into()),
            product_family: Some("Vera Rubin".into()),
            model: Some("Vera Rubin NVL72".into()),
            display_name: "NVIDIA Vera Rubin NVL72 (rack reference)".into(),
            qualification: HardwareProfileQualification::VendorDocumented,
            facts: serde_json::json!({
                "rack": {
                    "gpu_count": 72,
                    "vera_cpu_count": 36,
                    "compute_tray_count": 18,
                    "nvlink_switch_tray_count": 9,
                    "nvlink_generation": 6,
                    "nvlink_bandwidth_per_gpu_tb_s": 3.6,
                    "nvlink_scale_up_bandwidth_per_rack_tb_s": 260.0,
                    "approximate_weight_lb": 4000,
                    "coolant_inlet_temperature_c": 45,
                    "cooling": "liquid_cooled"
                },
                "compute_tray": {
                    "connectx9_supernic_count": 8,
                    "bluefield4_dpu_count": 1
                },
                "power_features": {
                    "rack_energy_storage_joules_per_gpu": 400,
                    "intelligent_power_smoothing": true
                },
                "specification_status": "published_architecture_reference"
            }),
            supported_components: vec!["gpu".into(), "cpu".into(), "compute_tray".into(), "nvlink_switch_tray".into(), "rack".into(), "network".into(), "dpu".into(), "power_cooling".into()],
            components: vec![
                HardwareProfileComponent { id: "rubin-gpu".into(), class: "gpu".into(), vendor: Some("NVIDIA".into()), model: Some("Rubin GPU".into()), quantity: Some(72), parent_component_id: Some("compute-tray".into()), facts: serde_json::json!({}), sources: vec![source_pod.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "vera-cpu".into(), class: "cpu".into(), vendor: Some("NVIDIA".into()), model: Some("Vera CPU".into()), quantity: Some(36), parent_component_id: Some("compute-tray".into()), facts: serde_json::json!({}), sources: vec![source_pod.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "compute-tray".into(), class: "compute_tray".into(), vendor: Some("NVIDIA".into()), model: Some("Vera Rubin compute tray".into()), quantity: Some(18), parent_component_id: None, facts: serde_json::json!({"connectx9_supernic_count_per_tray":8,"bluefield4_dpu_count_per_tray":1}), sources: vec![source_pod.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "connectx9-supernic".into(), class: "network_interface".into(), vendor: Some("NVIDIA".into()), model: Some("ConnectX-9 SuperNIC".into()), quantity: Some(144), parent_component_id: Some("compute-tray".into()), facts: serde_json::json!({}), sources: vec![source_pod.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "bluefield4-dpu".into(), class: "dpu".into(), vendor: Some("NVIDIA".into()), model: Some("BlueField-4 DPU".into()), quantity: Some(18), parent_component_id: Some("compute-tray".into()), facts: serde_json::json!({}), sources: vec![source_pod.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "nvlink-switch-tray".into(), class: "nvlink_switch_tray".into(), vendor: Some("NVIDIA".into()), model: Some("NVLink switch tray".into()), quantity: Some(9), parent_component_id: None, facts: serde_json::json!({}), sources: vec![source_pod.clone()], assets: Vec::new() }
            ],
            assets: Vec::new(),
            sources: vec![source_pod.clone()],
            notes: vec![
                "This is a rack-level architecture reference: 18 compute trays, 9 NVLink switch trays, 72 GPUs and 36 Vera CPUs.".into(),
                "Compute trays are not generic host servers; preserve tray identity in a dedicated rack-scale model rather than mapping them to hosts.".into(),
                "Approximate weight and coolant inlet temperature are source statements, not a complete mechanical or thermal model.".into(),
                "No complete electrical input, rack dimensions, manifold flow, CDU, hose routing or service-clearance data is supplied.".into(),
            ],
        },
        HardwareProfile {
            id: "nvidia.vera-rubin-ultra-nvl576".into(),
            version: 1,
            vendor: Some("NVIDIA".into()),
            product_family: Some("Vera Rubin Ultra".into()),
            model: Some("Vera Rubin Ultra NVL576".into()),
            display_name: "NVIDIA Vera Rubin Ultra NVL576 (architecture reference)".into(),
            qualification: HardwareProfileQualification::VendorDocumented,
            facts: serde_json::json!({
                "system": {
                    "gpu_count": 576,
                    "mgx_nvl_rack_count": 8,
                    "rubin_ultra_gpus_per_rack": 72,
                    "nvlink_topology": "two_layer_all_to_all",
                    "inter_rack_links": ["copper", "direct_optical"],
                    "nvlink_domain_gpu_count": 576
                },
                "specification_status": "published_architecture_reference"
            }),
            supported_components: vec!["gpu".into(), "rack".into(), "nvlink".into(), "optical_interconnect".into(), "copper_interconnect".into()],
            components: vec![
                HardwareProfileComponent { id: "rubin-ultra-gpu".into(), class: "gpu".into(), vendor: Some("NVIDIA".into()), model: Some("Rubin Ultra GPU".into()), quantity: Some(576), parent_component_id: Some("mgx-nvl-rack".into()), facts: serde_json::json!({}), sources: vec![source_pod.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "mgx-nvl-rack".into(), class: "rack".into(), vendor: Some("NVIDIA".into()), model: Some("MGX NVL rack".into()), quantity: Some(8), parent_component_id: None, facts: serde_json::json!({"rubin_ultra_gpus_per_rack":72}), sources: vec![source_pod.clone()], assets: Vec::new() },
                HardwareProfileComponent { id: "nvlink-domain".into(), class: "nvlink_domain".into(), vendor: Some("NVIDIA".into()), model: Some("Two-layer all-to-all".into()), quantity: Some(1), parent_component_id: None, facts: serde_json::json!({"gpu_count":576,"inter_rack_media":["copper","direct_optical"]}), sources: vec![source_pod.clone()], assets: Vec::new() }
            ],
            assets: Vec::new(),
            sources: vec![source_pod],
            notes: vec![
                "NVIDIA describes eight MGX NVL racks, each with 72 Rubin Ultra GPUs, forming one 576-GPU NVLink domain.".into(),
                "The two-layer all-to-all topology uses copper and direct optical connections between racks.".into(),
                "No final GPU memory, per-GPU bandwidth, rack electrical, detailed mechanical, cable schedule or vendor CAD values are asserted here.".into(),
                "This profile is an architecture reference, not a vendor-validated qualification package.".into(),
            ],
        },
    ]
}

/// Result of evaluating one explicitly declared numeric profile constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileConstraintResult {
    pub key: String,
    pub observed: f64,
    pub operator: String,
    pub expected: f64,
    pub status: String,
}

/// Profile constraints use the optional `facts.constraints` object. Each
/// leaf is an object containing one or more numeric operators: `min`,
/// `max`, or `equals`. Only supplied observations and constraints are
/// evaluated; absent values are reported as unknown rather than inferred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareProfileEvaluation {
    pub profile_id: String,
    pub profile_version: u32,
    pub evaluated: Vec<ProfileConstraintResult>,
    pub unknown: Vec<String>,
    pub status: String,
}

pub fn evaluate_numeric_constraints(
    profile: &HardwareProfile,
    observations: &std::collections::BTreeMap<String, f64>,
) -> HardwareProfileEvaluation {
    let mut evaluated = Vec::new();
    let mut unknown = Vec::new();
    let constraints = profile.facts.get("constraints").and_then(serde_json::Value::as_object);
    for (key, observed) in observations {
        let Some(rule) = constraints.and_then(|all| all.get(key)).and_then(serde_json::Value::as_object) else {
            unknown.push(key.clone());
            continue;
        };
        let mut found = false;
        for (operator, expected) in rule {
            let Some(expected) = expected.as_f64() else { continue; };
            let passed = match operator.as_str() {
                "min" => *observed >= expected,
                "max" => *observed <= expected,
                "equals" => (*observed - expected).abs() <= f64::EPSILON,
                _ => continue,
            };
            found = true;
            evaluated.push(ProfileConstraintResult {
                key: key.clone(),
                observed: *observed,
                operator: operator.clone(),
                expected,
                status: if passed { "pass" } else { "fail" }.into(),
            });
        }
        if !found { unknown.push(key.clone()); }
    }
    let status = if evaluated.iter().any(|result| result.status == "fail") {
        "fail"
    } else if !evaluated.is_empty() && unknown.is_empty() {
        "pass"
    } else if evaluated.is_empty() {
        "unknown"
    } else {
        "partial"
    };
    HardwareProfileEvaluation {
        profile_id: profile.id.clone(),
        profile_version: profile.version,
        evaluated,
        unknown,
        status: status.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_profile_is_explicitly_unqualified_and_valid() {
        let profile = generic_gpu_node_profile();
        assert!(profile.validate().is_empty());
        assert_eq!(profile.qualification, HardwareProfileQualification::TemplateOnly);
        assert!(profile.vendor.is_none());
        assert!(profile.facts.as_object().is_some_and(|facts| facts.is_empty()));
    }
    #[test]
    fn vendor_qualification_requires_provenance() {
        let mut profile = generic_gpu_node_profile();
        profile.qualification = HardwareProfileQualification::VendorDocumented;
        assert!(profile.validate().iter().any(|issue| issue.contains("provenance")));
    }

    #[test]
    fn profile_constraints_preserve_unknown_and_report_failures() {
        let mut profile = generic_gpu_node_profile();
        profile.facts = serde_json::json!({
            "constraints": {
                "gpus_per_host": { "max": 8 },
                "pcie_generation": { "min": 6 }
            }
        });
        let observations = std::collections::BTreeMap::from([
            ("gpus_per_host".to_string(), 10.0),
            ("pcie_generation".to_string(), 6.0),
            ("rack_power_watts".to_string(), 12000.0),
        ]);
        let result = evaluate_numeric_constraints(&profile, &observations);
        assert_eq!(result.status, "fail");
        assert!(result.evaluated.iter().any(|item| item.key == "gpus_per_host" && item.status == "fail"));
        assert!(result.unknown.contains(&"rack_power_watts".to_string()));
    }

    #[test]
    fn no_profile_constraints_means_unknown_not_pass() {
        let profile = generic_gpu_node_profile();
        let observations = std::collections::BTreeMap::from([
            ("gpus_per_host".to_string(), 8.0),
        ]);
        let result = evaluate_numeric_constraints(&profile, &observations);
        assert_eq!(result.status, "unknown");
        assert_eq!(result.unknown, vec!["gpus_per_host"]);
    }

    #[test]
    fn built_in_nvidia_reference_profiles_have_provenance_and_validate() {
        let profiles = builtin_hardware_profiles();
        assert!(profiles.len() >= 5);
        for profile in profiles {
            assert!(profile.validate().is_empty(), "{}: {:?}", profile.id, profile.validate());
            if profile.vendor.as_deref() == Some("NVIDIA") {
                assert_eq!(profile.qualification, HardwareProfileQualification::VendorDocumented);
                assert!(!profile.sources.is_empty());
            }
        }
    }

    #[test]
    fn invalid_constraint_operator_is_rejected() {
        let mut profile = generic_gpu_node_profile();
        profile.facts = serde_json::json!({
            "constraints": { "gpus_per_host": { "greater_than": 8 } }
        });
        assert!(profile.validate().iter().any(|issue| issue.contains("numeric min, max or equals")));
    }
}
