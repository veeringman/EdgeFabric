use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VirtualIotBackendRow {
    pub id: Uuid,
    pub name: String,
    pub provider: String,
    pub controller_type: String,
    pub region: String,
    pub status: String,
    pub vm_name: Option<String>,
    pub shell_host: Option<String>,
    pub shell_port: Option<i32>,
    pub config: serde_json::Value,
    pub capabilities: Vec<String>,
    pub attached_device_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct VirtualIotBackendRepo;

impl VirtualIotBackendRepo {
    pub async fn create(
        pool: &PgPool,
        name: &str,
        provider: &str,
        controller_type: &str,
        region: &str,
        status: &str,
        vm_name: Option<&str>,
        shell_host: Option<&str>,
        shell_port: Option<i32>,
        config: &serde_json::Value,
        capabilities: &[String],
    ) -> Result<VirtualIotBackendRow, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotBackendRow>(
            r#"INSERT INTO virtual_iot_backends
               (name, provider, controller_type, region, status, vm_name, shell_host, shell_port, config, capabilities)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               RETURNING id, name, provider, controller_type, region, status, vm_name, shell_host, shell_port,
                         config, capabilities, attached_device_id, created_at, updated_at"#,
        )
        .bind(name)
        .bind(provider)
        .bind(controller_type)
        .bind(region)
        .bind(status)
        .bind(vm_name)
        .bind(shell_host)
        .bind(shell_port)
        .bind(config)
        .bind(capabilities)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<VirtualIotBackendRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotBackendRow>(
            r#"SELECT id, name, provider, controller_type, region, status, vm_name, shell_host, shell_port,
                      config, capabilities, attached_device_id, created_at, updated_at
               FROM virtual_iot_backends WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list(
        pool: &PgPool,
        controller_type: Option<&str>,
        region: Option<&str>,
        only_available: bool,
    ) -> Result<Vec<VirtualIotBackendRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotBackendRow>(
            r#"SELECT id, name, provider, controller_type, region, status, vm_name, shell_host, shell_port,
                      config, capabilities, attached_device_id, created_at, updated_at
               FROM virtual_iot_backends
               WHERE ($1::text IS NULL OR controller_type = $1)
                 AND ($2::text IS NULL OR region = $2)
                 AND ($3::bool = false OR status = 'available')
               ORDER BY name"#,
        )
        .bind(controller_type)
        .bind(region)
        .bind(only_available)
        .fetch_all(pool)
        .await
    }

    pub async fn update_status_and_attachment(
        pool: &PgPool,
        id: Uuid,
        status: &str,
        attached_device_id: Option<Uuid>,
    ) -> Result<Option<VirtualIotBackendRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotBackendRow>(
            r#"UPDATE virtual_iot_backends
               SET status = $2, attached_device_id = $3, updated_at = now()
               WHERE id = $1
               RETURNING id, name, provider, controller_type, region, status, vm_name, shell_host, shell_port,
                         config, capabilities, attached_device_id, created_at, updated_at"#,
        )
        .bind(id)
        .bind(status)
        .bind(attached_device_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM virtual_iot_backends WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}