# Capability Taxonomy

Canonical capability model for EdgeFabric devices, hosts, runners, and external integration contracts.

## Purpose

This taxonomy defines normalized capability strings used across:

- discovery and host registration
- device and runner metadata
- scheduler matching and allocation
- dashboard filters and badges
- public API contracts for external applications

All capability values are lowercase snake_case strings.

## Scope

The capability model applies to:

- Virtual IoT devices (`os = virtual_iot`)
- Real IoT endpoints (`os = iot`)
- SSH hosts (`os = ssh`) — generic workers for macOS, Linux, and Windows
- macOS hosts used for iOS builds, macOS app deploy, and ML
- Linux hosts used for IoT/firmware, VeerOS/quantum, CUDA/LLM, and Linux app deploy
- Windows hosts used for MSBuild/.NET, WSL, CUDA/LLM, and Windows app deploy

## Naming Rules

- Use lowercase snake_case only.
- Capability names describe behavior or tool availability, not branding.
- Keep names stable; avoid renaming once public APIs consume them.
- New capabilities must be additive and backward compatible.

## Capability Groups

### 1) Connectivity And Transport

| Capability | Meaning | Typical Targets |
|---|---|---|
| `ssh` | Host accepts remote command execution over SSH | iot, ssh, runner hosts |
| `serial` | Host/device supports serial console connectivity | iot |
| `mqtt` | Host/device supports MQTT publish/subscribe workflows | iot |
| `mdns` | Host was discovered through mDNS or advertises mDNS service | iot, ssh |
| `ws_stream` | Device/host emits websocket stream payloads for runtime updates | virtual_iot, iot |

### 2) IoT Runtime And Simulation

| Capability | Meaning | Typical Targets |
|---|---|---|
| `iot` | Device is an IoT endpoint with command/telemetry semantics | iot |
| `virtual_iot` | Device is software-simulated IoT controller | virtual_iot |
| `sensor_inject` | Runtime supports API/UI sensor injection | virtual_iot, iot(backends) |
| `scenario_control` | Runtime supports scenario preset/apply APIs | virtual_iot |
| `telemetry_publish` | Endpoint can publish telemetry/event streams | iot, virtual_iot |

### 3) CI/CD Runner Capabilities

| Capability | Meaning | Typical Targets |
|---|---|---|
| `cicd_runner` | Host is eligible to execute scheduled build/test jobs | ssh hosts |
| `macos_runner` | Host is a macOS SSH worker (Apple Silicon or Intel) | macOS hosts |
| `macos_deploy` | Host can install/sign/run macOS or iOS apps (`xcodebuild` / `codesign`) | macOS hosts |
| `artifact_upload` | Host can publish artifacts back to EdgeFabric artifact APIs | ssh runners |
| `workspace_bootstrap` | Host supports deterministic build workspace bootstrap | ssh runners |
| `concurrency_control` | Host supports tracked concurrent leases | ssh runners |
| `linux_runner` | Host is a Linux SSH worker (server, workstation, or SBC) | linux ssh hosts |
| `linux_deploy` | Host can install/start Linux services or copy artifacts (`systemctl` / `scp`) | linux ssh hosts |
| `windows_runner` | Host is a Windows SSH worker (OpenSSH) | windows ssh hosts |
| `windows_deploy` | Host can build/install Windows apps (`dotnet` / `msbuild`) | windows ssh hosts |
| `msbuild` | MSBuild is available | windows ssh hosts |
| `powershell` | PowerShell is available for remote scripts | windows ssh hosts |
| `wsl` | Windows Subsystem for Linux is installed | windows ssh hosts |
| `docker` | Docker CLI is available | linux, windows, macos hosts |
| `cuda` | NVIDIA GPU toolchain (`nvidia-smi`) is available | linux, windows hosts |
| `iot_toolchain` | Runner has embedded build toolchains for IoT firmware/apps | linux ssh runners |
| `iot_deploy` | Runner can deploy built artifacts to IoT/Virtual IoT targets | linux ssh runners |
| `esp32` | Runner has ESP-IDF/PlatformIO/esptool capabilities | esp32 workflows |
| `rpi` | Runner can build/deploy Raspberry Pi workloads | rpi workflows |

