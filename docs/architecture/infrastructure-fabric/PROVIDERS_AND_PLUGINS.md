# Providers and Plugins

## Provider contract

A provider translates EdgeFabric's normalized resource/capability/job model to vendor tools, APIs and hardware. It declares:
- provider ID, version, compatibility and health;
- resource kinds and discovery sources;
- capabilities and schema versions;
- privilege requirements and safety classifications;
- rate limits, concurrency, timeout, cancellation and cleanup semantics;
- evidence emitted and raw-output handling;
- supported OS, firmware, runtime, SDK and device constraints.

Support is never inferred from a vendor name alone. Capabilities are verified at runtime and tied to exact provider/tool versions and hardware context.

## Provider families

| Domain | Examples |
|---|---|
| Generic host | OS inventory, CPU, memory, PCI, NUMA, kernel |
| NVIDIA | NVML, DCGM, CUDA, NCCL, NVLink, DOCA and platform tools |
| AMD | ROCm, ROCm SMI, HIP, rocprof |
| Intel | oneAPI, Level Zero, SYCL and platform tools |
| Apple | macOS inventory, Apple Silicon, Metal and Core ML within supported APIs |
| FPGA | Vendor toolchains, board management, bitstream metadata/programming |
| QPU | Cloud APIs, backend metadata, calibration, queue/job lifecycle |
| Network | Existing EdgeFabric discovery, Ethernet, InfiniBand, RoCE and switch APIs |
| Storage | NVMe, local/distributed storage and performance APIs |
| Power/cooling | Redfish, PDU, UPS, CDU and sensor interfaces |
| Lifecycle manager | Optional external bare-metal orchestration adapters |
| Simulation | Deterministic virtual resources, faults, delays and synthetic telemetry |

Keep the core provider-neutral and open to future resource classes.

## Reuse EdgeFabric network discovery

The network provider consumes EdgeFabric's existing automatic discovery results and provenance. It may add protocol-specific enrichment (for example LLDP, switch APIs or InfiniBand diagnostics) where genuinely absent. It must not create a competing scanner, duplicate network inventory or independently assign conflicting identities.

## Rust and Python boundary

- Use Rust edition 2024 for core domain services, agents, protocol handling, policy and performance-sensitive execution where it fits the workspace.
- Support Python providers/test plugins out of process when vendor SDKs, research libraries or test ecosystems make Python pragmatic.
- Use a versioned typed RPC/job envelope between core and plugin process.
- Constrain plugin CPU, memory, wall time, output size, filesystem, network access and scoped short-lived credentials.
- Validate and normalize plugin output at the boundary.

This is not a blanket Rust-only architecture; language follows the integration boundary.

## External lifecycle managers

Managers such as NVIDIA's NICo may be optional adapters for lifecycle functions they demonstrably support. Do not assume they cover deep qualification, every vendor, Rubin/Rubin Ultra or a specific Lenovo platform without checking current compatibility and release documentation. Keep EdgeFabric's normalized control interface and direct provider paths available.

## Simulation provider

Simulation implements the same provider contract as physical hardware. It supports graph fixtures, manifest validation/reconciliation, missing devices, identity conflicts, drift, telemetry, benchmark fixtures, provider failures, timeouts, cancellation, partial collection and QPU queue behavior. It is first-class in CI and UI demos.
