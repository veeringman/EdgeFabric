-- Runner and Build Job schema for Internet-facing external integrations.
-- Covers:
-- 1) SSH/macOS runner inventory and capability-based scheduling
-- 2) Credential reference mapping (no raw secrets stored)
-- 3) iOS/CI build job lifecycle and artifact indexing

CREATE TYPE build_job_status AS ENUM (
    'queued',
    'provisioning',
    'running',
    'uploading',
    'succeeded',
    'failed',
    'cancelled'
);

CREATE TABLE IF NOT EXISTS runners (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID REFERENCES tenants(id) ON DELETE SET NULL,
    device_id           UUID REFERENCES devices(id) ON DELETE SET NULL,
    name                TEXT NOT NULL,
    runner_class        TEXT NOT NULL DEFAULT 'generic',
    host                TEXT NOT NULL,
    port                INTEGER NOT NULL DEFAULT 22,
    host_os             TEXT NOT NULL DEFAULT 'linux',
    architecture        TEXT NOT NULL DEFAULT '',
    region              TEXT NOT NULL DEFAULT 'default',
    labels              TEXT[] NOT NULL DEFAULT '{}',
    capabilities        TEXT[] NOT NULL DEFAULT '{}',
    max_concurrency     INTEGER NOT NULL DEFAULT 1,
    drained             BOOLEAN NOT NULL DEFAULT FALSE,
    status              TEXT NOT NULL DEFAULT 'online',
    health              JSONB NOT NULL DEFAULT '{}'::jsonb,
    last_probe_at       TIMESTAMPTZ,
    last_probe_result   JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_runners_port_range CHECK (port > 0 AND port <= 65535),
    CONSTRAINT chk_runners_max_concurrency CHECK (max_concurrency > 0)
);

CREATE INDEX IF NOT EXISTS idx_runners_tenant_id
    ON runners (tenant_id);

CREATE INDEX IF NOT EXISTS idx_runners_device_id
    ON runners (device_id);

CREATE INDEX IF NOT EXISTS idx_runners_status_drained_region
    ON runners (status, drained, region);

CREATE INDEX IF NOT EXISTS idx_runners_capabilities
    ON runners USING GIN (capabilities);

CREATE INDEX IF NOT EXISTS idx_runners_labels
    ON runners USING GIN (labels);

CREATE TABLE IF NOT EXISTS runner_credentials (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID REFERENCES tenants(id) ON DELETE CASCADE,
    runner_id           UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    auth_mode           TEXT NOT NULL DEFAULT 'ssh_key',
    credential_ref      TEXT NOT NULL,
    metadata            JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_runner_credentials_mode UNIQUE (runner_id, auth_mode)
);

CREATE INDEX IF NOT EXISTS idx_runner_credentials_tenant_id
    ON runner_credentials (tenant_id);

CREATE INDEX IF NOT EXISTS idx_runner_credentials_runner_id
    ON runner_credentials (runner_id);

CREATE TABLE IF NOT EXISTS build_jobs (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    runner_id           UUID REFERENCES runners(id) ON DELETE SET NULL,
    device_id           UUID REFERENCES devices(id) ON DELETE SET NULL,
    build_type          TEXT NOT NULL DEFAULT 'ios',
    status              build_job_status NOT NULL DEFAULT 'queued',
    requested_by        TEXT NOT NULL DEFAULT 'system',
    source_ref          TEXT NOT NULL DEFAULT '',
    repository_url      TEXT,
    git_ref             TEXT,
    config              JSONB NOT NULL DEFAULT '{}'::jsonb,
    runtime             JSONB NOT NULL DEFAULT '{}'::jsonb,
    logs_path           TEXT,
    error_message       TEXT,
    retry_count         INTEGER NOT NULL DEFAULT 0,
    queued_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at          TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_build_jobs_retry_count CHECK (retry_count >= 0)
);

CREATE INDEX IF NOT EXISTS idx_build_jobs_tenant_status
    ON build_jobs (tenant_id, status);

CREATE INDEX IF NOT EXISTS idx_build_jobs_runner_id
    ON build_jobs (runner_id);

CREATE INDEX IF NOT EXISTS idx_build_jobs_created_at
    ON build_jobs (created_at DESC);

CREATE TABLE IF NOT EXISTS build_artifacts (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    build_job_id        UUID NOT NULL REFERENCES build_jobs(id) ON DELETE CASCADE,
    tenant_id           UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    artifact_type       TEXT NOT NULL,
    file_name           TEXT NOT NULL,
    storage_path        TEXT NOT NULL,
    size_bytes          BIGINT NOT NULL DEFAULT 0,
    checksum_sha256     TEXT,
    metadata            JSONB NOT NULL DEFAULT '{}'::jsonb,
    retention_until     TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT chk_build_artifacts_size_non_negative CHECK (size_bytes >= 0)
);

CREATE INDEX IF NOT EXISTS idx_build_artifacts_build_job_id
    ON build_artifacts (build_job_id);

CREATE INDEX IF NOT EXISTS idx_build_artifacts_tenant_id
    ON build_artifacts (tenant_id);

CREATE INDEX IF NOT EXISTS idx_build_artifacts_retention_until
    ON build_artifacts (retention_until);

CREATE TRIGGER trg_runners_updated
    BEFORE UPDATE ON runners
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_runner_credentials_updated
    BEFORE UPDATE ON runner_credentials
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_build_jobs_updated
    BEFORE UPDATE ON build_jobs
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
