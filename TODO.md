# EdgeFabric TODO

Execution plan for extending Virtual IoT + SSH infrastructure into Internet-usable CI/CD runners, including iOS builds triggered from Windows PCs over networked macOS hosts.

## How to Use This File

- Use these checkboxes as the source of truth for rollout status.
- Complete phases in order unless a task is explicitly marked parallel-safe.
- Every completed item must include a PR link and test evidence in the commit message.

## Priority Order (Execution Sequence)

1. Capability and data-model foundation (Phase 1)
2. Discovery/probing and host classification (Phase 2)
3. Scheduler leases and allocation constraints (Phase 3)
4. Remote runner execution and artifacts (Phase 4)
5. Public APIs and integration contracts (Phase 5)
6. Dashboard operator and developer UX (Phase 6)
7. CLI automation and external CI examples (Phase 7)
8. Security hardening and compliance controls (Phase 8)
9. Observability and SLO enforcement (Phase 9)
10. Release quality gates and rollout (Phase 10)

## Phase 0: Scope Lock And Delivery Baseline

- [ ] New scope item: **PCB / Electronic Circuit Design** (AI-assisted + manual,
  realistic visuals, drag-and-drop, config-driven). See
  `PCB_DESIGN_INTEGRATION_PLAN.md` for the full plan and phased rollout.
- [ ] New scope item: **Chip Design through Tape-Out** (RTL → sign-off,
  open-PDK first, FPGA parity, AI co-pilot). See
  `CHIP_DESIGN_INTEGRATION_PLAN.md` for the full plan and phased rollout.
- [ ] Finalize scope document: support three target classes
  - Real IoT devices (network-reachable or agent-attached)
  - Generic SSH hosts
  - macOS SSH hosts as iOS build runners
- [ ] Define "Windows to iOS build" flow
  - Trigger from Windows client (dashboard/API/CLI)
  - Schedule on macOS host in LAN/WAN
  - Return logs + artifacts to EdgeFabric APIs
- [ ] Confirm tenancy and trust boundaries
  - Public API exposure only through ingress/API gateway
  - No direct public SSH ingress to runner machines
- [ ] Explicit product goal statement for all teams
  - Virtual IoT and real IoT must be consumable by external applications over the Internet
  - EdgeFabric provides a stable public integration interface (REST/WebSocket/events)

## Phase 1: Data Model And Capability Taxonomy

- [x] Add capability taxonomy doc in `docs/` (canonical list + semantics)
  - `ssh`, `iot`, `cicd_runner`, `xcode`, `codesign`, `simctl`, `fastlane`, `artifact_upload`
- [x] Add migration for runner metadata (new table or extension table)
  - Runner class, host OS, architecture, labels, max concurrency, status, health
- [x] Add migration for build jobs and artifacts
  - Build job lifecycle: queued, provisioning, running, failed, succeeded, canceled
  - Artifact index (path, type, size, checksum, retention)
- [x] Add migration for runner credential references
  - Store references only (never raw private keys in plaintext)

## Phase 2: Discovery And Registration Extensions

- [ ] Extend agent discovery to classify discovered hosts into
  - IoT endpoints
  - SSH hosts
  - Candidate CI/CD runners
- [x] Ensure simulator/macOS runner onboarding can use ef-agent discovered devices already visible in dashboard
  - Runner registration supports linking from existing `device_id` and deriving host metadata
- [ ] Extend discovery and registration parity for Virtual IoT and real IoT
  - Ensure Virtual IoT entries and real IoT endpoints share consistent metadata contracts
  - Keep capability model aligned across `iot`, `virtual_iot`, and `ssh` backed devices
- [ ] Add macOS capability probing over SSH
- [x] Add macOS capability probing over SSH
  - Verify `xcodebuild`, `xcrun`, `simctl`, optional `fastlane`
  - Capture Xcode version, SDKs, and toolchain details
- [ ] Auto-register agent host self as runner when eligible
  - If host is macOS and capability checks pass
- [ ] Add manual registration path for remote hosts
  - Host + port + auth method + labels + region

## Phase 3: Scheduler And Allocation Logic

- [x] Extend scheduler filter model for runner jobs
  - Required capabilities
  - Region/tenant affinity
  - Host labels
  - Concurrency limits
- [x] Add reservation and lease model for build runners
  - Prevent over-allocation on same host
