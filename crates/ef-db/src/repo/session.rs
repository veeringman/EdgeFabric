use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub device_id: Uuid,
    pub tenant_id: Uuid,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

pub struct SessionRepo;

impl SessionRepo {
    pub async fn create(
        pool: &PgPool,
        device_id: Uuid,
        tenant_id: Uuid,
        expires_at: DateTime<Utc>,
    ) -> Result<SessionRow, sqlx::Error> {
        sqlx::query_as::<_, SessionRow>(
            r#"INSERT INTO sessions (device_id, tenant_id, expires_at)
               VALUES ($1, $2, $3)
               RETURNING id, device_id, tenant_id, status::text, created_at, expires_at, ended_at, updated_at"#,
        )
        .bind(device_id)
        .bind(tenant_id)
        .bind(expires_at)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<SessionRow>, sqlx::Error> {
        sqlx::query_as::<_, SessionRow>(
            r#"SELECT id, device_id, tenant_id, status::text, created_at, expires_at, ended_at, updated_at
               FROM sessions WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> Result<Vec<SessionRow>, sqlx::Error> {
        sqlx::query_as::<_, SessionRow>(
            r#"SELECT id, device_id, tenant_id, status::text, created_at, expires_at, ended_at, updated_at
               FROM sessions WHERE tenant_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
    }

    pub async fn set_status(
        pool: &PgPool,
        id: Uuid,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE sessions SET status = $2::session_status WHERE id = $1"#,
        )
        .bind(id)
        .bind(status)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn end_session(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE sessions SET status = 'completed'::session_status, ended_at = now() WHERE id = $1"#,
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn count_active_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM sessions
               WHERE tenant_id = $1 AND status IN ('active', 'streaming', 'testing')"#,
        )
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;
        Ok(row.0)
    }

    /// Find sessions that have passed their `expires_at` but are still in a non-terminal state.
    pub async fn list_expired(pool: &PgPool) -> Result<Vec<SessionRow>, sqlx::Error> {
        sqlx::query_as::<_, SessionRow>(
            r#"SELECT id, device_id, tenant_id, status::text, created_at, expires_at, ended_at, updated_at
               FROM sessions
               WHERE expires_at < now()
                 AND status NOT IN ('completed', 'failed')
               ORDER BY expires_at"#,
        )
        .fetch_all(pool)
        .await
    }

    /// Transition a session to a new status and return the updated row.
    pub async fn transition(
        pool: &PgPool,
        id: Uuid,
        from_status: &str,
        to_status: &str,
    ) -> Result<Option<SessionRow>, sqlx::Error> {
        sqlx::query_as::<_, SessionRow>(
            r#"UPDATE sessions
               SET status = $3::session_status, updated_at = now()
               WHERE id = $1 AND status = $2::session_status
               RETURNING id, device_id, tenant_id, status::text, created_at, expires_at, ended_at, updated_at"#,
        )
        .bind(id)
        .bind(from_status)
        .bind(to_status)
        .fetch_optional(pool)
        .await
    }

    /// List all active (non-terminal) sessions for a specific device.
    pub async fn list_active_by_device(
        pool: &PgPool,
        device_id: Uuid,
    ) -> Result<Vec<SessionRow>, sqlx::Error> {
        sqlx::query_as::<_, SessionRow>(
            r#"SELECT id, device_id, tenant_id, status::text, created_at, expires_at, ended_at, updated_at
               FROM sessions
               WHERE device_id = $1
                 AND status NOT IN ('completed', 'failed')
               ORDER BY created_at DESC"#,
        )
        .bind(device_id)
        .fetch_all(pool)
        .await
    }
}
