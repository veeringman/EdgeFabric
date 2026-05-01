//! Agent-side VeerOS VM management HTTP API.
//!
//! This module exposes `scripts/veeros-vm` and `veer-connect` as HTTP
//! endpoints so the control plane (ef-api) can drive VeerOS VMs on this
//! agent host without needing direct access to the scripts.
//!
//! Routes are mounted under `/veeros/*` in the agent's streaming server.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use ef_control::command::veeros::VeerOsDriver;
use ef_core::config::VeerOsConfig;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// Shared VeerOS config handed to every request handler.
#[derive(Clone)]
pub struct VeerOsState {
    pub cfg: Arc<VeerOsConfig>,
}

/// Build a router carrying only the VeerOS endpoints. The caller merges
/// this into the main agent router.
pub fn router(cfg: VeerOsConfig) -> Router {
    let state = VeerOsState { cfg: Arc::new(cfg) };

    Router::new()
        .route("/veeros/vms", post(create_vm))
        .route("/veeros/vms/{name}", get(vm_status).delete(destroy_vm))
        .route("/veeros/vms/{name}/start", post(start_vm))
        .route("/veeros/vms/{name}/stop", post(stop_vm))
        .route("/veeros/vms/{name}/restart", post(restart_vm))
        .route("/veeros/vms/{name}/config", get(vm_config))
        .route("/veeros/vms/{name}/shell_info", get(shell_info))
        .route("/veeros/vms/{name}/sensor", post(inject_sensor))
        .route("/ws/veeros/{name}/shell", get(ws_shell))
        .with_state(state)
}

#[derive(Deserialize)]
pub struct CreateVmRequest {
    pub vm_name: String,
    pub hostname: String,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default = "default_memory")]
    pub memory_mb: u32,
    #[serde(default = "default_net")]
    pub net_mode: String,
}

fn default_target() -> String { "esp32c6".into() }
fn default_memory() -> u32 { 128 }
fn default_net() -> String { "nat".into() }

#[derive(Deserialize)]
pub struct InjectSensorRequest {
    pub sensor: String,
    pub value: String,
}

fn map_err(e: ef_core::error::EdgeFabricError) -> (StatusCode, Json<Value>) {
    tracing::warn!("VeerOS driver error: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": e.to_string() })),
    )
}

async fn create_vm(
    State(state): State<VeerOsState>,
    Json(body): Json<CreateVmRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let out = VeerOsDriver::create_vm(
        &state.cfg,
        &body.vm_name,
        &body.hostname,
        &body.target,
        body.memory_mb,
        &body.net_mode,
    )
    .await
    .map_err(map_err)?;

    Ok(Json(json!({
        "vm_name": body.vm_name,
        "stdout": out.stdout,
        "stderr": out.stderr,
        "exit_code": out.exit_code,
    })))
}

async fn start_vm(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let out = VeerOsDriver::start_vm(&state.cfg, &name).await.map_err(map_err)?;
    Ok(Json(json!({ "vm_name": name, "stdout": out.stdout, "stderr": out.stderr, "exit_code": out.exit_code })))
}

async fn stop_vm(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let out = VeerOsDriver::stop_vm(&state.cfg, &name).await.map_err(map_err)?;
    Ok(Json(json!({ "vm_name": name, "stdout": out.stdout, "stderr": out.stderr, "exit_code": out.exit_code })))
}

async fn restart_vm(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let out = VeerOsDriver::restart_vm(&state.cfg, &name).await.map_err(map_err)?;
    Ok(Json(json!({ "vm_name": name, "stdout": out.stdout, "stderr": out.stderr, "exit_code": out.exit_code })))
}

async fn destroy_vm(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let out = VeerOsDriver::destroy_vm(&state.cfg, &name).await.map_err(map_err)?;
    Ok(Json(json!({ "vm_name": name, "stdout": out.stdout, "stderr": out.stderr, "exit_code": out.exit_code })))
}

async fn vm_status(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let status = VeerOsDriver::vm_status(&state.cfg, &name).await.map_err(map_err)?;
    Ok(Json(json!({ "vm_name": name, "status": status })))
}

