//! Provenance-aware infrastructure inventory primitives and read-only reconciliation.
//!
//! This module deliberately does not perform discovery or mutate infrastructure.
//! Callers adapt existing EdgeFabric discovery/provider records into these types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// A declaration for one expected infrastructure resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceDeclaration {
    /// Stable manifest-local identity, not an IP address or mutable display name.
    pub identity: String,
    pub kind: String,
    #[serde(default)]
    pub expected: BTreeMap<String, Value>,
}

/// A normalized resource reported by an existing discovery/provider source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredResource {
    pub identity: String,
    pub kind: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    /// Identifier of the EdgeFabric discovery/provider source.
    pub source: String,
    pub observed_at: DateTime<Utc>,
    #[serde(default)]
    pub verified: bool,
}

/// One bounded discovery snapshot. Incomplete coverage must never imply absence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoverySnapshot {
    pub observed_at: DateTime<Utc>,
    pub coverage_complete: bool,
    pub resources: Vec<DiscoveredResource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferenceKind {
    Matched,
    Missing,
    Unexpected,
    AttributeDrift,
    PartiallyDiscovered,
    IdentityConflict,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationDifference {
    pub kind: DifferenceKind,
    pub identity: String,
    pub resource_kind: String,
    pub fields: Vec<String>,
    pub declared: Option<ResourceDeclaration>,
    pub discovered: Vec<DiscoveredResource>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationResult {
    pub snapshot_at: DateTime<Utc>,
    pub coverage_complete: bool,
    pub declared_count: usize,
    pub discovered_count: usize,
    pub matched_count: usize,
    pub differences: Vec<ReconciliationDifference>,
}

/// Compare declarations with a single normalized snapshot.
///
/// Identity collisions are surfaced rather than silently merged. Missing
/// resources are only reported when the discovery snapshot declares complete
/// coverage. The function is deterministic and read-only.
pub fn reconcile(
    declarations: &[ResourceDeclaration],
    snapshot: &DiscoverySnapshot,
) -> ReconciliationResult {
    let mut by_identity: HashMap<&str, Vec<&DiscoveredResource>> = HashMap::new();
    for resource in &snapshot.resources {
        by_identity.entry(&resource.identity).or_default().push(resource);
    }

    let mut declared_ids: HashSet<&str> = HashSet::new();
    let mut differences = Vec::new();
    let mut matched_count = 0;

    for declaration in declarations {
        if !declared_ids.insert(&declaration.identity) {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::IdentityConflict,
                identity: declaration.identity.clone(),
                resource_kind: declaration.kind.clone(),
                fields: Vec::new(),
                declared: Some(declaration.clone()),
                discovered: by_identity
                    .get(declaration.identity.as_str())
                    .into_iter()
                    .flatten()
                    .map(|r| (*r).clone())
                    .collect(),
                detail: "The manifest declares the same identity more than once".into(),
            });
            continue;
        }

        let found = by_identity
            .get(declaration.identity.as_str())
            .cloned()
            .unwrap_or_default();

        if found.len() > 1 {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::IdentityConflict,
                identity: declaration.identity.clone(),
                resource_kind: declaration.kind.clone(),
                fields: Vec::new(),
                declared: Some(declaration.clone()),
                discovered: found.iter().map(|r| (*r).clone()).collect(),
                detail: "Multiple discovered resources claim the same identity".into(),
            });
            continue;
        }

        let Some(actual) = found.first() else {
            if snapshot.coverage_complete {
                differences.push(ReconciliationDifference {
                    kind: DifferenceKind::Missing,
                    identity: declaration.identity.clone(),
                    resource_kind: declaration.kind.clone(),
                    fields: Vec::new(),
                    declared: Some(declaration.clone()),
                    discovered: Vec::new(),
                    detail: "Declared resource was not present in a complete discovery snapshot".into(),
                });
            } else {
                differences.push(ReconciliationDifference {
                    kind: DifferenceKind::PartiallyDiscovered,
                    identity: declaration.identity.clone(),
                    resource_kind: declaration.kind.clone(),
                    fields: Vec::new(),
                    declared: Some(declaration.clone()),
                    discovered: Vec::new(),
                    detail: "Discovery coverage is incomplete; absence is not conclusive".into(),
                });
            }
            continue;
        };

        if actual.kind != declaration.kind {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::AttributeDrift,
                identity: declaration.identity.clone(),
                resource_kind: declaration.kind.clone(),
                fields: vec!["kind".into()],
                declared: Some(declaration.clone()),
                discovered: vec![(*actual).clone()],
                detail: format!("Declared kind '{}' differs from discovered kind '{}'", declaration.kind, actual.kind),
            });
            continue;
        }

        let mut drift = Vec::new();
        let mut unavailable = Vec::new();
        for (field, expected) in &declaration.expected {
            match actual.attributes.get(field) {
                Some(value) if value != expected => drift.push(field.clone()),
                Some(_) => {}
                None => unavailable.push(field.clone()),
            }
        }

        if !drift.is_empty() {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::AttributeDrift,
                identity: declaration.identity.clone(),
                resource_kind: declaration.kind.clone(),
                fields: drift,
                declared: Some(declaration.clone()),
                discovered: vec![(*actual).clone()],
                detail: "One or more comparable discovered attributes differ from declared values".into(),
            });
        } else if !actual.verified {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::Unverified,
                identity: declaration.identity.clone(),
                resource_kind: declaration.kind.clone(),
                fields: unavailable,
                declared: Some(declaration.clone()),
                discovered: vec![(*actual).clone()],
                detail: "Resource identity or discovery evidence is not verified".into(),
            });
        } else if !unavailable.is_empty() {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::PartiallyDiscovered,
                identity: declaration.identity.clone(),
                resource_kind: declaration.kind.clone(),
                fields: unavailable,
                declared: Some(declaration.clone()),
                discovered: vec![(*actual).clone()],
                detail: "Some declared attributes are not available from this discovery source".into(),
            });
        } else {
            matched_count += 1;
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::Matched,
                identity: declaration.identity.clone(),
                resource_kind: declaration.kind.clone(),
                fields: Vec::new(),
                declared: Some(declaration.clone()),
                discovered: vec![(*actual).clone()],
                detail: "Declared and discovered resource attributes agree".into(),
            });
        }
    }

    for (identity, resources) in &by_identity {
        if declarations.iter().any(|d| d.identity == *identity) {
            continue;
        }

        if resources.len() > 1 {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::IdentityConflict,
                identity: (*identity).to_string(),
                resource_kind: resources[0].kind.clone(),
                fields: Vec::new(),
                declared: None,
                discovered: resources.iter().map(|resource| (*resource).clone()).collect(),
                detail: "Multiple discovered resources claim the same identity".into(),
            });
        } else if let Some(resource) = resources.first() {
            differences.push(ReconciliationDifference {
                kind: DifferenceKind::Unexpected,
                identity: resource.identity.clone(),
                resource_kind: resource.kind.clone(),
                fields: Vec::new(),
                declared: None,
                discovered: vec![(*resource).clone()],
                detail: "Discovered resource has no matching declaration".into(),
            });
        }
    }

    ReconciliationResult {
        snapshot_at: snapshot.observed_at,
        coverage_complete: snapshot.coverage_complete,
        declared_count: declarations.len(),
        discovered_count: snapshot.resources.len(),
        matched_count,
        differences,
    }
}

