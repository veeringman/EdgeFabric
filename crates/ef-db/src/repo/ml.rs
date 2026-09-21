use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MlNodeRow {
    pub id: Uuid,
    pub tenant_id: Option<Uuid>,
    pub runner_id: Uuid,
    pub chip: String,
    pub cpu_cores: Option<i32>,
    pub gpu_cores: Option<i32>,
    pub ane_cores: Option<i32>,
    pub unified_memory_gb: Option<i32>,
    pub link_mbps: i32,
    pub mlx_version: String,
    pub free_memory_gb: Option<f64>,
    pub worker_host: String,
    pub worker_http_port: i32,
    pub worker_dataplane_port: i32,
    pub pipeline_rank: Option<i32>,
    pub labels: Vec<String>,
    pub status: String,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub heartbeat: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MlClusterRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub role: String,
    pub interconnect_cidrs: Vec<String>,
    pub rank0_url: String,
    pub status: String,
    pub config: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MlClusterMemberRow {
    pub cluster_id: Uuid,
    pub runner_id: Uuid,
    pub node_id: Option<Uuid>,
    pub lease_id: Option<Uuid>,
    pub rank: i32,
    pub dataplane_host: String,
    pub dataplane_port: i32,
    pub http_port: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MlJobRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub cluster_id: Option<Uuid>,
    pub kind: String,
    pub status: String,
    pub model_id: String,
    pub quantization: String,
    pub context_length: i32,
    pub parallelism: Value,
    pub metrics: Value,
    pub config: Value,
    pub error_message: Option<String>,
    pub requested_by: String,
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct MlRepo;

impl MlRepo {
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_node(
        pool: &PgPool,
        tenant_id: Uuid,
        runner_id: Uuid,
        chip: &str,
        cpu_cores: Option<i32>,
        gpu_cores: Option<i32>,
        ane_cores: Option<i32>,
        unified_memory_gb: Option<i32>,
        link_mbps: i32,
        mlx_version: &str,
        labels: &[String],
        status: &str,
    ) -> Result<MlNodeRow, sqlx::Error> {
        sqlx::query_as::<_, MlNodeRow>(
            r#"INSERT INTO ml_nodes (
                    tenant_id, runner_id, chip, cpu_cores, gpu_cores, ane_cores,
                    unified_memory_gb, link_mbps, mlx_version, labels, status
               ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
               ON CONFLICT (runner_id) DO UPDATE SET
                    chip = EXCLUDED.chip,
                    cpu_cores = EXCLUDED.cpu_cores,
                    gpu_cores = EXCLUDED.gpu_cores,
                    ane_cores = EXCLUDED.ane_cores,
                    unified_memory_gb = EXCLUDED.unified_memory_gb,
                    link_mbps = EXCLUDED.link_mbps,
                    mlx_version = EXCLUDED.mlx_version,
                    labels = EXCLUDED.labels,
                    status = EXCLUDED.status
               RETURNING id, tenant_id, runner_id, chip, cpu_cores, gpu_cores, ane_cores,
                         unified_memory_gb, link_mbps, mlx_version, free_memory_gb,
                         worker_host, worker_http_port, worker_dataplane_port, pipeline_rank,
                         labels, status, last_heartbeat_at, heartbeat, created_at, updated_at"#,
        )
        .bind(tenant_id)
        .bind(runner_id)
        .bind(chip)
        .bind(cpu_cores)
        .bind(gpu_cores)
        .bind(ane_cores)
        .bind(unified_memory_gb)
        .bind(link_mbps)
        .bind(mlx_version)
        .bind(labels)
        .bind(status)
        .fetch_one(pool)
        .await
    }

    pub async fn heartbeat(
        pool: &PgPool,
        runner_id: Uuid,
        worker_host: &str,
        http_port: i32,
        dataplane_port: i32,
        free_memory_gb: Option<f64>,
        pipeline_rank: Option<i32>,
        heartbeat: &Value,
    ) -> Result<Option<MlNodeRow>, sqlx::Error> {
        sqlx::query_as::<_, MlNodeRow>(
            r#"UPDATE ml_nodes SET
                    worker_host = $2,
                    worker_http_port = $3,
                    worker_dataplane_port = $4,
                    free_memory_gb = $5,
                    pipeline_rank = $6,
                    heartbeat = $7,
                    last_heartbeat_at = NOW(),
                    status = 'online'
               WHERE runner_id = $1
               RETURNING id, tenant_id, runner_id, chip, cpu_cores, gpu_cores, ane_cores,
                         unified_memory_gb, link_mbps, mlx_version, free_memory_gb,
                         worker_host, worker_http_port, worker_dataplane_port, pipeline_rank,
                         labels, status, last_heartbeat_at, heartbeat, created_at, updated_at"#,
        )
        .bind(runner_id)
        .bind(worker_host)
        .bind(http_port)
        .bind(dataplane_port)
        .bind(free_memory_gb)
        .bind(pipeline_rank)
        .bind(heartbeat)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_nodes(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<MlNodeRow>, sqlx::Error> {
        sqlx::query_as::<_, MlNodeRow>(
            r#"SELECT id, tenant_id, runner_id, chip, cpu_cores, gpu_cores, ane_cores,
                      unified_memory_gb, link_mbps, mlx_version, free_memory_gb,
                      worker_host, worker_http_port, worker_dataplane_port, pipeline_rank,
                      labels, status, last_heartbeat_at, heartbeat, created_at, updated_at
               FROM ml_nodes WHERE tenant_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_node_by_runner(
        pool: &PgPool,
        runner_id: Uuid,
    ) -> Result<Option<MlNodeRow>, sqlx::Error> {
        sqlx::query_as::<_, MlNodeRow>(
            r#"SELECT id, tenant_id, runner_id, chip, cpu_cores, gpu_cores, ane_cores,
                      unified_memory_gb, link_mbps, mlx_version, free_memory_gb,
                      worker_host, worker_http_port, worker_dataplane_port, pipeline_rank,
                      labels, status, last_heartbeat_at, heartbeat, created_at, updated_at
               FROM ml_nodes WHERE runner_id = $1"#,
        )
        .bind(runner_id)
        .fetch_optional(pool)
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_cluster(
        pool: &PgPool,
        tenant_id: Uuid,
        name: &str,
        role: &str,
        interconnect_cidrs: &[String],
        rank0_url: &str,
        config: &Value,
    ) -> Result<MlClusterRow, sqlx::Error> {
        sqlx::query_as::<_, MlClusterRow>(
            r#"INSERT INTO ml_clusters (tenant_id, name, role, interconnect_cidrs, rank0_url, config, status)
               VALUES ($1,$2,$3,$4,$5,$6,'idle')
               RETURNING id, tenant_id, name, role, interconnect_cidrs, rank0_url, status, config, created_at, updated_at"#,
        )
        .bind(tenant_id)
        .bind(name)
        .bind(role)
        .bind(interconnect_cidrs)
        .bind(rank0_url)
        .bind(config)
        .fetch_one(pool)
        .await
    }

    pub async fn add_member(
        pool: &PgPool,
        cluster_id: Uuid,
        runner_id: Uuid,
        node_id: Option<Uuid>,
        lease_id: Option<Uuid>,
        rank: i32,
        dataplane_host: &str,
        dataplane_port: i32,
        http_port: i32,
    ) -> Result<MlClusterMemberRow, sqlx::Error> {
        sqlx::query_as::<_, MlClusterMemberRow>(
            r#"INSERT INTO ml_cluster_members
                  (cluster_id, runner_id, node_id, lease_id, rank, dataplane_host, dataplane_port, http_port)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               RETURNING cluster_id, runner_id, node_id, lease_id, rank, dataplane_host, dataplane_port, http_port"#,
        )
        .bind(cluster_id)
        .bind(runner_id)
        .bind(node_id)
        .bind(lease_id)
        .bind(rank)
        .bind(dataplane_host)
        .bind(dataplane_port)
        .bind(http_port)
        .fetch_one(pool)
        .await
    }

    pub async fn list_clusters(
        pool: &PgPool,
        tenant_id: Uuid,
    ) -> Result<Vec<MlClusterRow>, sqlx::Error> {
        sqlx::query_as::<_, MlClusterRow>(
            r#"SELECT id, tenant_id, name, role, interconnect_cidrs, rank0_url, status, config, created_at, updated_at
               FROM ml_clusters WHERE tenant_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_cluster(
        pool: &PgPool,
        id: Uuid,
    ) -> Result<Option<MlClusterRow>, sqlx::Error> {
        sqlx::query_as::<_, MlClusterRow>(
            r#"SELECT id, tenant_id, name, role, interconnect_cidrs, rank0_url, status, config, created_at, updated_at
               FROM ml_clusters WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn list_members(
        pool: &PgPool,
        cluster_id: Uuid,
    ) -> Result<Vec<MlClusterMemberRow>, sqlx::Error> {
        sqlx::query_as::<_, MlClusterMemberRow>(
            r#"SELECT cluster_id, runner_id, node_id, lease_id, rank, dataplane_host, dataplane_port, http_port
               FROM ml_cluster_members WHERE cluster_id = $1 ORDER BY rank ASC"#,
        )
        .bind(cluster_id)
        .fetch_all(pool)
        .await
    }

    pub async fn set_cluster_status(
        pool: &PgPool,
        id: Uuid,
        status: &str,
        rank0_url: Option<&str>,
    ) -> Result<Option<MlClusterRow>, sqlx::Error> {
        sqlx::query_as::<_, MlClusterRow>(
            r#"UPDATE ml_clusters
               SET status = $2,
                   rank0_url = COALESCE($3, rank0_url)
               WHERE id = $1
               RETURNING id, tenant_id, name, role, interconnect_cidrs, rank0_url, status, config, created_at, updated_at"#,
        )
        .bind(id)
        .bind(status)
        .bind(rank0_url)
        .fetch_optional(pool)
        .await
    }

    pub async fn find_cluster_for_runner(
        pool: &PgPool,
        runner_id: Uuid,
    ) -> Result<Option<(MlClusterRow, MlClusterMemberRow)>, sqlx::Error> {
        let member = sqlx::query_as::<_, MlClusterMemberRow>(
            r#"SELECT cluster_id, runner_id, node_id, lease_id, rank, dataplane_host, dataplane_port, http_port
               FROM ml_cluster_members WHERE runner_id = $1
               ORDER BY rank ASC LIMIT 1"#,
        )
        .bind(runner_id)
        .fetch_optional(pool)
        .await?;
        let Some(member) = member else {
            return Ok(None);
        };
        let cluster = Self::find_cluster(pool, member.cluster_id).await?;
        Ok(cluster.map(|c| (c, member)))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_job(
        pool: &PgPool,
        tenant_id: Uuid,
        cluster_id: Option<Uuid>,
        kind: &str,
        model_id: &str,
        quantization: &str,
        context_length: i32,
        parallelism: &Value,
        config: &Value,
        requested_by: &str,
    ) -> Result<MlJobRow, sqlx::Error> {
        sqlx::query_as::<_, MlJobRow>(
            r#"INSERT INTO ml_jobs
                  (tenant_id, cluster_id, kind, status, model_id, quantization, context_length,
                   parallelism, config, requested_by)
               VALUES ($1,$2,$3,'queued',$4,$5,$6,$7,$8,$9)
               RETURNING id, tenant_id, cluster_id, kind, status, model_id, quantization, context_length,
                         parallelism, metrics, config, error_message, requested_by,
                         queued_at, started_at, completed_at, created_at, updated_at"#,
        )
        .bind(tenant_id)
        .bind(cluster_id)
        .bind(kind)
        .bind(model_id)
        .bind(quantization)
        .bind(context_length)
        .bind(parallelism)
        .bind(config)
        .bind(requested_by)
        .fetch_one(pool)
        .await
    }

    pub async fn list_jobs(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<MlJobRow>, sqlx::Error> {
        sqlx::query_as::<_, MlJobRow>(
            r#"SELECT id, tenant_id, cluster_id, kind, status, model_id, quantization, context_length,
                      parallelism, metrics, config, error_message, requested_by,
                      queued_at, started_at, completed_at, created_at, updated_at
               FROM ml_jobs WHERE tenant_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
    }

    pub async fn find_job(pool: &PgPool, id: Uuid) -> Result<Option<MlJobRow>, sqlx::Error> {
        sqlx::query_as::<_, MlJobRow>(
            r#"SELECT id, tenant_id, cluster_id, kind, status, model_id, quantization, context_length,
                      parallelism, metrics, config, error_message, requested_by,
                      queued_at, started_at, completed_at, created_at, updated_at
               FROM ml_jobs WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await
    }

    pub async fn active_job_for_cluster(
        pool: &PgPool,
        cluster_id: Uuid,
    ) -> Result<Option<MlJobRow>, sqlx::Error> {
        sqlx::query_as::<_, MlJobRow>(
            r#"SELECT id, tenant_id, cluster_id, kind, status, model_id, quantization, context_length,
                      parallelism, metrics, config, error_message, requested_by,
                      queued_at, started_at, completed_at, created_at, updated_at
               FROM ml_jobs
               WHERE cluster_id = $1 AND status IN ('queued','running','serving')
               ORDER BY created_at DESC LIMIT 1"#,
        )
        .bind(cluster_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn set_job_status(
        pool: &PgPool,
        id: Uuid,
        status: &str,
        error_message: Option<&str>,
        metrics: Option<&Value>,
    ) -> Result<Option<MlJobRow>, sqlx::Error> {
        sqlx::query_as::<_, MlJobRow>(
            r#"UPDATE ml_jobs SET
                    status = $2,
                    error_message = COALESCE($3, error_message),
                    metrics = COALESCE($4, metrics),
                    started_at = CASE WHEN $2 IN ('running','serving') AND started_at IS NULL THEN NOW() ELSE started_at END,
                    completed_at = CASE WHEN $2 IN ('succeeded','failed','cancelled') THEN NOW() ELSE completed_at END
               WHERE id = $1
               RETURNING id, tenant_id, cluster_id, kind, status, model_id, quantization, context_length,
                         parallelism, metrics, config, error_message, requested_by,
                         queued_at, started_at, completed_at, created_at, updated_at"#,
        )
        .bind(id)
        .bind(status)
        .bind(error_message)
        .bind(metrics)
        .fetch_optional(pool)
        .await
    }
}