async fn vm_config(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = VeerOsDriver::vm_config(&state.cfg, &name).await.map_err(map_err)?;
    Ok(Json(json!({ "vm_name": name, "config": config })))
}

/// Return the VM's connection info (shell host/port + connect_command).
/// Used by ef-api to record real values in the backend row when the VM is
/// hosted on this agent rather than on the control-plane host.
async fn shell_info(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let info = VeerOsDriver::shell_info(&state.cfg, &name).await.map_err(map_err)?;
    Ok(Json(json!({
        "vm_name": info.vm_name,
        "host": info.host,
        "port": info.port,
        "connect_command": info.connect_command,
    })))
}

async fn inject_sensor(
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
    Json(body): Json<InjectSensorRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let out = VeerOsDriver::sensor_inject(&state.cfg, &name, &body.sensor, &body.value)
        .await
        .map_err(map_err)?;
    Ok(Json(json!({
        "vm_name": name,
        "sensor": body.sensor,
        "value": body.value,
        "stdout": out.stdout,
        "stderr": out.stderr,
        "exit_code": out.exit_code,
    })))
}

/// WebSocket bridge: dashboard ↔ veer-connect shell process.
///
/// Text frames from the browser are forwarded to the child process stdin
/// and the child's stdout is forwarded back as binary frames.
async fn ws_shell(
    ws: WebSocketUpgrade,
    State(state): State<VeerOsState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    tracing::info!(vm_name = %name, "VeerOS shell WS upgrade");
    ws.on_upgrade(move |socket| handle_shell(socket, state, name))
}

async fn handle_shell(socket: WebSocket, state: VeerOsState, vm_name: String) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // Resolve host/port by asking the VM config
    let info = match VeerOsDriver::shell_info(&state.cfg, &vm_name).await {
        Ok(info) => info,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(format!("error: {e}\n").into())).await;
            return;
        }
    };

    // Folded VMs: stdin → sensor-feed FIFO, stdout ← `fold logs -f`
    if let Some(fifo_path) = info.sensor_feed_path.clone() {
        handle_shell_folded(ws_tx, ws_rx, &state, &info.connect_command, &fifo_path).await;
        return;
    }

    // Standard TCP VMs: single process stdin/stdout bridge
    let mut child = match spawn_shell_command(&state, &info.connect_command, &info.host, info.port) {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(format!("spawn failed: {e}\n").into())).await;
            return;
        }
    };

    let mut stdin = child.stdin.take();
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();

    // Browser → child stdin
    let ws_to_child = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(t) => {
                    if let Some(sin) = stdin.as_mut() {
                        if sin.write_all(t.as_bytes()).await.is_err() { break; }
                    }
                }
                Message::Binary(b) => {
                    if let Some(sin) = stdin.as_mut() {
                        if sin.write_all(&b).await.is_err() { break; }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Child stdout → browser
    let stdout_tx = tokio::spawn({
        async move {
            let mut buf = [0u8; 4096];
            if let Some(out) = stdout.as_mut() {
                loop {
                    match out.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await;
                        }
                        Err(_) => break,
                    }
                }
            }
            ws_tx
        }
    });

    let _ = child.wait().await;
    ws_to_child.abort();
    if let Ok(mut ws_tx) = stdout_tx.await {
        // drain stderr best-effort
        if let Some(err) = stderr.as_mut() {
            let mut ebuf = Vec::new();
            let _ = err.read_to_end(&mut ebuf).await;
            if !ebuf.is_empty() {
                let _ = ws_tx.send(Message::Binary(ebuf.into())).await;
            }
        }
        let _ = ws_tx.send(Message::Close(None)).await;
    }
}