/// Validate manifest declarations before persistence or reconciliation.
/// Returns all structural issues rather than failing at the first one.
pub fn validate_declarations(declarations: &[ResourceDeclaration]) -> Vec<String> {
    let mut issues = Vec::new();
    if declarations.len() > 10_000 {
        issues.push("A manifest may contain at most 10000 declarations".into());
    }

    let mut identities = HashSet::new();
    for (index, declaration) in declarations.iter().enumerate() {
        let identity = declaration.identity.trim();
        let kind = declaration.kind.trim();
        if identity.is_empty() {
            issues.push(format!("declarations[{index}].identity must not be empty"));
        } else {
            if declaration.identity != identity {
                issues.push(format!("declarations[{index}].identity must not have leading or trailing whitespace"));
            }
            if !identities.insert(identity.to_owned()) {
                issues.push(format!("declarations[{index}] duplicates identity '{identity}'"));
            }
        }
        if kind.is_empty() {
            issues.push(format!("declarations[{index}].kind must not be empty"));
        } else if declaration.kind != kind {
            issues.push(format!("declarations[{index}].kind must not have leading or trailing whitespace"));
        }
        for field in declaration.expected.keys() {
            if field.trim().is_empty() {
                issues.push(format!("declarations[{index}] contains an empty expected attribute name"));
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn declaration(identity: &str) -> ResourceDeclaration {
        ResourceDeclaration {
            identity: identity.into(),
            kind: "host".into(),
            expected: BTreeMap::from([
                ("architecture".into(), json!("aarch64")),
                ("cpu_cores".into(), json!(10)),
            ]),
        }
    }

    fn discovered(identity: &str) -> DiscoveredResource {
        DiscoveredResource {
            identity: identity.into(),
            kind: "host".into(),
            attributes: BTreeMap::from([
                ("architecture".into(), json!("aarch64")),
                ("cpu_cores".into(), json!(10)),
            ]),
            source: "edgefabric.runner_probe".into(),
            observed_at: Utc::now(),
            verified: true,
        }
    }

    fn snapshot(resources: Vec<DiscoveredResource>, coverage_complete: bool) -> DiscoverySnapshot {
        DiscoverySnapshot { observed_at: Utc::now(), coverage_complete, resources }
    }

    #[test]
    fn declaration_validation_rejects_empty_and_duplicate_identity() {
        let declarations = vec![
            ResourceDeclaration { identity: " ".into(), kind: "host".into(), expected: BTreeMap::new() },
            ResourceDeclaration { identity: "node-1".into(), kind: "host".into(), expected: BTreeMap::new() },
            ResourceDeclaration { identity: "node-1".into(), kind: " ".into(), expected: BTreeMap::new() },
        ];
        let issues = validate_declarations(&declarations);
        assert!(issues.iter().any(|issue| issue.contains("identity must not be empty")));
        assert!(issues.iter().any(|issue| issue.contains("duplicates identity")));
        assert!(issues.iter().any(|issue| issue.contains("kind must not be empty")));
    }

    #[test]
    fn identical_verified_resource_matches() {
        let result = reconcile(&[declaration("host-1")], &snapshot(vec![discovered("host-1")], true));
        assert_eq!(result.matched_count, 1);
        assert_eq!(result.differences[0].kind, DifferenceKind::Matched);
    }

    #[test]
    fn drift_is_reported_by_field() {
        let mut actual = discovered("host-1");
        actual.attributes.insert("cpu_cores".into(), json!(8));
        let result = reconcile(&[declaration("host-1")], &snapshot(vec![actual], true));
        assert_eq!(result.differences[0].kind, DifferenceKind::AttributeDrift);
        assert_eq!(result.differences[0].fields, vec!["cpu_cores"]);
    }

    #[test]
    fn incomplete_coverage_does_not_claim_missing() {
        let result = reconcile(&[declaration("host-1")], &snapshot(vec![], false));
        assert_eq!(result.differences[0].kind, DifferenceKind::PartiallyDiscovered);
    }

    #[test]
    fn complete_coverage_reports_missing() {
        let result = reconcile(&[declaration("host-1")], &snapshot(vec![], true));
        assert_eq!(result.differences[0].kind, DifferenceKind::Missing);
    }

    #[test]
    fn duplicate_discovery_identity_is_a_conflict() {
        let result = reconcile(
            &[declaration("host-1")],
            &snapshot(vec![discovered("host-1"), discovered("host-1")], true),
        );
        assert_eq!(result.differences[0].kind, DifferenceKind::IdentityConflict);
    }

    #[test]
    fn duplicate_undeclared_discovery_identity_is_one_conflict() {
        let result = reconcile(
            &[],
            &snapshot(vec![discovered("host-2"), discovered("host-2")], true),
        );
        assert_eq!(result.differences.len(), 1);
        assert_eq!(result.differences[0].kind, DifferenceKind::IdentityConflict);
        assert_eq!(result.differences[0].discovered.len(), 2);
    }

    #[test]
    fn undeclared_resource_is_unexpected() {
        let result = reconcile(&[], &snapshot(vec![discovered("host-2")], true));
        assert_eq!(result.differences[0].kind, DifferenceKind::Unexpected);
    }

    #[test]
    fn unavailable_attribute_is_not_called_drift() {
        let mut actual = discovered("host-1");
        actual.attributes.remove("cpu_cores");
        let result = reconcile(&[declaration("host-1")], &snapshot(vec![actual], true));
        assert_eq!(result.differences[0].kind, DifferenceKind::PartiallyDiscovered);
        assert_eq!(result.differences[0].fields, vec!["cpu_cores"]);
    }
}
