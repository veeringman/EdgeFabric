use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RunnerRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub name: String,
    pub runner_class: String,
    pub host: String,
    pub port: i32,
    pub host_os: String,
    pub architecture: String,
    pub region: String,
    pub labels: Vec<String>,
    pub capabilities: Vec<String>,
    pub max_concurrency: i32,
    pub drained: bool,
    pub status: String,
    pub health: Value,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub last_probe_result: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct RunnerRepo;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RunnerLeaseRow {
    pub id: Uuid,
    pub runner_id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub build_job_id: Option<Uuid>,
    pub status: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub released_at: Option<DateTime<Utc>>,
    pub metadata: Value,
}

impl RunnerRepo {
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        pool: &PgPool,
        tenant_id: Uuid,
        device_id: Option<Uuid>,
        name: &str,
        runner_class: &str,
        host: &str,
        port: i32,
        host_os: &str,
        architecture: &str,
        region: &str,
        labels: &[String],
        capabilities: &[String],
        max_concurrency: i32,
    ) -> Result<RunnerRow, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"INSERT INTO runners
                  (tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                   region, labels, capabilities, max_concurrency, drained, status)
               VALUES
                  ($1, $2, $3, $4, $5, $6, $7, $8,
                   $9, $10, $11, $12, FALSE, 'online')
               RETURNING id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                         region, labels, capabilities, max_concurrency, drained, status,
                         health, last_probe_at, last_probe_result, created_at, updated_at"#,
        )
        .bind(tenant_id)
        .bind(device_id)
        .bind(name)
        .bind(runner_class)
        .bind(host)
        .bind(port)
        .bind(host_os)
        .bind(architecture)
        .bind(region)
        .bind(labels)
        .bind(capabilities)
        .bind(max_concurrency)
        .fetch_one(pool)
        .await
    }

    pub async fn list_by_tenant(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"SELECT id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                      region, labels, capabilities, max_concurrency, drained, status,
                      health, last_probe_at, last_probe_result, created_at, updated_at
               FROM runners
               WHERE tenant_id = $1
               ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_by_device_id(
        pool: &PgPool,
        device_id: Uuid,
    ) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"SELECT id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                      region, labels, capabilities, max_concurrency, drained, status,
                      health, last_probe_at, last_probe_result, created_at, updated_at
               FROM runners
               WHERE device_id = $1
               ORDER BY updated_at DESC
               LIMIT 1"#,
        )
        .bind(device_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_host_port(
        pool: &PgPool,
        tenant_id: Uuid,
        host: &str,
        port: i32,
    ) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"SELECT id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                      region, labels, capabilities, max_concurrency, drained, status,
                      health, last_probe_at, last_probe_result, created_at, updated_at
               FROM runners
               WHERE (tenant_id = $1 OR tenant_id IS NULL)
                 AND host = $2 AND port = $3
               ORDER BY updated_at DESC
               LIMIT 1"#,
        )
        .bind(tenant_id)
        .bind(host)
        .bind(port)
        .fetch_optional(pool)
        .await
    }

    pub async fn update_registration(
        pool: &PgPool,
        id: Uuid,
        device_id: Option<Uuid>,
        name: &str,
        runner_class: &str,
        host: &str,
        port: i32,
        host_os: &str,
        architecture: &str,
        region: &str,
        labels: &[String],
        capabilities: &[String],
        max_concurrency: i32,
    ) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"UPDATE runners SET
                   device_id = COALESCE($2, device_id),
                   name = $3,
                   runner_class = $4,
                   host = $5,
                   port = $6,
                   host_os = $7,
                   architecture = CASE WHEN $8 = '' THEN architecture ELSE $8 END,
                   region = $9,
                   labels = $10,
                   capabilities = $11,
                   max_concurrency = $12,
                   drained = FALSE,
                   status = 'online'
               WHERE id = $1
               RETURNING id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                         region, labels, capabilities, max_concurrency, drained, status,
                         health, last_probe_at, last_probe_result, created_at, updated_at"#,
        )
        .bind(id)
        .bind(device_id)
        .bind(name)
        .bind(runner_class)
        .bind(host)
        .bind(port)
        .bind(host_os)
        .bind(architecture)
        .bind(region)
        .bind(labels)
        .bind(capabilities)
        .bind(max_concurrency)
        .fetch_optional(pool)
        .await
    }

    pub async fn link_device(
        pool: &PgPool,
        id: Uuid,
        device_id: Uuid,
    ) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"UPDATE runners SET device_id = $2
               WHERE id = $1
               RETURNING id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                         region, labels, capabilities, max_concurrency, drained, status,
                         health, last_probe_at, last_probe_result, created_at, updated_at"#,
        )
        .bind(id)
        .bind(device_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"SELECT id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                      region, labels, capabilities, max_concurrency, drained, status,
                      health, last_probe_at, last_probe_result, created_at, updated_at
               FROM runners
               WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn set_drained(
        pool: &PgPool,
        id: Uuid,
        drained: bool,
    ) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"UPDATE runners
               SET drained = $2, status = CASE WHEN $2 THEN 'maintenance' ELSE 'online' END
               WHERE id = $1
               RETURNING id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                         region, labels, capabilities, max_concurrency, drained, status,
                         health, last_probe_at, last_probe_result, created_at, updated_at"#,
        )
        .bind(id)
        .bind(drained)
        .fetch_optional(pool)
        .await
    }

    pub async fn update_probe_result(
        pool: &PgPool,
        id: Uuid,
        status: &str,
        result: &Value,
    ) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"UPDATE runners
               SET last_probe_at = now(),
                   last_probe_result = $3,
                   health = $3,
                   status = $2
               WHERE id = $1
               RETURNING id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                         region, labels, capabilities, max_concurrency, drained, status,
                         health, last_probe_at, last_probe_result, created_at, updated_at"#,
        )
        .bind(id)
        .bind(status)
        .bind(result)
        .fetch_optional(pool)
        .await
    }

    pub async fn update_probe_snapshot(
        pool: &PgPool,
        id: Uuid,
        status: &str,
        host_os: &str,
        capabilities: &[String],
        result: &Value,
    ) -> Result<Option<RunnerRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerRow>(
            r#"UPDATE runners
               SET last_probe_at = now(),
                   last_probe_result = $3,
                   health = $3,
                   status = $2,
                   host_os = $4,
                   capabilities = $5
               WHERE id = $1
               RETURNING id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                         region, labels, capabilities, max_concurrency, drained, status,
                         health, last_probe_at, last_probe_result, created_at, updated_at"#,
        )
        .bind(id)
        .bind(status)
        .bind(result)
        .bind(host_os)
        .bind(capabilities)
        .fetch_optional(pool)
        .await
    }

    pub async fn acquire_lease(
        pool: &PgPool,
        tenant_id: Uuid,
        required_capabilities: &[String],
        required_labels: &[String],
        region: Option<&str>,
        build_job_id: Option<Uuid>,
        ttl_seconds: i64,
        runner_class: Option<&str>,
    ) -> Result<Option<(RunnerRow, RunnerLeaseRow)>, sqlx::Error> {
        let cap_filter: Option<Vec<String>> = if required_capabilities.is_empty() {
            None
        } else {
            Some(required_capabilities.to_vec())
        };
        let label_filter: Option<Vec<String>> = if required_labels.is_empty() {
            None
        } else {
            Some(required_labels.to_vec())
        };

        let mut tx = pool.begin().await?;

        let candidate = sqlx::query(
            r#"WITH candidate AS (
                    SELECT r.*
                    FROM runners r
                    WHERE (r.tenant_id = $1 OR r.tenant_id IS NULL)
                      AND r.status = 'online'
                      AND r.drained = FALSE
                      AND ($2::text IS NULL OR r.region = $2)
                      AND ($3::text[] IS NULL OR r.capabilities @> $3::text[])
                      AND ($4::text[] IS NULL OR r.labels @> $4::text[])
                      AND ($5::text IS NULL OR r.runner_class = $5)
                    ORDER BY r.last_probe_at DESC NULLS LAST, r.created_at ASC
                    FOR UPDATE SKIP LOCKED
                )
                SELECT c.*, COALESCE(active.cnt, 0) AS active_count
                FROM candidate c
                LEFT JOIN LATERAL (
                    SELECT COUNT(*)::bigint AS cnt
                    FROM runner_leases l
                    WHERE l.runner_id = c.id
                      AND l.status = 'active'::runner_lease_status
                      AND (l.expires_at IS NULL OR l.expires_at > NOW())
                ) active ON TRUE
                WHERE COALESCE(active.cnt, 0) < c.max_concurrency
                LIMIT 1"#,
        )
        .bind(tenant_id)
        .bind(region)
        .bind(cap_filter)
        .bind(label_filter)
        .bind(runner_class)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = candidate else {
            tx.commit().await?;
            return Ok(None);
        };

        let runner = RunnerRow {
            id: row.try_get("id")?,
            tenant_id: row.try_get("tenant_id")?,
            device_id: row.try_get("device_id")?,
            name: row.try_get("name")?,
            runner_class: row.try_get("runner_class")?,
            host: row.try_get("host")?,
            port: row.try_get("port")?,
            host_os: row.try_get("host_os")?,
            architecture: row.try_get("architecture")?,
            region: row.try_get("region")?,
            labels: row.try_get("labels")?,
            capabilities: row.try_get("capabilities")?,
            max_concurrency: row.try_get("max_concurrency")?,
            drained: row.try_get("drained")?,
            status: row.try_get("status")?,
            health: row.try_get("health")?,
            last_probe_at: row.try_get("last_probe_at")?,
            last_probe_result: row.try_get("last_probe_result")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        };

        let lease = sqlx::query_as::<_, RunnerLeaseRow>(
            r#"INSERT INTO runner_leases (runner_id, tenant_id, build_job_id, status, expires_at, metadata)
               VALUES (
                   $1,
                   $2,
                   $3,
                   'active'::runner_lease_status,
                   CASE WHEN $4::bigint > 0 THEN NOW() + make_interval(secs => $4::int) ELSE NULL END,
                   jsonb_build_object('required_capabilities', $5::text[], 'required_labels', $6::text[], 'region', $7::text)
               )
               RETURNING id, runner_id, tenant_id, build_job_id, status::text,
                         acquired_at, expires_at, released_at, metadata"#,
        )
        .bind(runner.id)
        .bind(tenant_id)
        .bind(build_job_id)
        .bind(ttl_seconds)
        .bind(required_capabilities)
        .bind(required_labels)
        .bind(region)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Some((runner, lease)))
    }

    pub async fn acquire_lease_for_runner(
        pool: &PgPool,
        tenant_id: Uuid,
        runner_id: Uuid,
        ttl_seconds: i64,
    ) -> Result<Option<(RunnerRow, RunnerLeaseRow)>, sqlx::Error> {
        let mut tx = pool.begin().await?;
        let runner = sqlx::query_as::<_, RunnerRow>(
            r#"SELECT id, tenant_id, device_id, name, runner_class, host, port, host_os, architecture,
                      region, labels, capabilities, max_concurrency, drained, status,
                      health, last_probe_at, last_probe_result, created_at, updated_at
               FROM runners
               WHERE id = $1 AND (tenant_id = $2 OR tenant_id IS NULL)
                 AND status = 'online' AND drained = FALSE
               FOR UPDATE SKIP LOCKED"#,
        )
        .bind(runner_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(runner) = runner else {
            tx.commit().await?;
            return Ok(None);
        };
        let active: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)::bigint FROM runner_leases
               WHERE runner_id = $1 AND status = 'active'::runner_lease_status
                 AND (expires_at IS NULL OR expires_at > NOW())"#,
        )
        .bind(runner.id)
        .fetch_one(&mut *tx)
        .await?;
        if active >= i64::from(runner.max_concurrency) {
            tx.commit().await?;
            return Ok(None);
        }
        let lease = sqlx::query_as::<_, RunnerLeaseRow>(
            r#"INSERT INTO runner_leases (runner_id, tenant_id, status, expires_at, metadata)
               VALUES (
                   $1, $2, 'active'::runner_lease_status,
                   CASE WHEN $3::bigint > 0 THEN NOW() + make_interval(secs => $3::int) ELSE NULL END,
                   jsonb_build_object('purpose', 'host-lease')
               )
               RETURNING id, runner_id, tenant_id, build_job_id, status::text,
                         acquired_at, expires_at, released_at, metadata"#,
        )
        .bind(runner.id)
        .bind(tenant_id)
        .bind(ttl_seconds)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some((runner, lease)))
    }

    pub async fn release_lease(
        pool: &PgPool,
        lease_id: Uuid,
    ) -> Result<Option<RunnerLeaseRow>, sqlx::Error> {
        sqlx::query_as::<_, RunnerLeaseRow>(
            r#"UPDATE runner_leases
               SET status = 'released'::runner_lease_status,
                   released_at = NOW()
               WHERE id = $1
                 AND status = 'active'::runner_lease_status
               RETURNING id, runner_id, tenant_id, build_job_id, status::text,
                         acquired_at, expires_at, released_at, metadata"#,
        )
        .bind(lease_id)
        .fetch_optional(pool)
        .await
    }
}
