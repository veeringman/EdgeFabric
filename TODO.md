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
