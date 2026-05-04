//! VeerOS Host Discovery API routes.
//!
//! Routes mounted under `/api/v1/discovery/*`.  Each request fans out to all
//! registered agents, collects their discovered-host lists from
//! `GET /veeros/discovery/hosts`, merges the results, optionally persists them
//! to the `discovered_hosts` table, and returns the unified list to the caller.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use ef_db::repo::agent::AgentRepo;
use ef_db::repo::discovered_host::{DiscoveredHostRepo, DiscoveredHostRow};

use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;
use crate::veeros_proxy;

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

// ─── query params ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DiscoveryQuery {
    /// When true, only return hosts with quantum capabilities.
    #[serde(default)]
    pub quantum: bool,
    /// Filter to a specific agent.
    pub agent_id: Option<Uuid>,
}

// ─── GET /api/v1/discovery/hosts ─────────────────────────────────────────────

/// List all VeerOS hosts discovered across all agents.
///
/// ef-api fans out to every registered agent's `GET /veeros/discovery/hosts`,
/// upserts the results into `discovered_hosts`, then returns the merged list.
pub async fn list_hosts(
    State(state): State<AppState>,
    _auth: AuthContext,
    Query(q): Query<DiscoveryQuery>,
) -> ApiResult<Json<Value>> {
    // 1. Fetch all registered agents
    let agents = AgentRepo::list(&state.pool, None, None, false)
        .await
        .map_err(|e| ApiError::internal(format!("DB error listing agents: {e}")))?;

    // 2. Fan out to each agent in parallel
    let mut tasks = Vec::new();
    for agent in &agents {
        if let Some(filter_id) = q.agent_id {
            if agent.id != filter_id {
                continue;
            }
        }
        let base = match veeros_proxy::resolve_agent_base_url(&state.pool, agent.id, None).await {
            Ok(b) => b,
            Err(_) => continue,
        };
        let agent_id = agent.id;
        let quantum_only = q.quantum;
        let pool = state.pool.clone();

        tasks.push(tokio::spawn(async move {
            match veeros_proxy::discovery_hosts(&base, quantum_only).await {
                Ok(data) => {
                    // Upsert hosts returned by this agent
                    if let Some(hosts) = data.get("hosts").and_then(|v| v.as_array()) {
                        for host in hosts {
                            let _ = upsert_host_from_value(&pool, agent_id, host).await;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        agent_id = %agent_id,
                        err = %e,
                        "Failed to fetch discovery hosts from agent"
                    );
                }
            }
        }));
    }

    // Wait for all fan-out tasks
    for task in tasks {
        let _ = task.await;
    }

    // 3. Return merged list from DB (now up-to-date)
    let rows = DiscoveredHostRepo::list(&state.pool, q.agent_id, q.quantum)
        .await
        .map_err(|e| ApiError::internal(format!("DB error listing hosts: {e}")))?;

    let hosts: Vec<Value> = rows.iter().map(host_row_to_json).collect();
    let count = hosts.len();
    Ok(Json(json!({ "hosts": hosts, "count": count })))
}

// ─── GET /api/v1/discovery/hosts/:id ─────────────────────────────────────────

pub async fn get_host(
    State(state): State<AppState>,
    _auth: AuthContext,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = DiscoveredHostRepo::get(&state.pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {e}")))?
        .ok_or_else(|| ApiError::not_found("Discovered host not found"))?;

    Ok(Json(host_row_to_json(&row)))
}

// ─── DELETE /api/v1/discovery/hosts/:id ──────────────────────────────────────

pub async fn delete_host(
    State(state): State<AppState>,
    _auth: AuthContext,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<StatusCode> {
    DiscoveredHostRepo::delete(&state.pool, id)
        .await
        .map_err(|e| ApiError::internal(format!("DB error: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn host_row_to_json(row: &DiscoveredHostRow) -> Value {
    json!({
        "id":               row.id,
        "agent_id":         row.agent_id,
        "host_id":          row.host_id,
        "hostname":         row.hostname,
        "ip_address":       row.ip_address,
        "port":             row.port,
        "os_version":       row.os_version,
        "capabilities":     row.capabilities,
        "quantum_capable":  row.quantum_capable,
        "quantum_qubits":   row.quantum_qubits,
        "quantum_backends": row.quantum_backends,
        "verified":         row.verified,
        "last_seen_at":     row.last_seen_at,
        "created_at":       row.created_at,
    })
}

async fn upsert_host_from_value(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
    v: &Value,
) -> Result<(), sqlx::Error> {
    let host_id    = v.get("host_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let hostname   = v.get("hostname").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let ip         = v.get("ip_address").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let port       = v.get("port").and_then(|x| x.as_i64()).unwrap_or(2323) as i32;
    let os_version = v.get("os_version").and_then(|x| x.as_str()).map(|s| s.to_string());
    let caps: Vec<String> = v.get("capabilities")
        .and_then(|x| x.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let quantum_capable = v.get("quantum_capable").and_then(|x| x.as_bool()).unwrap_or(false);
    let quantum_qubits  = v.get("quantum_qubits").and_then(|x| x.as_i64()).map(|n| n as i32);
    let quantum_backends: Vec<String> = v.get("quantum_backends")
        .and_then(|x| x.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let verified = v.get("verified").and_then(|x| x.as_bool()).unwrap_or(false);

    if host_id.is_empty() || hostname.is_empty() {
        return Ok(());
    }

    DiscoveredHostRepo::upsert(
        pool,
        agent_id,
        &host_id,
        &hostname,
        &ip,
        port,
        os_version.as_deref(),
        &caps,
        quantum_capable,
        quantum_qubits,
        &quantum_backends,
        verified,
    )
    .await?;

    Ok(())
}
