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

- `GET /api/v1/infrastructure/hardware-profiles` — list built-in profiles and catalog metadata.
- `GET /api/v1/infrastructure/hardware-profiles/{id}` — retrieve a profile by stable ID.
- `POST /api/v1/infrastructure/hardware-profiles/validate` — validate a submitted profile document without saving it or granting qualification.

A profile is versioned and carries optional vendor/product identity, structured facts, supported component classes, source provenance, and licensed geometry asset references with SHA-256 digests. Unknown facts are omitted rather than inferred. Vendor-documentation, vendor-validation and field-verification states require provenance; validation is structural and does not independently prove that a source is authoritative.

The built-in `edgefabric.generic-gpu-node` profile is explicitly `template_only`. It has no vendor, product model, dimensions, port map, power envelope or CAD asset. It is suitable for generic topology authoring only. No Rubin or Rubin Ultra product profile is currently published by this catalog. Those profiles must be populated from authoritative vendor documentation and reviewed CAD/asset packages before they can be used for physical qualification.

This first catalog is read-only and built into the API; tenant-persisted profile authoring, approval workflow, profile-to-simulation binding and asset resolution are subsequent implementation steps. Profile validation does not persist input and never changes a simulation or physical inventory.
