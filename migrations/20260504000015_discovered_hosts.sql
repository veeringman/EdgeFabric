-- VeerOS Host Advertisement: stores hosts discovered via UDP multicast advertisement
CREATE TABLE IF NOT EXISTS discovered_hosts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id        UUID        NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    host_id         TEXT        NOT NULL,   -- ed25519 pubkey fingerprint or fallback hostname
    hostname        TEXT        NOT NULL,
    ip_address      TEXT        NOT NULL,
    port            INTEGER     NOT NULL DEFAULT 2323,
    os_version      TEXT,
    capabilities    TEXT[]      NOT NULL DEFAULT '{}',
    quantum_capable BOOLEAN     NOT NULL DEFAULT FALSE,
    quantum_qubits  INTEGER,
    quantum_backends TEXT[]     NOT NULL DEFAULT '{}',
    verified        BOOLEAN     NOT NULL DEFAULT FALSE,  -- signature verified
    last_seen_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (agent_id, host_id)
);

CREATE INDEX IF NOT EXISTS idx_discovered_hosts_agent  ON discovered_hosts (agent_id);
CREATE INDEX IF NOT EXISTS idx_discovered_hosts_quantum ON discovered_hosts (agent_id, quantum_capable)
    WHERE quantum_capable = TRUE;
