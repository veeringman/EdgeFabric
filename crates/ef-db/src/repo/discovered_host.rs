use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// A VeerOS host discovered via UDP multicast advertisement from an ef-agent.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DiscoveredHostRow {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub host_id: String,
    pub hostname: String,
    pub ip_address: String,
    pub port: i32,
    pub os_version: Option<String>,
    pub capabilities: Vec<String>,
    pub quantum_capable: bool,
    pub quantum_qubits: Option<i32>,
    pub quantum_backends: Vec<String>,
    pub verified: bool,
    pub last_seen_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct DiscoveredHostRepo;

impl DiscoveredHostRepo {
    /// Upsert a discovered host (insert or update on conflict).
    pub async fn upsert(
        pool: &PgPool,
        agent_id: Uuid,
        host_id: &str,
        hostname: &str,
        ip_address: &str,
        port: i32,
        os_version: Option<&str>,
        capabilities: &[String],
        quantum_capable: bool,
        quantum_qubits: Option<i32>,
        quantum_backends: &[String],
        verified: bool,
    ) -> Result<DiscoveredHostRow, sqlx::Error> {
        sqlx::query_as::<_, DiscoveredHostRow>(
            r#"
            INSERT INTO discovered_hosts
                (agent_id, host_id, hostname, ip_address, port, os_version,
                 capabilities, quantum_capable, quantum_qubits, quantum_backends, verified, last_seen_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,NOW(),NOW())
            ON CONFLICT (agent_id, host_id) DO UPDATE SET
                hostname        = EXCLUDED.hostname,
                ip_address      = EXCLUDED.ip_address,
                port            = EXCLUDED.port,
                os_version      = EXCLUDED.os_version,
                capabilities    = EXCLUDED.capabilities,
                quantum_capable = EXCLUDED.quantum_capable,
                quantum_qubits  = EXCLUDED.quantum_qubits,
                quantum_backends= EXCLUDED.quantum_backends,
                verified        = EXCLUDED.verified,
                last_seen_at    = NOW(),
                updated_at      = NOW()
            RETURNING *
            "#,
        )
        .bind(agent_id)
        .bind(host_id)
        .bind(hostname)
        .bind(ip_address)
        .bind(port)
        .bind(os_version)
        .bind(capabilities)
        .bind(quantum_capable)
        .bind(quantum_qubits)
        .bind(quantum_backends)
        .bind(verified)
        .fetch_one(pool)
        .await
    }

    /// List all discovered hosts, optionally filtered to quantum-capable only.
    /// When the same host_id is reported by multiple agents, only the most
    /// recently seen record is returned (deduplicated by host_id).
    pub async fn list(
        pool: &PgPool,
        agent_id: Option<Uuid>,
        quantum_only: bool,
    ) -> Result<Vec<DiscoveredHostRow>, sqlx::Error> {
        sqlx::query_as::<_, DiscoveredHostRow>(
            r#"
            SELECT DISTINCT ON (host_id) *
            FROM discovered_hosts
            WHERE ($1::uuid IS NULL OR agent_id = $1)
              AND ($2 = FALSE OR quantum_capable = TRUE)
            ORDER BY host_id, last_seen_at DESC
            "#,
        )
        .bind(agent_id)
        .bind(quantum_only)
        .fetch_all(pool)
        .await
    }

    /// Get a single host by its database ID.
    pub async fn get(pool: &PgPool, id: Uuid) -> Result<Option<DiscoveredHostRow>, sqlx::Error> {
        sqlx::query_as::<_, DiscoveredHostRow>(
            "SELECT * FROM discovered_hosts WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// Mark a host as offline (sets last_seen_at to a long time ago so dashboards dim it).
    pub async fn mark_offline(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE discovered_hosts SET last_seen_at = '1970-01-01', updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Delete a discovered host record.
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM discovered_hosts WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Delete hosts from an agent not seen in the last N seconds.
    pub async fn prune_stale(
        pool: &PgPool,
        agent_id: Uuid,
        max_age_secs: i64,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            r#"DELETE FROM discovered_hosts
               WHERE agent_id = $1
                 AND last_seen_at < NOW() - ($2 || ' seconds')::INTERVAL"#,
        )
        .bind(agent_id)
        .bind(max_age_secs)
        .execute(pool)
        .await?;
        Ok(res.rows_affected())
    }
}
