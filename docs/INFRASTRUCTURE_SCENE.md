# Infrastructure Simulation Scene Projection

## Purpose

The scene projection is a renderer-neutral view of an immutable, tenant-owned infrastructure simulation. It is derived from the same `SimulationInstance` resource graph and typed topology models used by validation and reconciliation. It is not a second inventory database and it does not execute a simulator.

## Endpoint

```http
GET /api/v1/infrastructure/simulations/{id}/scene
Authorization: Bearer <tenant JWT>
```

The simulation lookup is tenant-scoped. A simulation belonging to another tenant is returned as not found.

## Scene document

The response contains:

- `scene_id` and `simulation_id`: stable scene and persisted simulation identifiers.
- `coordinate_units`: `mm`; `coordinate_convention`: right-handed.
- `nodes[]`: graph resources with stable IDs, kind, attributes, provenance, optional transform, optional axis-aligned bounds, optional geometry asset reference, and a render hint.
- `edges[]`: graph relationships with stable IDs, source/target IDs, kind, attributes and provenance.
- `topology_overlays`: typed PCIe, GPU peer, network, storage, CXL and rack power/cooling structures. These retain the domain-specific link properties instead of flattening every fabric into generic edges.
- `scene_metadata`: node/edge and spatial/asset/fallback counts.
- `fidelity`: explicit limits on what the scene represents.

## Renderer behavior

For each node, `render_hint` is one of:

- `asset` — a geometry asset reference is present. The client resolves the asset through the configured artifact/object-storage integration and verifies its digest.
- `bounds_primitive` — no asset is attached; render a box or other simple placeholder from `bounds_mm`.
- `unresolved` — neither asset nor bounds are available; show the node in the scene hierarchy but do not invent dimensions.

Transforms are optional. A missing transform must not be interpreted as a zero-position measurement. The scene projection does not embed binary geometry, vendor CAD, or proprietary GLB files.

## Topology visualization

Render graph edges as the general relationship layer. Use typed overlays for domain-specific visualization:

- PCIe: upstream/downstream endpoints, generation and lane width.
- GPU peer fabric: NVLink full-mesh or NVSwitch logical links, distinct from PCIe.
- Network: Ethernet, RoCE or InfiniBand paths and configured link speed.
- Storage: direct NVMe, NVMe-oF or Fibre Channel paths and configured capacity/rate.
- CXL: memory expander attachment, generation, lane width and NUMA association.
- Rack power/cooling: rack load/capacity, PDU and UPS identities, rack-unit placements and cooling design capacity.

A renderer may expose independent visibility toggles for each overlay. Overlay edges should reference the same node IDs as the graph.

## Fidelity and safety

All resources in this endpoint are marked `inventory_class: simulated`. `execution_started` is always false. Geometry asset references are not proof of vendor approval or physical fit. Rack power and cooling values are declared design envelopes, not live telemetry or CFD. PCIe/NVLink/network/storage topology is structural and does not imply measured or predicted bandwidth, latency or application performance.

The current projection is read-only. It cannot provision hardware, change physical inventory, start simulation workloads, or apply reconciliation plans.


## Dashboard viewer

The Leptos dashboard exposes the viewer at `/infrastructure/scene` under the Infrastructure Fabric navigation section. It loads the tenant-scoped simulation registry, lets an operator select/paste a simulation UUID, and requests the scene endpoint. The Three.js renderer provides orbit, zoom and pan controls, a resource list/inspector, and independent visibility toggles for PCIe, GPU peer, network, storage/CXL and power overlays.

The renderer currently uses bounded primitive geometry for spatial resources. Geometry asset references are retained in the scene response but are not yet fetched or rendered by the browser; this avoids treating an unresolved asset URI as a usable CAD model. The viewer remains read-only.
\n
### Dashboard simulation builder

The same dashboard page includes a GPU cluster template builder. It submits a deterministic template to `POST /api/v1/infrastructure/simulations/templates/gpu-cluster` and opens the returned tenant-owned simulation in the scene viewer.

The current builder exposes host/GPU counts, PCIe generation, GPU memory, NVLink/NVSwitch selection, Ethernet/RoCE/InfiniBand network selection, optional per-host DPU, direct NVMe or shared NVMe-oF/Fibre Channel storage, optional CXL memory expansion, hosts-per-rack, rack-unit capacity, and redundant A/B power with UPS. Chassis dimensions, per-device power assumptions, link rates and storage capacities are starter-profile values in this iteration; they are not vendor-certified specifications. Backend template validation remains authoritative and may reject a configuration whose declared rack, power, cooling, port or topology budgets do not fit.

The builder does not create vendor-specific HGX/DGX CAD geometry and does not claim Rubin compatibility from a generic topology. Vendor-qualified chassis and component profiles are a separate next step.

