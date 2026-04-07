-- Add agents table to track edge agents with their streaming URLs.
-- Previously agent_id was just a UUID column on devices with no backing table.

CREATE TABLE agents (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    region          TEXT NOT NULL DEFAULT 'default',
    streaming_url   TEXT,                          -- e.g. http://192.168.29.6:7070
    status          TEXT NOT NULL DEFAULT 'online', -- online | offline
    last_heartbeat  TIMESTAMPTZ NOT NULL DEFAULT now(),
    registered_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_agents_status ON agents (status);
