use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct JobRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub status: String,
    pub parallelism: i32,
    pub total_tests: i32,
    pub passed: i32,
    pub failed: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub webhook_url: Option<String>,
    pub test_suite: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TestCaseRow {
    pub id: Uuid,
    pub job_id: Uuid,
    pub name: String,
    pub device_id: Option<Uuid>,
    pub status: String,
    pub attempt: i32,
    pub max_retries: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i32>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ArtifactRow {
    pub id: Uuid,
    pub job_id: Uuid,
    pub test_case_id: Option<Uuid>,
    pub kind: String,
    pub path: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

pub struct JobRepo;

impl JobRepo {
    pub async fn create(
        pool: &PgPool,
        tenant_id: Uuid,
        parallelism: i32,
        test_suite: &str,
        webhook_url: Option<&str>,
    ) -> Result<JobRow, sqlx::Error> {
        sqlx::query_as::<_, JobRow>(
            r#"INSERT INTO jobs (tenant_id, parallelism, test_suite, webhook_url)
               VALUES ($1, $2, $3, $4)
               RETURNING id, tenant_id, status::text, parallelism, total_tests, passed, failed,
                         created_at, started_at, completed_at, updated_at, webhook_url, test_suite"#,
        )
        .bind(tenant_id)
        .bind(parallelism)
        .bind(test_suite)
        .bind(webhook_url)
        .fetch_one(pool)
        .await
    }

    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<JobRow>, sqlx::Error> {
        sqlx::query_as::<_, JobRow>(
            r#"SELECT id, tenant_id, status::text, parallelism, total_tests, passed, failed,
                      created_at, started_at, completed_at, updated_at, webhook_url, test_suite
               FROM jobs WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_by_tenant(
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> Result<Vec<JobRow>, sqlx::Error> {
        sqlx::query_as::<_, JobRow>(
            r#"SELECT id, tenant_id, status::text, parallelism, total_tests, passed, failed,
                      created_at, started_at, completed_at, updated_at, webhook_url, test_suite
               FROM jobs WHERE tenant_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
    }

    pub async fn set_status(
        pool: &PgPool,
        id: Uuid,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE jobs SET status = $2::job_status, updated_at = now() WHERE id = $1"#,
        )
        .bind(id)
        .bind(status)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_started(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE jobs SET status = 'running'::job_status, started_at = now(), updated_at = now()
               WHERE id = $1"#,
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_completed(
        pool: &PgPool,
        id: Uuid,
        status: &str,
        total: i32,
        passed: i32,
        failed: i32,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE jobs
               SET status = $2::job_status, total_tests = $3, passed = $4, failed = $5,
                   completed_at = now(), updated_at = now()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(status)
        .bind(total)
        .bind(passed)
        .bind(failed)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_progress(
        pool: &PgPool,
        id: Uuid,
        total_tests: i32,
        passed: i32,
        failed: i32,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE jobs SET total_tests = $2, passed = $3, failed = $4, updated_at = now()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(total_tests)
        .bind(passed)
        .bind(failed)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// List queued jobs ready for execution (FIFO).
    pub async fn list_queued(pool: &PgPool, limit: i64) -> Result<Vec<JobRow>, sqlx::Error> {
        sqlx::query_as::<_, JobRow>(
            r#"SELECT id, tenant_id, status::text, parallelism, total_tests, passed, failed,
                      created_at, started_at, completed_at, updated_at, webhook_url, test_suite
               FROM jobs WHERE status = 'queued'
               ORDER BY created_at ASC
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await
    }
}

// ── TestCase Repo ─────────────────────────────────────────────────

pub struct TestCaseRepo;

impl TestCaseRepo {
    pub async fn create_batch(
        pool: &PgPool,
        job_id: Uuid,
        names: &[String],
        max_retries: i32,
    ) -> Result<Vec<TestCaseRow>, sqlx::Error> {
        let mut cases = Vec::with_capacity(names.len());
        for name in names {
            let row = sqlx::query_as::<_, TestCaseRow>(
                r#"INSERT INTO test_cases (job_id, name, max_retries)
                   VALUES ($1, $2, $3)
                   RETURNING *"#,
            )
            .bind(job_id)
            .bind(name)
            .bind(max_retries)
            .fetch_one(pool)
            .await?;
            cases.push(row);
        }
        Ok(cases)
    }

    pub async fn list_by_job(pool: &PgPool, job_id: Uuid) -> Result<Vec<TestCaseRow>, sqlx::Error> {
        sqlx::query_as::<_, TestCaseRow>(
            r#"SELECT * FROM test_cases WHERE job_id = $1 ORDER BY created_at"#,
        )
        .bind(job_id)
        .fetch_all(pool)
        .await
    }

    pub async fn list_pending(pool: &PgPool, job_id: Uuid, limit: i64) -> Result<Vec<TestCaseRow>, sqlx::Error> {
        sqlx::query_as::<_, TestCaseRow>(
            r#"SELECT * FROM test_cases
               WHERE job_id = $1 AND status IN ('pending', 'retrying')
               ORDER BY created_at
               LIMIT $2"#,
        )
        .bind(job_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    }

    pub async fn mark_running(
        pool: &PgPool,
        id: Uuid,
        device_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE test_cases
               SET status = 'running', device_id = $2, started_at = now(), attempt = attempt + 1
               WHERE id = $1 AND status IN ('pending', 'retrying')"#,
        )
        .bind(id)
        .bind(device_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_passed(
        pool: &PgPool,
        id: Uuid,
        duration_ms: i32,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE test_cases
               SET status = 'passed', finished_at = now(), duration_ms = $2
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(duration_ms)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn mark_failed(
        pool: &PgPool,
        id: Uuid,
        duration_ms: i32,
        error: &str,
    ) -> Result<bool, sqlx::Error> {
        // If retries remain, set to 'retrying'; else 'failed'
        let result = sqlx::query(
            r#"UPDATE test_cases
               SET status = CASE WHEN attempt < max_retries THEN 'retrying' ELSE 'failed' END,
                   finished_at = now(), duration_ms = $2, error = $3
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(duration_ms)
        .bind(error)
        .execute(pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Aggregate results for a job.
    pub async fn aggregate(pool: &PgPool, job_id: Uuid) -> Result<(i32, i32, i32, i32), sqlx::Error> {
        let row: (i64, i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                 COUNT(*)::bigint,
                 COUNT(*) FILTER (WHERE status = 'passed')::bigint,
                 COUNT(*) FILTER (WHERE status = 'failed')::bigint,
                 COUNT(*) FILTER (WHERE status = 'skipped')::bigint
               FROM test_cases WHERE job_id = $1"#,
        )
        .bind(job_id)
        .fetch_one(pool)
        .await?;
        Ok((row.0 as i32, row.1 as i32, row.2 as i32, row.3 as i32))
    }
}

// ── Artifact Repo ─────────────────────────────────────────────────

pub struct ArtifactRepo;

impl ArtifactRepo {
    pub async fn create(
        pool: &PgPool,
        job_id: Uuid,
        test_case_id: Option<Uuid>,
        kind: &str,
        path: &str,
        size_bytes: i64,
    ) -> Result<ArtifactRow, sqlx::Error> {
        sqlx::query_as::<_, ArtifactRow>(
            r#"INSERT INTO artifacts (job_id, test_case_id, kind, path, size_bytes)
               VALUES ($1, $2, $3, $4, $5)
               RETURNING *"#,
        )
        .bind(job_id)
        .bind(test_case_id)
        .bind(kind)
        .bind(path)
        .bind(size_bytes)
        .fetch_one(pool)
        .await
    }

    pub async fn list_by_job(pool: &PgPool, job_id: Uuid) -> Result<Vec<ArtifactRow>, sqlx::Error> {
        sqlx::query_as::<_, ArtifactRow>(
            r#"SELECT * FROM artifacts WHERE job_id = $1 ORDER BY created_at"#,
        )
        .bind(job_id)
        .fetch_all(pool)
        .await
    }

    pub async fn list_by_test(pool: &PgPool, test_case_id: Uuid) -> Result<Vec<ArtifactRow>, sqlx::Error> {
        sqlx::query_as::<_, ArtifactRow>(
            r#"SELECT * FROM artifacts WHERE test_case_id = $1 ORDER BY created_at"#,
        )
        .bind(test_case_id)
        .fetch_all(pool)
        .await
    }
}
