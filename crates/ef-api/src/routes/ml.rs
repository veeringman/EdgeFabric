use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;
use ef_db::repo::ml::MlRepo;
use ef_db::repo::runner::RunnerRepo;
use ef_scheduler::{RunnerFilter, Scheduler};

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub runner_id: Uuid,
    pub worker_host: String,
    #[serde(default = "default_http_port")]
    pub worker_http_port: i32,
    #[serde(default = "default_dataplane_port")]
    pub worker_dataplane_port: i32,
    pub free_memory_gb: Option<f64>,
    pub pipeline_rank: Option<i32>,
    #[serde(default)]
    pub heartbeat: Value,
}

fn default_http_port() -> i32 {
    8090
}
fn default_dataplane_port() -> i32 {
    8091
}

#[derive(Debug, Deserialize)]
pub struct CreateClusterRequest {
    pub name: String,
    #[serde(default = "default_pipeline")]
    pub role: String,
    #[serde(default)]
    pub interconnect_cidrs: Vec<String>,
    #[serde(default)]
    pub runner_ids: Vec<Uuid>,
    pub size: Option<usize>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub required_labels: Vec<String>,
    #[serde(default = "default_ttl")]
    pub lease_ttl_seconds: i64,
}

fn default_pipeline() -> String {
    "pipeline".to_string()
}
fn default_ttl() -> i64 {
    3600
}

#[derive(Debug, Deserialize)]
pub struct CreateMlJobRequest {
    pub cluster_id: Uuid,
    pub kind: String,
    #[serde(default = "default_model")]
    pub model_id: String,
    #[serde(default = "default_quant")]
    pub quantization: String,
    #[serde(default = "default_ctx")]
    pub context_length: i32,
    #[serde(default)]
    pub n_layers: Option<i32>,
    #[serde(default)]
    pub hidden_size: Option<i32>,
    #[serde(default)]
    pub local_steps: Option<i32>,
    #[serde(default)]
    pub microbatch: Option<i32>,
    #[serde(default)]
    pub steps: Option<i32>,
}

fn default_model() -> String {
    "toy-pp".to_string()
}
fn default_quant() -> String {
    "q4".to_string()
}
fn default_ctx() -> i32 {
    2048
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    #[serde(default)]
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    pub cluster_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

fn default_max_tokens() -> u32 {
    64
}

#[derive(Debug, Deserialize)]
pub struct JobMetricsRequest {
    pub metrics: Value,
    pub status: Option<String>,
    pub error_message: Option<String>,
}

pub async fn list_nodes(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let nodes = MlRepo::list_nodes(&state.pool, auth.tenant_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "list ml nodes");
            ApiError::internal("Failed to list ML nodes")
        })?;
    Ok(Json(json!({ "nodes": nodes, "total": nodes.len() })))
}

pub async fn node_heartbeat(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<HeartbeatRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let runner = RunnerRepo::find_by_id(&state.pool, body.runner_id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
        .ok_or_else(|| ApiError::not_found("runner not found"))?;
    if runner.tenant_id != Some(auth.tenant_id) {
        return Err(ApiError::not_found("runner not found"));
    }

    let node = match MlRepo::heartbeat(
        &state.pool,
        body.runner_id,
        &body.worker_host,
        body.worker_http_port,
        body.worker_dataplane_port,
        body.free_memory_gb,
        body.pipeline_rank,
        &body.heartbeat,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "ml heartbeat");
        ApiError::internal("Heartbeat failed")
    })? {
        Some(n) => n,
        None => MlRepo::upsert_node(
            &state.pool,
            auth.tenant_id,
            body.runner_id,
            "unknown",
            None,
            None,
            None,
            None,
            2500,
            "",
            &[],
            "online",
        )
        .await
        .map_err(|_| ApiError::internal("Failed to register ML node"))?,
    };

    let _ = MlRepo::heartbeat(
        &state.pool,
        body.runner_id,
        &body.worker_host,
        body.worker_http_port,
        body.worker_dataplane_port,
        body.free_memory_gb,
        body.pipeline_rank,
        &body.heartbeat,
    )
    .await;

    let assignment = assignment_for_runner(&state, body.runner_id).await?;
    Ok(Json(json!({
        "node": node,
        "assignment": assignment,
    })))
}

