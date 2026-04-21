-- Add capabilities array to agents so the control plane can route
-- capability-specific requests (veeros, android, ios, runner, ...) to
-- the right edge agent. Bind virtual_iot_backends to an owning agent
-- so VeerOS VM lifecycle can be proxied through that agent instead
-- of being executed on the API host.

ALTER TABLE agents
ADD COLUMN IF NOT EXISTS capabilities TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_agents_capabilities
    ON agents USING GIN (capabilities);

ALTER TABLE virtual_iot_backends
ADD COLUMN IF NOT EXISTS agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
ADD COLUMN IF NOT EXISTS agent_base_url TEXT;

CREATE INDEX IF NOT EXISTS idx_virtual_iot_backends_agent_id
    ON virtual_iot_backends (agent_id);
