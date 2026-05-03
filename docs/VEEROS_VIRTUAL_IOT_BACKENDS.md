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
- currently backed by `veeros-vm`
- stores backend provider, compatible controller type, region, VM name, shell host/port, attachment status, and provider-specific config JSON

This separation is intentional:

- one dashboard page provisions and manages backend capacity
- the IoT designer selects from compatible available backends
- backends can be detached and reattached to different Virtual IoTs

## Current Provider

The initial provider is `veeros-vm`.

Each backend entry maps to a VeerOS-managed VM created by the existing VeerOS orchestration surface:

- `scripts/veeros-vm create`
- `scripts/veeros-vm start`
- `scripts/veeros-vm stop`
- `scripts/veeros-vm restart`
- `scripts/veeros-vm sensor inject`
- `veer-connect shell`

EdgeFabric does not reimplement VM orchestration. It delegates to VeerOS and stores enough metadata to:

- list compatible backends
- attach a backend to a Virtual IoT
- inject sensor values into the attached backend
- open a VeerOS shell session for the attached backend

## ESP32-C6 Runtime Modes

ESP32-C6 virtual IoT backends support two runtime modes selected via `net_mode` at create time:

### `net_mode=nat` — QEMU user-net (default)

| | |
|---|---|
| Runtime | `qemu-system-riscv32` |
| Kernel | `target/riscv32imc-unknown-none-elf/{debug,release}/kernel-qemu-esp32c6` |
| Network | QEMU user-net with `hostfwd tcp:0.0.0.0:{port}->:2323` |
| Platforms | Linux, macOS |
| Root required | No |
| Shell access | `veer-connect shell localhost {port}` or `nc localhost {port}` |

Use when you want the widest compatibility or when the `veer-vm` binary is not yet built.

### `net_mode=user` — veer-vm binary (rootless VMM)

| | |
|---|---|
| Runtime | `target/debug/veer-vm` or `target/release/veer-vm` |
| Kernel | same RISC-V ELF as above |
| Network | veer-vm built-in user-net with `hostfwd` on `{port}` |
| Platforms | macOS (Apple Hypervisor framework) · Linux (KVM) |
| Root required | No |
| Shell access | `veer-connect shell localhost {port}` |

Preferred on macOS for production use — no QEMU install required, lighter resource usage. Build `veer-vm` once:

```bash
cargo build -p veer-vm          # debug
cargo build -p veer-vm --release # optimised
```

Then create the backend with `net_mode=user`:

```bash
curl -X POST http://127.0.0.1:7070/veeros/vms \
  -H 'Content-Type: application/json' \
  -d '{"vm_name":"my-esp32","hostname":"my-esp32","target":"esp32c6","net_mode":"user"}'
```

### Choosing a mode

| Situation | Recommended mode |
|---|---|
| Linux CI / remote agent | `nat` (QEMU, always available) |
| macOS, veer-vm already built | `user` (rootless, lighter) |
| macOS, no veer-vm build | `nat` (QEMU via Homebrew) |
| Multiple concurrent VMs | Both modes support concurrent instances with unique auto-assigned ports |

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

### Control Plane (ef-api, port 6060)

| Method | Path | Description |
|--------|------|-------------|
| `GET`  | `/api/v1/devices/virtual-iot/backends` | List all provisioned backends |
| `POST` | `/api/v1/devices/virtual-iot/backends` | Create and start a new VeerOS backend |
| `POST` | `/api/v1/devices/{id}/virtual-iot/backend/attach` | Attach a backend to a Virtual IoT |
| `POST` | `/api/v1/devices/{id}/virtual-iot/backend/detach` | Detach backend from a Virtual IoT |
| `POST` | `/api/v1/devices/{id}/virtual-iot/sensors` | Inject sensor value into attached backend |
| `GET`  | `/api/v1/devices/{id}/virtual-iot/shell` | Shell connection info for attached backend |
| `GET`  | `/api/v1/devices/{id}/virtual-iot/shell/ws` | WebSocket shell bridge to attached backend |