- [ ] Add preflight checks before allocation
  - SSH reachability
  - Disk space and temp path availability
  - Optional keychain/signing readiness
- [ ] Add failover behavior
  - Retry on alternate runner for infrastructure failures

## Phase 4: Remote Build Execution (Core)

- [x] Add new orchestrator runner path: `ios_remote_ssh`
  - Execute `xcodebuild` on remote macOS over SSH
  - Stream stdout/stderr incrementally
- [ ] Implement artifact collection pipeline
  - Pull `.xcresult`, `.ipa`, logs via SCP/rsync
  - Upload and index in EdgeFabric artifacts store
- [ ] Add deterministic workspace bootstrap on runner
  - Checkout path policy
  - Dependency restore strategy (SPM/CocoaPods)
  - Cleanup policy after completion
- [ ] Add cancellation handling
  - Stop remote process
  - Mark lease released

## Phase 5: API Surface (Internet Team Consumption)

- [x] Add runner management API
  - `POST /api/v1/runners/register`
  - `GET /api/v1/runners`
  - `GET /api/v1/runners/{id}`
  - `POST /api/v1/runners/{id}/probe`
  - `POST /api/v1/runners/{id}/drain`
- [ ] Add iOS build API
- [ ] Add iOS build API
- [x] Add iOS build API
  - `POST /api/v1/builds/ios`
  - `GET /api/v1/builds/{id}`
  - `GET /api/v1/builds/{id}/logs`
  - `GET /api/v1/builds/{id}/artifacts`
  - `POST /api/v1/builds/{id}/cancel`
- [ ] Ensure all endpoints are tenant-scoped and audited
- [ ] Add explicit Virtual IoT and IoT APIs where missing
  - Virtual IoT lifecycle: template create/update, backend attach/detach, scenario controls
  - IoT endpoint lifecycle: register/probe/update capabilities/health
  - Unified query filters across device classes for team consumption
- [ ] Define and publish public API contract for external integrators
  - Versioned endpoint policy (`/api/v1/...`) and deprecation windows
  - Idempotency strategy for create/update operations
  - Pagination, filtering, and error contract consistency
- [ ] Add public event interface for external apps
  - Webhooks for device status, build status, and alert events
  - Event stream/WebSocket topics for sensor frames and state transitions
  - Delivery semantics with retries, signing, and replay window
- [ ] Add integration credentials model
  - Service accounts/API keys per external app
  - Scoped permissions per tenant/project/environment
  - Key rotation and revocation endpoints

## Phase 6: Dashboard And UX

- [ ] Add Dashboard information architecture updates
  - New top-level navigation sections: Devices, Runners, Builds, Artifacts, Admin
  - Context-aware tabs for Virtual IoT, IoT Hosts, SSH Hosts, and macOS Runners
  - Global quick actions: register host, run probe, submit iOS build
- [ ] Add dedicated Virtual IoT management experience
  - Virtual IoT catalog view with controller profile, sensors, backend binding, simulation status
  - Designer shortcuts for create/edit/clone and scenario presets
  - Runtime controls for simulation rate, pause/resume, and sensor injection shortcuts
- [ ] Add dedicated IoT hosts management experience
  - Real IoT inventory view with connectivity mode (SSH/serial/MQTT), health, capabilities, region
  - Probe/refresh actions and troubleshooting hints when host becomes unreachable
  - Host detail with last telemetry, command history, and session shortcuts
- [ ] Add "Runners" inventory page
  - Health/state badges, capabilities, queue depth, concurrency, current lease owner, last heartbeat
  - Advanced filters: region, labels, capabilities, tenant scope, online/offline, drained state
  - Bulk actions: probe selected, drain selected, resume selected
- [ ] Add runner detail page
  - Runtime summary: OS/arch, Xcode versions, SDKs, simctl availability, fastlane availability
  - Capacity chart: running jobs vs max concurrency
  - Runner history: last N builds, failures, average duration
  - Actions: probe now, drain, resume, maintenance mode toggle
- [ ] Add host registration wizard
  - Manual host onboarding: host, port, auth mode, labels, region, runner class
  - Connectivity test step before save
  - Capability probe preview with pass/fail rows
- [ ] Add "Create iOS Build" workflow
  - Source selector (repo URL/project path), branch/tag/commit, scheme/configuration, destination profile
  - Signing profile selector (tenant-scoped), export options, artifact retention options
  - Runner policy selector: auto, preferred labels, fixed runner
  - Dry-run validation before submit (missing scheme/signing/capability checks)
