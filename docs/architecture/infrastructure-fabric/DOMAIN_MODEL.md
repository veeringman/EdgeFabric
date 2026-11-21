# Domain Model

## Core entities

| Entity | Purpose |
|---|---|
| Site | Physical or logical facility boundary |
| Zone | Room, row, cage or operational subdivision |
| Rack | Rack identity, U position and physical context |
| Chassis | Enclosure containing nodes or modules |
| Node / Host | Compute or management endpoint |
| Resource | Typed inventory object, physical or logical |
| Relationship | Typed edge between resources |
| Manifest | Versioned declared infrastructure intent |
| Fact | Attributed field value from a source |
| ReconciliationRun | Snapshot comparison and result |
| Difference | Explicit divergence between sources |
| Provider | Versioned adapter and its health |
| Capability | Versioned typed operation contract |
| Operation / Job | Auditable execution request and lifecycle |
| QualificationCampaign | Pinned suite of tests against a scope |
| Observation | Time-series or point-in-time measurement |
| Evidence | Immutable reference to logs, reports, traces or raw results |
| Approval | Authorization for a bounded plan or operation |

## Resource identity and facts

A resource has an EdgeFabric-generated stable ID, tenant/site scope, resource kind, lifecycle state, labels and timestamps. External identifiers are attached as attributed identifiers with issuer, value, validity, confidence and observation time.

A fact is not merely a key/value pair. It carries:
- resource ID and field path;
- typed value and schema version;
- source kind and source/provider identity;
- collection time and freshness/expiry policy;
- confidence and verification status;
- optional evidence reference.

Do not collapse facts from different sources into a single mutable row without preserving history and provenance.

## Resource kinds

Support open-ended kinds rather than a vendor-centric enum. Initial families:
- Facility: site, zone, rack, row, power feed, UPS, cooling loop, CDU, sensor.
- Compute: chassis, node, host, VM, container, CPU package/core, NUMA node, memory.
- Accelerators: GPU, NPU, FPGA, accelerator partition, QPU and remote quantum backend.
- Fabric: NIC, DPU, switch, port, cable, Ethernet, InfiniBand and RoCE fabric.
- Storage: device, controller, volume, filesystem, object or distributed store.
- Software: firmware, driver, kernel, runtime, SDK, compiler and workload profile.

## Typed relationships

Use explicit edge types such as `CONTAINS`, `HOSTED_BY`, `ATTACHED_TO`, `CONNECTED_TO`, `ROUTES_THROUGH`, `POWERED_BY`, `COOLED_BY`, `MANAGED_BY`, `CONTROLLED_BY`, `DEPENDS_ON`, `COMPATIBLE_WITH`, `PARTITION_OF`, `SHARES_MEMORY_WITH`, `SCHEDULED_ON` and `RESERVED_BY`.

Relationships also need source, confidence, validity interval and discovery time. A declared cable map and an observed LLDP link are distinct claims until reconciled.

## Implemented graph primitives

The core crate now defines `ResourceRelationship` and `InfrastructureGraph` as provider-neutral, provenance-attributed graph types. `validate_graph` checks duplicate edge identities and missing or ambiguous endpoint identities without mutating the graph. These are domain primitives and unit-tested validation; they are not yet a persisted graph store or a tenant-facing graph API. Those layers must preserve tenant/site scope, source provenance, observation time and partial-coverage semantics.

## Capability model

Capabilities describe verified behavior, not brand or model. Each descriptor has stable ID, schema version, supported resource kinds, typed input/output, parameter constraints, privilege requirements, safety class, provider compatibility, timeout/cancellation semantics and evidence contract.

Reuse and extend EdgeFabric's existing lowercase snake_case capability taxonomy additively. Existing host capabilities such as `cuda`, `mlx`, `ane`, `llm_infer`, `quantum_sdk`, `quantum_simulator`, `ssh` and runner capabilities remain valid host facts; do not overload them as complete hardware inventory.

## Qualification entities

A qualification campaign pins target selector, platform profile, workload profile, qualification profile, provider/capability versions, test suite revision, environment, policy and time window. Each test result links to observations, raw evidence, normalized outcome and execution context. Results are reproducible only to the extent that hardware, firmware, software and environment versions are captured.

## Lifecycle

Resources may be discovered, declared, verified, active, degraded, stale, quarantined, retired or deleted. Lifecycle transitions are audited. Missing discovery does not automatically mean physical removal; apply freshness and grace-period policy.
