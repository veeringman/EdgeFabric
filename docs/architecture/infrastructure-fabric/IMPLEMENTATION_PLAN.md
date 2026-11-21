# Implementation Plan

## Phase 0 — Repository-fit design

Before coding:
1. Map current host/device/resource models, capability taxonomy, runner probes, ML-node records and discovery implementation.
2. Trace API router/auth/tenant/audit patterns, database migrations, event bus, job lifecycle, agent protocol and plugin lifecycle.
3. Identify reusable types and existing APIs; document gaps.
4. Write an ADR mapping each Infrastructure Fabric concept to an existing crate/module or justified new module.
5. Confirm Rust workspace edition and dependency policy. Workspace crates currently inherit edition from the root manifest; do not independently drift editions.

Deliverable: repository-fit architecture decision, sequence diagrams and minimal schema proposal.

## Phase 1 — Read-only inventory graph

- Normalize existing discovered hosts, devices, network facts and runner hardware metadata into resource/fact views.
- Preserve source attribution and stable identity mapping.
- Add typed relationships and queryable topology.
- Expose read-only inventory APIs using existing auth, tenant and pagination conventions.
- Add simulation fixtures and integration tests.

Acceptance: existing EdgeFabric discovery is reused; no second scanner or duplicate inventory source is introduced.

## Phase 2 — Manifests and reconciliation

- Add versioned site/rack/node manifests and profile references.
- Implement offline validation, revision pinning and manifest diff.
- Implement reconciliation states, confidence/freshness and identity conflict handling.
- Add operator resolution and audit.
- Keep reconciliation read-only.

Acceptance: tests cover matched, missing, unexpected, drift, topology mismatch, partial discovery, stale facts and identity conflicts.

## Phase 3 — Provider and capability execution

- Define versioned provider/capability contract.
- Adapt existing host probes and discovery facts first.
- Add Redfish/BMC and selected accelerator/network providers incrementally.
- Add isolated Python plugin runner only where required by real integration needs.
- Enforce typed params, allow-listed operations, timeout, cancellation and output bounds.

Acceptance: capability support is runtime-verified; unsupported hardware is represented honestly.

## Phase 4 — Qualification and evidence

- Add qualification profiles and campaign/job orchestration.
- Capture environment, firmware, driver, runtime and workload context.
- Store normalized results and immutable evidence references.
- Add report generation and repeatability metadata.

Acceptance: simulated qualification runs are reproducible and auditable end to end.

## Phase 5 — Safe apply and operations

- Introduce immutable plans, idempotency, precondition revalidation and resource locks.
- Add approval workflow and maintenance windows.
- Implement selected non-disruptive operations before disruptive classes.
- Add rollback/recovery hooks where technically possible.

Acceptance: no S4/S5 action can execute without policy-required approval and full audit.

## First vertical slice

Start with one discovered generic host and its existing probe metadata:
1. Read current discovered host/runner data.
2. Normalize CPU, memory, OS, architecture, PCI/accelerator facts with provenance.
3. Declare expected node manifest.
4. Reconcile declaration against discovered facts.
5. Display drift and evidence in read-only mode.
6. Exercise the same flow with simulation fixtures in CI.

Do not begin with broad firmware orchestration or assume Rubin/Rubin Ultra qualification support exists before provider/hardware validation.

## Engineering standards

- Rust edition 2024 for Rust additions where consistent with workspace.
- Backward-compatible API/schema evolution.
- Migration tests and tenant-isolation tests.
- Property tests for identity resolution and reconciliation.
- Deterministic provider simulation in CI.
- No hardware-mutating tests in ordinary CI.
