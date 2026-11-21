-- Tenant-scoped, immutable simulation scenario records. Simulation data is
-- explicitly separated from physical inventory and qualification evidence.
CREATE TABLE IF NOT EXISTS infrastructure_simulations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    identity TEXT NOT NULL,
    simulation_kind TEXT NOT NULL,
    scenario_id TEXT NOT NULL,
    engine TEXT NOT NULL,
    engine_version TEXT NOT NULL,
    seed BIGINT NOT NULL,
    source_manifest_revision TEXT,
    document JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, identity)
);

CREATE INDEX IF NOT EXISTS idx_infrastructure_simulations_tenant_created
    ON infrastructure_simulations (tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_infrastructure_simulations_tenant_scenario
    ON infrastructure_simulations (tenant_id, scenario_id, created_at DESC);
