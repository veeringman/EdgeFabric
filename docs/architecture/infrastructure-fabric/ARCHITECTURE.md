# Architecture

## 1. Product boundary

Infrastructure Fabric extends EdgeFabric's existing control and execution planes. It is not a new repository, identity system, API gateway, network-discovery service, observability stack or competing inventory database.

Repository assessment identified relevant existing foundations: Rust workspace crates including `ef-core`, `ef-api`, and `ef-agent`; a shared capability taxonomy; generic SSH host enrollment and runner probing; ML-node inventory; and discovery APIs. Map the actual implementations and persistence/event contracts before selecting exact code locations.

## 2. Three-source state model

- **Declared:** what should exist and intended configuration.
- **Discovered:** what existing EdgeFabric discovery and providers identify.
- **Observed:** measured health, telemetry, diagnostics and test outcomes.

A reconciler derives a resolved graph and explicit differences while retaining source, timestamp, confidence, freshness and provider version for every fact.

Canonical difference states include `MATCHED`, `MISSING`, `UNEXPECTED`, `ATTRIBUTE_DRIFT`, `TOPOLOGY_MISMATCH`, `PARTIALLY_DISCOVERED`, `IDENTITY_CONFLICT`, `STALE`, and `UNVERIFIED`.

## 3. Logical architecture

```text
Existing EdgeFabric Console / API / CLI
                 |
       Existing Auth / Tenant / RBAC
                 |
+--------------------------------------------------+
|             Infrastructure Fabric               |
| Manifest Registry       Reconciliation Engine   |
| Resource Graph           Capability Catalog      |
| Provider Registry        Operations / Jobs       |
| Qualification            Evidence / Observations |
| Policy / Approval        Topology                |
+--------------------------------------------------+
       |             |              |
 Existing APIs   Existing events  Existing storage
       |
 Existing EF agents / optional site coordinator
       |
 Provider adapters: Host, Network, BMC, GPU, FPGA,
 QPU, Storage, Power/Cooling, Simulation
```

This is a logical view, not a deployment mandate. Prefer extending current services/modules. Add a service only when repository evidence justifies isolation or independent scaling.

## 4. Responsibilities

### Control plane
- Store and version manifests; validate and compose profiles.
- Schedule reconciliation and calculate diffs.
- Maintain resource graph, provenance and typed relationships.
- Register providers and versioned capability contracts.
- Plan and orchestrate operations, reservations and approvals.
- Coordinate qualification campaigns and index evidence.
- Expose API/UI and audit events using EdgeFabric conventions.

### Execution plane
- Execute approved provider capabilities near hardware or through management APIs.
- Collect inventory, telemetry, diagnostics and qualification results.
- Enforce target scope, privileges, deadlines, output bounds, locks and safety classes.
- Return normalized results plus scrubbed vendor evidence.
- Support bounded operation during transient control-plane disconnection when job policy allows.

Reuse existing EdgeFabric agents and secure transport. A site coordinator is optional.

## 5. Resource graph

Resource classes include site, zone, cluster, pool, rack, chassis, node, host, VM, container; CPU, NUMA and memory; GPU/NPU/FPGA/QPU; NIC, DPU, switch, port, cable and fabric; storage; BMC, PDU, UPS, cooling loop and sensors; firmware, driver, runtime, SDK and workload profile.

Relationships include `CONTAINS`, `HOSTED_BY`, `ATTACHED_TO`, `CONNECTED_TO`, `ROUTES_THROUGH`, `POWERED_BY`, `COOLED_BY`, `MANAGED_BY`, `DEPENDS_ON`, `PARTITION_OF`, `COMPATIBLE_WITH` and `RESERVED_BY`.

Identity is EdgeFabric-stable and independent of vendor labels. Serial, UUID, PCI BDF, MAC and provider IDs are identifiers with provenance, not interchangeable primary keys. Never rely solely on mutable IP addresses. Ambiguous identity produces a conflict requiring resolution.

## 6. Virtual infrastructure and simulation layer

Simulation is a first-class provider-neutral capability, not a second inventory truth source. A simulation instance is an isolated, versioned scenario with an explicit simulation kind, engine/version, deterministic seed, source manifest revision, parameters and a synthetic resource graph. Synthetic resources and observations carry simulation provenance and must never be promoted to physically discovered or verified facts without a separate real provider observation and reconciliation decision.

Support two complementary model families:

- **Spatial/digital-twin models:** hierarchical site/room/row/rack/chassis/board layouts; coordinate frames; transforms; geometry assets; physical volumes; cable routes; power and cooling zones. Store geometry as content-addressed artifacts in existing artifact/object storage and reference it from graph resources. Keep graph identity and geometry identity separate.
- **Virtual hardware/topology instances:** configurable hosts, CPU/NUMA/memory, PCIe switches and bridges, PCIe-connected GPUs, NICs/DPUs, NVLink/NVSwitch fabrics, storage and network topologies. Model PCIe generation, lane width, negotiated link state, switch hierarchy, NUMA affinity, bandwidth/latency constraints, power/thermal envelopes and accelerator interconnects as typed attributes/relationships.

The simulator contract must expose model fidelity, supported phenomena, assumptions, uncertainty, engine/version, deterministic seed and input manifest hash. Distinguish topology validation, capacity/performance estimation, power/thermal modeling and hardware emulation; a topology simulator must not imply cycle-accurate GPU performance. Simulated qualification results are labeled separately from physical hardware qualification.

A 3D viewer consumes graph and geometry APIs; it is not the system of record. Use standard interchange formats (for example glTF/GLB for real-time scenes and STEP/mesh/voxel references where appropriate), explicit units and coordinate frames, and level-of-detail/streaming for large sites. Do not embed large geometry blobs in relational graph rows.

## 7. Reconciliation lifecycle
## 6. Reconciliation lifecycle

1. Capture a time-bounded snapshot from existing discovery and selected providers.
2. Normalize facts into source-attributed records.
3. Resolve identities; do not merge ambiguous resources.
4. Compare declarations, discoveries and observations.
5. Produce differences, confidence/freshness and policy outcomes.
6. Generate a separately versioned plan if remediation is requested.
7. Obtain approval for disruptive actions.
8. Apply only the approved plan with idempotency, locks and precondition checks.
9. Reconcile again to verify convergence.

Reconciliation is read-only by default. It never silently provisions, reconfigures, reboots, flashes firmware or changes network settings.

## 7. Field authority

Authority is policy-controlled per field and context. For example, manifests may own intended node count; a provider may be authoritative for PCI identity; telemetry is authoritative for measured temperature. Preserve conflicting claims and their provenance rather than overwriting source records.

## 8. Operational invariants

- Immutable, hash-addressed manifest revisions and plans.
- Plans pin manifest, schema, provider, capability and policy versions.
- Apply requires idempotency, resource locks and precondition revalidation.
- Partial discovery is explicit; coverage and freshness are visible.
- Every resolution and mutation is tenant-scoped and audited.
