# Security and Operations

## Reuse the EdgeFabric trust plane

Use existing EdgeFabric identity, tenant isolation, RBAC, secure transport, secret management, audit and agent enrollment. Do not create parallel credentials or a second trust root.

## Credentials

- Manifests contain secret/identity references, never credential values.
- Resolve secrets just-in-time at the provider boundary.
- Prefer short-lived, scoped credentials and rotation.
- Redact secrets from logs, raw output, events, evidence and errors.
- Bind credentials to tenant, site, provider, operation and target scope.
- Never expose unrestricted controller credentials to test plugins.

## Safety classes

| Class | Meaning | Examples |
|---|---|---|
| S0 | Read-only inventory | Host/device identity |
| S1 | Read-only telemetry/diagnostics | Temperature, health, firmware inventory |
| S2 | Non-disruptive validation | Link state, configuration checks |
| S3 | Resource-consuming test | GPU/network/storage benchmarks |
| S4 | Disruptive change | Firmware update, reboot, reconfiguration |
| S5 | Destructive/fault injection | Destructive stress, fault injection, risky FPGA programming |

S4/S5 require immutable plan, explicit approval, maintenance window, resource locks, precondition revalidation, rollback/recovery plan where feasible and complete audit. S3 requires reservation and workload-impact policy. Site policy may raise a capability's class.

## Execution isolation

- Provider operations are allow-listed capabilities, not arbitrary shell strings.
- Signed job envelope identifies tenant, principal, targets, provider, capability, params, plan hash, expiry and permissions.
- Enforce timeout, cancellation, output limits, concurrency and resource locks.
- Run Python plugins out of process with restricted OS permissions and resource budgets.
- Validate plugin outputs against declared schemas.
- Preserve raw output only after secret scrubbing and size/type checks.

## Platform-specific boundaries

macOS agents honor SIP, TCC, user consent and supported OS APIs. Do not promise unsupported low-level telemetry or privileged operations. QPU providers are commonly remote/asynchronous: model provider queue IDs, job state, calibration snapshots, backend availability, quotas and result retrieval. Do not model a QPU as a local PCI device.

## Resilience and evidence

Use durable jobs, heartbeats, leases, retry budgets, checkpoints and cancellation. Surface provider health, backpressure, partial results, discovery coverage and snapshot freshness. Detect event sequence gaps and resync from APIs. Evidence records include hashes, provenance, retention policy and export controls.
