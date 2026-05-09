use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IntegrationEndpointRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub transport: String,
    pub kind: String,
    pub enabled: bool,
    pub config: Value,
    pub last_error: Option<String>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IntegrationDeliveryAttemptRow {
    pub id: Uuid,
    pub endpoint_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub sensor: Option<String>,
    pub payload: Value,
    pub status: String,
    pub attempt: i32,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct IntegrationRepo;

impl IntegrationRepo {
    pub async fn create_endpoint(
        pool: &PgPool,
        tenant_id: Uuid,
        name: &str,
        transport: &str,
        kind: &str,
        config: &Value,
    ) -> Result<IntegrationEndpointRow, sqlx::Error> {
        sqlx::query_as::<_, IntegrationEndpointRow>(
            r#"INSERT INTO integration_endpoints
                  (tenant_id, name, transport, kind, enabled, config)
               VALUES
                  ($1, $2, $3, $4, TRUE, $5)
               RETURNING id, tenant_id, name, transport, kind, enabled, config,
                         last_error, last_success_at, created_at, updated_at"#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(transport)
        .bind(kind)
        .bind(config)
        .fetch_one(pool)
        .await
    }

    pub async fn list_by_tenant_transport(
        pool: &PgPool,
        tenant_id: Uuid,
        transport: &str,
    ) -> Result<Vec<IntegrationEndpointRow>, sqlx::Error> {
        sqlx::query_as::<_, IntegrationEndpointRow>(
            r#"SELECT id, tenant_id, name, transport, kind, enabled, config,
                      last_error, last_success_at, created_at, updated_at
               FROM integration_endpoints
               WHERE tenant_id = $1
                 AND transport = $2
               ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .bind(transport)
        .fetch_all(pool)
        .await
    }

    pub async fn list_enabled_by_transport(
        pool: &PgPool,
        transport: &str,
    ) -> Result<Vec<IntegrationEndpointRow>, sqlx::Error> {
        sqlx::query_as::<_, IntegrationEndpointRow>(
            r#"SELECT id, tenant_id, name, transport, kind, enabled, config,
                      last_error, last_success_at, created_at, updated_at
               FROM integration_endpoints
               WHERE transport = $1
                 AND enabled = TRUE
               ORDER BY created_at ASC"#,
        )
        .bind(transport)
        .fetch_all(pool)
        .await
    }

    pub async fn list_enabled_by_tenant_transport(
        pool: &PgPool,
        tenant_id: Uuid,
        transport: &str,
    ) -> Result<Vec<IntegrationEndpointRow>, sqlx::Error> {
        sqlx::query_as::<_, IntegrationEndpointRow>(
            r#"SELECT id, tenant_id, name, transport, kind, enabled, config,
                      last_error, last_success_at, created_at, updated_at
               FROM integration_endpoints
               WHERE tenant_id = $1
                 AND transport = $2
                 AND enabled = TRUE
               ORDER BY created_at ASC"#,
        )
        .bind(tenant_id)
        .bind(transport)
        .fetch_all(pool)
        .await
    }

    pub async fn delete_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let affected = sqlx::query(
            r#"DELETE FROM integration_endpoints
               WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await?
        .rows_affected();

        Ok(affected > 0)
    }

    pub async fn mark_delivery_success(
        pool: &PgPool,
        endpoint_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE integration_endpoints
               SET last_error = NULL,
                   last_success_at = NOW(),
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(endpoint_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn mark_delivery_error(
        pool: &PgPool,
        endpoint_id: Uuid,
        error_message: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE integration_endpoints
               SET last_error = $2,
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(endpoint_id)
        .bind(error_message)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn create_delivery_attempt(
        pool: &PgPool,
        endpoint_id: Uuid,
        tenant_id: Option<Uuid>,
        device_id: Option<Uuid>,
        sensor: Option<&str>,
        payload: &Value,
        status: &str,
        attempt: i32,
        error_message: Option<&str>,
    ) -> Result<IntegrationDeliveryAttemptRow, sqlx::Error> {
        sqlx::query_as::<_, IntegrationDeliveryAttemptRow>(
            r#"INSERT INTO integration_delivery_attempts
                  (endpoint_id, tenant_id, device_id, sensor, payload, status, attempt, error_message)
               VALUES
                  ($1, $2, $3, $4, $5, $6, $7, $8)
               RETURNING id, endpoint_id, tenant_id, device_id, sensor, payload,
                         status, attempt, error_message, created_at"#,
        )
        .bind(endpoint_id)
        .bind(tenant_id)
        .bind(device_id)
        .bind(sensor)
        .bind(payload)
        .bind(status)
        .bind(attempt)
        .bind(error_message)
        .fetch_one(pool)
        .await
    }

    pub async fn list_delivery_attempts_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<IntegrationDeliveryAttemptRow>, sqlx::Error> {
        sqlx::query_as::<_, IntegrationDeliveryAttemptRow>(
            r#"SELECT d.id, d.endpoint_id, d.tenant_id, d.device_id, d.sensor, d.payload,
                      d.status, d.attempt, d.error_message, d.created_at
               FROM integration_delivery_attempts d
               LEFT JOIN integration_endpoints e ON e.id = d.endpoint_id
               WHERE e.tenant_id = $1
                 AND ($2::text IS NULL OR d.status = $2)
               ORDER BY d.created_at DESC
               LIMIT $3"#,
        )
        .bind(tenant_id)
        .bind(status)
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}