- [ ] Add Builds list and detail pages
  - Build queue table with status, requester, runner, duration, retries
  - Build detail timeline: queued -> allocated -> running -> upload -> completed
  - Retry and cancel controls with reason capture
- [ ] Add build log viewer
  - Live stream with pause/follow mode, severity highlighting, search/filter
  - Download raw logs
  - Segment logs by phases (bootstrap, dependency install, build, archive, export)
- [ ] Add artifacts experience
  - Artifact panel with type icon, size, checksum, retention countdown
  - One-click download links and copyable API URLs
  - Integrity indicator (checksum verified/not verified)
- [ ] Add governance and access UX
  - Tenant-aware visibility for runners/builds/artifacts
  - RBAC-aware action states (hidden/disabled with permission hints)
  - Audit panel for runner changes and build operations
- [ ] Add dashboard notifications
  - Toast + in-app notifications for build started/failed/succeeded/canceled
  - Optional webhook status mirrors for external team channels
- [ ] Add Windows-first compatibility hardening
  - Validate dashboard behavior on Edge/Chrome on Windows 10/11
  - Ensure copy/paste and key handling work in terminal/log widgets on Windows keyboards
  - Confirm full flow from browser on Windows to remote macOS execution
- [ ] Add observability widgets on dashboard home
  - Fleet health summary (hosts/runners by status)
  - Build throughput and failure trends
  - Queue latency and runner utilization heatmap
  - Virtual IoT simulation health and real IoT connectivity health tiles
- [ ] Add UI test coverage for new dashboard paths
  - Playwright E2E for register host -> probe -> submit build -> stream logs -> download artifacts
  - Permission matrix tests across admin/operator/viewer roles
  - Virtual IoT E2E for create -> attach backend -> stream sensor frames
  - Real IoT E2E for discover/register -> health check -> remote command/session

## Phase 7: CLI And Automation

- [ ] Extend `ef-cli` with runner commands
  - `ef runners list`, `ef runners probe`, `ef runners drain`
- [ ] Extend `ef-cli` with build commands
  - `ef builds ios submit`, `ef builds status`, `ef builds logs`, `ef builds artifacts`
- [ ] Publish CI examples
  - GitHub Actions and Jenkins snippets for Windows/Linux initiators

## Phase 8: Security And Compliance Hardening

- [ ] Integrate secrets backend references for SSH keys and signing assets
- [ ] Enforce per-tenant RBAC on runner and build endpoints
- [ ] Add command allowlist for remote execution templates
- [ ] Add immutable audit trail for
  - Runner registration changes
  - Build execution commands
  - Artifact access/download
- [ ] Add rate limits and quota controls
  - Builds per minute
  - Concurrent runner usage
- [ ] Add public API protection controls
  - WAF, abuse detection, request signature verification for webhooks
  - Per-app and per-tenant throttling policies
  - Outbound IP allowlist option for enterprise consumers

## Phase 9: Observability And SLOs

- [ ] Add metrics
  - Queue wait time, build duration, success rate, retry rate
- [ ] Add logs/traces
  - Build lifecycle spans with runner IDs
- [ ] Add alerts
  - Runner offline
  - Build failure spikes
  - Artifact upload failures
- [ ] Define initial SLOs
  - Build start latency
  - Build success reliability

## Phase 10: Testing And Release

- [ ] Unit and integration tests
  - Scheduler capability matching
  - Remote execution wrapper behavior
  - Artifact indexing correctness
- [ ] End-to-end tests
  - Windows client trigger -> macOS runner -> artifact retrieval
  - Virtual IoT + real IoT + SSH host coexistence
  - Cross-device workflow where Virtual IoT and real IoT are both usable from the same team project
- [ ] Security tests
  - Tenant isolation and authz checks
- [ ] Documentation updates before release
  - `docs/API_REFERENCE.md`
  - `docs/DEVICE_REGISTRATION.md`
  - `docs/SYSTEM_DESIGN.md`
- [ ] Publish external developer integration docs
  - Quickstart for external REST/WebSocket integration
  - Webhook setup and signature verification guide
  - End-to-end sample app consuming Virtual IoT + real IoT APIs
- [ ] Rollout strategy
  - Dev -> staging -> production with feature flags

## Immediate Sprint Starter (Next 5 Working Sessions)

