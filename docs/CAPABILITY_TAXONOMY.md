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
- SSH hosts (`os = ssh`)
- macOS build runners (typically `os = ssh` + runner capabilities)

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
| `cicd_runner` | Host is eligible to execute scheduled build/test jobs | ssh runners |
| `artifact_upload` | Host can publish artifacts back to EdgeFabric artifact APIs | ssh runners |
| `workspace_bootstrap` | Host supports deterministic build workspace bootstrap | ssh runners |
| `concurrency_control` | Host supports tracked concurrent leases | ssh runners |
| `linux_runner` | Runner host is Linux-based and suitable for IoT/edge workloads | linux ssh runners |
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

### 6) Quantum And VeerOS

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

### Generic SSH Host

Required:

- `ssh`
- `tenant_scoped`
- `audit_trail`

Optional:

- `cicd_runner`

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

### macOS iOS Build Runner

Required:

- `ssh`
- `cicd_runner`
- `xcode`
- `artifact_upload`
- `workspace_bootstrap`
- `tenant_scoped`
- `audit_trail`

Optional:

- `simctl`
- `codesign`
- `fastlane`
- `cocoapods`
- `swiftpm`

## Scheduler Matching Rules

For runner jobs, requested capabilities must be a subset of host capabilities.

Example iOS build request constraints:

- required: `cicd_runner`, `xcode`, `artifact_upload`
- optional preference: `codesign`, `fastlane`
- optional labels: region, team, environment

Example IoT deploy request constraints:

- required: `cicd_runner`, `linux_runner`, `iot_toolchain`, `iot_deploy`, `artifact_upload`
- board specific: `esp32` or `rpi`

Example quantum program request constraints:

- required: `cicd_runner`, `linux_runner`, `veeros_host`, `quantum_sdk`
- backend specific: `quantum_simulator` (virtual) or `quantum_hardware` (real)

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

Taxonomy version: `v1` (initial).

Any breaking rename requires:

- dual-write/dual-read compatibility window
- migration notes in API reference
- deprecation notice in release notes
