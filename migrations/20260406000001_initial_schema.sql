-- EdgeFabric — Initial Schema
-- Covers: tenants, devices, sessions, jobs, audit_log

-- Tenants (multi-org support)
CREATE TABLE tenants (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL UNIQUE,
    api_key     TEXT NOT NULL UNIQUE,
    max_sessions INTEGER NOT NULL DEFAULT 10,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Extensions
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Enum types
CREATE TYPE device_os      AS ENUM ('android', 'ios', 'iot');
CREATE TYPE device_kind    AS ENUM ('real', 'virtual');
CREATE TYPE device_status  AS ENUM ('available', 'busy', 'offline', 'maintenance');
CREATE TYPE session_status AS ENUM ('requested', 'queued', 'allocating', 'active',
                                    'streaming', 'testing', 'releasing', 'resetting',
                                    'completed', 'failed');
CREATE TYPE job_status     AS ENUM ('queued', 'running', 'completed', 'failed', 'cancelled');

-- Devices
CREATE TABLE devices (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    os              device_os NOT NULL,
    os_version      TEXT NOT NULL DEFAULT '',
    kind            device_kind NOT NULL DEFAULT 'real',
    status          device_status NOT NULL DEFAULT 'offline',
    region          TEXT NOT NULL DEFAULT 'default',
    capabilities    TEXT[] NOT NULL DEFAULT '{}',
    agent_id        UUID,
    registered_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_heartbeat  TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_devices_status ON devices (status);
CREATE INDEX idx_devices_os     ON devices (os);
CREATE INDEX idx_devices_region ON devices (region);

-- Sessions
CREATE TABLE sessions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id   UUID NOT NULL REFERENCES devices(id),
    tenant_id   UUID NOT NULL REFERENCES tenants(id),
    status      session_status NOT NULL DEFAULT 'requested',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    ended_at    TIMESTAMPTZ,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_sessions_device  ON sessions (device_id);
CREATE INDEX idx_sessions_tenant  ON sessions (tenant_id);
CREATE INDEX idx_sessions_status  ON sessions (status);

-- Test Jobs
CREATE TABLE jobs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL REFERENCES tenants(id),
    status        job_status NOT NULL DEFAULT 'queued',
    parallelism   INTEGER NOT NULL DEFAULT 1,
    total_tests   INTEGER NOT NULL DEFAULT 0,
    passed        INTEGER NOT NULL DEFAULT 0,
    failed        INTEGER NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at    TIMESTAMPTZ,
    completed_at  TIMESTAMPTZ,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_jobs_tenant ON jobs (tenant_id);
CREATE INDEX idx_jobs_status ON jobs (status);

-- Audit Log (immutable append-only)
CREATE TABLE audit_log (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   UUID REFERENCES tenants(id),
    actor       TEXT NOT NULL,
    action      TEXT NOT NULL,
    resource    TEXT NOT NULL,
    resource_id UUID,
    detail      JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_tenant   ON audit_log (tenant_id);
CREATE INDEX idx_audit_resource ON audit_log (resource, resource_id);
CREATE INDEX idx_audit_time     ON audit_log (created_at);

-- Updated-at trigger function
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_tenants_updated  BEFORE UPDATE ON tenants  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_devices_updated  BEFORE UPDATE ON devices  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_sessions_updated BEFORE UPDATE ON sessions FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_jobs_updated     BEFORE UPDATE ON jobs     FOR EACH ROW EXECUTE FUNCTION set_updated_at();