### Agent Streaming Server (ef-agent, port 7070)

The agent exposes a local HTTP/WS API for direct VM management. This server starts
**immediately on agent launch**, independent of control-plane connectivity.

| Method | Path | Description |
|--------|------|-------------|
| `GET`  | `/health` | Agent streaming server health check |
| `GET`  | `/veeros/vms` | List all VMs as JSON array |
| `POST` | `/veeros/vms` | Create a VM (`vm_name`, `hostname`, `target`, `net_mode`, `memory_mb`) |
| `GET`  | `/veeros/vms/{name}` | Get VM runtime status |
| `POST` | `/veeros/vms/{name}/start` | Start VM |
| `POST` | `/veeros/vms/{name}/stop` | Stop VM (graceful) |
| `POST` | `/veeros/vms/{name}/restart` | Restart VM |
| `GET`  | `/veeros/vms/{name}/config` | Get VM configuration key-value pairs |
| `GET`  | `/veeros/vms/{name}/shell_info` | Shell host, port, and connect command |
| `POST` | `/veeros/vms/{name}/sensor` | Inject sensor value (`sensor`, `value`) |
| `DELETE`| `/veeros/vms/{name}` | Destroy VM and remove its data |
| `GET`  | `/ws/veeros/{name}/shell` | WebSocket shell session for VM |

**Example — list running VMs:**
```bash
curl http://127.0.0.1:7070/veeros/vms
```
```json
[
  {
    "name": "my-esp32",
    "target": "esp32c6",
    "hostname": "my-esp32",
    "memory": 128,
    "ssh_port": 2230,
    "net_mode": "nat",
    "status": "running",
    "created": "2026-05-03T10:00:00+05:30",
    "last_started": "2026-05-03T10:01:00+05:30"
  }
]
```

**Example — create and start a VM:**
```bash
curl -X POST http://127.0.0.1:7070/veeros/vms \
  -H 'Content-Type: application/json' \
  -d '{"vm_name":"my-esp32","hostname":"my-esp32","target":"esp32c6","net_mode":"nat"}'

curl -X POST http://127.0.0.1:7070/veeros/vms/my-esp32/start
```

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
- On **macOS**, the agent auto-detects a local VeerOS checkout at `~/VeerOS` when starting via `scripts/deploy.sh agent`. Override with `EF__VEEROS__ROOT_DIR`, `EF__VEEROS__VM_SCRIPT_PATH`, and `EF__VEEROS__VEER_CONNECT_PATH`.
- The agent streaming server (port 7070) now starts **immediately** on agent launch, before the control-plane connection is established. VeerOS VM management and sensor APIs are available even when the control plane is offline.
- The current implementation delegates VM operations to `scripts/veeros-vm` (VeerOS repo). EdgeFabric does not re-implement VM orchestration.
- Multiple backends can run concurrently. Each VM receives a unique host-forwarded shell port (starting from 2230). Port allocation is serialized with a lock to avoid collisions under concurrent creates.
- If a VM start detects a stale process (orphaned qemu or veer-vm) from a previous unclean stop, it kills the orphan automatically before launching a new instance.
- ESP32-C6 VMs support two runtime modes: `net_mode=nat` (bare QEMU, works everywhere) and `net_mode=user` (veer-vm binary, rootless, preferred on macOS). See [ESP32-C6 Runtime Modes](#esp32-c6-runtime-modes) above.
- `veer-connect` is used for shell access in both modes. `nc localhost {port}` also works for `net_mode=nat`.

## Next Extensions

Natural next steps:

1. Add backend health polling so backend runtime status updates automatically.
2. Add backend destroy and reprovision operations from the dashboard.
3. Add richer compatibility rules based on capabilities, firmware, and transport support.
4. Add session-aware shell audit logging and shell access controls.
5. Replace the minimal browser shell with a proper terminal emulator component.