- [x] Session 1: Data model migrations + capability taxonomy draft
- [ ] Session 2: Runner probing implementation (macOS/Xcode checks)
- [x] Session 2: Runner probing implementation (macOS/Xcode checks)
  - [x] Session 2A: Runner management API (`register/list/get/drain/probe`) with tenant scope
  - [x] Session 2B: Deep probe over SSH for macOS toolchain (`xcodebuild`, `xcrun`, `simctl`, `fastlane`)
- [ ] Session 3: Scheduler capability filters + runner lease model
- [x] Session 3: Scheduler capability filters + runner lease model
- [ ] Session 4: `ios_remote_ssh` runner + live log streaming
- [ ] Session 4: `ios_remote_ssh` runner + live log streaming
- [x] Session 4: `ios_remote_ssh` runner + live log streaming
  - [x] Session 4A: remote `xcodebuild` execution over SSH with lease acquire/release
  - [x] Session 4B: build job persistence + build status/logs endpoints
  - [x] Session 4C: incremental live log streaming + cancel endpoint
- [ ] Session 5: Minimal build API + dashboard read-only build status page

## Done Definition

- [ ] A Windows user can start an iOS build from EdgeFabric UI/API
- [ ] Build runs on an allocated macOS SSH runner in network
- [ ] Deployment target can be any registered/discovered iOS device in dashboard inventory (real iPhone or simulator)
- [ ] Logs stream live and artifacts are downloadable
- [ ] Access is tenant-scoped, audited, and quota-controlled
- [ ] End-to-end tests pass in CI
- [ ] External application can integrate over public Internet interface without private network access

---

# Chip Design Track (RTL → Tape-out, FPGA Parity, Quantum/PQC)

Tracks all work for the **Chip Design** section in the dashboard and its supporting
backend services. See `CHIP_DESIGN_INTEGRATION_PLAN.md` and
`QUANTUM_INTEGRATION_PLAN.md` for architectural context.

## Chip · Phase A: Dashboard UX (Studio Surface)

- [x] Studio landing page `/silicon/studio` with project tiles
- [x] RTL workbench page `/silicon/rtl` (read-only mock)
- [x] Qubit/quantum design page `/silicon/qubit`
- [x] Floorplan viewer `/silicon/floorplan`
- [x] 3D stack-up viewer `/silicon/stackup` (isometric SVG, layer toggles)
- [x] **Schematic Editor** `/silicon/schematic` (shared engine, drag-and-drop, wire routing, presets: NAND2 · SQC-Q5 transmon · RV32I 5-stage core · PQC Kyber accelerator)
- [x] **Sim / FPGA Backends** `/silicon/backends` (veer-vm vFPGA + hardware FPGA cards, provision form, filters, status actions)
- [ ] Add "View Backends" entry card on `/silicon/studio` landing
- [ ] DRC/LVS results panel (per-cell errors with highlights)
- [ ] Timing/STA summary panel (slack histogram, top critical paths)
- [ ] Power/IR-drop heatmap viewer
- [ ] Tape-out checklist page (sign-off gates with green/red status)
- [ ] Waveform viewer (VCD/FST) embedded in Backends → Open Waveform
- [ ] Console/log streaming pane for backend jobs (live tail)
- [ ] Cell library browser (std-cells + macros, with characterization summary)
- [ ] PDK manager (install/select open-PDK: SKY130, GF180MCU, IHP-SG13G2)
- [ ] AI co-pilot side-panel hooks (RTL suggest, lint fix, constraint authoring)
- [ ] Project import/export (.zip with RTL + constraints + SDC + LEF/DEF)
- [ ] Schematic ↔ Netlist round-trip (export Verilog netlist from schematic)
- [ ] Schematic versioning + diff view
- [ ] Multi-user collaboration cursors on schematic canvas (later)

## Chip · Phase B: Schematic Editor Engine Hardening

- [x] Shared engine `components/schematic.rs` (Send+Sync `SymbolLibrary` trait)
- [x] Drag-from-palette, pin-to-pin wiring, ortho routing, undo, JSON export
- [x] Chip skin `ChipLib` with ~30 symbols (transistors, passives, ports, quantum, RV32I, PQC)
- [ ] Bus/multi-bit wires (e.g. `instr[31:0]`)
- [ ] Hierarchical sub-circuits (group → symbol)
- [ ] ERC (Electrical Rules Check): floating pins, shorts, missing power
- [ ] Auto-wire router (A* on grid)
- [ ] Net auto-naming and back-annotation from netlist
- [ ] Persist schematic to backend (REST + revision history)
- [ ] Import KiCad/EAGLE `.sch` for chip-level macros
- [ ] Export to: structural Verilog, SPICE deck, Spectre netlist