/// Shell bridge for folded VMs:
///   - stdout: `fold logs -f <name>` (a child process whose stdout we stream)
///   - stdin:  sensor-feed FIFO (writes go directly to the file, bypassing the child)
async fn handle_shell_folded(
    mut ws_tx: futures::stream::SplitSink<WebSocket, Message>,
    mut ws_rx: futures::stream::SplitStream<WebSocket>,
    state: &VeerOsState,
    logs_command: &str,  // e.g. "fold logs -f <vm>"
    fifo_path: &str,
) {
    let _ = ws_tx
        .send(Message::Text(
            "\r\n[EdgeFabric] Connected to VeerOS folded runtime (root shell, no login).\r\n".into(),
        ))
        .await;

    // Open sensor-feed FIFO for writing (non-blocking open so we don't hang
    // forever if no reader is on the other side yet).
    let fifo_path = fifo_path.to_owned();
    let logs_command = logs_command.to_owned();

    // Spawn `fold logs -f <vm>` — its stdout is the VM console output stream.
    let mut parts = logs_command.split_whitespace();
    let program = match parts.next() {
        Some(p) => p.to_owned(),
        None => {
            let _ = ws_tx.send(Message::Text("error: empty logs command\n".into())).await;
            return;
        }
    };
    let args: Vec<String> = parts.map(|s| s.to_owned()).collect();

    let mut child = match Command::new(&program)
        .current_dir(&state.cfg.root_dir)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = ws_tx.send(Message::Text(format!("spawn failed: {e}\n").into())).await;
            return;
        }
    };

    let mut stdout = child.stdout.take();
    let fifo_path_for_stdout = fifo_path.clone();

    // WS input → sensor-feed FIFO (VM keyboard input via UART)
    let (fifo_ready_tx, mut fifo_ready_rx) = tokio::sync::mpsc::unbounded_channel::<Result<(), String>>();

    let fifo_writer = tokio::spawn(async move {
        // Open FIFO with O_WRONLY|O_NONBLOCK; retry briefly if not ready.
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            let mut attempts = 0u8;
            loop {
                match std::fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&fifo_path)
                {
                    Ok(f) => {
                        // Convert std::fs::File → tokio::fs::File
                        break Some(tokio::fs::File::from_std(f));
                    }
                    Err(_) if attempts < 10 => {
                        attempts += 1;
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    }
                    Err(_) => break None,
                }
            }
        };
        let mut file = match file {
            Some(f) => {
                let _ = fifo_ready_tx.send(Ok(()));
                f
            }
            None => {
                let _ = fifo_ready_tx.send(Err(format!(
                    "cannot open sensor FIFO {} for shell input (VM may still be booting)",
                    fifo_path
                )));
                return;
            }
        };
        // Send a single newline so the VM redraws a fresh prompt line for
        // this session. VeerOS has no login gate; it's already root.
        let _ = tokio::io::AsyncWriteExt::write_all(&mut file, b"\n").await;
        while let Some(Ok(msg)) = ws_rx.next().await {
            let bytes = match msg {
                Message::Text(t) => t.as_bytes().to_vec(),
                Message::Binary(b) => b.to_vec(),
                Message::Close(_) => break,
                _ => continue,
            };
            if tokio::io::AsyncWriteExt::write_all(&mut file, &bytes).await.is_err() {
                break;
            }
        }
    });

    // Child stdout → WS (VM console output)
    let _ = fifo_path_for_stdout; // legacy: previously used for forced-logout hack
    let stdout_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        if let Some(out) = stdout.as_mut() {
            loop {
                match tokio::io::AsyncReadExt::read(out, &mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        ws_tx
    });

    if let Some(fifo_result) = fifo_ready_rx.recv().await {
        if let Err(msg) = fifo_result {
            if let Ok(mut tx) = stdout_task.await {
                let _ = tx
                    .send(Message::Text(format!("\r\n[EdgeFabric] {msg}\r\n").into()))
                    .await;
                let _ = tx.send(Message::Close(None)).await;
            }
            let _ = child.kill().await;
            return;
        }
    }

    let _ = child.wait().await;
    fifo_writer.abort();
    if let Ok(mut ws_tx) = stdout_task.await {
        let _ = ws_tx.send(Message::Close(None)).await;
    }
}

fn spawn_shell_command(
    state: &VeerOsState,
    connect_command: &str,
    host: &str,
    port: u16,
) -> Result<tokio::process::Child, std::io::Error> {
    let command_line = if connect_command.trim().is_empty() {
        format!("{} shell {} {}", state.cfg.veer_connect_path, host, port)
    } else {
        connect_command.trim().to_string()
    };

    let mut parts = command_line.split_whitespace();
    let program = parts.next().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "shell command is empty")
    })?;

    Command::new(program)
        .current_dir(&state.cfg.root_dir)
        .args(parts)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
}
