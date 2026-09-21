use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{Json, Router, routing::{get, post}};
use clap::Parser;
use serde::Deserialize;
use serde_json::{Value, json};

mod protocol;

#[derive(Parser, Debug, Clone)]
#[command(name = "ef-ml-worker")]
struct Args {
    /// EdgeFabric control plane, e.g. http://192.168.29.10:8080
    #[arg(long, env = "EF_CONTROL_URL", default_value = "http://127.0.0.1:8080")]
    control_url: String,
    #[arg(long, env = "EF_TOKEN", default_value = "")]
    token: String,
    #[arg(long, env = "EF_RUNNER_ID", default_value = "")]
    runner_id: String,
    #[arg(long, env = "EF_WORKER_HOST", default_value = "127.0.0.1")]
    worker_host: String,
    #[arg(long, default_value_t = 8090)]
    http_port: u16,
    #[arg(long, default_value_t = 8091)]
    dataplane_port: u16,
    #[arg(long, default_value = "")]
    runtime: String,
}

#[derive(Clone)]
struct AppState {
    python_in: Arc<Mutex<ChildStdin>>,
    python_lock: Arc<Mutex<()>>,
}

#[derive(Deserialize)]
struct ChatReq {
    #[serde(default)]
    model: String,
    messages: Vec<ChatMsg>,
    #[serde(default = "default_max")]
    max_tokens: u32,
}

#[derive(Deserialize)]
struct ChatMsg {
    role: String,
    content: String,
}

fn default_max() -> u32 {
    64
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let runtime = resolve_runtime(&args.runtime)?;
    tracing::info!(path = %runtime.display(), "starting MLX/numpy runtime");

    let mut child = Command::new("python3")
        .arg(&runtime)
        .arg("--http-port")
        .arg(args.http_port.to_string())
        .arg("--dataplane-port")
        .arg(args.dataplane_port.to_string())
        .arg("--bind")
        .arg(&args.worker_host)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn python3 mlx_runtime")?;

    let stdin = child.stdin.take().context("python stdin")?;
    let stdout = child.stdout.take().context("python stdout")?;
    let python_in = Arc::new(Mutex::new(stdin));
    let reader = Arc::new(Mutex::new(BufReader::new(stdout)));

    // Drain runtime banners until READY.
    {
        let mut r = reader.lock().unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            let n = r.read_line(&mut line)?;
            if n == 0 {
                anyhow::bail!("runtime exited before READY");
            }
            if line.contains("EF_ML_READY") {
                break;
            }
        }
    }

    let state = AppState {
        python_in: python_in.clone(),
        python_lock: Arc::new(Mutex::new(())),
    };

    let http_state = state.clone();
    let reader_http = reader.clone();
    let app = Router::new()
        .route("/health", get(|| async { Json(json!({"status":"ok","role":"ef-ml-worker"})) }))
        .route(
            "/v1/chat/completions",
            post(move |Json(body): Json<ChatReq>| {
                let http_state = http_state.clone();
                let reader_http = reader_http.clone();
                async move { chat(http_state, reader_http, body) }
            }),
        )
        .with_state(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], args.http_port));
    tracing::info!("OpenAI-compatible HTTP on {addr} (rank-0 only for serve)");

    let bind = tokio::net::TcpListener::bind(addr).await?;
    let server = axum::serve(bind, app);

    let hb_args = args.clone();
    let hb_in = python_in.clone();
    tokio::spawn(async move {
        if let Err(e) = heartbeat_loop(hb_args, hb_in).await {
            tracing::error!(error = %e, "heartbeat loop ended");
        }
    });

    tokio::select! {
        res = server => { res?; }
        status = tokio::task::spawn_blocking(move || child.wait()) => {
            let st = status??;
            anyhow::bail!("python runtime exited: {st}");
        }
    }
    Ok(())
}

