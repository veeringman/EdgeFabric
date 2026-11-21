# Virtual Infrastructure, Digital Twins and Simulation

## Purpose

Virtual infrastructure is a first-class Infrastructure Fabric scope. It enables design, topology validation, capacity planning, visualization and controlled simulation before physical hardware is available, and supports reproducing known hardware layouts for qualification planning.

A virtual instance is not a physical asset. Its resources and observations carry simulation provenance and are isolated from physical discovery, operational telemetry and physical qualification evidence.

## Model layers

1. **Intent:** versioned manifest describing the desired site, rack, host, accelerator and fabric topology.
2. **Virtual graph:** synthetic resources and typed relationships instantiated from the manifest or a scenario template.
3. **Spatial scene:** coordinate frames, transforms, geometry references, physical volumes and placement constraints.
4. **Simulation engine:** one or more versioned engines operating on the virtual graph.
5. **Results:** synthetic observations, constraints, estimates and evidence, explicitly labeled with model fidelity and uncertainty.
6. **Presentation:** 3D/volumetric viewer consuming graph and geometry APIs. The viewer is not the source of truth.

## GPU cluster and PCIe topology scenarios

A GPU cluster scenario should represent more than a number of GPUs. Model:

- Site, room, row, rack, chassis and server placement.
- Host CPU sockets, NUMA nodes and memory domains.
- PCIe root complexes, root ports, bridges and PCIe switches.
- PCIe generation and lane width for each link, plus negotiated link state when modeling observed systems.
- GPU identity, slot placement, partitioning profile and peer-to-peer capability.
- NVLink/NVSwitch domains and other accelerator interconnects as separate links from PCIe.
- NICs, DPUs, storage adapters and their PCIe attachment paths.
- Power feeds, PDUs, cooling zones, thermal limits and rack-level capacity.
- Network fabric topology and oversubscription.

A link's theoretical bandwidth, negotiated bandwidth and measured throughput are different facts. Preserve each with units, directionality, source and confidence. Do not infer end-to-end GPU collective performance from PCIe link width alone.

## Implemented virtual GPU cluster template

The first template expander is available through the tenant-scoped API:

`POST /api/v1/infrastructure/simulations/templates/gpu-cluster`

Example request:

```json
{
  "identity": "lab:gpu-cluster",
  "scenario_id": "rubin-qualification-lab",
  "hosts": 2,
  "gpus_per_host": 4,
  "pcie_generation": 6,
  "root_complex_lanes": 32,
  "switch_uplink_lanes": 16,
  "gpu_lanes": 16,
  "switch_downstream_ports": 8,
  "gpu_memory_gib": 288,
  "numa_nodes_per_host": 2,
  "seed": 42
}
```

The template expands into synthetic host, root-complex, PCIe-switch and GPU resources, containment/connection relationships, and typed PCIe links. It validates host/GPU/NUMA bounds, PCIe generations and lane widths, switch port counts, and root-complex uplink limits. Each switch records its downstream lane budget and the ratio between aggregate GPU downstream lanes and uplink lanes. This is a structural oversubscription indicator, not a measured bandwidth or performance prediction.

For the example above, the deterministic graph contains 2 hosts, 2 root complexes, 2 PCIe switches and 8 GPUs; 12 PCIe devices and 10 PCIe links. Synthetic resources are unverified and tagged with the simulation source. Repeated expansion yields the same resource identities and topology for the same template. The seed is retained as scenario provenance; this structural template currently does not introduce random placement.

The current template creates PCIe topology only. NVLink/NVSwitch, NIC/DPU, network fabrics, storage, power and cooling are subsequent typed extensions. It does not emulate a GPU, launch a workload, or qualify hardware.

## Example scenario

The following is illustrative declarative intent, not a claim of tested or supported hardware. It describes two virtual hosts, each with four virtual GPUs behind a PCIe switch, and a separate GPU peer fabric relationship.

