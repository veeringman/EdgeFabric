use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// DB row representation of an edge agent.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentRow {
    pub id: Uuid,
    pub name: String,
    pub region: String,
    pub streaming_url: Option<String>,
    pub status: String,
    pub last_heartbeat: DateTime<Utc>,
    pub registered_at: DateTime<Utc>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

pub struct AgentRepo;

impl AgentRepo {
    /// Upsert an agent — insert or update on conflict.
    pub async fn upsert(
        pool: &PgPool,
        id: Uuid,
        name: &str,
        region: &str,
        streaming_url: Option<&str>,
        capabilities: &[String],
    ) -> sqlx::Result<AgentRow> {
        sqlx::query_as::<_, AgentRow>(
            r#"
            INSERT INTO agents (id, name, region, streaming_url, status, last_heartbeat, capabilities)
            VALUES ($1, $2, $3, $4, 'online', now(), $5)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                region = EXCLUDED.region,
                streaming_url = EXCLUDED.streaming_url,
                status = 'online',
                last_heartbeat = now(),
                capabilities = CASE
                    WHEN array_length(EXCLUDED.capabilities, 1) IS NULL THEN agents.capabilities
                    ELSE EXCLUDED.capabilities
                END
            RETURNING id, name, region, streaming_url, status, last_heartbeat, registered_at, capabilities
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(region)
        .bind(streaming_url)
        .bind(capabilities)
        .fetch_one(pool)
        .await
    }

    /// Find an agent by ID.
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<AgentRow>> {
        sqlx::query_as::<_, AgentRow>(
            "SELECT id, name, region, streaming_url, status, last_heartbeat, registered_at, capabilities FROM agents WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    /// List all agents, optionally filtered by required capability and/or region.
    pub async fn list(
        pool: &PgPool,
        capability: Option<&str>,
        region: Option<&str>,
        only_online: bool,
    ) -> sqlx::Result<Vec<AgentRow>> {
        sqlx::query_as::<_, AgentRow>(
            r#"SELECT id, name, region, streaming_url, status, last_heartbeat, registered_at, capabilities
               FROM agents
               WHERE ($1::text IS NULL OR $1 = ANY(capabilities))
                 AND ($2::text IS NULL OR region = $2)
                 AND ($3::bool = false OR status = 'online')
               ORDER BY name"#,
        )
        .bind(capability)
        .bind(region)
        .bind(only_online)
        .fetch_all(pool)
        .await
    }

    /// Update heartbeat timestamp.
    pub async fn heartbeat(pool: &PgPool, id: Uuid) -> sqlx::Result<bool> {
        let result = sqlx::query("UPDATE agents SET last_heartbeat = now(), status = 'online' WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Mark agent as offline.
    pub async fn set_offline(pool: &PgPool, id: Uuid) -> sqlx::Result<bool> {
        let result = sqlx::query("UPDATE agents SET status = 'offline' WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
