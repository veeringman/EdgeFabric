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
    pub controller_types: Vec<String>,
    pub region: String,
    pub status: String,
    pub vm_name: Option<String>,
    pub shell_host: Option<String>,
    pub shell_port: Option<i32>,
    pub config: serde_json::Value,
    pub capabilities: Vec<String>,
    pub attached_device_id: Option<Uuid>,
    #[serde(default)]
    pub agent_id: Option<Uuid>,
    #[serde(default)]
    pub agent_base_url: Option<String>,
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
        controller_types: &[String],
        region: &str,
        status: &str,
        vm_name: Option<&str>,
        shell_host: Option<&str>,
        shell_port: Option<i32>,
        config: &serde_json::Value,
        capabilities: &[String],
        agent_id: Option<Uuid>,
        agent_base_url: Option<&str>,
    ) -> Result<VirtualIotBackendRow, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotBackendRow>(
            r#"INSERT INTO virtual_iot_backends
               (name, provider, controller_type, controller_types, region, status, vm_name, shell_host, shell_port, config, capabilities, agent_id, agent_base_url)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
               RETURNING id, name, provider, controller_type, controller_types, region, status, vm_name, shell_host, shell_port,
                         config, capabilities, attached_device_id, agent_id, agent_base_url, created_at, updated_at"#,
        )
        .bind(name)
        .bind(provider)
        .bind(controller_type)
        .bind(controller_types)
        .bind(region)
        .bind(status)
        .bind(vm_name)
        .bind(shell_host)
        .bind(shell_port)
        .bind(config)
        .bind(capabilities)
        .bind(agent_id)
        .bind(agent_base_url)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<VirtualIotBackendRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotBackendRow>(
            r#"SELECT id, name, provider, controller_type, controller_types, region, status, vm_name, shell_host, shell_port,
                      config, capabilities, attached_device_id, agent_id, agent_base_url, created_at, updated_at
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
            r#"SELECT id, name, provider, controller_type, controller_types, region, status, vm_name, shell_host, shell_port,
                      config, capabilities, attached_device_id, agent_id, agent_base_url, created_at, updated_at
               FROM virtual_iot_backends
               WHERE ($1::text IS NULL OR $1 = ANY(controller_types))
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

    pub async fn update(
        pool: &PgPool,
        id: Uuid,
        name: Option<&str>,
        region: Option<&str>,
        controller_types: Option<&[String]>,
    ) -> Result<Option<VirtualIotBackendRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotBackendRow>(
            r#"UPDATE virtual_iot_backends
               SET name             = COALESCE($2, name),
                   region           = COALESCE($3, region),
                   controller_type  = COALESCE(CASE WHEN array_length($4, 1) > 0 THEN ($4)[1] END, controller_type),
                   controller_types = COALESCE(NULLIF($4, '{}'::text[]), controller_types),
                   updated_at       = now()
               WHERE id = $1
               RETURNING id, name, provider, controller_type, controller_types, region, status, vm_name, shell_host, shell_port,
                         config, capabilities, attached_device_id, agent_id, agent_base_url, created_at, updated_at"#,
        )
        .bind(id)
        .bind(name)
        .bind(region)
        .bind(controller_types)
        .fetch_optional(pool)
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
               RETURNING id, name, provider, controller_type, controller_types, region, status, vm_name, shell_host, shell_port,
                         config, capabilities, attached_device_id, agent_id, agent_base_url, created_at, updated_at"#,
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