## Chip · Phase C: Backends — veer-vm (Virtual FPGA)

- [ ] veer-vm controller crate `ef-veer` (spawn, lifecycle, IPC)
- [ ] Wrap VeerOS veer-vm as a long-running process per backend instance
- [ ] Load RTL (Verilog/SV) into vFPGA snapshot
- [ ] Drive testbench, capture stdout + VCD waveform
- [ ] Coverage report extraction (line/branch/toggle)
- [ ] Snapshot/restore for fast regression
- [ ] Cluster scheduling (N vFPGAs across agent hosts)
- [ ] Resource accounting (CPU, RAM, sim-cycles/sec)
- [ ] Auto-detect veer-vm install on agent capability probe

## Chip · Phase D: Backends — Hardware FPGA

- [ ] Hardware FPGA adapter trait `FpgaAdapter` (program, reset, probe, jtag)
- [ ] Vendor adapters: Xilinx Vivado (Vitis HLS), Intel Quartus, Lattice Diamond/Radiant
- [ ] Bitstream upload + program over USB-JTAG (xvcd / openOCD)
- [ ] In-circuit emulation: host-FPGA data pipe (PCIe BAR / Ethernet / USB)
- [ ] Logic analyzer / ChipScope-style waveform capture
- [ ] Board inventory (VCU118, Alveo U280, Arty A7, custom boards) with health
- [ ] Lease/reservation integration with EdgeFabric scheduler
- [ ] Power telemetry (board sensors → metrics)
- [ ] Remote board access via lab-attached agent host

## Chip · Phase E: Data Model & Persistence

- [ ] Migration: `chip_projects` (id, tenant, name, pdk, kind: digital|analog|quantum|mixed)
- [ ] Migration: `chip_schematics` (id, project_id, revision, json, author, created_at)
- [ ] Migration: `chip_rtl_sources` (id, project_id, path, language, sha)
- [ ] Migration: `chip_constraints` (sdc, upf, def, lef references)
- [ ] Migration: `chip_backends` (id, kind: vfpga|fpga, name, host, target, capabilities[], status, util)
- [ ] Migration: `chip_backend_leases` (backend_id, project_id, owner, acquired_at, ttl)
- [ ] Migration: `chip_jobs` (id, project_id, backend_id, kind: sim|synth|pnr|sta|drc|lvs|program, status, started_at, finished_at)
- [ ] Migration: `chip_artifacts` (job_id, kind: vcd|fst|log|netlist|bitstream|report, path, size, sha)
- [ ] Migration: `chip_signoff_gates` (project_id, gate_name, status, evidence_artifact_id)
- [ ] Migration: `pdk_installs` (id, name, version, path, installed_at)

## Chip · Phase F: REST/WebSocket APIs