### 4) Apple/iOS Build Toolchain

| Capability | Meaning | Typical Targets |
|---|---|---|
| `xcode` | `xcodebuild` available and executable | macOS runners |
| `simctl` | `xcrun simctl` available | macOS runners |
| `codesign` | signing identities and keychain access available | macOS runners |
| `fastlane` | fastlane toolchain available | macOS runners |
| `cocoapods` | CocoaPods available | macOS runners |
| `swiftpm` | Swift Package Manager workflows supported | macOS runners |

### 5) Governance And Integration

| Capability | Meaning | Typical Targets |
|---|---|---|
| `tenant_scoped` | Resource enforces tenant isolation in access paths | all managed resources |
| `audit_trail` | Mutations and sensitive operations are auditable | all managed resources |
| `webhook_events` | Resource emits integration webhooks | platform integration |
| `public_api` | Resource is exposed via documented public integration APIs | platform integration |

### 6) Apple Silicon ML Cluster

| Capability | Meaning | Typical Targets |
|---|---|---|
| `mlx` | MLX (or compatible) runtime is importable for GPU/ANE workloads | macOS M-series Minis |
| `ane` | Apple Neural Engine / GPU Neural Accelerators are available | Apple Silicon hosts |
| `llm_infer` | Host can serve LLM inference (replica or pipeline stage) | macOS SSH hosts |
| `llm_train` | Host can run LoRA / DiLoCo / pipeline fine-tune jobs | macOS SSH hosts |
| `pipeline_pp` | Host can participate in LAN pipeline-parallel collectives | macOS SSH hosts |

### 7) Quantum And VeerOS

| Capability | Meaning | Typical Targets |
|---|---|---|
| `veeros_host` | Runner/host can execute workloads on VeerOS environments | veeros ssh runners |
| `quantum_sdk` | Runner has quantum SDK/runtime (e.g. Qiskit) | quantum runners |
| `quantum_simulator` | Runner can execute virtual quantum processors/simulators | quantum runners |
| `quantum_hardware` | Runner can route jobs to real quantum hardware backends | quantum runners |

## Minimum Capability Profiles

### Virtual IoT (Public Integration Ready)

Required:

- `virtual_iot`
- `ws_stream`
- `sensor_inject`
- `telemetry_publish`
- `tenant_scoped`
- `audit_trail`

Optional:

- `scenario_control`
- `webhook_events`

### Real IoT Endpoint (Managed)

Required:

- `iot`
- one of: `ssh` or `mqtt` or `serial`
- `telemetry_publish`
- `tenant_scoped`
- `audit_trail`

Optional:

- `sensor_inject`
- `ws_stream`

### Generic SSH Host (shared worker)

One SSH host is one `generic` runner linked to one `os=ssh` device. Workloads are selected by **capabilities**, not by a separate runner class.

Required:

- `ssh`
- `tenant_scoped`
- `audit_trail`

Optional (any OS):

- `cicd_runner`
- `docker`
- `cuda`
- `llm_infer`, `llm_train`, `pipeline_pp`

Optional (macOS):

- `macos_runner`, `macos_deploy`
- `xcode`, `simctl`, `codesign`, `swiftpm`
- `mlx`, `ane`

Optional (Linux):

- `linux_runner`, `linux_deploy`
- `iot_toolchain`, `iot_deploy`, `esp32`, `rpi`
- `veeros_host`, `quantum_sdk`

Optional (Windows):

- `windows_runner`, `windows_deploy`
- `msbuild`, `powershell`, `wsl`

`runner_class = mlx-cluster` is accepted as an alias and stored as `generic`.

### Linux IoT Runner (ESP32C6/RPi)

Required:

- `ssh`
- `linux_runner`
- `cicd_runner`
- `iot_toolchain`
- `iot_deploy`
- `artifact_upload`
- `workspace_bootstrap`
- `tenant_scoped`
- `audit_trail`

Optional:

- `esp32`
- `rpi`

### VeerOS Quantum Runner

Required:

- `ssh`
- `linux_runner`
- `cicd_runner`
- `veeros_host`
- `quantum_sdk`
- `tenant_scoped`
- `audit_trail`

