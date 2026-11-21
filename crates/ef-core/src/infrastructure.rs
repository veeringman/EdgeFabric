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

/// A typed edge between two normalized infrastructure resources.
///
/// Relationship identity is stable within the graph snapshot and must not be
/// derived from mutable display names or network addresses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceRelationship {
    pub identity: String,
    pub source_identity: String,
    pub target_identity: String,
    /// Examples: contains, connected_to, powered_by, cooled_by, attached_to.
    pub kind: String,
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    pub source: String,
    pub observed_at: DateTime<Utc>,
    #[serde(default)]
    pub verified: bool,
}

/// Simulation modes are explicit so synthetic resources cannot be mistaken for
/// physically discovered or verified inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationKind {
    HardwareEmulation,
    TopologySimulation,
    CapacityModel,
    PowerThermalModel,
    SpatialVisualization,
}

/// Functional role of a device in a simulated PCIe fabric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PcieDeviceRole {
    RootComplex,
    Switch,
    Endpoint,
    Bridge,
}

/// PCIe capability envelope for one graph resource. The values describe the
/// simulated port/device capability, not a live hardware probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcieDevice {
    pub resource_identity: String,
    pub role: PcieDeviceRole,
    pub max_generation: u8,
    pub max_lane_width: u8,
    pub numa_node: Option<u16>,
}

/// A point-to-point PCIe link with negotiated generation and lane width.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcieLink {
    pub identity: String,
    pub upstream_identity: String,
    pub downstream_identity: String,
    pub generation: u8,
    pub lane_width: u8,
}

/// Virtual PCIe topology for GPU, NIC/DPU, storage and accelerator instances.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcieTopology {
    pub devices: Vec<PcieDevice>,
    pub links: Vec<PcieLink>,
}

/// Validate device membership, unique edges, and negotiated link limits.
pub fn validate_pcie_topology(topology: &PcieTopology, graph: &InfrastructureGraph) -> Vec<String> {
    let mut issues = Vec::new();
    let mut devices = HashMap::new();
    for device in &topology.devices {
        if devices.insert(device.resource_identity.as_str(), device).is_some() {
            issues.push(format!("Duplicate PCIe device '{}'", device.resource_identity));
        }
        if !graph.resources.iter().any(|resource| resource.identity == device.resource_identity) {
            issues.push(format!("PCIe device '{}' is not present in the resource graph", device.resource_identity));
        }
        if !(1..=7).contains(&device.max_generation) {
            issues.push(format!("PCIe device '{}' has unsupported maximum generation", device.resource_identity));
        }
        if ![1, 2, 4, 8, 16, 32].contains(&device.max_lane_width) {
            issues.push(format!("PCIe device '{}' has invalid maximum lane width", device.resource_identity));
        }
    }

    let mut link_ids = HashSet::new();
    let mut endpoint_pairs = HashSet::new();
    for link in &topology.links {
        if link.identity.trim().is_empty() || !link_ids.insert(link.identity.as_str()) {
            issues.push(format!("PCIe link identity '{}' is empty or duplicated", link.identity));
        }
        if link.upstream_identity == link.downstream_identity {
            issues.push(format!("PCIe link '{}' cannot connect a device to itself", link.identity));
        }
        let pair = if link.upstream_identity <= link.downstream_identity {
            (link.upstream_identity.as_str(), link.downstream_identity.as_str())
        } else {
            (link.downstream_identity.as_str(), link.upstream_identity.as_str())
        };
        if !endpoint_pairs.insert(pair) {
            issues.push(format!("PCIe endpoint pair on link '{}' is duplicated", link.identity));
        }
        let upstream = devices.get(link.upstream_identity.as_str());
        let downstream = devices.get(link.downstream_identity.as_str());
        match (upstream, downstream) {
            (Some(upstream), Some(downstream)) => {
                let max_generation = upstream.max_generation.min(downstream.max_generation);
                let max_width = upstream.max_lane_width.min(downstream.max_lane_width);
                if !(1..=7).contains(&link.generation) || link.generation > max_generation {
                    issues.push(format!("PCIe link '{}' exceeds endpoint generation capability", link.identity));
                }
                if ![1, 2, 4, 8, 16, 32].contains(&link.lane_width) || link.lane_width > max_width {
                    issues.push(format!("PCIe link '{}' exceeds endpoint lane-width capability", link.identity));
                }
            }
            _ => issues.push(format!("PCIe link '{}' references a device missing from its topology", link.identity)),
        }
    }
    issues
}

/// GPU peer-fabric topology is modeled independently from PCIe attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuPeerFabricKind {
    NvLinkFullMesh,
    NvSwitch,
}

/// One logical peer-fabric edge. For NVSwitch, the switch resource is an endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuPeerLink {
    pub identity: String,
    pub source_identity: String,
    pub target_identity: String,
}

/// Synthetic GPU peer connectivity, separate from PCIe topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuPeerFabricTopology {
    pub kind: GpuPeerFabricKind,
    pub links: Vec<GpuPeerLink>,
}

/// Network transport represented by a simulated cluster fabric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkFabricKind {
    Ethernet,
    Roce,
    InfiniBand,
}

/// Per-host network attachment and shared fabric configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkFabricTemplate {
    pub kind: NetworkFabricKind,
    pub nics_per_host: u8,
    pub ports_per_nic: u8,
    pub link_speed_gbps: u32,
    pub switch_ports: u32,
    #[serde(default)]
    pub dpu_per_host: bool,
}

/// One physical/logical network hop in a synthetic topology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkFabricLink {
    pub identity: String,
    pub source_identity: String,
    pub target_identity: String,
    pub speed_gbps: u32,
    pub fabric: NetworkFabricKind,
}

/// Network connectivity remains distinct from PCIe and GPU peer fabrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkFabricTopology {
    pub kind: NetworkFabricKind,
    pub links: Vec<NetworkFabricLink>,
}

/// Per-cluster storage attachment and capacity configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageFabricTemplate {
    pub kind: StorageFabricKind,
    /// Used by direct NVMe; ignored for shared-array transports.
    pub drives_per_host: u8,
    pub capacity_gib_per_drive: u64,
    /// Used by NVMe-oF and Fibre Channel shared-array transports.
    pub shared_array_capacity_gib: u64,
    pub link_speed_gbps: u32,
    /// Required for shared-array transports; represents host-facing ports.
    pub switch_ports: u32,
}

/// Per-host CXL memory expansion configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CxlMemoryTemplate {
    pub devices_per_host: u8,
    pub capacity_gib_per_device: u64,
    pub generation: u8,
    pub lane_width: u8,
}

/// Storage transport represented independently from PCIe and host networking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageFabricKind {
    DirectNvme,
    NvmeOfTcp,
    NvmeOfRdma,
    FibreChannel,
}

/// A logical storage path. Capacity is the addressable capacity represented by
/// this path; speed is the configured link rate, not measured throughput.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageFabricLink {
    pub identity: String,
    pub source_identity: String,
    pub target_identity: String,
    pub kind: StorageFabricKind,
    pub capacity_gib: u64,
    pub link_speed_gbps: u32,
}

/// Storage paths can describe local NVMe or networked storage endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageTopology {
    pub links: Vec<StorageFabricLink>,
}

/// CXL memory is modeled separately from ordinary PCIe endpoint inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CxlMemoryDevice {
    pub resource_identity: String,
    pub capacity_gib: u64,
    pub numa_node: Option<u16>,
}

/// One host-to-expander CXL memory attachment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CxlMemoryLink {
    pub identity: String,
    pub host_identity: String,
    pub memory_device_identity: String,
    pub generation: u8,
    pub lane_width: u8,
}

