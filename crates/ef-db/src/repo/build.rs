use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BuildJobRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub runner_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub build_type: String,
    pub status: String,
    pub requested_by: String,
    pub source_ref: String,
    pub repository_url: Option<String>,
    pub git_ref: Option<String>,
    pub config: Value,
    pub runtime: Value,
    pub logs_path: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct BuildJobRepo;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BuildArtifactRow {
    pub id: Uuid,
    pub build_job_id: Uuid,
    pub tenant_id: Uuid,
    pub artifact_type: String,
    pub file_name: String,
    pub storage_path: String,
    pub size_bytes: i64,
    pub checksum_sha256: Option<String>,
    pub metadata: Value,
    pub retention_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct BuildStatusCounts {
    pub queued: i64,
    pub provisioning: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub cancelled: i64,
}

impl BuildJobRepo {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_job(
        pool: &PgPool,
        tenant_id: Uuid,
        build_type: &str,
        requested_by: &str,
        source_ref: &str,
        repository_url: Option<&str>,
        git_ref: Option<&str>,
        config: &Value,
        runtime: &Value,
        device_id: Option<Uuid>,
    ) -> Result<BuildJobRow, sqlx::Error> {
        sqlx::query_as::<_, BuildJobRow>(
            r#"INSERT INTO build_jobs
               (tenant_id, build_type, status, requested_by, source_ref, repository_url, git_ref, config, runtime, device_id)
               VALUES
               ($1, $2, 'queued'::build_job_status, $3, $4, $5, $6, $7, $8, $9)
               RETURNING id, tenant_id, runner_id, device_id, build_type, status::text,
                         requested_by, source_ref, repository_url, git_ref,
                         config, runtime, logs_path, error_message, retry_count,
                         queued_at, started_at, completed_at, created_at, updated_at"#,
        )
        .bind(tenant_id)
        .bind(build_type)
        .bind(requested_by)
        .bind(source_ref)
        .bind(repository_url)
        .bind(git_ref)
        .bind(config)
        .bind(runtime)
        .bind(device_id)
        .fetch_one(pool)
        .await
    }

    pub async fn create_ios(
        pool: &PgPool,
        tenant_id: Uuid,
        requested_by: &str,
        source_ref: &str,
        repository_url: Option<&str>,
        git_ref: Option<&str>,
        config: &Value,
        runtime: &Value,
        device_id: Option<Uuid>,
    ) -> Result<BuildJobRow, sqlx::Error> {
        Self::create_job(
            pool,
            tenant_id,
            "ios",
            requested_by,
            source_ref,
            repository_url,
            git_ref,
            config,
            runtime,
            device_id,
        )
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<BuildJobRow>, sqlx::Error> {
        sqlx::query_as::<_, BuildJobRow>(
            r#"SELECT id, tenant_id, runner_id, device_id, build_type, status::text,
                      requested_by, source_ref, repository_url, git_ref,
                      config, runtime, logs_path, error_message, retry_count,
                      queued_at, started_at, completed_at, created_at, updated_at
               FROM build_jobs
               WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
        status: Option<&str>,
        device_q: Option<&str>,
        source_q: Option<&str>,
    ) -> Result<Vec<BuildJobRow>, sqlx::Error> {
        sqlx::query_as::<_, BuildJobRow>(
            r#"SELECT id, tenant_id, runner_id, device_id, build_type, status::text,
                      requested_by, source_ref, repository_url, git_ref,
                      config, runtime, logs_path, error_message, retry_count,
                      queued_at, started_at, completed_at, created_at, updated_at
               FROM build_jobs
               WHERE tenant_id = $1
                 AND ($4::text IS NULL OR lower(status::text) = lower($4))
                 AND ($5::text IS NULL OR device_id::text ILIKE ('%' || $5 || '%'))
                 AND ($6::text IS NULL OR source_ref ILIKE ('%' || $6 || '%'))
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .bind(status)
        .bind(device_q)
        .bind(source_q)
        .fetch_all(pool)
        .await
    }

    pub async fn count_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
        status: Option<&str>,
        device_q: Option<&str>,
        source_q: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)::bigint
               FROM build_jobs
               WHERE tenant_id = $1
                 AND ($2::text IS NULL OR lower(status::text) = lower($2))
                 AND ($3::text IS NULL OR device_id::text ILIKE ('%' || $3 || '%'))
                 AND ($4::text IS NULL OR source_ref ILIKE ('%' || $4 || '%'))"#,
        )
        .bind(tenant_id)
        .bind(status)
        .bind(device_q)
        .bind(source_q)
        .fetch_one(pool)
        .await?;

        Ok(count)
    }

    pub async fn count_status_facets_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
        device_q: Option<&str>,
        source_q: Option<&str>,
    ) -> Result<BuildStatusCounts, sqlx::Error> {
        sqlx::query_as::<_, BuildStatusCounts>(
            r#"SELECT
                    COUNT(*) FILTER (WHERE status::text = 'queued')::bigint AS queued,
                    COUNT(*) FILTER (WHERE status::text = 'provisioning')::bigint AS provisioning,
                    COUNT(*) FILTER (WHERE status::text = 'running')::bigint AS running,
                    COUNT(*) FILTER (WHERE status::text IN ('succeeded', 'completed'))::bigint AS succeeded,
                    COUNT(*) FILTER (WHERE status::text = 'failed')::bigint AS failed,
                    COUNT(*) FILTER (WHERE status::text IN ('cancelled', 'canceled'))::bigint AS cancelled
               FROM build_jobs
               WHERE tenant_id = $1
                 AND ($2::text IS NULL OR device_id::text ILIKE ('%' || $2 || '%'))
                 AND ($3::text IS NULL OR source_ref ILIKE ('%' || $3 || '%'))"#,
        )
        .bind(tenant_id)
        .bind(device_q)
        .bind(source_q)
        .fetch_one(pool)
        .await
    }

    pub async fn set_provisioning(
        pool: &PgPool,
        id: Uuid,
        runner_id: Uuid,
        runtime: &Value,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE build_jobs
               SET status = 'provisioning'::build_job_status,
                   runner_id = $2,
                   runtime = runtime || $3::jsonb,
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(runner_id)
        .bind(runtime)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_running(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE build_jobs
               SET status = 'running'::build_job_status,
                   started_at = NOW(),
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_terminal(
        pool: &PgPool,
        id: Uuid,
        status: &str,
        runtime: &Value,
        error_message: Option<&str>,
        logs_path: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE build_jobs
               SET status = $2::build_job_status,
                   runtime = runtime || $3::jsonb,
                   error_message = $4,
                   logs_path = $5,
                   completed_at = NOW(),
                   updated_at = NOW()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(status)
        .bind(runtime)
        .bind(error_message)
        .bind(logs_path)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

pub struct BuildArtifactRepo;

impl BuildArtifactRepo {
    pub async fn list_by_build_job(
        pool: &PgPool,
        build_job_id: Uuid,
    ) -> Result<Vec<BuildArtifactRow>, sqlx::Error> {
        sqlx::query_as::<_, BuildArtifactRow>(
            r#"SELECT id, build_job_id, tenant_id, artifact_type, file_name, storage_path,
                      size_bytes, checksum_sha256, metadata, retention_until, created_at
               FROM build_artifacts
               WHERE build_job_id = $1
               ORDER BY created_at"#,
        )
        .bind(build_job_id)
        .fetch_all(pool)
        .await
    }
}
