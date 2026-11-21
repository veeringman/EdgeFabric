use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InfrastructureManifestRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub version: i32,
    pub description: String,
    pub document: Value,
    pub created_at: DateTime<Utc>,
}

pub struct InfrastructureManifestRepo;

impl InfrastructureManifestRepo {
    pub async fn create(
        pool: &PgPool,
        tenant_id: Uuid,
        name: &str,
        version: i32,
        description: &str,
        document: &Value,
    ) -> Result<InfrastructureManifestRow, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureManifestRow>(
            r#"INSERT INTO infrastructure_manifests
                   (tenant_id, name, version, description, document)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id, tenant_id, name, version, description, document, created_at"#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(version)
        .bind(description)
        .bind(document)
        .fetch_one(pool)
        .await
    }

    pub async fn list(
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> Result<Vec<InfrastructureManifestRow>, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureManifestRow>(
            r#"SELECT id, tenant_id, name, version, description, document, created_at
               FROM infrastructure_manifests
               WHERE tenant_id = $1
               ORDER BY name, version DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
    }

    pub async fn get(
        pool: &PgPool,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<InfrastructureManifestRow>, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureManifestRow>(
            r#"SELECT id, tenant_id, name, version, description, document, created_at
               FROM infrastructure_manifests
               WHERE tenant_id = $1 AND id = $2"#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(pool)
        .await
    }
}
