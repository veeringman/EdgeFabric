-- Tenant-scoped binary geometry assets. The database stores metadata only;
-- bytes are stored by the configured local filesystem provider.
CREATE TABLE infrastructure_geometry_assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    format TEXT NOT NULL CHECK (format IN ('glb')),
    media_type TEXT NOT NULL CHECK (media_type = 'model/gltf-binary'),
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0 AND size_bytes <= 104857600),
    license TEXT NOT NULL,
    units TEXT,
    storage_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, sha256, format)
);
CREATE INDEX idx_infrastructure_geometry_assets_tenant
    ON infrastructure_geometry_assets (tenant_id, created_at DESC);
