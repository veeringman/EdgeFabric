# API and Events

## API principles

Use existing EdgeFabric API versioning, authentication, tenant scoping, RBAC, pagination, error envelope, audit hooks and job model. The paths below are logical proposals, not a mandate for a new gateway or duplicated routes.

### Inventory and topology
- `GET /infrastructure/resources`
- `GET /infrastructure/resources/{id}`
- `GET /infrastructure/resources/{id}/relationships`
- `GET /infrastructure/topology?site=...`
- `GET /infrastructure/capabilities?resource_id=...`

### Manifests and reconciliation
- `POST /manifests`, `GET /manifests/{id}`
- `POST /manifests/{id}/validate`
- `POST /manifests/{id}/plan`
- `POST /manifests/{id}/apply`
- `GET /manifests/{id}/diff`
- `POST /reconciliations`, `GET /reconciliations/{run_id}`
- `POST /reconciliations/{run_id}/resolve`

### Providers, operations and qualification
- `GET /providers`, `GET /providers/{id}/health`, `GET /providers/{id}/capabilities`
- `POST /operations`, `GET /jobs/{job_id}`, `POST /jobs/{job_id}/cancel`
- `GET /tests`, `POST /campaigns`, `GET /campaigns/{id}`, `POST /campaigns/{id}/run`
- `GET /evidence/{id}`, `GET /resources/{id}/observations`

Mutating or long-running actions return auditable job/operation references. Do not expose arbitrary shell execution as a general API. Map paths to existing router conventions after code inspection.

## Implemented first slice

The initial code slice exposes two authenticated, read-only endpoints backed by the existing tenant-scoped runner repository:

- `GET /api/v1/infrastructure/inventory` returns normalized registered runners. Resource identity is `runner:<runner-uuid>`; probe facts are attributed to `edgefabric.runner_probe`, otherwise registration facts are attributed to `edgefabric.runner_registration`.
- `POST /api/v1/infrastructure/reconcile` accepts a complete declaration set for the tenant's intended runner scope and compares it with the registered runner snapshot.

Example request:

```json
{
  "declarations": [
    {
      "identity": "runner:00000000-0000-0000-0000-000000000001",
      "kind": "host",
      "expected": {
        "host_os": "linux",
        "architecture": "x86_64",
        "capabilities": ["ssh", "linux_runner"]
      }
    }
  ]
}
```

The current endpoint intentionally marks discovery coverage incomplete because registered runners are not equivalent to a complete physical-site/network scan. Therefore an absent declaration target is reported as partially discovered, not conclusively missing. Discovered runners outside the submitted declaration set are unexpected within that declared scope. Versioned tenant-scoped manifest storage is implemented at `GET/POST /api/v1/infrastructure/manifests` and `GET /api/v1/infrastructure/manifests/{id}`. `POST /api/v1/infrastructure/manifests/validate` performs non-persistent structural validation, including empty and duplicate identities, and create applies the same checks before persistence. `POST /api/v1/infrastructure/manifests/{id}/reconcile` reconciles a stored revision against the current registered-runner inventory. Each `(tenant, name, version)` is immutable and unique; a new revision is created with a higher version. Full multi-source graph assembly and site-wide coverage tracking remain subsequent work.

## Virtual infrastructure and spatial simulation registry

A tenant-scoped simulation registry now stores immutable simulation input documents separately from physical inventory:

- `POST /api/v1/infrastructure/simulations/validate` validates a simulation envelope, graph endpoints, geometry references and spatial transforms without persisting or executing it.
- `POST /api/v1/infrastructure/simulations` stores a validated scenario input under the authenticated tenant. Simulation identity is unique within that tenant.
- `GET /api/v1/infrastructure/simulations` lists the tenant's scenarios; optional `scenario_id` filters the result.
- `GET /api/v1/infrastructure/simulations/{id}` retrieves a tenant-owned simulation record.

