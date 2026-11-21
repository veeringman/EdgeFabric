# Manifests and Reconciliation

## Manifest composition

Prefer composable, independently versioned documents:
- **Site manifest:** facility, zones, network boundaries and operational policy.
- **Rack manifest:** rack identity, U layout, power feeds, cooling and declared cabling.
- **Node manifest:** expected host, components, slots, identifiers and management endpoint references.
- **Platform profile:** supported hardware/firmware/driver/runtime combinations.
- **Workload profile:** workload intent, resource requirements and constraints.
- **Qualification profile:** tests, acceptance thresholds, safety class and evidence requirements.

A root manifest references immutable revisions of its components. Changes create a new revision; they do not rewrite historical intent.

## Illustrative YAML

```yaml
apiVersion: infrastructure.edgefabric.io/v1alpha1
kind: Site
metadata:
  name: lab-west
  labels:
    environment: qualification
spec:
  region: apac
  discovery:
    reuseEdgeFabric: true
    sources: [edgefabric-network-discovery, enrolled-agents, redfish]
  racks:
    - name: rack-r01
      manifestRef: rack-r01@sha256:REPLACE_WITH_DIGEST
```

```yaml
apiVersion: infrastructure.edgefabric.io/v1alpha1
kind: Rack
metadata:
  name: rack-r01
spec:
  siteRef: lab-west
  units: 48
  powerFeeds:
    - name: feed-a
      sourceRef: pdu-a
    - name: feed-b
      sourceRef: pdu-b
  nodes:
    - name: compute-r01-u10
      uPosition: 10
      manifestRef: compute-r01-u10@sha256:REPLACE_WITH_DIGEST
```

```yaml
apiVersion: infrastructure.edgefabric.io/v1alpha1
kind: Node
metadata:
  name: compute-r01-u10
spec:
  expected:
    vendor: Lenovo
    platformProfile: accelerator-platform@sha256:REPLACE_WITH_DIGEST
    accelerators:
      count: 8
      class: gpu
      vendor: nvidia
      architecture: rubin
    interconnect:
      fabric: infiniband
  management:
    provider: redfish
    endpointSecretRef: secret://tenant/site/bmc-credential
```

Examples are schema illustrations, not assertions that a given SKU or provider is supported. Secrets are references only; never embed credentials, tokens or private keys.

## Validation

Validation is offline and must not touch hardware. Check:
- API/schema version and required fields;
- reference existence and pinned revision;
- unique names/IDs within scope;
- valid resource kinds, relationship types and capability schemas;
- physical constraints (rack U positions, slot counts where known);
- selector scope and tenant ownership;
- secret reference authorization without resolving secret material;
- profile compatibility and policy constraints.

Report warnings separately from errors and preserve unknown/unverified fields.

## Reconciliation

Reconciliation takes a manifest revision, scope, discovery snapshot window, provider set and policy revision. It is read-only by default.

Canonical outcomes:
- `MATCHED`: declared and discovered identity/attributes agree.
- `MISSING`: declared resource not found within adequate discovery coverage.
- `UNEXPECTED`: discovered resource has no declaration/claim.
- `ATTRIBUTE_DRIFT`: a comparable field differs.
- `TOPOLOGY_MISMATCH`: expected relationship is absent or differs.
- `PARTIALLY_DISCOVERED`: resource found but key facts unavailable.
- `IDENTITY_CONFLICT`: identifiers ambiguously map to multiple resources.
- `STALE`: source facts exceed freshness policy.
- `UNVERIFIED`: evidence is insufficient to decide.

Do not label a resource missing when discovery coverage is incomplete or stale; report incomplete coverage.

## Difference record

Each difference includes resource/relationship, type, severity, declared and discovered values, source references, confidence, freshness, policy outcome, evidence and optional remediation proposal. Operator resolutions are attributed, time-stamped and audited.

## Plan and apply

A plan is immutable and pins manifest revision, graph revision, provider/capability versions, policy revision, target set, preconditions, ordered actions, expected impact, safety class, timeout, rollback/recovery strategy and evidence requirements.

Apply is a separate explicit operation. It requires idempotency key, target locks, precondition revalidation and approvals according to policy. Stale plans are rejected and regenerated. No automatic disruptive remediation.