/// CXL memory pooling/expansion topology and its addressable devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CxlTopology {
    pub devices: Vec<CxlMemoryDevice>,
    pub links: Vec<CxlMemoryLink>,
}

/// Declarative inputs for a reproducible multi-host virtual GPU cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuClusterTemplate {
    pub identity: String,
    pub scenario_id: String,
    pub hosts: u16,
    pub gpus_per_host: u16,
    pub pcie_generation: u8,
    pub root_complex_lanes: u8,
    pub switch_uplink_lanes: u8,
    pub gpu_lanes: u8,
    pub switch_downstream_ports: u16,
    pub gpu_memory_gib: u32,
    pub numa_nodes_per_host: u8,
    #[serde(default)]
    pub gpu_peer_fabric: Option<GpuPeerFabricKind>,
    #[serde(default)]
    pub network_fabric: Option<NetworkFabricTemplate>,
    #[serde(default)]
    pub storage: Option<StorageFabricTemplate>,
    #[serde(default)]
    pub cxl_memory: Option<CxlMemoryTemplate>,
    pub seed: u64,
}

/// Expand a template into a deterministic synthetic graph and PCIe topology.
/// Resource identities are stable for a given template; timestamps are metadata.
pub fn instantiate_gpu_cluster_template(
    template: &GpuClusterTemplate,
) -> Result<SimulationInstance, Vec<String>> {
    let mut issues = Vec::new();
    if template.identity.trim().is_empty() || template.identity.len() > 200 {
        issues.push("Template identity is required and must be at most 200 characters".into());
    }
    if template.scenario_id.trim().is_empty() || template.scenario_id.len() > 200 {
        issues.push("Scenario ID is required and must be at most 200 characters".into());
    }
    if template.hosts == 0 || template.hosts > 256 { issues.push("Hosts must be between 1 and 256".into()); }
    if template.gpus_per_host == 0 || template.gpus_per_host > 64 { issues.push("GPUs per host must be between 1 and 64".into()); }
    if u32::from(template.hosts) * (u32::from(template.gpus_per_host) + 3) > 10_000 {
        issues.push("Expanded GPU cluster may contain at most 10000 resources".into());
    }
    let network_resource_count = template.network_fabric.as_ref().map_or(0, |network| {
        u32::from(template.hosts) * (u32::from(network.nics_per_host) + if network.dpu_per_host { 1 } else { 0 }) + 1
    });
    let nvswitch_resource_count = if template.gpu_peer_fabric == Some(GpuPeerFabricKind::NvSwitch) {
        u32::from(template.hosts)
    } else { 0 };
    let storage_resource_count = template.storage.as_ref().map_or(0, |storage| {
        if storage.kind == StorageFabricKind::DirectNvme {
            u32::from(template.hosts) * u32::from(storage.drives_per_host)
        } else { 2 }
    });
    let cxl_resource_count = template.cxl_memory.as_ref().map_or(0, |cxl| {
        u32::from(template.hosts) * u32::from(cxl.devices_per_host)
    });
    if u32::from(template.hosts) * (u32::from(template.gpus_per_host) + 3)
        + nvswitch_resource_count + network_resource_count
        + storage_resource_count + cxl_resource_count > 10_000 {
        issues.push("Expanded GPU cluster including peer, network, storage and CXL resources may contain at most 10000 resources".into());
    }
    if template.gpu_peer_fabric == Some(GpuPeerFabricKind::NvLinkFullMesh) {
        let per_host_links = u32::from(template.gpus_per_host) * u32::from(template.gpus_per_host.saturating_sub(1)) / 2;
        if u32::from(template.hosts) * per_host_links > 20_000 {
            issues.push("GPU peer fabric may contain at most 20000 links".into());
        }
    }
    if template.numa_nodes_per_host == 0 || template.numa_nodes_per_host > 8 { issues.push("NUMA nodes per host must be between 1 and 8".into()); }
    if ![1, 2, 4, 8, 16, 32].contains(&template.root_complex_lanes)
        || ![1, 2, 4, 8, 16, 32].contains(&template.switch_uplink_lanes)
        || ![1, 2, 4, 8, 16, 32].contains(&template.gpu_lanes) {
        issues.push("PCIe lane widths must be one of 1, 2, 4, 8, 16 or 32".into());
    }
    if !(1..=7).contains(&template.pcie_generation) {
        issues.push("PCIe generation must be between 1 and 7".into());
    }
    if template.gpu_memory_gib == 0 { issues.push("GPU memory must be greater than zero".into()); }
    if template.switch_downstream_ports < template.gpus_per_host {
        issues.push("PCIe switch downstream port count is smaller than GPUs per host".into());
    }
    if template.switch_uplink_lanes > template.root_complex_lanes {
        issues.push("PCIe switch uplink exceeds root-complex lane capability".into());
    }
    if !issues.is_empty() { return Err(issues); }

    let now = Utc::now();
    let source = "edgefabric.simulation.gpu_cluster_template";
    let mut resources = Vec::new();
    let mut relationships = Vec::new();
    let mut devices = Vec::new();
    let mut links = Vec::new();
    let mut peer_links = Vec::new();
    let mut network_links = Vec::new();
    let mut storage_links = Vec::new();
    let mut cxl_devices = Vec::new();
    let mut cxl_links = Vec::new();

    if let Some(network) = &template.network_fabric {
        if network.nics_per_host == 0 || network.nics_per_host > 16 { issues.push("NICs per host must be between 1 and 16".into()); }
        if network.ports_per_nic == 0 || network.ports_per_nic > 16 { issues.push("Ports per NIC must be between 1 and 16".into()); }
        if network.link_speed_gbps == 0 || network.link_speed_gbps > 10_000 { issues.push("Network link speed must be between 1 and 10000 Gbps".into()); }
        if network.switch_ports < u32::from(template.hosts) * u32::from(network.nics_per_host) { issues.push("Network switch port count is smaller than the total NIC uplinks".into()); }
    }
    if let Some(storage) = &template.storage {
        if storage.link_speed_gbps == 0 || storage.link_speed_gbps > 10_000 {
            issues.push("Storage link speed must be between 1 and 10000 Gbps".into());
        }
        match storage.kind {
            StorageFabricKind::DirectNvme => {
                if storage.drives_per_host == 0 || storage.drives_per_host > 32 {
                    issues.push("Direct NVMe drives per host must be between 1 and 32".into());
                }
                if storage.capacity_gib_per_drive == 0 {
                    issues.push("Direct NVMe drive capacity must be greater than zero".into());
                }
            }
            StorageFabricKind::NvmeOfTcp | StorageFabricKind::NvmeOfRdma | StorageFabricKind::FibreChannel => {
                if storage.shared_array_capacity_gib == 0 {
                    issues.push("Shared storage array capacity must be greater than zero".into());
                }
                if storage.switch_ports < u32::from(template.hosts) {
                    issues.push("Storage fabric switch port count is smaller than host count".into());
                }
            }
        }
    }
    if let Some(cxl) = &template.cxl_memory {
        if cxl.devices_per_host == 0 || cxl.devices_per_host > 16 {
            issues.push("CXL memory devices per host must be between 1 and 16".into());
        }
        if cxl.capacity_gib_per_device == 0 {
            issues.push("CXL memory device capacity must be greater than zero".into());
        }
        if !(1..=3).contains(&cxl.generation) {
            issues.push("CXL generation must be between 1 and 3".into());
        }
        if ![1, 2, 4, 8, 16].contains(&cxl.lane_width) {
            issues.push("CXL lane width must be one of 1, 2, 4, 8 or 16".into());
        }
    }
    let storage_link_count = template.storage.as_ref().map_or(0, |storage| {
        if storage.kind == StorageFabricKind::DirectNvme {
            u32::from(template.hosts) * u32::from(storage.drives_per_host)
        } else { u32::from(template.hosts) * 2 + 1 }
    });
    let cxl_link_count = template.cxl_memory.as_ref().map_or(0, |cxl| {
        u32::from(template.hosts) * u32::from(cxl.devices_per_host)
    });
    if storage_link_count > 20_000 || cxl_link_count > 20_000 {
        issues.push("Expanded storage or CXL topology may contain at most 20000 links".into());
    }
    if !issues.is_empty() { return Err(issues); }

    let network_switch = format!("{}:network-fabric-switch", template.identity);
    if let Some(network) = &template.network_fabric {
        resources.push(DiscoveredResource {
            identity: network_switch.clone(), kind: match network.kind { NetworkFabricKind::Ethernet => "ethernet_switch", NetworkFabricKind::Roce => "roce_switch", NetworkFabricKind::InfiniBand => "infiniband_switch" }.into(),
            attributes: BTreeMap::from([("virtual".into(), Value::Bool(true)), ("ports".into(), Value::from(network.switch_ports)), ("fabric".into(), serde_json::to_value(network.kind).unwrap_or(Value::Null))]),
            source: source.into(), observed_at: now, verified: false,
        });
    }

    let storage_switch = format!("{}:storage-fabric-switch", template.identity);
    let storage_array = format!("{}:storage-array", template.identity);
    if let Some(storage) = &template.storage {
        if storage.kind != StorageFabricKind::DirectNvme {
            let switch_kind = match storage.kind {
                StorageFabricKind::NvmeOfTcp | StorageFabricKind::NvmeOfRdma => "storage_fabric_switch",
                StorageFabricKind::FibreChannel => "fibre_channel_switch",
                StorageFabricKind::DirectNvme => unreachable!(),
            };
            resources.push(DiscoveredResource {
                identity: storage_switch.clone(), kind: switch_kind.into(),
                attributes: BTreeMap::from([
                    ("virtual".into(), Value::Bool(true)),
                    ("ports".into(), Value::from(storage.switch_ports)),
                    ("transport".into(), serde_json::to_value(storage.kind).unwrap_or(Value::Null)),
                ]),
                source: source.into(), observed_at: now, verified: false,
            });
            resources.push(DiscoveredResource {
                identity: storage_array.clone(), kind: "storage_array".into(),
                attributes: BTreeMap::from([
                    ("virtual".into(), Value::Bool(true)),
                    ("capacity_gib".into(), Value::from(storage.shared_array_capacity_gib)),
                    ("transport".into(), serde_json::to_value(storage.kind).unwrap_or(Value::Null)),
                ]),
                source: source.into(), observed_at: now, verified: false,
            });
            relationships.push(ResourceRelationship {
                identity: format!("{}:storage-switch-array", template.identity),
                source_identity: storage_switch.clone(), target_identity: storage_array.clone(),
                kind: "storage_connected_to".into(), attributes: BTreeMap::new(),
                source: source.into(), observed_at: now, verified: false,
            });
            storage_links.push(StorageFabricLink {
                identity: format!("{}:storage-switch-array-link", template.identity),
                source_identity: storage_switch.clone(), target_identity: storage_array.clone(),
                kind: storage.kind, capacity_gib: storage.shared_array_capacity_gib,
                link_speed_gbps: storage.link_speed_gbps,
            });
        }
    }

    for host_index in 0..template.hosts {
        let host = format!("{}:host-{:03}", template.identity, host_index + 1);
        let root = format!("{host}:root-complex");
        let switch = format!("{host}:pcie-switch");
        let peer_switch = format!("{host}:nvswitch");
        let mut host_attributes = BTreeMap::new();
        host_attributes.insert("virtual".into(), Value::Bool(true));
        host_attributes.insert("host_index".into(), Value::from(host_index + 1));
        host_attributes.insert("gpus".into(), Value::from(template.gpus_per_host));
        host_attributes.insert("numa_nodes".into(), Value::from(template.numa_nodes_per_host));
        host_attributes.insert("simulation_seed".into(), Value::from(template.seed));
        resources.push(DiscoveredResource {
            identity: host.clone(), kind: "host".into(), attributes: host_attributes,
            source: source.into(), observed_at: now, verified: false,
        });
        if let Some(storage) = &template.storage {
            if storage.kind == StorageFabricKind::DirectNvme {
                for drive_index in 0..storage.drives_per_host {
                    let drive = format!("{host}:nvme-{:02}", drive_index + 1);
                    resources.push(DiscoveredResource {
                        identity: drive.clone(), kind: "nvme_ssd".into(),
                        attributes: BTreeMap::from([
                            ("virtual".into(), Value::Bool(true)),
                            ("capacity_gib".into(), Value::from(storage.capacity_gib_per_drive)),
                            ("transport".into(), serde_json::to_value(storage.kind).unwrap_or(Value::Null)),
                        ]),
                        source: source.into(), observed_at: now, verified: false,
                    });
                    relationships.push(ResourceRelationship {
                        identity: format!("{host}:contains-nvme:{drive_index:02}"),
                        source_identity: host.clone(), target_identity: drive.clone(),
                        kind: "contains".into(), attributes: BTreeMap::new(),
                        source: source.into(), observed_at: now, verified: false,
                    });
                    storage_links.push(StorageFabricLink {
                        identity: format!("{host}:direct-nvme-link:{drive_index:02}"),
                        source_identity: host.clone(), target_identity: drive,
                        kind: storage.kind, capacity_gib: storage.capacity_gib_per_drive,
                        link_speed_gbps: storage.link_speed_gbps,
                    });
                }
            } else {
                storage_links.push(StorageFabricLink {
                    identity: format!("{host}:storage-uplink"),
                    source_identity: host.clone(), target_identity: storage_switch.clone(),
                    kind: storage.kind, capacity_gib: storage.shared_array_capacity_gib,
                    link_speed_gbps: storage.link_speed_gbps,
                });
                relationships.push(ResourceRelationship {
                    identity: format!("{host}:storage-switch-edge"),
                    source_identity: host.clone(), target_identity: storage_switch.clone(),
                    kind: "storage_connected_to".into(), attributes: BTreeMap::new(),
                    source: source.into(), observed_at: now, verified: false,
                });
            }
        }
        if let Some(cxl) = &template.cxl_memory {
            for device_index in 0..cxl.devices_per_host {
                let memory = format!("{host}:cxl-memory-{:02}", device_index + 1);
                let numa = u16::from(device_index) % u16::from(template.numa_nodes_per_host);
                resources.push(DiscoveredResource {
                    identity: memory.clone(), kind: "cxl_memory_expander".into(),
                    attributes: BTreeMap::from([
                        ("virtual".into(), Value::Bool(true)),
                        ("capacity_gib".into(), Value::from(cxl.capacity_gib_per_device)),
                        ("generation".into(), Value::from(cxl.generation)),
                        ("lane_width".into(), Value::from(cxl.lane_width)),
                        ("numa_node".into(), Value::from(numa)),
                    ]),
                    source: source.into(), observed_at: now, verified: false,
                });
                relationships.push(ResourceRelationship {
                    identity: format!("{host}:contains-cxl-memory:{device_index:02}"),
                    source_identity: host.clone(), target_identity: memory.clone(),
                    kind: "memory_attached_to".into(), attributes: BTreeMap::new(),
                    source: source.into(), observed_at: now, verified: false,
                });
                cxl_devices.push(CxlMemoryDevice {
                    resource_identity: memory.clone(),
                    capacity_gib: cxl.capacity_gib_per_device,
                    numa_node: Some(numa),
                });
                cxl_links.push(CxlMemoryLink {
                    identity: format!("{host}:cxl-link:{device_index:02}"),
                    host_identity: host.clone(), memory_device_identity: memory,
                    generation: cxl.generation, lane_width: cxl.lane_width,
                });
            }
        }
        if let Some(network) = &template.network_fabric {
            for nic_index in 0..network.nics_per_host {
                let nic = format!("{host}:nic-{:02}", nic_index + 1);
                resources.push(DiscoveredResource {
                    identity: nic.clone(), kind: "network_adapter".into(),
                    attributes: BTreeMap::from([
                        ("virtual".into(), Value::Bool(true)),
                        ("ports".into(), Value::from(network.ports_per_nic)),
                        ("speed_gbps".into(), Value::from(network.link_speed_gbps)),
                        ("fabric".into(), serde_json::to_value(network.kind).unwrap_or(Value::Null)),
                    ]),
                    source: source.into(), observed_at: now, verified: false,
                });
                relationships.push(ResourceRelationship {
                    identity: format!("{host}:contains:{nic_index}:nic"),
                    source_identity: host.clone(), target_identity: nic.clone(),
                    kind: "contains".into(), attributes: BTreeMap::new(),
                    source: source.into(), observed_at: now, verified: false,
                });
                let endpoint = if network.dpu_per_host {
                    let dpu = format!("{host}:dpu");
                    if !resources.iter().any(|r| r.identity == dpu) {
                        resources.push(DiscoveredResource {
                            identity: dpu.clone(), kind: "dpu".into(),
                            attributes: BTreeMap::from([("virtual".into(), Value::Bool(true))]),
                            source: source.into(), observed_at: now, verified: false,
                        });
                        relationships.push(ResourceRelationship {
                            identity: format!("{host}:contains:dpu"), source_identity: host.clone(),
                            target_identity: dpu.clone(), kind: "contains".into(),
                            attributes: BTreeMap::new(), source: source.into(), observed_at: now, verified: false,
                        });
                    }
                    network_links.push(NetworkFabricLink {
                        identity: format!("{host}:nic-dpu:{nic_index:02}"),
                        source_identity: nic.clone(), target_identity: dpu.clone(),
                        speed_gbps: network.link_speed_gbps, fabric: network.kind,
                    });
                    relationships.push(ResourceRelationship {
                        identity: format!("{host}:nic-dpu-edge:{nic_index:02}"),
                        source_identity: nic, target_identity: dpu.clone(),
                        kind: "network_attached_to".into(), attributes: BTreeMap::new(),
                        source: source.into(), observed_at: now, verified: false,
                    });
                    dpu
                } else { nic.clone() };
                network_links.push(NetworkFabricLink {
                    identity: format!("{host}:fabric-uplink:{nic_index:02}"),
                    source_identity: endpoint.clone(), target_identity: network_switch.clone(),
                    speed_gbps: network.link_speed_gbps, fabric: network.kind,
                });
                relationships.push(ResourceRelationship {
                    identity: format!("{host}:fabric-uplink-edge:{nic_index:02}"),
                    source_identity: endpoint, target_identity: network_switch.clone(),
                    kind: "network_connected_to".into(), attributes: BTreeMap::new(),
                    source: source.into(), observed_at: now, verified: false,
                });
            }
        }
        resources.push(DiscoveredResource {
            identity: root.clone(), kind: "pcie_root_complex".into(),
            attributes: BTreeMap::from([("virtual".into(), Value::Bool(true))]),
            source: source.into(), observed_at: now, verified: false,
        });
        resources.push(DiscoveredResource {
            identity: switch.clone(), kind: "pcie_switch".into(),
            attributes: BTreeMap::from([
                ("virtual".into(), Value::Bool(true)),
                ("downstream_ports".into(), Value::from(template.switch_downstream_ports)),
                ("uplink_lanes".into(), Value::from(template.switch_uplink_lanes)),
                ("downstream_lane_budget".into(), Value::from(u32::from(template.gpu_lanes) * u32::from(template.gpus_per_host))),
                ("pcie_lane_oversubscription_ratio".into(), Value::from(
                    (f64::from(template.gpu_lanes) * f64::from(template.gpus_per_host))
                        / f64::from(template.switch_uplink_lanes)
                )),
            ]),
            source: source.into(), observed_at: now, verified: false,
        });
        relationships.push(ResourceRelationship {
            identity: format!("{host}:contains-root"), source_identity: host.clone(),
            target_identity: root.clone(), kind: "contains".into(), attributes: BTreeMap::new(),
            source: source.into(), observed_at: now, verified: false,
        });
        relationships.push(ResourceRelationship {
            identity: format!("{host}:contains-switch"), source_identity: host.clone(),
            target_identity: switch.clone(), kind: "contains".into(), attributes: BTreeMap::new(),
            source: source.into(), observed_at: now, verified: false,
        });
        devices.push(PcieDevice {
            resource_identity: root.clone(), role: PcieDeviceRole::RootComplex,
            max_generation: template.pcie_generation, max_lane_width: template.root_complex_lanes,
            numa_node: Some(0),
        });
        devices.push(PcieDevice {
            resource_identity: switch.clone(), role: PcieDeviceRole::Switch,
            max_generation: template.pcie_generation, max_lane_width: 32, numa_node: Some(0),
        });
        links.push(PcieLink {
            identity: format!("{host}:uplink"), upstream_identity: root.clone(),
            downstream_identity: switch.clone(), generation: template.pcie_generation,
            lane_width: template.switch_uplink_lanes,
        });

        if template.gpu_peer_fabric == Some(GpuPeerFabricKind::NvSwitch) {
            resources.push(DiscoveredResource {
                identity: peer_switch.clone(), kind: "nvswitch".into(),
                attributes: BTreeMap::from([("virtual".into(), Value::Bool(true))]),
                source: source.into(), observed_at: now, verified: false,
            });
            relationships.push(ResourceRelationship {
                identity: format!("{host}:contains-nvswitch"), source_identity: host.clone(),
                target_identity: peer_switch.clone(), kind: "contains".into(),
                attributes: BTreeMap::new(), source: source.into(), observed_at: now, verified: false,
            });
        }
        let mut host_gpus = Vec::new();
        for gpu_index in 0..template.gpus_per_host {
            let gpu = format!("{host}:gpu-{:02}", gpu_index + 1);
            let numa = gpu_index % u16::from(template.numa_nodes_per_host);
            host_gpus.push(gpu.clone());
            resources.push(DiscoveredResource {
                identity: gpu.clone(), kind: "gpu".into(),
                attributes: BTreeMap::from([
                    ("virtual".into(), Value::Bool(true)),
                    ("memory_gib".into(), Value::from(template.gpu_memory_gib)),
                    ("numa_node".into(), Value::from(numa)),
                    ("pcie_generation".into(), Value::from(template.pcie_generation)),
                    ("pcie_lanes".into(), Value::from(template.gpu_lanes)),
                ]),
                source: source.into(), observed_at: now, verified: false,
            });
            relationships.push(ResourceRelationship {
                identity: format!("{switch}:connects:{gpu_index:02}"),
                source_identity: switch.clone(), target_identity: gpu.clone(),
                kind: "connected_to".into(), attributes: BTreeMap::new(),
                source: source.into(), observed_at: now, verified: false,
            });
            devices.push(PcieDevice {
                resource_identity: gpu.clone(), role: PcieDeviceRole::Endpoint,
                max_generation: template.pcie_generation, max_lane_width: template.gpu_lanes,
                numa_node: Some(numa),
            });
            links.push(PcieLink {
                identity: format!("{host}:gpu-link-{:02}", gpu_index + 1),
                upstream_identity: switch.clone(), downstream_identity: gpu.clone(),
                generation: template.pcie_generation, lane_width: template.gpu_lanes,
            });
            match template.gpu_peer_fabric {
                Some(GpuPeerFabricKind::NvSwitch) => {
                    peer_links.push(GpuPeerLink {
                        identity: format!("{host}:nvswitch-link-{:02}", gpu_index + 1),
                        source_identity: gpu.clone(), target_identity: peer_switch.clone(),
                    });
                    relationships.push(ResourceRelationship {
                        identity: format!("{host}:peer-connects-nvswitch:{gpu_index:02}"),
                        source_identity: gpu, target_identity: peer_switch.clone(),
                        kind: "gpu_peer_connected_to".into(), attributes: BTreeMap::new(),
                        source: source.into(), observed_at: now, verified: false,
                    });
                }
                Some(GpuPeerFabricKind::NvLinkFullMesh) | None => {}
            }
        }
        if template.gpu_peer_fabric == Some(GpuPeerFabricKind::NvLinkFullMesh) {
            for left in 0..host_gpus.len() {
                for right in (left + 1)..host_gpus.len() {
                    let a = &host_gpus[left];
                    let b = &host_gpus[right];
                    peer_links.push(GpuPeerLink {
                        identity: format!("{host}:nvlink:{left:02}:{right:02}"),
                        source_identity: a.clone(), target_identity: b.clone(),
                    });
                    relationships.push(ResourceRelationship {
                        identity: format!("{host}:nvlink-edge:{left:02}:{right:02}"),
                        source_identity: a.clone(), target_identity: b.clone(),
                        kind: "gpu_peer_connected_to".into(), attributes: BTreeMap::new(),
                        source: source.into(), observed_at: now, verified: false,
                    });
                }
            }
        }
    }

    if relationships.len() > 20_000 {
        return Err(vec!["Expanded GPU cluster may contain at most 20000 graph relationships".into()]);
    }
    if network_links.len() > 20_000 {
        return Err(vec!["Network fabric may contain at most 20000 links".into()]);
    }
    if storage_links.len() > 20_000 || cxl_links.len() > 20_000 {
        return Err(vec!["Storage and CXL topologies may contain at most 20000 links each".into()]);
    }

    let graph = InfrastructureGraph {
        observed_at: now, coverage_complete: true, resources, relationships,
    };
    let pcie_topology = PcieTopology { devices, links };
    let instance = SimulationInstance {
        identity: template.identity.clone(),
        kind: SimulationKind::TopologySimulation,
        scenario_id: template.scenario_id.clone(),
        engine: "edgefabric.gpu_cluster_template".into(),
        engine_version: "1.0.0".into(),
        seed: template.seed,
        created_at: now,
        source_manifest_revision: None,
        graph,
        pcie_topology: Some(pcie_topology),
        gpu_peer_fabric: template.gpu_peer_fabric.map(|kind| GpuPeerFabricTopology { kind, links: peer_links }),
        network_fabric: template.network_fabric.as_ref().map(|network| NetworkFabricTopology { kind: network.kind, links: network_links }),
        storage_topology: template.storage.as_ref().map(|_| StorageTopology { links: storage_links }),
        cxl_topology: template.cxl_memory.as_ref().map(|_| CxlTopology { devices: cxl_devices, links: cxl_links }),
        geometry: BTreeMap::new(),
        poses: BTreeMap::new(),
        parameters: BTreeMap::from([
            ("template".into(), serde_json::to_value(template).unwrap_or(Value::Null)),
            ("deterministic_topology".into(), Value::Bool(true)),
            ("simulation_only".into(), Value::Bool(true)),
        ]),
    };
    let validation = validate_simulation(&instance);
    if validation.is_empty() { Ok(instance) } else { Err(validation) }
}