Optional:

- `quantum_simulator`
- `quantum_hardware`

### macOS Host (ML + iOS build + macOS deploy)

Required:

- `ssh`
- `macos_runner`
- `cicd_runner`
- `tenant_scoped`
- `audit_trail`

For ML jobs also:

- `mlx`
- `llm_infer`

For iOS / macOS app jobs also:

- `xcode`
- `macos_deploy`
- `artifact_upload`
- `workspace_bootstrap`

Optional:

- `ane`, `llm_train`, `pipeline_pp`
- `simctl`, `codesign`, `fastlane`, `cocoapods`, `swiftpm`

Register as `runner_class = generic` (or omit). Probe records hardware in `last_probe_result.hardware` and upserts `ml_nodes` when ML capabilities or host hardware are present. The same host is the Devices SSH target and the scheduler lease target.

### Linux Host (IoT / VeerOS / CUDA / Linux deploy)

Required:

- `ssh`
- `linux_runner`
- `cicd_runner`
- `tenant_scoped`
- `audit_trail`

Optional:

- `linux_deploy`, `docker`, `cuda`
- `iot_toolchain`, `iot_deploy`, `esp32`, `rpi`
- `veeros_host`, `quantum_sdk`, `quantum_simulator`
- `llm_infer`, `llm_train` (Ollama, PyTorch, or `ef-ml-worker`)

### Windows Host (MSBuild / .NET / WSL / CUDA)

Required:

- `ssh` (OpenSSH Server)
- `windows_runner`
- `cicd_runner`
- `tenant_scoped`
- `audit_trail`

Optional:

- `windows_deploy`, `msbuild`, `powershell`, `wsl`
- `docker`, `cuda`
- `llm_infer` (Ollama or NVIDIA)

Windows probes use `where.exe` / `cmd.exe` so they work under PowerShell or cmd over SSH.

## Scheduler Matching Rules

For runner jobs, requested capabilities must be a subset of host capabilities.

Example iOS build request constraints:

- required: `cicd_runner`, `xcode`, `artifact_upload`
- optional preference: `codesign`, `fastlane`
- optional labels: region, team, environment

Example Linux app / IoT deploy request constraints:

- required: `cicd_runner`, `linux_runner`, `linux_deploy` (or `iot_deploy`)
- board specific: `esp32` or `rpi`
- GPU: `cuda`

Example Windows app deploy request constraints:

- required: `ssh`, `windows_runner`, `windows_deploy`
- optional: `msbuild`, `wsl`, `cuda`

Example quantum program request constraints:

- required: `cicd_runner`, `linux_runner`, `veeros_host`, `quantum_sdk`
- backend specific: `quantum_simulator` (virtual) or `quantum_hardware` (real)

Example M6 Mini ML cluster request constraints:

- required: `mlx`, `llm_infer` (any `generic` online SSH host)
- pipeline shard: `pipeline_pp`
- training: `llm_train`
- optional preference: `ane`, `unified_memory_gb >= 32` (probe hardware, not a capability string)

Example macOS app deploy request constraints:

- required: `ssh`, `macos_runner`, `macos_deploy`, `codesign`

## Public API Contract Guidance

- APIs must expose capabilities as string arrays in resource payloads.
- Capability filters should support:
  - exact match (`capability=xcode`)
  - all-of match (`capabilities_all=cicd_runner,xcode`)
  - any-of match (`capabilities_any=simctl,fastlane`)
- Unknown capability values must be ignored by old clients, not treated as fatal.

## Probe Contract (Discovery/Registration)

Capability probes should return:

- `capability`
- `status`: `pass`, `fail`, `unknown`
- `evidence`: command/output summary or detection reason
- `checked_at`

This supports dashboard probe views and auditability.

## Change Management

Before introducing a new capability:

1. Add it to this document.
2. Add probe logic (if discoverable).
3. Add scheduler policy semantics (if allocatable).
4. Add dashboard badge/filter metadata.
5. Update API docs with usage examples.

## Versioning

Taxonomy version: `v1.1` (ML cluster capabilities).

Any breaking rename requires:

- dual-write/dual-read compatibility window
- migration notes in API reference
- deprecation notice in release notes
