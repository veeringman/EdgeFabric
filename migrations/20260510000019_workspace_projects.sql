-- Persistent storage for in-browser IDE workspace projects.
-- Each tenant can save and reload named projects so users can return to
-- the latest state of an in-progress workspace from any session.

CREATE TABLE IF NOT EXISTS workspace_projects (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    files       JSONB NOT NULL DEFAULT '[]'::jsonb,
    file_count  INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT workspace_projects_tenant_name_uq UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_workspace_projects_tenant_updated_at
    ON workspace_projects (tenant_id, updated_at DESC);
