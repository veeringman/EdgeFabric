-- Runner leases for concurrency-safe allocation.
-- A runner can have up to max_concurrency active leases.

CREATE TYPE runner_lease_status AS ENUM (
    'active',
    'released',
    'expired'
);

CREATE TABLE IF NOT EXISTS runner_leases (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    runner_id       UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    tenant_id       UUID REFERENCES tenants(id) ON DELETE CASCADE,
    build_job_id    UUID REFERENCES build_jobs(id) ON DELETE SET NULL,
    status          runner_lease_status NOT NULL DEFAULT 'active',
    acquired_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ,
    released_at     TIMESTAMPTZ,
    metadata        JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_runner_leases_runner_status
    ON runner_leases (runner_id, status);

CREATE INDEX IF NOT EXISTS idx_runner_leases_tenant_status
    ON runner_leases (tenant_id, status);

CREATE INDEX IF NOT EXISTS idx_runner_leases_expires_at
    ON runner_leases (expires_at)
    WHERE status = 'active';