## Versioned hardware profile catalog

The authenticated API exposes a starter catalog:

- `GET /api/v1/infrastructure/hardware-profiles` — list built-in and tenant-owned profiles.
- `GET /api/v1/infrastructure/hardware-profiles/{uuid}` — retrieve a tenant-owned immutable version.
- `POST /api/v1/infrastructure/hardware-profiles` — register a new tenant-owned immutable version.
- `POST /api/v1/infrastructure/hardware-profiles/validate` — validate a submitted profile document without saving it or granting qualification.

A profile is versioned and carries optional vendor/product identity, structured facts, supported component classes, source provenance, and licensed geometry asset references with SHA-256 digests. Unknown facts are omitted rather than inferred. Vendor-documentation, vendor-validation and field-verification states require provenance; validation is structural and does not independently prove that a source is authoritative.

The built-in `edgefabric.generic-gpu-node` profile is explicitly `template_only`. It has no vendor, product model, dimensions, port map, power envelope or CAD asset. It is suitable for generic topology authoring only. No Rubin or Rubin Ultra product profile is currently published by this catalog. Those profiles must be populated from authoritative vendor documentation and reviewed CAD/asset packages before they can be used for physical qualification.

The built-in generic profile is read-only; tenant profile versions are persisted separately. Profile validation does not persist input and never changes a simulation or physical inventory.

### Tenant-persisted profile versions

The catalog now supports tenant-owned immutable profile versions:

- `GET /api/v1/infrastructure/hardware-profiles` — lists tenant versions and the built-in generic template. An optional `profile_id` query filters tenant versions.
- `GET /api/v1/infrastructure/hardware-profiles/{uuid}` — retrieves one profile version within the caller's tenant.
- `POST /api/v1/infrastructure/hardware-profiles/validate` — validates a payload without saving it.
- `POST /api/v1/infrastructure/hardware-profiles` — validates and stores a new immutable version.

The database enforces uniqueness for `(tenant_id, profile_id, version)`; a changed profile is a new version rather than an update. Stored records begin in `draft` review state. The schema reserves `submitted`, `approved` and `rejected` states for a subsequent controlled review workflow; this iteration does not expose approval transitions and does not grant qualification based on registration. The qualification field is provenance metadata supplied in the submitted document, not an independently verified assertion by EdgeFabric.

Tenant isolation is enforced in repository queries. The built-in generic profile remains a separate read-only template and is not implicitly copied into a tenant's registry. Profile-to-simulation binding and physical-fit qualification remain subsequent steps.

### Hardware profile binding on GPU cluster simulations

The GPU cluster template endpoint accepts an optional top-level `hardware_profile_record_id` UUID alongside the existing flattened template fields. When supplied, the API resolves that immutable record inside the caller's tenant, checks that the stored document's profile ID and version agree with its database metadata, and embeds a read-only `hardware_profile_binding` record in the simulation parameters. The create response and scene projection expose the binding, including qualification and review-status metadata.

The dashboard builder exposes the optional profile-version UUID field. Obtain candidate UUIDs from `GET /api/v1/infrastructure/hardware-profiles`. The reference does not copy mutable profile data into the simulation and does not confer vendor qualification. It currently records association and provenance only; physical fit, PCIe lane mapping, power/cooling limits, cable maps and vendor-specific topology are not yet evaluated against profile facts. The referenced profile must remain in the same tenant.

### Hardware profile constraints and submission

Profile authors may supply machine-readable numeric limits under `facts.constraints`. Each constraint key maps to an object containing one or more of `min`, `max`, and `equals` numeric operators. The current GPU-cluster evaluator maps template inputs to these observation keys:

- `hosts`, `gpus_per_host`, `pcie_generation`, `root_complex_lanes`, `switch_uplink_lanes`, `gpu_lanes`, `gpu_memory_gib`, `numa_nodes_per_host`
- `network_link_speed_gbps`, `nics_per_host`, `network_switch_ports`
- `storage_link_speed_gbps`, `storage_capacity_gib`
- `hosts_per_rack`, `rack_units`, `host_power_watts`, `gpu_power_watts`, `rack_power_capacity_watts`, `cooling_capacity_watts`, `rack_depth_mm`, `rack_width_mm`, `host_depth_mm`, `host_width_mm`

Only explicitly declared numeric constraints are evaluated. Missing constraints or missing observations remain `unknown`; EdgeFabric does not infer values. Evaluation status is `pass`, `fail`, `partial`, or `unknown`. This is a profile-vs-template structural check, not a vendor certification, electrical simulation, thermal/CFD model, measured performance result, or physical deployment approval.

A tenant may submit a draft profile for review with `POST /api/v1/infrastructure/hardware-profiles/{id}/submit`. This is a one-way `draft` to `submitted` transition scoped to the owning tenant. Approval and rejection are deliberately not exposed by this endpoint; they require a separately authorized review workflow. Submission never grants qualification.

