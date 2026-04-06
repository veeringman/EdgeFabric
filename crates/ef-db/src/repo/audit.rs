use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub tenant_id: Option<Uuid>,
    pub actor: String,
    pub action: String,
    pub resource: String,
    pub resource_id: Option<Uuid>,
    pub detail: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

pub struct AuditRepo;

impl AuditRepo {
    pub async fn insert(
        pool: &PgPool,
        tenant_id: Option<Uuid>,
        actor: &str,
        action: &str,
        resource: &str,
        resource_id: Option<Uuid>,
        detail: Option<serde_json::Value>,
    ) -> Result<AuditEntry, sqlx::Error> {
        sqlx::query_as::<_, AuditEntry>(
            r#"INSERT INTO audit_log (tenant_id, actor, action, resource, resource_id, detail)
               VALUES ($1, $2, $3, $4, $5, $6)
               RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(actor)
        .bind(action)
        .bind(resource)
        .bind(resource_id)
        .bind(detail)
        .fetch_one(pool)
        .await
    }

    pub async fn list_by_resource(
        pool: &PgPool,
        resource: &str,
        resource_id: Uuid,
    ) -> Result<Vec<AuditEntry>, sqlx::Error> {
        sqlx::query_as::<_, AuditEntry>(
            r#"SELECT * FROM audit_log
               WHERE resource = $1 AND resource_id = $2
               ORDER BY created_at DESC"#,
        )
        .bind(resource)
        .bind(resource_id)
        .fetch_all(pool)
        .await
    }

    pub async fn list_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, sqlx::Error> {
        sqlx::query_as::<_, AuditEntry>(
            r#"SELECT * FROM audit_log
               WHERE tenant_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}
