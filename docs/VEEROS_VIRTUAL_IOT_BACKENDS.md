# VeerOS Virtual IoT Backend Integration

## Purpose

EdgeFabric Virtual IoTs are controller definitions, not backends themselves. A Virtual IoT describes:

- the controller profile such as `esp32-devkit`
- the enabled sensor set
- the simulation cadence
- the graphical representation shown in the designer

The execution backend is a separate VeerOS VM resource. This document defines how EdgeFabric binds those two concepts.

## Design Model

There are now two separate entities:

1. `virtual_iot_configs`

- one row per Virtual IoT device
- stores controller type, enabled sensors, simulation rate
- stores the currently attached backend reference and a cached backend payload

2. `virtual_iot_backends`

- one row per provisioned backend
- currently backed by `veer-vm` (folded `veerm-vm` runtime)
- stores backend provider, compatible controller type, region, VM name, shell host/port, attachment status, and provider-specific config JSON

This separation is intentional:

- one dashboard page provisions and manages backend capacity
- the IoT designer selects from compatible available backends
- backends can be detached and reattached to different Virtual IoTs

## Current Provider

The canonical provider is `veer-vm`.

For backward compatibility, EdgeFabric also accepts these aliases on create requests:

- `veerm-vm`
- `veeros-vm`

Each backend entry maps to a VeerOS-managed VM created by the folded microCM orchestration surface:

- materialize a per-instance fold manifest
- `target/debug/fold spawn --manifest <path>`
- `target/debug/fold list`
- `target/debug/fold logs <name>`
- `target/debug/fold stop <name>`
- `target/debug/fold rm <name> --force`
- `veer-connect shell`

EdgeFabric does not reimplement VM orchestration. It delegates to VeerOS and stores enough metadata to:

- list compatible backends
- attach a backend to a Virtual IoT
- inject sensor values into the attached backend
- open a VeerOS shell session for the attached backend

## Compatibility Rules

Compatibility is controller-first.

A backend is considered attachable when:

- `virtual_iot_backends.controller_type == virtual_iot_configs.controller_type`
- the backend is not attached to another device, or is already attached to the same device
- the region matches the requested region when the UI filters by region

This keeps the first implementation simple and deterministic.

Future compatibility can expand with capabilities such as:

- WiFi/BLE requirements
- memory class
- architecture
- custom firmware feature flags

## Dashboard Flow

### Backend Management Page

The dashboard provides a backend provisioning page that:

- creates VeerOS VM-backed backends
- records backend metadata in EdgeFabric
- shows current controller compatibility, runtime status, VM name, and shell endpoint

### Virtual IoT Designer

The designer now:

- creates or updates the Virtual IoT definition
- lists compatible backends for the selected controller and region
- attaches or detaches a backend from the Virtual IoT
- exposes an `Open VeerOS Shell` action for attached backends

## Shell Access

Shell access is exposed through a WebSocket bridge in EdgeFabric.

Flow:

1. Dashboard opens `GET /api/v1/devices/{device_id}/virtual-iot/shell/ws`
2. API resolves the attached backend
3. API spawns `veer-connect shell <host> <port>`
4. WebSocket text frames are bridged bidirectionally to the child process
5. The dashboard renders a minimal browser terminal view

The shell bridge is intentionally thin. Authentication remains the normal VeerOS shell login:

- `root / toor`
- `user / veeros`

## API Surface

### Backend registry

- `GET /api/v1/devices/virtual-iot/backends`
- `POST /api/v1/devices/virtual-iot/backends`

### Virtual IoT attachment

- `POST /api/v1/devices/{id}/virtual-iot/backend/attach`
- `POST /api/v1/devices/{id}/virtual-iot/backend/detach`

### Runtime control

- `POST /api/v1/devices/{id}/virtual-iot/sensors`
- `GET /api/v1/devices/{id}/virtual-iot/shell`
- `GET /api/v1/devices/{id}/virtual-iot/shell/ws`

## Data Ownership

EdgeFabric owns:

- backend registry rows
- Virtual IoT to backend attachment state
- compatibility filtering
- browser shell bridging
- UI state and operator workflows

VeerOS owns:

- VM provisioning and runtime
- sensor injection semantics inside the VM
- secure shell transport via `veer-connect`
- actual ESP32 VM behavior

## Operational Notes

- The API config contains a `veeros` block so the EdgeFabric control plane knows where the VeerOS checkout and tools live.
- The current implementation assumes the EdgeFabric API host can execute the VeerOS scripts locally.
- If EdgeFabric later runs remotely, this provider should be moved behind an agent or RPC adapter rather than direct local process execution.

## Next Extensions

Natural next steps:

1. Add backend health polling so backend runtime status updates automatically.
2. Add backend destroy and reprovision operations from the dashboard.
3. Add richer compatibility rules based on capabilities, firmware, and transport support.
4. Add session-aware shell audit logging and shell access controls.
5. Replace the minimal browser shell with a proper terminal emulator component.