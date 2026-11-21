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
  "gpu_peer_fabric": "nv_switch",
  "seed": 42
}
```

The template expands into synthetic host, root-complex, PCIe-switch and GPU resources, containment/connection relationships, and typed PCIe links. It validates host/GPU/NUMA bounds, PCIe generations and lane widths, switch port counts, and root-complex uplink limits. Each switch records its downstream lane budget and the ratio between aggregate GPU downstream lanes and uplink lanes. This is a structural oversubscription indicator, not a measured bandwidth or performance prediction.

For the example above, the deterministic graph contains 2 hosts, 2 root complexes, 2 PCIe switches and 8 GPUs; 12 PCIe devices and 10 PCIe links. Synthetic resources are unverified and tagged with the simulation source. Repeated expansion yields the same resource identities and topology for the same template. The seed is retained as scenario provenance; this structural template currently does not introduce random placement.

The template can optionally add a GPU peer fabric, independent of PCIe: `nv_link_full_mesh` creates direct GPU-to-GPU links within each host, while `nv_switch` creates a synthetic NVSwitch resource per host and GPU-to-switch peer links. The peer-fabric validator checks endpoint membership, self-links, duplicate identities and duplicate endpoint pairs. For two hosts with four GPUs each, full-mesh mode creates 12 peer links (six per host); NVSwitch mode creates eight GPU-to-switch links and two NVSwitch resources. These are structural abstractions: they do not assert a particular NVLink generation, bandwidth, switch radix, NVLink lane count, or measured peer performance. The template still does not emulate a GPU, launch a workload, or qualify hardware. NIC/DPU, network fabrics, storage, CXL memory and rack power/cooling now have separate structural topology models.

## Storage and CXL topology primitives

Simulation documents may now carry two additional typed topologies, each validated independently from PCIe, GPU peer connectivity and host networking:

- **Storage topology:** links identify source and target resources, transport (`direct_nvme`, `nvme_of_tcp`, `nvme_of_rdma` or `fibre_channel`), addressable capacity in GiB and configured link rate in Gbps. This can describe local NVMe paths or networked storage endpoints without conflating storage transport with the general-purpose network fabric.
- **CXL memory topology:** memory devices identify graph resources, addressable capacity in GiB and optional NUMA locality. Host-to-expander links carry CXL generation and lane width and must reference a declared CXL memory device and a resource in the graph.

The validators reject duplicate/empty identities, missing endpoints, self-links, zero capacities or rates, duplicate endpoint pairs, and unsupported CXL generation/lane-width values. API payloads are bounded to 20,000 storage links, 10,000 CXL memory devices and 20,000 CXL links.

These are topology and declared-capacity primitives, not a storage performance model or a CXL memory-coherency emulator. They do not predict IOPS, latency, bandwidth under contention, memory pooling behavior, decoder/interleave configuration, or NUMA performance. The GPU-cluster template now synthesizes local NVMe drives or a simple shared storage array/fabric, plus per-host CXL memory expanders. Multipath storage, richer CXL decoder/port modeling and performance behavior remain follow-on work.

## Template-generated storage and CXL resources

The GPU-cluster template now accepts optional `storage` and `cxl_memory` objects. These generate synthetic resources in the same graph as the hosts and GPUs, plus typed storage/CXL topology records and graph relationships.

- `storage.kind: direct_nvme` creates the configured number of `nvme_ssd` resources per host and host-to-drive links. Set `drives_per_host`, `capacity_gib_per_drive` and `link_speed_gbps`.
- `storage.kind: nvme_of_tcp`, `nvme_of_rdma` or `fibre_channel` creates one shared storage array and a matching fabric-switch resource. Each host gets a path to the switch, and the switch gets a path to the array. Configure `shared_array_capacity_gib`, `switch_ports` and `link_speed_gbps`.
- `cxl_memory` creates the configured number of `cxl_memory_expander` resources per host, assigns NUMA nodes round-robin across the host's declared NUMA nodes, and creates CXL links carrying generation and lane width.

Example additions to a GPU-cluster template:

```json
{
  "storage": {
    "kind": "nvme_of_rdma",
    "drives_per_host": 0,
    "capacity_gib_per_drive": 0,
    "shared_array_capacity_gib": 1048576,
    "link_speed_gbps": 400,
    "switch_ports": 3
  },
  "cxl_memory": {
    "devices_per_host": 2,
    "capacity_gib_per_device": 1024,
    "generation": 3,
    "lane_width": 8
  }
}
```

The template validates non-zero capacities and link rates, supported CXL generation/lane widths, per-host device bounds, aggregate resource limits and topology link limits. API responses report generated storage-link, CXL-device and CXL-link counts. Direct NVMe drive count and shared-array capacity are alternative modes: the fields not used by the selected mode are ignored.

The generated topology is deterministic in identity and connectivity for a fixed template. Capacity and link speed are declared structural inputs, not measured throughput or performance predictions. The shared-array model is intentionally a simple single-switch/single-array topology; multipath, dual-fabric redundancy, RAID layouts, namespaces, drive endurance, IOPS and latency are not modeled. CXL memory devices are structural expanders; decoder windows, interleave sets, coherency behavior, pooling protocols and bandwidth contention are not modeled.

## Rack placement, power feeds and cooling envelope

The GPU-cluster template optionally accepts a `power_cooling` object. It deterministically places hosts into racks using `hosts_per_rack`, creates synthetic rack, PDU and cooling-zone resources, and records `contains`, `powered_by` and `cooled_by` relationships.

The declared component envelopes are summed per host:
- host chassis power;
- GPU power multiplied by GPUs per host;
- NIC power multiplied by configured NICs per host;
- direct-NVMe drive power (shared storage-array power is not yet included);
- CXL expander power.

For each rack, the generator compares the resulting design load against rack power capacity, the configured PDU capacity and cooling capacity. When `require_redundant_pdu` is true, at least two PDUs are required and the capacity check uses the remaining PDU capacity after loss of one unit (an N+1-style capacity check). This is a simplified feed-capacity abstraction; it does not model phase balancing, breaker curves, voltage, UPS/generator transition dynamics or actual power telemetry.

Example template addition:

```json
{
  "power_cooling": {
    "hosts_per_rack": 2,
    "host_power_watts": 1200,
    "gpu_power_watts": 700,
    "nic_power_watts": 100,
    "storage_device_power_watts": 30,
    "cxl_device_power_watts": 50,
    "rack_power_capacity_watts": 18000,
    "pdus_per_rack": 2,
    "pdu_capacity_watts": 12000,
    "cooling_capacity_watts": 18000,
    "require_redundant_pdu": true
  }
}
```

The rack template can additionally describe physical placement with `rack_units`, `host_height_u`, `rack_depth_mm`, `rack_width_mm`, `host_depth_mm`, `host_width_mm` and `rack_spacing_mm`. The generator assigns non-overlapping rack-unit slots and creates spatial poses in a named millimetre coordinate frame. The current arrangement is a deterministic row of racks and vertical host positions; it is not a CAD fit check for rails, cable bend radius, rear-door clearance or service access.

For redundant power, an even PDU count is split into independent logical A and B groups. Each host is attached to one PDU on each feed. Optional UPS units are created per feed, with PDU-to-UPS relationships and per-feed UPS capacity checks. Under redundant mode, the effective available capacity is the lesser of one feed's PDU capacity and its UPS capacity, so the rack load must fit on either surviving feed. This models capacity paths, not electrical protection coordination, actual circuit isolation or transfer behavior.

All values are declared design envelopes in watts, not measured draw, heat output or a CFD/thermal prediction. Shared-array storage power, switch power, conversion losses, power factor, transient peaks, airflow, coolant flow/temperature, hot/cold aisle containment and facility-level cooling plant constraints remain outside this first structural model. API responses report generated rack and PDU counts; rack records retain per-rack design load and capacity values.

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


## NIC, DPU and network fabric topology

The GPU-cluster template optionally accepts a `network_fabric` object with a fabric kind (`ethernet`, `roce` or `infini_band`), NICs per host, ports per NIC, link speed in Gbps, aggregate switch port count and an optional per-host DPU. Expansion creates synthetic NIC and fabric-switch resources, optional DPU resources, host containment relationships and explicit NIC-to-DPU-to-switch or NIC-to-switch links. Network paths are a separate topology from PCIe attachment and GPU peer connectivity. The validator checks endpoint existence, non-zero speed, duplicate links and fabric-kind consistency; template validation also checks NIC counts, speed bounds and aggregate switch-port capacity.

For example, two hosts with two 400-Gbps RoCE NICs each and one DPU per host produce four NIC resources, two DPU resources, one shared RoCE switch, and eight logical links (NIC-to-DPU and DPU-to-switch). This is a structural capacity/topology description, not a claim about a specific NIC, DPU, switch model, RDMA configuration, routing, oversubscription behavior or measured throughput.