/// Position and orientation in a named coordinate frame. Translation units
/// are defined by the referenced scene/model; rotation is a normalized quaternion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpatialPose {
    pub coordinate_frame: String,
    pub translation: [f64; 3],
    pub rotation_xyzw: [f64; 4],
    pub scale: [f64; 3],
}

/// Immutable reference to a geometry asset; geometry bytes live in the existing
/// artifact/object storage layer, not in the infrastructure graph row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeometryAsset {
    pub asset_uri: String,
    pub sha256: String,
    /// Examples: glb, gltf, obj, stl, step, voxel.
    pub format: String,
    pub units: String,
}

/// A reproducible simulation scenario or virtual hardware instance.
/// Its outputs are synthetic observations and never physical discovery facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationInstance {
    pub identity: String,
    pub kind: SimulationKind,
    pub scenario_id: String,
    pub engine: String,
    pub engine_version: String,
    pub seed: u64,
    pub created_at: DateTime<Utc>,
    pub source_manifest_revision: Option<String>,
    pub graph: InfrastructureGraph,
    #[serde(default)]
    pub pcie_topology: Option<PcieTopology>,
    #[serde(default)]
    pub gpu_peer_fabric: Option<GpuPeerFabricTopology>,
    #[serde(default)]
    pub network_fabric: Option<NetworkFabricTopology>,
    #[serde(default)]
    pub storage_topology: Option<StorageTopology>,
    #[serde(default)]
    pub cxl_topology: Option<CxlTopology>,
    #[serde(default)]
    pub geometry: BTreeMap<String, GeometryAsset>,
    #[serde(default)]
    pub poses: BTreeMap<String, SpatialPose>,
    #[serde(default)]
    pub parameters: BTreeMap<String, Value>,
}

