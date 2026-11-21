# EdgeFabric Infrastructure Fabric

Infrastructure Fabric is an additive EdgeFabric capability for declaring, discovering, reconciling, observing, operating, and qualifying heterogeneous infrastructure. It is not a separate product or parallel control plane.

It covers hosts, racks, CPUs, GPUs, NPUs, FPGAs, QPUs, network and storage fabrics, power, cooling, management controllers, firmware, runtimes, and future accelerator classes.

## Three sources of truth

1. **Declared state** — versioned manifests, BOMs, site/rack/node definitions, platform/workload/qualification profiles.
2. **Discovered state** — EdgeFabric's existing automatic network discovery, enrolled agents, host probes, Redfish/BMC and provider integrations.
3. **Observed state** — telemetry, diagnostics, topology checks, benchmarks and qualification evidence.

These remain separately attributable and are reconciled into a versioned resource graph. Discovery enriches and challenges declared intent; it never silently overwrites it.

## Repository fit

The current repository already has a Rust workspace, `ef-core`, `ef-api`, `ef-agent`, capability taxonomy, generic SSH host enrollment, runner probing, ML-node inventory and existing discovery APIs. Extend these contracts rather than introducing duplicate inventory, discovery, gateway, identity, scheduler, event bus or agent enrollment systems.

The first implementation task is a repository-fit ADR and code-path map. Confirm exact persistence, API, event, agent and plugin integration points before adding modules or dependencies.

## Documents

- [Architecture](ARCHITECTURE.md)
- [Domain model](DOMAIN_MODEL.md)
- [Manifests and reconciliation](MANIFESTS_AND_RECONCILIATION.md)
- [API and events](API_AND_EVENTS.md)
- [Providers and plugins](PROVIDERS_AND_PLUGINS.md)
- [Security and operations](SECURITY_AND_OPERATIONS.md)
- [Implementation plan](IMPLEMENTATION_PLAN.md)
- [Architecture decisions](DECISIONS.md)

## Integration invariants

- Reuse EdgeFabric automatic network discovery and preserve its provenance.
- Reuse auth, tenant isolation, RBAC, APIs, audit, agents, secure transport, eventing, persistence, jobs and plugin lifecycle.
- Keep declaration, discovery, observation and operator resolution distinct.
- Make reconciliation read-only by default. Mutations require an explicit plan and policy.
- Model capabilities as versioned, verified contracts, not vendor-name assumptions.
- Rust core work uses edition 2024 where consistent with the workspace; Python is supported out-of-process for provider/test ecosystems where appropriate.

This is an architecture handoff, not a claim that implementation or hardware qualification has been completed.