- [ ] `GET    /api/v1/chip/projects` · list (tenant-scoped, filter by pdk/kind)
- [ ] `POST   /api/v1/chip/projects` · create
- [ ] `GET    /api/v1/chip/projects/{id}` · detail
- [ ] `PATCH  /api/v1/chip/projects/{id}` · update metadata
- [ ] `DELETE /api/v1/chip/projects/{id}` · soft delete
- [ ] `GET    /api/v1/chip/projects/{id}/schematics` · list revisions
- [ ] `POST   /api/v1/chip/projects/{id}/schematics` · save new revision
- [ ] `GET    /api/v1/chip/projects/{id}/schematics/{rev}` · fetch JSON
- [ ] `POST   /api/v1/chip/projects/{id}/schematics/{rev}/export` · `{format: verilog|spice|json}`
- [ ] `GET    /api/v1/chip/projects/{id}/rtl` · list source files
- [ ] `PUT    /api/v1/chip/projects/{id}/rtl/{path}` · upload/update source
- [ ] `POST   /api/v1/chip/projects/{id}/lint` · run linter (async job)
- [ ] `GET    /api/v1/chip/backends` · list backends
- [ ] `POST   /api/v1/chip/backends` · provision new backend
- [ ] `GET    /api/v1/chip/backends/{id}` · detail
- [ ] `POST   /api/v1/chip/backends/{id}/start` · boot
- [ ] `POST   /api/v1/chip/backends/{id}/stop` · halt
- [ ] `POST   /api/v1/chip/backends/{id}/recover` · reset on error
- [ ] `DELETE /api/v1/chip/backends/{id}` · deprovision
- [ ] `POST   /api/v1/chip/backends/{id}/attach` · `{project_id}`
- [ ] `POST   /api/v1/chip/backends/{id}/lease` · acquire lease (with TTL)
- [ ] `DELETE /api/v1/chip/backends/{id}/lease` · release
- [ ] `POST   /api/v1/chip/jobs` · submit job `{project_id, backend_id, kind, opts}`
- [ ] `GET    /api/v1/chip/jobs/{id}` · status
- [ ] `GET    /api/v1/chip/jobs/{id}/logs` · stream (NDJSON / WebSocket)
- [ ] `GET    /api/v1/chip/jobs/{id}/artifacts` · list
- [ ] `GET    /api/v1/chip/jobs/{id}/artifacts/{name}` · download
- [ ] `POST   /api/v1/chip/jobs/{id}/cancel` · cancel
- [ ] `GET    /api/v1/chip/pdks` · list installed PDKs
- [ ] `POST   /api/v1/chip/pdks/install` · install/register a PDK
- [ ] WebSocket `/api/v1/chip/backends/{id}/console` · live stdout/stderr
- [ ] WebSocket `/api/v1/chip/jobs/{id}/waveform` · live waveform deltas
- [ ] Webhook events: `chip.job.completed`, `chip.backend.status_changed`, `chip.signoff.gate_passed`

## Chip · Phase G: Tooling Integrations (Open EDA First)

- [ ] Yosys (synthesis) wrapper
- [ ] nextpnr (place & route for FPGA targets)
- [ ] OpenROAD flow (ASIC RTL → GDS) for SKY130/GF180/IHP
- [ ] Verilator (cycle-accurate sim, alternative to veer-vm)
- [ ] Icarus Verilog (event-driven sim, lightweight)
- [ ] GTKWave server-side waveform → SVG/PNG snapshot endpoint
- [ ] OpenSTA (timing)
- [ ] Magic / KLayout (DRC + GDS view)
- [ ] Netgen (LVS)
- [ ] Optional commercial bridges: Cadence/Synopsys (license-gated)

## Chip · Phase H: AI Co-pilot

- [ ] RTL generation from natural-language spec
- [ ] Constraint authoring assistant (SDC suggestions from design intent)
- [ ] Lint/error explanation with patch suggestions
- [ ] Test-bench generation from module port list
- [ ] Schematic auto-layout suggestion
- [ ] PQC/quantum block templating

## Chip · Phase I: Quantum & PQC Extensions

- [x] X-mon + JJ + resonator symbols in schematic library
- [x] PQC Kyber accelerator preset (AXI, FSM, TRNG, Keccak, sampler, NTT, polymul, modred, polyram)
- [ ] Superconducting-qubit physical params overlay (Ej/Ec, ω_q, T1/T2 estimates)
- [ ] Pulse-level control schematic skin (XY/Z drive lines)
- [ ] PQC algorithm presets: Dilithium, Falcon, SPHINCS+
- [ ] Quantum sim backend adapter (Qiskit/Cirq via veer-vm)

## Chip · Phase J: Security, Tenancy & Audit

- [ ] All chip projects tenant-scoped
- [ ] RBAC roles: chip.viewer, chip.designer, chip.signoff_owner
- [ ] Backend access ACL (which projects can use which physical FPGA boards)
- [ ] Audit log: schematic edits, backend lease grants, bitstream programs
- [ ] IP encryption-at-rest for RTL/GDS
- [ ] Per-tenant PDK access controls (NDA-gated PDKs)

## Chip · Phase K: Testing

- [ ] Unit: schematic engine geometry/wiring
- [ ] Unit: backend state machine transitions
- [ ] Integration: provision vFPGA → load RTL → run sim → fetch VCD
- [ ] Integration: program hardware FPGA → in-circuit test → release lease
- [ ] E2E (Playwright): schematic drag/drop/wire/save/export
- [ ] E2E: end-to-end job submission from Studio UI

---

# PCB / Electronic Circuit Design Track

