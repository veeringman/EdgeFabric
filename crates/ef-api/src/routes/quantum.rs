//! Quantum computing API routes.
//!
//! Routes are mounted under `/api/v1/quantum/*`.  Requests that target a
//! specific VeerOS backend are proxied to the responsible ef-agent using the
//! same veeros_proxy helper pattern used by virtual-IoT.  Requests that don't
//! need a live agent (e.g. listing supported backends) are answered locally.

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
use crate::veeros_proxy;

// ─── request / response types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SimulateRequest {
    /// Named circuit template: "bell", "ghz3", "qft3"
    pub program: Option<String>,
    /// Raw OpenQASM 2.0 source (alternative to `program`)
    pub qasm: Option<String>,
    /// Number of measurement shots (default 1024)
    #[serde(default = "default_shots")]
    pub shots: u32,
    /// Optional PRNG seed for reproducible histograms
    #[serde(default)]
    pub seed: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct IbmRunRequest {
    /// IBM Quantum API token
    pub token: String,
    /// IBM instance string (e.g. "ibm-q/open/main")
    pub instance: String,
    /// Backend name (e.g. "ibmq_qasm_simulator", "ibm_kyoto")
    pub backend: String,
    /// OpenQASM 2.0 source to execute
    pub qasm: String,
    /// Number of shots (default 1024)
    #[serde(default = "default_shots")]
    pub shots: u32,
    /// Whether to include histogram in response (default true)
    #[serde(default = "default_true")]
    pub histogram: bool,
    /// Use IBM staging endpoint instead of production
    #[serde(default)]
    pub endpoint_staging: bool,
}

fn default_shots() -> u32 { 1024 }
fn default_true() -> bool { true }

// ─── helpers ─────────────────────────────────────────────────────────────────

fn proxy_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    ApiError::internal(e.to_string())
}

// ─── handlers ────────────────────────────────────────────────────────────────

/// `GET /api/v1/quantum/backends`
///
/// Returns the static catalog of supported quantum backends (simulator + cloud
/// providers).  No agent required.
pub async fn list_backends(
    _auth: AuthContext,
) -> Json<Value> {
    Json(json!({
        "backends": [
            {
                "id": "veeros-simulator",
                "name": "VeerOS Quantum Simulator",
                "provider": "veeros",
                "type": "simulator",
                "qubits": 12,
                "description": "In-process 12-qubit state-vector simulator built into VeerOS",
                "available": true,
                "quantum_volume": null,
                "basis_gates": ["cx", "u1", "u2", "u3", "h", "x", "y", "z", "s", "t", "measure"]
            },
            {
                "id": "ibmq-qasm-simulator",
                "name": "IBM Quantum QASM Simulator",
                "provider": "ibm",
                "type": "cloud-simulator",
                "qubits": 32,
                "description": "IBM Quantum cloud QASM simulator (requires API token)",
                "available": true,
                "quantum_volume": null,
                "basis_gates": ["cx", "u1", "u2", "u3", "id", "x", "y", "z", "h", "s", "sdg", "t", "tdg", "rx", "ry", "rz", "measure", "barrier", "reset"]
            },
            {
                "id": "ibm-nairobi",
                "name": "IBM Nairobi (7-qubit)",
                "provider": "ibm",
                "type": "cloud-hardware",
                "qubits": 7,
                "description": "IBM Nairobi 7-qubit QPU (requires IBM Quantum Premium access)",
                "available": false,
                "quantum_volume": 32,
                "basis_gates": ["cx", "id", "rz", "sx", "x", "measure", "reset"]
            },
            {
                "id": "ibm-kyoto",
                "name": "IBM Kyoto (127-qubit)",
                "provider": "ibm",
                "type": "cloud-hardware",
                "qubits": 127,
                "description": "IBM Eagle r3 127-qubit QPU (requires IBM Quantum Premium access)",
                "available": false,
                "quantum_volume": 128,
                "basis_gates": ["cx", "id", "rz", "sx", "x", "measure", "reset"]
            }
        ]
    }))
}

/// `POST /api/v1/quantum/agents/{agent_id}/simulate`
///
/// Run a quantum simulation on a named agent's local VeerOS QPU simulator.
/// Proxies to `POST /veeros/quantum/simulate` on the target agent.
pub async fn simulate_on_agent(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<SimulateRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let base = veeros_proxy::resolve_agent_base_url(&state.pool, agent_id, None)
        .await
        .map_err(proxy_err)?;

    let req_body = json!({
        "program": body.program,
        "qasm": body.qasm,
        "shots": body.shots,
        "seed": body.seed,
    });

    let result = veeros_proxy::quantum_simulate(&base, req_body)
        .await
        .map_err(proxy_err)?;

    Ok(Json(result))
}

/// `POST /api/v1/quantum/agents/{agent_id}/ibm/run`
///
/// Submit an OpenQASM circuit to an IBM Quantum backend via a named agent.
/// Proxies to `POST /veeros/quantum/ibm/run` on the target agent.
pub async fn ibm_run_on_agent(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Json(body): Json<IbmRunRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let base = veeros_proxy::resolve_agent_base_url(&state.pool, agent_id, None)
        .await
        .map_err(proxy_err)?;

    let req_body = json!({
        "token": body.token,
        "instance": body.instance,
        "backend": body.backend,
        "qasm": body.qasm,
        "shots": body.shots,
        "histogram": body.histogram,
        "endpoint_staging": body.endpoint_staging,
    });

    let result = veeros_proxy::quantum_ibm_run(&base, req_body)
        .await
        .map_err(proxy_err)?;

    Ok(Json(result))
}

/// `GET /api/v1/quantum/agents/{agent_id}/backends`
///
/// Query a specific agent for the quantum backends it can reach (simulator
/// always available; IBM cloud only when token is configured).
pub async fn agent_quantum_backends(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let base = veeros_proxy::resolve_agent_base_url(&state.pool, agent_id, None)
        .await
        .map_err(proxy_err)?;

    let result = veeros_proxy::quantum_list_backends(&base)
        .await
        .map_err(proxy_err)?;

    Ok(Json(result))
}

/// `GET /api/v1/quantum/agents/{agent_id}/vms/{vm_name}/quantum`
///
/// Return quantum access info for a specific VM running on a given agent.
pub async fn vm_quantum_info(
    _auth: AuthContext,
    State(state): State<AppState>,
    Path((agent_id, vm_name)): Path<(Uuid, String)>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let base = veeros_proxy::resolve_agent_base_url(&state.pool, agent_id, None)
        .await
        .map_err(proxy_err)?;

    let result = veeros_proxy::vm_quantum_info(&base, &vm_name)
        .await
        .map_err(proxy_err)?;

    Ok(Json(result))
}