/// Validate the simulation envelope and its spatial metadata. This does not
/// execute a simulator or claim physical equivalence with real hardware.
pub fn validate_simulation(instance: &SimulationInstance) -> Vec<String> {
    let mut issues = Vec::new();
    if instance.identity.trim().is_empty() { issues.push("Simulation identity must not be empty".into()); }
    if instance.scenario_id.trim().is_empty() { issues.push("Simulation scenario_id must not be empty".into()); }
    if instance.engine.trim().is_empty() || instance.engine_version.trim().is_empty() {
        issues.push("Simulation engine and engine_version are required".into());
    }
    for (identity, pose) in &instance.poses {
        if !instance.graph.resources.iter().any(|resource| resource.identity == *identity) {
            issues.push(format!("Spatial pose references unknown resource '{identity}'"));
        }
        if pose.coordinate_frame.trim().is_empty() {
            issues.push(format!("Spatial pose for '{identity}' has no coordinate frame"));
        }
        if pose.translation.iter().any(|value| !value.is_finite())
            || pose.rotation_xyzw.iter().any(|value| !value.is_finite())
            || pose.scale.iter().any(|value| !value.is_finite() || *value <= 0.0) {
            issues.push(format!("Spatial pose for '{identity}' contains invalid transform values"));
        }
        let norm = pose.rotation_xyzw.iter().map(|value| value * value).sum::<f64>().sqrt();
        if !norm.is_finite() || (norm - 1.0).abs() > 1e-3 {
            issues.push(format!("Spatial pose for '{identity}' rotation quaternion must be normalized"));
        }
    }
    for (identity, asset) in &instance.geometry {
        if !instance.graph.resources.iter().any(|resource| resource.identity == *identity) {
            issues.push(format!("Geometry asset references unknown resource '{identity}'"));
        }
        if asset.asset_uri.trim().is_empty() || asset.sha256.trim().is_empty()
            || asset.format.trim().is_empty() || asset.units.trim().is_empty() {
            issues.push(format!("Geometry asset for '{identity}' requires URI, SHA-256, format and units"));
        }
    }
    if let Some(topology) = &instance.pcie_topology {
        issues.extend(validate_pcie_topology(topology, &instance.graph));
    }
    if let Some(fabric) = &instance.network_fabric {
        let resource_ids = instance.graph.resources.iter().map(|r| r.identity.as_str()).collect::<HashSet<_>>();
        let mut link_ids = HashSet::new();
        let mut endpoint_pairs = HashSet::new();
        for link in &fabric.links {
            if link.identity.trim().is_empty() || !link_ids.insert(link.identity.as_str()) {
                issues.push(format!("Network link identity '{}' is empty or duplicated", link.identity));
            }
            if link.speed_gbps == 0 {
                issues.push(format!("Network link '{}' has zero speed", link.identity));
            }
            if link.source_identity == link.target_identity {
                issues.push(format!("Network link '{}' cannot connect a resource to itself", link.identity));
            }
            if !resource_ids.contains(link.source_identity.as_str()) || !resource_ids.contains(link.target_identity.as_str()) {
                issues.push(format!("Network link '{}' references a resource missing from the graph", link.identity));
            }
            let pair = if link.source_identity <= link.target_identity {
                (link.source_identity.as_str(), link.target_identity.as_str())
            } else {
                (link.target_identity.as_str(), link.source_identity.as_str())
            };
            if !endpoint_pairs.insert(pair) {
                issues.push(format!("Network endpoint pair on '{}' is duplicated", link.identity));
            }
            if link.fabric != fabric.kind {
                issues.push(format!("Network link '{}' fabric kind differs from its topology", link.identity));
            }
        }
    }
    if let Some(storage) = &instance.storage_topology {
        let resource_ids = instance.graph.resources.iter().map(|r| r.identity.as_str()).collect::<HashSet<_>>();
        let mut link_ids = HashSet::new();
        let mut endpoint_pairs = HashSet::new();
        for link in &storage.links {
            if link.identity.trim().is_empty() || !link_ids.insert(link.identity.as_str()) {
                issues.push(format!("Storage link identity '{}' is empty or duplicated", link.identity));
            }
            if link.capacity_gib == 0 || link.link_speed_gbps == 0 {
                issues.push(format!("Storage link '{}' requires non-zero capacity and link speed", link.identity));
            }
            if link.source_identity == link.target_identity {
                issues.push(format!("Storage link '{}' cannot connect a resource to itself", link.identity));
            }
            if !resource_ids.contains(link.source_identity.as_str()) || !resource_ids.contains(link.target_identity.as_str()) {
                issues.push(format!("Storage link '{}' references a resource missing from the graph", link.identity));
            }
            let pair = if link.source_identity <= link.target_identity {
                (link.source_identity.as_str(), link.target_identity.as_str())
            } else {
                (link.target_identity.as_str(), link.source_identity.as_str())
            };
            if !endpoint_pairs.insert(pair) {
                issues.push(format!("Storage endpoint pair on '{}' is duplicated", link.identity));
            }
        }
    }
    if let Some(cxl) = &instance.cxl_topology {
        let resource_ids = instance.graph.resources.iter().map(|r| r.identity.as_str()).collect::<HashSet<_>>();
        let mut devices = HashSet::new();
        for device in &cxl.devices {
            if device.resource_identity.trim().is_empty() || !devices.insert(device.resource_identity.as_str()) {
                issues.push(format!("CXL memory device identity '{}' is empty or duplicated", device.resource_identity));
            }
            if device.capacity_gib == 0 {
                issues.push(format!("CXL memory device '{}' has zero capacity", device.resource_identity));
            }
            if !resource_ids.contains(device.resource_identity.as_str()) {
                issues.push(format!("CXL memory device '{}' is missing from the resource graph", device.resource_identity));
            }
        }
        let mut link_ids = HashSet::new();
        let mut endpoint_pairs = HashSet::new();
        for link in &cxl.links {
            if link.identity.trim().is_empty() || !link_ids.insert(link.identity.as_str()) {
                issues.push(format!("CXL link identity '{}' is empty or duplicated", link.identity));
            }
            if link.host_identity == link.memory_device_identity {
                issues.push(format!("CXL link '{}' cannot connect a resource to itself", link.identity));
            }
            if !(1..=3).contains(&link.generation) || ![1, 2, 4, 8, 16].contains(&link.lane_width) {
                issues.push(format!("CXL link '{}' has an unsupported generation or lane width", link.identity));
            }
            if !resource_ids.contains(link.host_identity.as_str()) || !devices.contains(link.memory_device_identity.as_str()) {
                issues.push(format!("CXL link '{}' must connect a graph host to a declared CXL memory device", link.identity));
            }
            let pair = (link.host_identity.as_str(), link.memory_device_identity.as_str());
            if !endpoint_pairs.insert(pair) {
                issues.push(format!("CXL endpoint pair on '{}' is duplicated", link.identity));
            }
        }
    }
    if let Some(fabric) = &instance.gpu_peer_fabric {
        let resource_ids = instance.graph.resources.iter().map(|r| r.identity.as_str()).collect::<HashSet<_>>();
        let mut link_ids = HashSet::new();
        let mut endpoint_pairs = HashSet::new();
        for link in &fabric.links {
            if link.identity.trim().is_empty() || !link_ids.insert(link.identity.as_str()) {
                issues.push(format!("GPU peer link identity '{}' is empty or duplicated", link.identity));
            }
            if link.source_identity == link.target_identity {
                issues.push(format!("GPU peer link '{}' cannot connect a resource to itself", link.identity));
            }
            if !resource_ids.contains(link.source_identity.as_str()) || !resource_ids.contains(link.target_identity.as_str()) {
                issues.push(format!("GPU peer link '{}' references a resource missing from the graph", link.identity));
            }
            let pair = if link.source_identity <= link.target_identity {
                (link.source_identity.as_str(), link.target_identity.as_str())
            } else {
                (link.target_identity.as_str(), link.source_identity.as_str())
            };
            if !endpoint_pairs.insert(pair) {
                issues.push(format!("GPU peer endpoint pair on '{}' is duplicated", link.identity));
            }
        }
    }
    for issue in validate_graph(&instance.graph) {
        issues.push(format!("Graph relationship '{}': {}", issue.relationship_identity, issue.detail));
    }
    issues
}