Tracks all work for the **PCB Design** section in the dashboard and its supporting
backend services. See `PCB_DESIGN_INTEGRATION_PLAN.md` for architectural context.

## PCB · Phase A: Dashboard UX

- [x] PCB studio landing page
- [x] Standalone hardware schematic page `hw_schematic.rs` (legacy, pre-shared-engine)
- [x] PCB 3D viewer (basic)
- [ ] **Migrate** `hw_schematic.rs` to use the shared `components/schematic.rs` engine with a `PcbLib` skin
- [ ] PCB symbol library skin `PcbLib` (R/L/C, diodes, ICs, connectors, MCU footprints, opamp, regulators, MOSFET, BJT)
- [ ] Footprint editor (pads, courtyard, silkscreen)
- [ ] Board outline + layer stack-up editor
- [ ] PCB layout canvas (2D) with autorouter hooks
- [ ] 3D realistic board viewer (components + traces + silkscreen)
- [ ] Net highlighting across schematic ↔ layout
- [ ] DRC results panel (clearance, width, drill, courtyard)
- [ ] BOM viewer + supplier links (Octopart/Digi-Key style)
- [ ] Gerber/ODB++ export panel
- [ ] Assembly drawing + pick-and-place export
- [ ] Drag-and-drop part placement from palette to canvas
- [ ] AI-assisted schematic capture from spec
- [ ] AI-assisted footprint suggestion from datasheet PDF

## PCB · Phase B: Schematic Engine Reuse

- [ ] Implement `PcbLib: SymbolLibrary` (~60 standard symbols)
- [ ] Hierarchical sheets (multi-page schematic)
- [ ] Bus wires + net classes
- [ ] Annotation (auto-reference designators R1, C2, U3…)
- [ ] ERC ruleset for PCB (no-connect, power-flag, mixed input/output)
- [ ] Symbol/footprint association mapping
- [ ] Schematic ↔ netlist round-trip (KiCad netlist format)

## PCB · Phase C: Layout Engine

- [ ] 2D layout canvas (SVG/Canvas) with layer ordering
- [ ] Component placement with rotation + locked state
- [ ] Trace routing (manual + push-and-shove later)
- [ ] Copper pour / ground plane
- [ ] Via management (through/blind/buried)
- [ ] DRC engine (clearance, width, annular ring, courtyard overlap)
- [ ] 3D rendering pipeline (component STEP models + extruded copper)
- [ ] Layer visibility + render presets (top/bottom/multilayer)

## PCB · Phase D: Backends & Manufacturing

- [ ] Gerber RS-274X exporter
- [ ] Excellon drill exporter
- [ ] ODB++ exporter
- [ ] IPC-2581 exporter (modern alternative)
- [ ] Pick-and-place CSV exporter (vendor-specific dialects)
- [ ] BOM exporter (CSV, KiCad, OctoBOM)
- [ ] Manufacturer integration hooks (JLCPCB / PCBWay / OSH Park submit)
- [ ] DFM analysis service (clearance vs fab capability)
- [ ] Cost estimator (per quantity, per layer count)

## PCB · Phase E: Simulation & Verification

- [ ] SPICE simulation backend (ngspice wrapper)
- [ ] Signal-integrity (SI) sim hooks (HyperLynx-style, open alternatives)
- [ ] Power-integrity (PI) analysis
- [ ] Thermal simulation (basic FEM)
- [ ] EMI/EMC pre-compliance check
- [ ] Use shared **Sim / FPGA Backends** infra where applicable (e.g. attached MCU emulation via veer-vm)

## PCB · Phase F: Data Model

- [ ] Migration: `pcb_projects` (id, tenant, name, board_class, layer_count)
- [ ] Migration: `pcb_schematics` (id, project_id, revision, json)
- [ ] Migration: `pcb_layouts` (id, project_id, revision, json, board_outline, stackup)
- [ ] Migration: `pcb_libraries` (id, name, kind: symbol|footprint|3d, tenant_scope)
- [ ] Migration: `pcb_parts` (id, mpn, manufacturer, symbol_id, footprint_id, datasheet_url)
- [ ] Migration: `pcb_jobs` (id, project_id, kind: drc|erc|gerber|spice|3d_render, status)
- [ ] Migration: `pcb_artifacts` (job_id, kind, path, size, sha)
- [ ] Migration: `pcb_bom_lines` (project_id, ref_des[], part_id, qty)