```yaml
apiVersion: infrastructure.edgefabric/v1
kind: SimulationScenario
metadata:
  name: dual-node-8gpu-lab
  revision: 1
simulation:
  kind: topology_simulation
  engine: edgefabric.topology
  engineVersion: "0.1"
  seed: 42
  fidelity:
    topology: structural
    bandwidth: theoretical_estimate
    performance: not_modeled
    powerThermal: not_modeled
resources:
  - id: rack-a
    kind: rack
  - id: host-a
    kind: host
    attributes: { numa_nodes: 2, cpu_sockets: 2 }
  - id: host-b
    kind: host
    attributes: { numa_nodes: 2, cpu_sockets: 2 }
  - id: pcie-a
    kind: pcie_switch
    attributes: { generation: 6, upstream_lanes: 16 }
  - id: pcie-b
    kind: pcie_switch
    attributes: { generation: 6, upstream_lanes: 16 }
  - id: gpu-a0
    kind: gpu
    attributes: { memory_gib: 288, virtual: true }
  - id: gpu-a1
    kind: gpu
    attributes: { memory_gib: 288, virtual: true }
  - id: gpu-a2
    kind: gpu
    attributes: { memory_gib: 288, virtual: true }
  - id: gpu-a3
    kind: gpu
    attributes: { memory_gib: 288, virtual: true }
relationships:
  - { from: rack-a, to: host-a, kind: contains }
  - { from: host-a, to: pcie-a, kind: contains }
  - { from: pcie-a, to: gpu-a0, kind: connected_to, generation: 6, lanes: 16 }
  - { from: pcie-a, to: gpu-a1, kind: connected_to, generation: 6, lanes: 16 }
  - { from: pcie-a, to: gpu-a2, kind: connected_to, generation: 6, lanes: 16 }
  - { from: pcie-a, to: gpu-a3, kind: connected_to, generation: 6, lanes: 16 }
```

A production schema will use typed relationship attributes and validate aggregate lane budgets, switch port counts, root-complex reachability, NUMA locality, GPU peer paths, power and cooling constraints. This abbreviated example intentionally illustrates the model rather than prescribing final schema syntax.

## Simulation classes and fidelity

| Class | Primary question | Typical outputs |
|---|---|---|
| Structural topology | Are components connected as intended? | Reachability, path, port/lane constraint violations |
| Capacity | Can the planned cluster fit? | Rack units, power budget, cooling budget, port capacity |
| Performance estimate | What throughput/latency might be expected? | Modeled bandwidth, contention and workload estimates |
| Power/thermal | Are power and heat constraints plausible? | Load envelopes, thermal estimates, cooling constraints |
| Hardware emulation | How does software interact with an emulated device? | Device/API behavior within declared emulation limits |
| Spatial visualization | Where are resources and how do they fit? | 3D scenes, volumes, clearances and cable routes |

Every result records engine/version, model version, input manifest hash, seed, parameters, assumptions, units, confidence or uncertainty, and fidelity. Never label an estimate as measured performance.

## Spatial and volumetric representation

- Use named coordinate frames with explicit units and transforms.
- Reference geometry assets by immutable URI and content hash.
- Prefer glTF/GLB for real-time scene delivery; retain CAD/mesh/voxel formats where they serve engineering workflows.
- Support bounding volumes, rack-unit coordinates, clearances, cable paths, airflow zones and thermal overlays.
- Stream large scenes and use level-of-detail; do not load an entire data centre as one monolithic mesh.
- Separate physical geometry from telemetry overlays and simulated fields.
- Keep geometry assets in artifact/object storage and graph relationships in the resource graph.

## Safety and isolation

- Simulation providers are non-mutating with respect to physical infrastructure.
- A simulation result cannot satisfy a physical qualification requirement.
- Simulated resource IDs occupy a distinct namespace and retain scenario/run provenance.
- Exporting a simulation into a deployment plan creates a new reviewed plan; it does not directly provision hardware.
- Cross-tenant access to manifests, geometry assets, runs and results is prohibited.
- Resource and geometry uploads require size, type, content-hash and access-policy validation.

## Implementation sequence

1. Core simulation envelope and spatial metadata validation.
2. Scenario manifest schema and deterministic virtual graph instantiation.
3. Structural PCIe/GPU topology validator and fixture suite.
4. Geometry artifact references and read-only scene API.
5. 3D viewer consuming scene and graph APIs.
6. Capacity and power/thermal models with explicit fidelity.
7. Performance modeling and hardware emulation only when backed by validated engines and reference measurements.
