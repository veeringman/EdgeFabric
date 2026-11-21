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
