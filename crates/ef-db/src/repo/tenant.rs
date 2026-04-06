use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub api_key: String,
    pub max_sessions: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct TenantRepo;

impl TenantRepo {
    pub async fn create(pool: &PgPool, name: &str, api_key: &str) -> Result<Tenant, sqlx::Error> {
        sqlx::query_as::<_, Tenant>(
            r#"INSERT INTO tenants (name, api_key) VALUES ($1, $2) RETURNING *"#,
        )
        .bind(name)
        .bind(api_key)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Tenant>, sqlx::Error> {
        sqlx::query_as::<_, Tenant>(r#"SELECT * FROM tenants WHERE id = $1"#)
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn find_by_api_key(
        pool: &PgPool,
        api_key: &str,
    ) -> Result<Option<Tenant>, sqlx::Error> {
        sqlx::query_as::<_, Tenant>(r#"SELECT * FROM tenants WHERE api_key = $1"#)
            .bind(api_key)
            .fetch_optional(pool)
            .await
    }

    pub async fn list(pool: &PgPool) -> Result<Vec<Tenant>, sqlx::Error> {
        sqlx::query_as::<_, Tenant>(r#"SELECT * FROM tenants ORDER BY created_at"#)
            .fetch_all(pool)
            .await
    }
}
