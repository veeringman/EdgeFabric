-- M6 Mini ML fabric: nodes, clusters, pipeline/replica/diloco jobs, multi-runner leases.

CREATE TABLE IF NOT EXISTS ml_nodes (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID REFERENCES tenants(id) ON DELETE CASCADE,
    runner_id           UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    chip                TEXT NOT NULL DEFAULT '',
    cpu_cores           INTEGER,
    gpu_cores           INTEGER,
    ane_cores           INTEGER,
    unified_memory_gb   INTEGER,
    link_mbps           INTEGER NOT NULL DEFAULT 2500,
    mlx_version         TEXT NOT NULL DEFAULT '',
    free_memory_gb      DOUBLE PRECISION,
    worker_host         TEXT NOT NULL DEFAULT '',
    worker_http_port    INTEGER NOT NULL DEFAULT 8090,
    worker_dataplane_port INTEGER NOT NULL DEFAULT 8091,
    pipeline_rank       INTEGER,
    labels              TEXT[] NOT NULL DEFAULT '{}',
    status              TEXT NOT NULL DEFAULT 'offline',
    last_heartbeat_at   TIMESTAMPTZ,
    heartbeat           JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_ml_nodes_runner UNIQUE (runner_id)
);

CREATE INDEX IF NOT EXISTS idx_ml_nodes_tenant ON ml_nodes (tenant_id);
CREATE INDEX IF NOT EXISTS idx_ml_nodes_status ON ml_nodes (status);

CREATE TABLE IF NOT EXISTS ml_clusters (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    role                TEXT NOT NULL DEFAULT 'pipeline',
    interconnect_cidrs  TEXT[] NOT NULL DEFAULT '{}',
    rank0_url           TEXT NOT NULL DEFAULT '',
    status              TEXT NOT NULL DEFAULT 'idle',
    config              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_ml_clusters_role CHECK (role IN ('pipeline', 'replica', 'diloco')),
    CONSTRAINT uq_ml_clusters_tenant_name UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_ml_clusters_tenant ON ml_clusters (tenant_id);

CREATE TABLE IF NOT EXISTS ml_cluster_members (
    cluster_id          UUID NOT NULL REFERENCES ml_clusters(id) ON DELETE CASCADE,
    runner_id           UUID NOT NULL REFERENCES runners(id) ON DELETE CASCADE,
    node_id             UUID REFERENCES ml_nodes(id) ON DELETE SET NULL,
    lease_id            UUID REFERENCES runner_leases(id) ON DELETE SET NULL,
    rank                INTEGER NOT NULL,
    dataplane_host      TEXT NOT NULL DEFAULT '',
    dataplane_port      INTEGER NOT NULL DEFAULT 8091,
    http_port           INTEGER NOT NULL DEFAULT 8090,
    PRIMARY KEY (cluster_id, rank),
    CONSTRAINT uq_ml_cluster_member_runner UNIQUE (cluster_id, runner_id)
);

CREATE TABLE IF NOT EXISTS ml_jobs (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    cluster_id          UUID REFERENCES ml_clusters(id) ON DELETE SET NULL,
    kind                TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'queued',
    model_id            TEXT NOT NULL DEFAULT 'toy-pp',
    quantization        TEXT NOT NULL DEFAULT 'q4',
    context_length      INTEGER NOT NULL DEFAULT 2048,
    parallelism         JSONB NOT NULL DEFAULT '{}'::jsonb,
    metrics             JSONB NOT NULL DEFAULT '{}'::jsonb,
    config              JSONB NOT NULL DEFAULT '{}'::jsonb,
    error_message       TEXT,
    requested_by        TEXT NOT NULL DEFAULT 'system',
    queued_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at          TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_ml_jobs_kind CHECK (kind IN ('infer_serve', 'train_lora', 'train_diloco', 'train_pipeline')),
    CONSTRAINT chk_ml_jobs_status CHECK (status IN ('queued', 'running', 'serving', 'succeeded', 'failed', 'cancelled')),
    CONSTRAINT chk_ml_jobs_context CHECK (context_length > 0)
);

CREATE INDEX IF NOT EXISTS idx_ml_jobs_tenant_status ON ml_jobs (tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_ml_jobs_cluster ON ml_jobs (cluster_id);

CREATE TRIGGER trg_ml_nodes_updated
    BEFORE UPDATE ON ml_nodes
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_ml_clusters_updated
    BEFORE UPDATE ON ml_clusters
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_ml_jobs_updated
    BEFORE UPDATE ON ml_jobs
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();
