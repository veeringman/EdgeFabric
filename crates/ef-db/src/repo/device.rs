use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// DB row representation of a device.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeviceRow {
    pub id: Uuid,
    pub name: String,
    pub serial: Option<String>,
    pub os: String,
    pub os_version: String,
    pub kind: String,
    pub status: String,
    pub region: String,
    pub capabilities: Vec<String>,
    pub agent_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

pub struct DeviceRepo;

impl DeviceRepo {
    pub async fn create(
        pool: &PgPool,
        name: &str,
        serial: Option<&str>,
        os: &str,
        os_version: &str,
        kind: &str,
        region: &str,
        capabilities: &[String],
    ) -> Result<DeviceRow, sqlx::Error> {
        sqlx::query_as::<_, DeviceRow>(
            r#"INSERT INTO devices (name, serial, os, os_version, kind, region, capabilities, status)
               VALUES ($1, $2, $3::device_os, $4, $5::device_kind, $6, $7, 'available'::device_status)
               RETURNING id, name, serial, os::text, os_version, kind::text, status::text,
                         region, capabilities, agent_id, ip_address, registered_at, last_heartbeat, updated_at"#,
        )
        .bind(name)
        .bind(serial)
        .bind(os)
        .bind(os_version)
        .bind(kind)
        .bind(region)
        .bind(capabilities)
        .fetch_one(pool)
        .await
    }

    pub async fn update(
        pool: &PgPool,
        id: Uuid,
        name: &str,
        serial: Option<&str>,
        os_version: &str,
        region: &str,
        capabilities: &[String],
    ) -> Result<Option<DeviceRow>, sqlx::Error> {
        sqlx::query_as::<_, DeviceRow>(
            r#"UPDATE devices SET name = $2, serial = $3, os_version = $4, region = $5, capabilities = $6
               WHERE id = $1
               RETURNING id, name, serial, os::text, os_version, kind::text, status::text,
                         region, capabilities, agent_id, ip_address, registered_at, last_heartbeat, updated_at"#,
        )
        .bind(id)
        .bind(name)
        .bind(serial)
        .bind(os_version)
        .bind(region)
        .bind(capabilities)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<DeviceRow>, sqlx::Error> {
        sqlx::query_as::<_, DeviceRow>(
            r#"SELECT id, name, serial, os::text, os_version, kind::text, status::text,
                      region, capabilities, agent_id, ip_address, registered_at, last_heartbeat, updated_at
               FROM devices WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list(pool: &PgPool) -> Result<Vec<DeviceRow>, sqlx::Error> {
        sqlx::query_as::<_, DeviceRow>(
            r#"SELECT id, name, serial, os::text, os_version, kind::text, status::text,
                      region, capabilities, agent_id, ip_address, registered_at, last_heartbeat, updated_at
               FROM devices ORDER BY registered_at"#,
        )
        .fetch_all(pool)
        .await
    }

    pub async fn list_available(
        pool: &PgPool,
        os: Option<&str>,
        region: Option<&str>,
    ) -> Result<Vec<DeviceRow>, sqlx::Error> {
        sqlx::query_as::<_, DeviceRow>(
            r#"SELECT id, name, serial, os::text, os_version, kind::text, status::text,
                      region, capabilities, agent_id, ip_address, registered_at, last_heartbeat, updated_at
               FROM devices
               WHERE status = 'available'
                 AND ($1::text IS NULL OR os::text = $1)
                 AND ($2::text IS NULL OR region = $2)
               ORDER BY registered_at"#,
        )
        .bind(os)
        .bind(region)
        .fetch_all(pool)
        .await
    }

    pub async fn set_status(
        pool: &PgPool,
        id: Uuid,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE devices SET status = $2::device_status WHERE id = $1"#,
        )
        .bind(id)
        .bind(status)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_heartbeat(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query(r#"UPDATE devices SET last_heartbeat = now() WHERE id = $1"#)
                .bind(id)
                .execute(pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(r#"DELETE FROM devices WHERE id = $1"#)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Upsert a device by serial — used by edge agents to register discovered devices.
    /// If a device with the same serial exists, update it and mark available;
    /// otherwise create a new one.
    pub async fn upsert_by_serial(
        pool: &PgPool,
        name: &str,
        serial: &str,
        os: &str,
        os_version: &str,
        kind: &str,
        region: &str,
        capabilities: &[String],
        agent_id: Uuid,
        ip_address: Option<&str>,
    ) -> Result<DeviceRow, sqlx::Error> {
        sqlx::query_as::<_, DeviceRow>(
            r#"INSERT INTO devices (name, serial, os, os_version, kind, region, capabilities, status, agent_id, ip_address)
               VALUES ($1, $2, $3::device_os, $4, $5::device_kind, $6, $7, 'available'::device_status, $8, $9)
               ON CONFLICT (serial) WHERE serial IS NOT NULL DO UPDATE SET
                   name = EXCLUDED.name,
                   os_version = EXCLUDED.os_version,
                   region = EXCLUDED.region,
                   capabilities = EXCLUDED.capabilities || ARRAY(
                       SELECT unnest(devices.capabilities)
                       WHERE unnest LIKE 'bezel:%'
                   ),
                   agent_id = EXCLUDED.agent_id,
                   ip_address = EXCLUDED.ip_address,
                   status = 'available'::device_status,
                   last_heartbeat = now()
               RETURNING id, name, serial, os::text, os_version, kind::text, status::text,
                         region, capabilities, agent_id, ip_address, registered_at, last_heartbeat, updated_at"#,
        )
        .bind(name)
        .bind(serial)
        .bind(os)
        .bind(os_version)
        .bind(kind)
        .bind(region)
        .bind(capabilities)
        .bind(agent_id)
        .bind(ip_address)
        .fetch_one(pool)
        .await
    }

    /// Mark all devices owned by an agent as offline (e.g. agent disconnected).
    pub async fn set_agent_offline(pool: &PgPool, agent_id: Uuid) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE devices SET status = 'offline'::device_status WHERE agent_id = $1 AND status != 'offline'"#,
        )
        .bind(agent_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    /// Set or update a single prefixed capability like `bezel:iphone-modern`.
    /// Removes any existing entry with the same prefix and appends the new one.
    pub async fn set_preference(
        pool: &PgPool,
        id: Uuid,
        key: &str,
        value: &str,
    ) -> Result<bool, sqlx::Error> {
        let entry = format!("{key}:{value}");
        let like_pattern = format!("{key}:%");
        let result = sqlx::query(
            r#"UPDATE devices
               SET capabilities = array_append(
                   ARRAY(SELECT unnest(capabilities) WHERE NOT unnest LIKE $2),
                   $3
               )
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(&like_pattern)
        .bind(&entry)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
