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
        if self.sources.len() > 100 || self.assets.len() > 100 { issues.push("A profile may contain at most 100 sources and 100 assets".into()); }
        for source in &self.sources {
            if source.title.trim().is_empty() || source.uri.trim().is_empty() || source.publisher.trim().is_empty() {
                issues.push("Each source requires a title, URI and publisher".into());
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
        assets: Vec::new(),
        sources: Vec::new(),
        notes: vec![
            "Topology template only; not a vendor-qualified system profile.".into(),
            "No chassis dimensions, slot maps, GPU counts, power limits or cable maps are asserted.".into(),
            "Use synthetic cluster simulation until authoritative profile data is attached and reviewed.".into(),
        ],
    }
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
}