A deterministic GPU-cluster template expander is available at `POST /api/v1/infrastructure/simulations/templates/gpu-cluster`. It creates tenant-owned synthetic host, PCIe root-complex, PCIe-switch and GPU resources, relationships and PCIe links from bounded template inputs. Optional GPU peer fabrics, NIC/DPU network fabrics, storage and CXL memory are expanded as distinct typed topologies. Storage supports direct NVMe drives or a shared NVMe-oF TCP/RDMA or Fibre Channel array path; CXL expansion creates per-host memory-expander resources with NUMA attachment metadata. The output reports PCIe, peer, network, storage and CXL topology counts. Switch downstream lane budget and PCIe uplink oversubscription ratio are structural attributes, not performance measurements. The expander is structural only; it does not instantiate a GPU emulator or execute workloads.

The document carries simulation kind (hardware emulation, topology, capacity, power/thermal or spatial visualization), scenario and engine versions, deterministic seed, optional source-manifest revision, a normalized resource graph, optional typed PCIe topology, content-addressed geometry references and per-resource poses in explicit coordinate frames. PCIe topology models root complexes, switches, bridges and endpoints (including GPUs, NICs/DPUs and storage devices), NUMA locality, device generation/lane capabilities, and negotiated point-to-point links. Validation rejects dangling/duplicate identities and links exceeding endpoint generation or lane-width capabilities. Geometry bytes remain in EdgeFabric artifact/object storage; the registry stores immutable references and hashes.

This is a scenario registry and structural topology generator, not a simulator runtime. It does not create device emulators, execute workloads, mutate physical inventory or produce qualification evidence. Simulated resources remain explicitly classified as simulated. Follow-on work includes rack power/cooling constraints, provider-backed simulation adapters and a 3D/volume viewer consuming graph and geometry APIs.

## Event envelope

Use the existing EdgeFabric event mechanism. Logical envelope:

```json
{
  "event_id": "uuid",
  "event_type": "infrastructure.reconciliation.completed",
  "schema_version": "1",
  "tenant_id": "tenant",
  "site_id": "site",
  "resource_id": null,
  "job_id": "job",
  "correlation_id": "trace-or-request",
  "sequence": 42,
  "occurred_at": "RFC3339",
  "producer": {"id": "reconciler", "version": "x.y.z"},
  "payload": {}
}
```

## Event families

- `infrastructure.resource.discovered|updated|stale|removed`
- `infrastructure.relationship.discovered|changed`
- `infrastructure.manifest.created|validated|planned|applied`
- `infrastructure.reconciliation.started|difference|completed|failed`
- `infrastructure.identity.conflict`
- `infrastructure.provider.registered|health_changed|capability_changed`
- `infrastructure.job.accepted|started|progress|completed|failed|cancelled`
- `infrastructure.observation.recorded`
- `infrastructure.qualification.started|test_completed|completed`
- `infrastructure.evidence.created`
- `infrastructure.approval.requested|granted|rejected`

Events are notifications, not the sole source of truth. Consumers detect sequence gaps and resync from APIs. Persist job state and evidence references before publishing terminal events.

## Operation request

An operation names target resource IDs or bounded selector, capability ID/version constraint, typed parameters, deadline, idempotency key, requested safety class, evidence requirements and any maintenance-window/approval reference. The server resolves the provider; clients cannot select arbitrary executables. The plan pins provider/capability versions and records preconditions.


### GPU peer-fabric template options

The GPU-cluster template accepts optional `gpu_peer_fabric`: `nv_link_full_mesh` generates direct per-host GPU peer edges, while `nv_switch` generates a synthetic per-host NVSwitch resource and GPU-to-switch peer edges. These edges are validated and counted independently of PCIe links. They are structural abstractions only; the API does not claim NVLink generation, bandwidth, switch radix or measured performance.


### NIC/DPU and network fabric template options

The GPU-cluster template optionally accepts `network_fabric` with `kind` (`ethernet`, `roce`, `infini_band`), `nics_per_host`, `ports_per_nic`, `link_speed_gbps`, `switch_ports` and `dpu_per_host`. The response includes `network_fabric_kind` and `network_fabric_link_count`. Network links are modeled separately from PCIe and GPU peer-fabric links; no throughput or protocol qualification is inferred.