async fn assignment_for_runner(
    state: &AppState,
    runner_id: Uuid,
) -> Result<Value, (StatusCode, Json<ApiError>)> {
    let Some((cluster, member)) = MlRepo::find_cluster_for_runner(&state.pool, runner_id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
    else {
        return Ok(json!(null));
    };
    let members = MlRepo::list_members(&state.pool, cluster.id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?;
    let job = MlRepo::active_job_for_cluster(&state.pool, cluster.id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?;
    Ok(json!({
        "cluster": cluster,
        "rank": member.rank,
        "members": members,
        "job": job,
    }))
}

pub async fn list_clusters(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let clusters = MlRepo::list_clusters(&state.pool, auth.tenant_id)
        .await
        .map_err(|_| ApiError::internal("Failed to list clusters"))?;
    let mut out = Vec::new();
    for c in clusters {
        let members = MlRepo::list_members(&state.pool, c.id)
            .await
            .unwrap_or_default();
        out.push(json!({ "cluster": c, "members": members }));
    }
    Ok(Json(json!({ "clusters": out, "total": out.len() })))
}

pub async fn get_cluster(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let cluster = MlRepo::find_cluster(&state.pool, id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
        .ok_or_else(|| ApiError::not_found(format!("Cluster {id} not found")))?;
    if cluster.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Cluster {id} not found")));
    }
    let members = MlRepo::list_members(&state.pool, id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?;
    Ok(Json(json!({ "cluster": cluster, "members": members })))
}

pub async fn create_cluster(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CreateClusterRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    if !matches!(body.role.as_str(), "pipeline" | "replica" | "diloco") {
        return Err(ApiError::bad_request("role must be pipeline, replica, or diloco"));
    }
    let scheduler = Scheduler::new(state.pool.clone());
    let pairs = if !body.runner_ids.is_empty() {
        let mut leased = Vec::new();
        for rid in &body.runner_ids {
            let pair = RunnerRepo::acquire_lease_for_runner(
                &state.pool,
                auth.tenant_id,
                *rid,
                body.lease_ttl_seconds,
            )
            .await
            .map_err(|_| ApiError::internal("Database error"))?
            .ok_or_else(|| {
                ApiError::bad_request(format!(
                    "runner {rid} is not available for host lease"
                ))
            })?;
            leased.push(pair);
        }
        leased
    } else {
        let size = body.size.unwrap_or(1).max(1);
        let caps = if body.required_capabilities.is_empty() {
            vec!["mlx".into(), "llm_infer".into()]
        } else {
            body.required_capabilities.clone()
        };
        let filter = RunnerFilter {
            region: body.region.clone(),
            required_capabilities: caps,
            required_labels: body.required_labels.clone(),
            lease_ttl_seconds: body.lease_ttl_seconds,
            runner_class: None,
        };
        scheduler
            .allocate_runner_set(auth.tenant_id, &filter, size)
            .await
            .map_err(|_| {
                ApiError::not_found(
                    "Not enough matching SSH hosts (need online generic runners with the requested ML capabilities)",
                )
            })?
    };

    if pairs.is_empty() {
        return Err(ApiError::bad_request("cluster requires at least one runner"));
    }

    let cluster = MlRepo::create_cluster(
        &state.pool,
        auth.tenant_id,
        body.name.trim(),
        &body.role,
        &body.interconnect_cidrs,
        "",
        &json!({ "lan_only": true }),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "create cluster");
        ApiError::internal("Failed to create cluster")
    })?;

    let mut members = Vec::new();
    let mut rank0_url = String::new();
    for (rank, (runner, lease)) in pairs.into_iter().enumerate() {
        let node = MlRepo::find_node_by_runner(&state.pool, runner.id)
            .await
            .ok()
            .flatten();
        let host = node
            .as_ref()
            .map(|n| n.worker_host.clone())
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| runner.host.clone());
        let http_port = node.as_ref().map(|n| n.worker_http_port).unwrap_or(8090);
        let dp_port = node.as_ref().map(|n| n.worker_dataplane_port).unwrap_or(8091);
        if rank == 0 {
            rank0_url = format!("http://{host}:{http_port}");
        }
        let member = MlRepo::add_member(
            &state.pool,
            cluster.id,
            runner.id,
            node.as_ref().map(|n| n.id),
            Some(lease.id),
            rank as i32,
            &host,
            dp_port,
            http_port,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "add cluster member");
            ApiError::internal("Failed to add cluster member")
        })?;
        members.push(json!({ "member": member, "runner": runner, "lease": lease }));
    }

    let cluster = MlRepo::set_cluster_status(&state.pool, cluster.id, "idle", Some(&rank0_url))
        .await
        .map_err(|_| ApiError::internal("Failed to update cluster"))?
        .unwrap_or(cluster);

    Ok((
        StatusCode::CREATED,
        Json(json!({ "cluster": cluster, "members": members })),
    ))
}

pub async fn list_jobs(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let jobs = MlRepo::list_jobs(&state.pool, auth.tenant_id)
        .await
        .map_err(|_| ApiError::internal("Failed to list ML jobs"))?;
    Ok(Json(json!({ "jobs": jobs, "total": jobs.len() })))
}

pub async fn get_job(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let job = MlRepo::find_job(&state.pool, id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;
    if job.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Job {id} not found")));
    }
    Ok(Json(json!({ "job": job })))
}