## PCB · Phase G: REST APIs

- [ ] `GET    /api/v1/pcb/projects` · list
- [ ] `POST   /api/v1/pcb/projects` · create
- [ ] `GET    /api/v1/pcb/projects/{id}` · detail
- [ ] `PATCH  /api/v1/pcb/projects/{id}` · update
- [ ] `DELETE /api/v1/pcb/projects/{id}` · soft delete
- [ ] `GET    /api/v1/pcb/projects/{id}/schematic` · current revision
- [ ] `POST   /api/v1/pcb/projects/{id}/schematic` · save revision
- [ ] `GET    /api/v1/pcb/projects/{id}/layout` · current revision
- [ ] `POST   /api/v1/pcb/projects/{id}/layout` · save revision
- [ ] `POST   /api/v1/pcb/projects/{id}/erc` · run ERC
- [ ] `POST   /api/v1/pcb/projects/{id}/drc` · run DRC
- [ ] `POST   /api/v1/pcb/projects/{id}/export` · `{format: gerber|odb|ipc2581|stl|step}`
- [ ] `GET    /api/v1/pcb/projects/{id}/bom` · BOM
- [ ] `POST   /api/v1/pcb/projects/{id}/sim` · `{kind: spice|si|pi|thermal, opts}`
- [ ] `GET    /api/v1/pcb/jobs/{id}` · status
- [ ] `GET    /api/v1/pcb/jobs/{id}/logs` · stream
- [ ] `GET    /api/v1/pcb/jobs/{id}/artifacts` · list/download
- [ ] `POST   /api/v1/pcb/jobs/{id}/cancel`
- [ ] `GET    /api/v1/pcb/libraries` · list
- [ ] `POST   /api/v1/pcb/libraries/import` · import KiCad/EAGLE/Altium libraries
- [ ] `GET    /api/v1/pcb/parts?q=` · search part catalog
- [ ] Webhooks: `pcb.export.completed`, `pcb.drc.failed`, `pcb.fab.submitted`

## PCB · Phase H: Tooling Integrations

- [ ] KiCad CLI (kicad-cli) for headless ERC/DRC/Gerber/3D
- [ ] ngspice for SPICE
- [ ] FreeCAD / OpenSCAD for STEP/STL export pipeline
- [ ] OpenEMS for EM simulation (advanced)
- [ ] Octopart/Digi-Key API integration for part lookup
- [ ] JLCPCB/PCBWay submit APIs

## PCB · Phase I: AI Co-pilot

- [ ] Schematic-from-spec generation (LLM)
- [ ] Part recommendation from functional requirement
- [ ] Footprint suggestion from datasheet PDF (OCR + LLM)
- [ ] DRC error explanation + auto-fix suggestion
- [ ] Layout placement suggestion
- [ ] BOM optimization (lower cost, fewer suppliers)

## PCB · Phase J: Security & Tenancy

- [ ] Tenant-scoped projects, libraries, parts
- [ ] RBAC: pcb.viewer / pcb.designer / pcb.releaser
- [ ] Sign-off workflow for fab submission
- [ ] Audit trail on schematic/layout edits and exports

## PCB · Phase K: Testing

- [ ] Unit: ERC/DRC rules engine
- [ ] Unit: Gerber generator (byte-exact known-good fixtures)
- [ ] Integration: KiCad CLI roundtrip
- [ ] E2E (Playwright): create project → schematic capture → layout → DRC → export Gerber → download

---

# Cross-Cutting (Chip + PCB Shared)

- [x] Shared schematic engine `components/schematic.rs` (Send+Sync, drag-and-drop, ortho wire routing, undo, JSON export)
- [ ] Shared **Backends/Runner** infrastructure (reuse `runners` + `leases` tables for FPGA boards and veer-vm processes)
- [ ] Shared artifact storage layer (S3-compatible) for VCD, GDS, Gerber, bitstreams
- [ ] Shared job orchestration (extend `ef-orchestrator` with `chip.*` and `pcb.*` job kinds)
- [ ] Shared audit log schema across both domains
- [ ] Shared AI co-pilot service (`ef-copilot` crate) with domain-specific prompts
- [ ] Unified search across chip + pcb projects/parts/symbols
- [ ] Common entitlements model (PDK access, vendor-tool licenses, fab credits)
- [ ] Common observability dashboard tile: "EDA jobs running / queued / failed"