### NVIDIA Rubin and Vera Rubin Ultra reference catalog

The built-in catalog now includes read-only, source-linked reference profiles for the Rubin GPU, HGX Rubin NVL8, Vera Rubin NVL72 and Vera Rubin Ultra NVL576. These entries are `vendor_documented` references, not vendor-validated qualification packages. Their source URLs and publication/access revisions are retained in the profile documents.

The Rubin GPU reference records NVIDIA-published 288 GB HBM4, 22 TB/s memory bandwidth and sixth-generation NVLink at 3.6 TB/s per GPU. HGX Rubin NVL8 records the eight-GPU baseboard and the published system-level HBM4, NVLink-switch and networking figures. NVIDIA labels its HGX specifications preliminary and subject to change; consumers must preserve this caveat and the source revision.

The Vera Rubin NVL72 reference records 72 GPUs, 36 Vera CPUs, 18 compute trays, nine NVLink switch trays, ConnectX-9 SuperNIC and BlueField-4 DPU counts per compute tray, NVLink 6 bandwidth, approximate rack weight and the published 45°C coolant inlet design point. The Vera Rubin Ultra NVL576 reference records the announced eight-rack, 576-GPU, two-layer all-to-all NVLink domain using copper and direct optical inter-rack connections.

**Modeling boundary:** an NVL72 compute tray is not a generic host server. Do not translate its 18 compute trays into 18 conventional hosts. The existing generic GPU-cluster template remains a topology simulation, not a faithful NVL72/NVL576 system model. Exact rack mechanical envelopes, electrical input and transient envelopes, coolant flow/pressure, CDU/manifold design, service clearances, detailed tray layouts, cable schedules, PCIe slot maps and OEM variants require their respective authoritative documents and licensed assets. Unknowns are intentionally omitted.

Read-only built-ins can be copied into a tenant's immutable profile registry with `POST /api/v1/infrastructure/hardware-profiles/clone` and body `{"profile_id":"nvidia.hgx-rubin-nvl8"}` (replace with another built-in ID as required). The clone retains its public source provenance and vendor-documented classification but begins in tenant review state `draft`. It can then be submitted with `POST /api/v1/infrastructure/hardware-profiles/{id}/submit`. This does not grant vendor validation or field verification.

### Component-level profile inventory

Hardware profiles now support typed component entries in addition to the profile-wide `supported_components` class list. Each entry has a stable component ID, class, optional vendor/model/quantity, an optional parent component ID for containment, a facts object, optional component-specific provenance sources and optional licensed geometry assets. Unknown quantities remain absent. The built-in Rubin references use these entries for GPU, CPU, baseboard, compute-tray, NVLink-switch-tray, rack and NVLink-domain inventory. This inventory is descriptive; it does not synthesize missing cable maps, slot maps or physical dimensions.

Reference material used for the built-in NVIDIA entries (retrieved 2026-11-21):

- NVIDIA HGX Platform specifications: https://www.nvidia.com/en-us/data-center/hgx/
- NVIDIA Vera Rubin POD architecture and Vera Rubin Ultra NVL576 topology: https://developer.nvidia.com/blog/nvidia-vera-rubin-pod-seven-chips-five-rack-scale-systems-one-ai-supercomputer

The public NVIDIA sources are stored on the profile records themselves. Re-fetch and review source revisions before any real hardware qualification or procurement decision.

### Profile-derived structural simulation

A tenant-owned profile with component inventory can now be expanded into a structural simulation with `POST /api/v1/infrastructure/simulations/templates/hardware-profile`. The request contains `identity`, `scenario_id`, `hardware_profile_record_id` and optional `seed`. The endpoint expands each declared component quantity into simulated resource nodes and builds only the declared containment hierarchy. Component quantities that are unknown remain represented as unspecified component nodes rather than guessed counts.

The dashboard offers **Create profile-derived component twin** after a tenant profile has been selected. For Vera Rubin NVL72, the reference component inventory expands 72 GPUs, 36 Vera CPUs, 18 compute trays, 9 NVLink switch trays, 144 ConnectX-9 SuperNICs and 18 BlueField-4 DPUs. For Vera Rubin Ultra NVL576, it expands eight MGX NVL racks, 576 GPUs and one NVLink-domain resource. Parent-child containment is represented; detailed NVLink edge/cable routing is not fabricated.

The generated graph is always marked with incomplete physical discovery coverage. It contains no vendor CAD geometry, measured telemetry, PCIe slot map, explicit cable topology, facility power model or CFD. The 3D viewer therefore uses generic visual primitives until licensed geometry assets and coordinate metadata are supplied. This profile-derived twin is a structural planning artifact, not an exact hardware emulator or qualification result.
