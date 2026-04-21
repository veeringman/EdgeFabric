use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// DB row for a virtual IoT configuration.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VirtualIotConfigRow {
    pub device_id: Uuid,
    pub controller_type: String,
    pub enabled_sensors: serde_json::Value,
    pub backend_id: Option<Uuid>,
    pub backend: serde_json::Value,
    pub sim_rate_ms: i32,
    pub event_definitions: serde_json::Value,
    pub sensor_state: serde_json::Value,
    pub sensor_log: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct VirtualIotConfigRepo;

impl VirtualIotConfigRepo {
    pub async fn create(
        pool: &PgPool,
        device_id: Uuid,
        controller_type: &str,
        enabled_sensors: &serde_json::Value,
        backend_id: Option<Uuid>,
        backend: &serde_json::Value,
        sim_rate_ms: i32,
        event_definitions: &serde_json::Value,
        sensor_state: &serde_json::Value,
        sensor_log: &serde_json::Value,
    ) -> Result<VirtualIotConfigRow, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotConfigRow>(
            r#"INSERT INTO virtual_iot_configs (device_id, controller_type, enabled_sensors, backend_id, backend, sim_rate_ms, event_definitions, sensor_state, sensor_log)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING device_id, controller_type, enabled_sensors, backend_id, backend, sim_rate_ms, event_definitions, sensor_state, sensor_log, created_at, updated_at"#,
        )
        .bind(device_id)
        .bind(controller_type)
        .bind(enabled_sensors)
        .bind(backend_id)
        .bind(backend)
        .bind(sim_rate_ms)
        .bind(event_definitions)
        .bind(sensor_state)
        .bind(sensor_log)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_device_id(
        pool: &PgPool,
        device_id: Uuid,
    ) -> Result<Option<VirtualIotConfigRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotConfigRow>(
            r#"SELECT device_id, controller_type, enabled_sensors, backend_id, backend, sim_rate_ms, event_definitions, sensor_state, sensor_log, created_at, updated_at
               FROM virtual_iot_configs WHERE device_id = $1"#,
        )
        .bind(device_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn update(
        pool: &PgPool,
        device_id: Uuid,
        controller_type: &str,
        enabled_sensors: &serde_json::Value,
        backend_id: Option<Uuid>,
        backend: &serde_json::Value,
        sim_rate_ms: i32,
        event_definitions: &serde_json::Value,
    ) -> Result<Option<VirtualIotConfigRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotConfigRow>(
            r#"UPDATE virtual_iot_configs
               SET controller_type = $2, enabled_sensors = $3, backend_id = $4, backend = $5, sim_rate_ms = $6, event_definitions = $7, updated_at = now()
               WHERE device_id = $1
               RETURNING device_id, controller_type, enabled_sensors, backend_id, backend, sim_rate_ms, event_definitions, sensor_state, sensor_log, created_at, updated_at"#,
        )
        .bind(device_id)
        .bind(controller_type)
        .bind(enabled_sensors)
        .bind(backend_id)
        .bind(backend)
        .bind(sim_rate_ms)
        .bind(event_definitions)
        .fetch_optional(pool)
        .await
    }

    pub async fn update_backend(
        pool: &PgPool,
        device_id: Uuid,
        backend_id: Option<Uuid>,
        backend: &serde_json::Value,
    ) -> Result<Option<VirtualIotConfigRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotConfigRow>(
            r#"UPDATE virtual_iot_configs
               SET backend_id = $2, backend = $3, updated_at = now()
               WHERE device_id = $1
               RETURNING device_id, controller_type, enabled_sensors, backend_id, backend, sim_rate_ms, event_definitions, sensor_state, sensor_log, created_at, updated_at"#,
        )
        .bind(device_id)
        .bind(backend_id)
        .bind(backend)
        .fetch_optional(pool)
        .await
    }

    pub async fn update_sensor_runtime(
        pool: &PgPool,
        device_id: Uuid,
        sensor_state: &serde_json::Value,
        sensor_log: &serde_json::Value,
    ) -> Result<Option<VirtualIotConfigRow>, sqlx::Error> {
        sqlx::query_as::<_, VirtualIotConfigRow>(
            r#"UPDATE virtual_iot_configs
               SET sensor_state = $2, sensor_log = $3, updated_at = now()
               WHERE device_id = $1
               RETURNING device_id, controller_type, enabled_sensors, backend_id, backend, sim_rate_ms, event_definitions, sensor_state, sensor_log, created_at, updated_at"#,
        )
        .bind(device_id)
        .bind(sensor_state)
        .bind(sensor_log)
        .fetch_optional(pool)
        .await
    }

    pub async fn delete(pool: &PgPool, device_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM virtual_iot_configs WHERE device_id = $1")
            .bind(device_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
