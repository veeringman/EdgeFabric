use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InfrastructureHardwareProfileRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub profile_id: String,
    pub version: i32,
    pub qualification: String,
    pub review_status: String,
    pub document: Value,
    pub created_at: DateTime<Utc>,
}

pub struct InfrastructureHardwareProfileRepo;

impl InfrastructureHardwareProfileRepo {
    pub async fn create(
        pool: &PgPool,
        tenant_id: Uuid,
        profile_id: &str,
        version: i32,
        qualification: &str,
        document: &Value,
    ) -> Result<InfrastructureHardwareProfileRow, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureHardwareProfileRow>(
            r#"INSERT INTO infrastructure_hardware_profiles
                   (tenant_id, profile_id, version, qualification, document)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING id, tenant_id, profile_id, version, qualification,
                         review_status, document, created_at"#,
        )
        .bind(tenant_id)
        .bind(profile_id)
        .bind(version)
        .bind(qualification)
        .bind(document)
        .fetch_one(pool)
        .await
    }

    pub async fn list(
        pool: &PgPool,
        tenant_id: Uuid,
        profile_id: Option<&str>,
    ) -> Result<Vec<InfrastructureHardwareProfileRow>, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureHardwareProfileRow>(
            r#"SELECT id, tenant_id, profile_id, version, qualification,
                      review_status, document, created_at
               FROM infrastructure_hardware_profiles
               WHERE tenant_id = $1 AND ($2::text IS NULL OR profile_id = $2)
               ORDER BY profile_id, version DESC"#,
        )
        .bind(tenant_id)
        .bind(profile_id)
        .fetch_all(pool)
        .await
    }

    /// One-way draft -> submitted transition. Approval/rejection is intentionally
    /// not exposed here; those require a separately authorized review workflow.
    pub async fn submit(
        pool: &PgPool,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<InfrastructureHardwareProfileRow>, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureHardwareProfileRow>(
            r#"UPDATE infrastructure_hardware_profiles
               SET review_status = 'submitted'
               WHERE tenant_id = $1 AND id = $2 AND review_status = 'draft'
               RETURNING id, tenant_id, profile_id, version, qualification,
                         review_status, document, created_at"#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn get(
        pool: &PgPool,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<InfrastructureHardwareProfileRow>, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureHardwareProfileRow>(
            r#"SELECT id, tenant_id, profile_id, version, qualification,
                      review_status, document, created_at
               FROM infrastructure_hardware_profiles
               WHERE tenant_id = $1 AND id = $2"#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(pool)
        .await
    }
}
