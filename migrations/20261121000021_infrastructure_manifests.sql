-- Versioned, tenant-scoped declarative infrastructure manifests.
CREATE TABLE IF NOT EXISTS infrastructure_manifests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    version INTEGER NOT NULL CHECK (version > 0),
    description TEXT NOT NULL DEFAULT '',
    document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, name, version)
);

CREATE INDEX IF NOT EXISTS idx_infrastructure_manifests_tenant_name
    ON infrastructure_manifests (tenant_id, name, version DESC);