pub async fn create_job(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CreateMlJobRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    if !matches!(
        body.kind.as_str(),
        "infer_serve" | "train_lora" | "train_diloco" | "train_pipeline"
    ) {
        return Err(ApiError::bad_request(
            "kind must be infer_serve, train_lora, train_diloco, or train_pipeline",
        ));
    }
    let cluster = MlRepo::find_cluster(&state.pool, body.cluster_id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
        .ok_or_else(|| ApiError::not_found("cluster not found"))?;
    if cluster.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found("cluster not found"));
    }
    let members = MlRepo::list_members(&state.pool, cluster.id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?;
    if members.is_empty() {
        return Err(ApiError::bad_request("cluster has no members"));
    }
    let n_layers = body.n_layers.unwrap_or(12);
    let hidden = body.hidden_size.unwrap_or(256);
    let parallelism = json!({
        "world_size": members.len(),
        "n_layers": n_layers,
        "hidden_size": hidden,
        "local_steps": body.local_steps.unwrap_or(8),
        "microbatch": body.microbatch.unwrap_or(1),
        "steps": body.steps.unwrap_or(16),
        "context_length": body.context_length,
    });
    let config = json!({
        "lan_only": true,
        "quantization": body.quantization,
    });
    let job = MlRepo::create_job(
        &state.pool,
        auth.tenant_id,
        Some(cluster.id),
        &body.kind,
        &body.model_id,
        &body.quantization,
        body.context_length,
        &parallelism,
        &config,
        &auth.tenant_name,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "create ml job");
        ApiError::internal("Failed to create ML job")
    })?;

    let status = if body.kind == "infer_serve" {
        "serving"
    } else {
        "running"
    };
    let job = MlRepo::set_job_status(&state.pool, job.id, status, None, None)
        .await
        .ok()
        .flatten()
        .unwrap_or(job);
    let _ = MlRepo::set_cluster_status(&state.pool, cluster.id, status, None).await;

    Ok((StatusCode::CREATED, Json(json!({ "job": job }))))
}

pub async fn cancel_job(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let job = MlRepo::find_job(&state.pool, id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;
    if job.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Job {id} not found")));
    }
    let job = MlRepo::set_job_status(&state.pool, id, "cancelled", None, None)
        .await
        .map_err(|_| ApiError::internal("Failed to cancel job"))?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;
    if let Some(cid) = job.cluster_id {
        let _ = MlRepo::set_cluster_status(&state.pool, cid, "idle", None).await;
    }
    Ok(Json(json!({ "job": job })))
}

pub async fn report_job_metrics(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<JobMetricsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let job = MlRepo::find_job(&state.pool, id)
        .await
        .map_err(|_| ApiError::internal("Database error"))?
        .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;
    if job.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found(format!("Job {id} not found")));
    }
    let status = body.status.as_deref().unwrap_or(&job.status);
    let job = MlRepo::set_job_status(
        &state.pool,
        id,
        status,
        body.error_message.as_deref(),
        Some(&body.metrics),
    )
    .await
    .map_err(|_| ApiError::internal("Failed to update metrics"))?
    .ok_or_else(|| ApiError::not_found(format!("Job {id} not found")))?;
    Ok(Json(json!({ "job": job })))
}

pub async fn chat_completions(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<ChatCompletionRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let cluster = if let Some(id) = body.cluster_id {
        MlRepo::find_cluster(&state.pool, id)
            .await
            .map_err(|_| ApiError::internal("Database error"))?
            .ok_or_else(|| ApiError::not_found("cluster not found"))?
    } else {
        MlRepo::list_clusters(&state.pool, auth.tenant_id)
            .await
            .map_err(|_| ApiError::internal("Database error"))?
            .into_iter()
            .find(|c| c.status == "serving" && !c.rank0_url.is_empty())
            .ok_or_else(|| ApiError::not_found("No serving ML cluster"))?
    };
    if cluster.tenant_id != auth.tenant_id {
        return Err(ApiError::not_found("cluster not found"));
    }
    if cluster.rank0_url.is_empty() {
        return Err(ApiError::bad_request("cluster rank-0 URL is not published yet"));
    }
    let url = format!("{}/v1/chat/completions", cluster.rank0_url.trim_end_matches('/'));
    let payload = json!({
        "model": if body.model.is_empty() { "toy-pp" } else { &body.model },
        "messages": body.messages.iter().map(|m| json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
        "max_tokens": body.max_tokens,
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("rank-0 unreachable: {e}")))?;
    let status = resp.status();
    let value: Value = resp.json().await.map_err(|e| ApiError::internal(e.to_string()))?;
    if !status.is_success() {
        return Err(ApiError::internal(format!("rank-0 error {status}: {value}")));
    }
    Ok(Json(value))
}
