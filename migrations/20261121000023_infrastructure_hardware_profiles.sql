-- Tenant-scoped immutable hardware profile versions and review lifecycle.
CREATE TABLE IF NOT EXISTS infrastructure_hardware_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    profile_id TEXT NOT NULL CHECK (length(trim(profile_id)) BETWEEN 1 AND 160),
    version INTEGER NOT NULL CHECK (version > 0),
    qualification TEXT NOT NULL CHECK (qualification IN (
        'draft', 'template_only', 'vendor_documented', 'vendor_validated', 'field_verified'
    )),
    review_status TEXT NOT NULL DEFAULT 'draft' CHECK (review_status IN (
        'draft', 'submitted', 'approved', 'rejected'
    )),
    document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, profile_id, version)
);

CREATE INDEX IF NOT EXISTS idx_infrastructure_hardware_profiles_tenant_profile
    ON infrastructure_hardware_profiles (tenant_id, profile_id, version DESC);
CREATE INDEX IF NOT EXISTS idx_infrastructure_hardware_profiles_tenant_created
    ON infrastructure_hardware_profiles (tenant_id, created_at DESC);