fn resolve_runtime(explicit: &str) -> Result<PathBuf> {
    if !explicit.is_empty() {
        return Ok(PathBuf::from(explicit));
    }
    if let Ok(p) = std::env::var("EF_ML_RUNTIME") {
        return Ok(PathBuf::from(p));
    }
    let crate_runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/mlx_runtime.py");
    if crate_runtime.exists() {
        return Ok(crate_runtime);
    }
    let candidates = [
        PathBuf::from("crates/ef-ml-worker/runtime/mlx_runtime.py"),
        PathBuf::from("runtime/mlx_runtime.py"),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("mlx_runtime.py");
            if beside.exists() {
                return Ok(beside);
            }
            let rel = dir.join("../runtime/mlx_runtime.py");
            if rel.exists() {
                return Ok(rel.canonicalize().unwrap_or(rel));
            }
        }
    }
    for c in candidates {
        if c.exists() {
            return Ok(c);
        }
    }
    anyhow::bail!("mlx_runtime.py not found; set --runtime or EF_ML_RUNTIME");
}

fn chat(
    state: AppState,
    reader: Arc<Mutex<BufReader<std::process::ChildStdout>>>,
    body: ChatReq,
) -> Json<Value> {
    let prompt = body
        .messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");
    let cmd = json!({
        "op": "infer",
        "model": body.model,
        "prompt": prompt,
        "max_tokens": body.max_tokens,
    });
    match roundtrip(&state, &reader, cmd) {
        Ok(v) => Json(openai_wrap(&body.model, v.get("text").and_then(|t| t.as_str()).unwrap_or(""))),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

fn openai_wrap(model: &str, text: &str) -> Value {
    json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": if model.is_empty() { "toy-pp" } else { model },
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 0, "completion_tokens": text.split_whitespace().count(), "total_tokens": 0 }
    })
}

fn roundtrip(
    state: &AppState,
    reader: &Arc<Mutex<BufReader<std::process::ChildStdout>>>,
    cmd: Value,
) -> Result<Value> {
    let _g = state.python_lock.lock().unwrap();
    {
        let mut stdin = state.python_in.lock().unwrap();
        writeln!(stdin, "{cmd}")?;
        stdin.flush()?;
    }
    let mut r = reader.lock().unwrap();
    let mut line = String::new();
    loop {
        line.clear();
        r.read_line(&mut line)?;
        if line.is_empty() {
            anyhow::bail!("runtime closed");
        }
        let t = line.trim();
        if t.starts_with('{') {
            return Ok(serde_json::from_str(t)?);
        }
    }
}

async fn heartbeat_loop(args: Args, python_in: Arc<Mutex<ChildStdin>>) -> Result<()> {
    if args.token.is_empty() || args.runner_id.is_empty() {
        tracing::warn!("EF_TOKEN/EF_RUNNER_ID unset — heartbeat disabled (local mode)");
        return Ok(());
    }
    let client = reqwest::Client::new();
    let url = format!(
        "{}/api/v1/ml/nodes/heartbeat",
        args.control_url.trim_end_matches('/')
    );
    let mut last_job = String::new();
    loop {
        let body = json!({
            "runner_id": args.runner_id,
            "worker_host": args.worker_host,
            "worker_http_port": args.http_port,
            "worker_dataplane_port": args.dataplane_port,
            "free_memory_gb": None::<f64>,
            "heartbeat": {
                "pid": std::process::id(),
                "ts": chrono::Utc::now().to_rfc3339(),
            }
        });
        match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", args.token))
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                if let Ok(v) = resp.json::<Value>().await {
                    if let Some(asg) = v.get("assignment") {
                        if !asg.is_null() {
                            let key = asg.to_string();
                            if key != last_job {
                                last_job = key;
                                let cmd = json!({"op":"assign","assignment": asg});
                                if let Ok(mut stdin) = python_in.lock() {
                                    let _ = writeln!(stdin, "{cmd}");
                                    let _ = stdin.flush();
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => tracing::warn!(error = %e, "heartbeat failed"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
