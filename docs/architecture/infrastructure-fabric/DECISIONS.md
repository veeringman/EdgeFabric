# Architecture Decisions

## Accepted decisions

1. **Additive EdgeFabric capability.** Infrastructure Fabric lives inside the existing EdgeFabric repository and product.
2. **Three independent state sources.** Declared, discovered and observed facts remain attributable; reconciliation produces a resolved view and differences.
3. **Reuse automatic network discovery.** Existing EdgeFabric discovery is the source of network-discovery facts. Providers enrich it but do not duplicate it.
4. **Manifest support is complementary.** Declarative manifests express intended site/rack/node/platform/workload/qualification state; they do not replace discovery.
5. **Provider-neutral resource model.** CPUs, GPUs, NPUs, FPGAs, QPUs, network, storage, power, cooling and future devices fit an extensible model.
6. **Capability-driven integration.** Runtime-verified, versioned capability contracts determine supported operations, not vendor-name checks.
7. **Read-only reconciliation by default.** Plans and apply are distinct. Disruptive actions require explicit policy approval.
8. **Rust core, Python plugin option.** Rust edition 2024 is preferred for core services/agents where appropriate; Python is supported out-of-process for vendor/scientific ecosystems.
9. **Simulation is first-class.** The simulation provider supports CI and safe development.
10. **External lifecycle managers are adapters.** Integrate only for demonstrated supported operations; do not make them mandatory or assume complete coverage.

## Explicit non-goals

- A second network discovery engine.
- A parallel identity, API gateway, event bus, scheduler, agent enrollment or inventory database.
- A GPU-only architecture.
- Arbitrary remote shell execution as a public operation API.
- Automatic destructive remediation.
- Claims of Rubin/Rubin Ultra or Lenovo platform support without provider and hardware validation.

## Open decisions for repository-fit ADR

- Exact crate/module placement and whether any new crate is warranted.
- Reuse vs extension of current host/device/runner/ML-node persistence.
- Canonical resource/fact storage and graph query strategy.
- Event subject naming and durable job integration.
- Provider process boundary and plugin packaging.
- API path and schema versioning conventions.
- Evidence storage, retention and export policy.
- Initial provider/hardware matrix and qualification acceptance thresholds.