/// A provenance-aware resource graph assembled from bounded source snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfrastructureGraph {
    pub observed_at: DateTime<Utc>,
    pub coverage_complete: bool,
    pub resources: Vec<DiscoveredResource>,
    #[serde(default)]
    pub relationships: Vec<ResourceRelationship>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphValidationIssue {
    pub relationship_identity: String,
    pub detail: String,
}

/// Check graph edge identities and endpoint references without mutating data.
/// Multiple resources with the same identity are treated as ambiguous endpoints.
pub fn validate_graph(graph: &InfrastructureGraph) -> Vec<GraphValidationIssue> {
    let mut resource_counts: HashMap<&str, usize> = HashMap::new();
    for resource in &graph.resources {
        *resource_counts.entry(resource.identity.as_str()).or_default() += 1;
    }

    let mut edge_ids = HashSet::new();
    let mut issues = Vec::new();
    for relationship in &graph.relationships {
        if relationship.identity.trim().is_empty() {
            issues.push(GraphValidationIssue {
                relationship_identity: relationship.identity.clone(),
                detail: "Relationship identity must not be empty".into(),
            });
        } else if !edge_ids.insert(relationship.identity.as_str()) {
            issues.push(GraphValidationIssue {
                relationship_identity: relationship.identity.clone(),
                detail: "Duplicate relationship identity".into(),
            });
        }
        if relationship.kind.trim().is_empty() {
            issues.push(GraphValidationIssue {
                relationship_identity: relationship.identity.clone(),
                detail: "Relationship kind must not be empty".into(),
            });
        }
        for endpoint in [relationship.source_identity.as_str(), relationship.target_identity.as_str()] {
            match resource_counts.get(endpoint).copied().unwrap_or(0) {
                0 => issues.push(GraphValidationIssue {
                    relationship_identity: relationship.identity.clone(),
                    detail: format!("Relationship endpoint '{endpoint}' is not present in this graph snapshot"),
                }),
                1 => {},
                _ => issues.push(GraphValidationIssue {
                    relationship_identity: relationship.identity.clone(),
                    detail: format!("Relationship endpoint '{endpoint}' is ambiguous because its resource identity is duplicated"),
                }),
            }
        }
    }
    issues
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

    fn sample_gpu_template() -> GpuClusterTemplate {
        GpuClusterTemplate {
            identity: "lab:gpu-cluster".into(),
            scenario_id: "rubin-qualification-lab".into(),
            hosts: 2,
            gpus_per_host: 4,
            pcie_generation: 6,
            root_complex_lanes: 32,
            switch_uplink_lanes: 16,
            gpu_lanes: 16,
            switch_downstream_ports: 8,
            gpu_memory_gib: 288,
            numa_nodes_per_host: 2,
            gpu_peer_fabric: None,
            network_fabric: None,
            storage: None,
            cxl_memory: None,
            seed: 42,
        }
    }

    #[test]
    fn gpu_cluster_template_expands_deterministic_topology() {
        let template = sample_gpu_template();
        let first = instantiate_gpu_cluster_template(&template).unwrap();
        let second = instantiate_gpu_cluster_template(&template).unwrap();
        assert_eq!(first.graph.resources.len(), 14);
        assert_eq!(first.graph.relationships.len(), 12);
        assert_eq!(first.pcie_topology.as_ref().unwrap().devices.len(), 12);
        assert_eq!(first.pcie_topology.as_ref().unwrap().links.len(), 10);
        let switch = first.graph.resources.iter().find(|resource| resource.kind == "pcie_switch").unwrap();
        assert_eq!(switch.attributes.get("pcie_lane_oversubscription_ratio"), Some(&json!(4.0)));
        let identities = |instance: &SimulationInstance| {
            instance.graph.resources.iter().map(|resource| resource.identity.clone()).collect::<Vec<_>>()
        };
        assert_eq!(identities(&first), identities(&second));
        assert_eq!(first.pcie_topology, second.pcie_topology);
        assert!(first.graph.resources.iter().all(|resource| !resource.verified));
        assert!(validate_simulation(&first).is_empty());
    }

    #[test]
    fn gpu_cluster_template_builds_nvlink_full_mesh_separately_from_pcie() {
        let mut template = sample_gpu_template();
        template.gpu_peer_fabric = Some(GpuPeerFabricKind::NvLinkFullMesh);
        let instance = instantiate_gpu_cluster_template(&template).unwrap();
        let fabric = instance.gpu_peer_fabric.as_ref().unwrap();
        assert_eq!(fabric.kind, GpuPeerFabricKind::NvLinkFullMesh);
        assert_eq!(fabric.links.len(), 12); // 4 GPUs choose 2, for each of 2 hosts.
        assert_eq!(instance.pcie_topology.as_ref().unwrap().links.len(), 10);
        assert!(fabric.links.iter().all(|link| link.source_identity.contains(":gpu-")
            && link.target_identity.contains(":gpu-")));
        assert!(validate_simulation(&instance).is_empty());
    }

    #[test]
    fn gpu_cluster_template_builds_nvswitch_domains_separately_from_pcie() {
        let mut template = sample_gpu_template();
        template.gpu_peer_fabric = Some(GpuPeerFabricKind::NvSwitch);
        let instance = instantiate_gpu_cluster_template(&template).unwrap();
        let fabric = instance.gpu_peer_fabric.as_ref().unwrap();
        assert_eq!(fabric.kind, GpuPeerFabricKind::NvSwitch);
        assert_eq!(fabric.links.len(), 8);
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "nvswitch").count(), 2);
        assert_eq!(instance.pcie_topology.as_ref().unwrap().links.len(), 10);
        assert!(fabric.links.iter().all(|link| link.target_identity.ends_with(":nvswitch")));
        assert!(validate_simulation(&instance).is_empty());
    }

    #[test]
    fn gpu_cluster_template_models_roce_nics_dpus_and_fabric_paths() {
        let mut template = sample_gpu_template();
        template.network_fabric = Some(NetworkFabricTemplate {
            kind: NetworkFabricKind::Roce,
            nics_per_host: 2,
            ports_per_nic: 2,
            link_speed_gbps: 400,
            switch_ports: 4,
            dpu_per_host: true,
        });
        let instance = instantiate_gpu_cluster_template(&template).unwrap();
        let fabric = instance.network_fabric.as_ref().unwrap();
        assert_eq!(fabric.kind, NetworkFabricKind::Roce);
        assert_eq!(fabric.links.len(), 8);
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "network_adapter").count(), 4);
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "dpu").count(), 2);
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "roce_switch").count(), 1);
        assert!(fabric.links.iter().all(|link| link.speed_gbps == 400));
        assert!(validate_simulation(&instance).is_empty());
    }

    #[test]
    fn gpu_cluster_template_rejects_insufficient_network_switch_ports() {
        let mut template = sample_gpu_template();
        template.network_fabric = Some(NetworkFabricTemplate {
            kind: NetworkFabricKind::InfiniBand,
            nics_per_host: 2,
            ports_per_nic: 1,
            link_speed_gbps: 800,
            switch_ports: 3,
            dpu_per_host: false,
        });
        let issues = instantiate_gpu_cluster_template(&template).unwrap_err();
        assert!(issues.iter().any(|issue| issue.contains("network switch port count")));
    }

    #[test]
    fn gpu_cluster_template_expands_direct_nvme_and_per_host_cxl_memory() {
        let mut template = sample_gpu_template();
        template.storage = Some(StorageFabricTemplate {
            kind: StorageFabricKind::DirectNvme,
            drives_per_host: 2,
            capacity_gib_per_drive: 7680,
            shared_array_capacity_gib: 0,
            link_speed_gbps: 64,
            switch_ports: 0,
        });
        template.cxl_memory = Some(CxlMemoryTemplate {
            devices_per_host: 2,
            capacity_gib_per_device: 1024,
            generation: 3,
            lane_width: 8,
        });
        let instance = instantiate_gpu_cluster_template(&template).unwrap();
        let storage = instance.storage_topology.as_ref().unwrap();
        let cxl = instance.cxl_topology.as_ref().unwrap();
        assert_eq!(storage.links.len(), 4);
        assert!(storage.links.iter().all(|link| link.kind == StorageFabricKind::DirectNvme
            && link.capacity_gib == 7680 && link.link_speed_gbps == 64));
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "nvme_ssd").count(), 4);
        assert_eq!(cxl.devices.len(), 4);
        assert_eq!(cxl.links.len(), 4);
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "cxl_memory_expander").count(), 4);
        assert_eq!(cxl.devices.iter().map(|device| device.numa_node).collect::<Vec<_>>(),
            vec![Some(0), Some(1), Some(0), Some(1)]);
        assert!(validate_simulation(&instance).is_empty());
    }

    #[test]
    fn gpu_cluster_template_expands_shared_nvmeof_rdma_array() {
        let mut template = sample_gpu_template();
        template.storage = Some(StorageFabricTemplate {
            kind: StorageFabricKind::NvmeOfRdma,
            drives_per_host: 0,
            capacity_gib_per_drive: 0,
            shared_array_capacity_gib: 1_048_576,
            link_speed_gbps: 400,
            switch_ports: 2,
        });
        let instance = instantiate_gpu_cluster_template(&template).unwrap();
        let storage = instance.storage_topology.as_ref().unwrap();
        assert_eq!(storage.links.len(), 3);
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "storage_array").count(), 1);
        assert_eq!(instance.graph.resources.iter().filter(|r| r.kind == "storage_fabric_switch").count(), 1);
        assert_eq!(storage.links.iter().filter(|link| link.source_identity.contains(":host-")).count(), 2);
        assert!(storage.links.iter().all(|link| link.kind == StorageFabricKind::NvmeOfRdma
            && link.capacity_gib == 1_048_576 && link.link_speed_gbps == 400));
        assert!(validate_simulation(&instance).is_empty());
    }

    #[test]
    fn gpu_cluster_template_rejects_invalid_cxl_configuration() {
        let mut template = sample_gpu_template();
        template.cxl_memory = Some(CxlMemoryTemplate {
            devices_per_host: 1,
            capacity_gib_per_device: 512,
            generation: 4,
            lane_width: 32,
        });
        let issues = instantiate_gpu_cluster_template(&template).unwrap_err();
        assert!(issues.iter().any(|issue| issue.contains("CXL generation")));
        assert!(issues.iter().any(|issue| issue.contains("CXL lane width")));
    }

    #[test]
    fn gpu_cluster_template_rejects_insufficient_switch_ports() {
        let mut template = sample_gpu_template();
        template.switch_downstream_ports = 2;
        let issues = instantiate_gpu_cluster_template(&template).unwrap_err();
        assert!(issues.iter().any(|issue| issue.contains("downstream port count")));
    }

    #[test]
    fn pcie_topology_rejects_links_above_endpoint_capabilities() {
        let graph = InfrastructureGraph {
            observed_at: Utc::now(),
            coverage_complete: false,
            resources: vec![discovered("root-1"), discovered("gpu-1")],
            relationships: vec![],
        };
        let topology = PcieTopology {
            devices: vec![
                PcieDevice {
                    resource_identity: "root-1".into(),
                    role: PcieDeviceRole::RootComplex,
                    max_generation: 4,
                    max_lane_width: 16,
                    numa_node: Some(0),
                },
                PcieDevice {
                    resource_identity: "gpu-1".into(),
                    role: PcieDeviceRole::Endpoint,
                    max_generation: 4,
                    max_lane_width: 16,
                    numa_node: Some(0),
                },
            ],
            links: vec![PcieLink {
                identity: "pcie-root-gpu".into(),
                upstream_identity: "root-1".into(),
                downstream_identity: "gpu-1".into(),
                generation: 5,
                lane_width: 32,
            }],
        };
        let issues = validate_pcie_topology(&topology, &graph);
        assert!(issues.iter().any(|issue| issue.contains("generation capability")));
        assert!(issues.iter().any(|issue| issue.contains("lane-width capability")));
    }

    #[test]
    fn simulation_validates_storage_and_cxl_as_distinct_topologies() {
        let mut instance = SimulationInstance {
            identity: "sim-storage-cxl".into(),
            kind: SimulationKind::TopologySimulation,
            scenario_id: "storage-cxl-lab".into(),
            engine: "fixture".into(),
            engine_version: "1".into(),
            seed: 7,
            created_at: Utc::now(),
            source_manifest_revision: None,
            graph: InfrastructureGraph {
                observed_at: Utc::now(),
                coverage_complete: true,
                resources: vec![
                    discovered("host-1"),
                    discovered("nvme-array-1"),
                    discovered("cxl-expander-1"),
                ],
                relationships: vec![],
            },
            pcie_topology: None,
            gpu_peer_fabric: None,
            network_fabric: None,
            storage_topology: Some(StorageTopology {
                links: vec![StorageFabricLink {
                    identity: "nvmeof-path-1".into(),
                    source_identity: "host-1".into(),
                    target_identity: "nvme-array-1".into(),
                    kind: StorageFabricKind::NvmeOfRdma,
                    capacity_gib: 1024 * 1024,
                    link_speed_gbps: 400,
                }],
            }),
            cxl_topology: Some(CxlTopology {
                devices: vec![CxlMemoryDevice {
                    resource_identity: "cxl-expander-1".into(),
                    capacity_gib: 2048,
                    numa_node: Some(1),
                }],
                links: vec![CxlMemoryLink {
                    identity: "cxl-host-expander-1".into(),
                    host_identity: "host-1".into(),
                    memory_device_identity: "cxl-expander-1".into(),
                    generation: 3,
                    lane_width: 16,
                }],
            }),
            geometry: BTreeMap::new(),
            poses: BTreeMap::new(),
            parameters: BTreeMap::new(),
        };
        assert!(validate_simulation(&instance).is_empty());

        instance.storage_topology.as_mut().unwrap().links[0].capacity_gib = 0;
        instance.cxl_topology.as_mut().unwrap().links[0].generation = 4;
        let issues = validate_simulation(&instance);
        assert!(issues.iter().any(|issue| issue.contains("non-zero capacity")));
        assert!(issues.iter().any(|issue| issue.contains("unsupported generation")));
    }

    #[test]
    fn simulation_rejects_spatial_metadata_for_unknown_resource() {
        let instance = SimulationInstance {
            identity: "sim-1".into(),
            kind: SimulationKind::SpatialVisualization,
            scenario_id: "rack-layout".into(),
            engine: "fixture".into(),
            engine_version: "1".into(),
            seed: 42,
            created_at: Utc::now(),
            source_manifest_revision: None,
            graph: InfrastructureGraph {
                observed_at: Utc::now(), coverage_complete: false,
                resources: vec![discovered("host-1")], relationships: vec![],
            },
            pcie_topology: None,
            gpu_peer_fabric: None,
            network_fabric: None,
            storage_topology: None,
            cxl_topology: None,
            geometry: BTreeMap::new(),
            poses: BTreeMap::from([("gpu-missing".into(), SpatialPose {
                coordinate_frame: "rack-origin".into(), translation: [0.0, 0.0, 0.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0], scale: [1.0, 1.0, 1.0],
            })]),
            parameters: BTreeMap::new(),
        };
        assert!(validate_simulation(&instance).iter().any(|issue| issue.contains("unknown resource")));
    }

    #[test]
    fn graph_validation_rejects_missing_and_ambiguous_endpoints() {
        let left = discovered("host-1");
        let right = discovered("switch-1");
        let duplicate = discovered("host-1");
        let relationship = ResourceRelationship {
            identity: "edge-1".into(),
            source_identity: "host-1".into(),
            target_identity: "rack-404".into(),
            kind: "connected_to".into(),
            attributes: BTreeMap::new(),
            source: "test".into(),
            observed_at: Utc::now(),
            verified: true,
        };
        let graph = InfrastructureGraph {
            observed_at: Utc::now(),
            coverage_complete: false,
            resources: vec![left.clone(), right],
            relationships: vec![relationship.clone()],
        };
        let issues = validate_graph(&graph);
        assert!(issues.iter().any(|issue| issue.detail.contains("not present")));

        let mut ambiguous = graph;
        ambiguous.resources.push(duplicate);
        ambiguous.relationships[0].target_identity = "switch-1".into();
        let issues = validate_graph(&ambiguous);
        assert!(issues.iter().any(|issue| issue.detail.contains("ambiguous")));
    }

    #[test]
    fn graph_validation_rejects_duplicate_relationship_identity() {
        let edge = ResourceRelationship {
            identity: "edge-1".into(),
            source_identity: "host-1".into(),
            target_identity: "switch-1".into(),
            kind: "connected_to".into(),
            attributes: BTreeMap::new(),
            source: "test".into(),
            observed_at: Utc::now(),
            verified: true,
        };
        let graph = InfrastructureGraph {
            observed_at: Utc::now(),
            coverage_complete: true,
            resources: vec![discovered("host-1"), discovered("switch-1")],
            relationships: vec![edge.clone(), edge],
        };
        let issues = validate_graph(&graph);
        assert!(issues.iter().any(|issue| issue.detail == "Duplicate relationship identity"));
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
