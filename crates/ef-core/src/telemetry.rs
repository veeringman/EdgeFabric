//! Unified telemetry initialization for EdgeFabric services.
//!
//! Provides:
//! - **Structured logging** with `tracing` (JSON or pretty, with spans)
//! - **Distributed tracing** via OpenTelemetry OTLP exporter
//! - **Prometheus metrics** recorder + scrape endpoint body
//!
//! Both binaries (ef-api, ef-agent) call [`init`] at startup.

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use serde::Deserialize;
use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt, util::SubscriberInitExt,
};

/// Telemetry configuration (loaded from config files / env).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    /// OTLP exporter endpoint (e.g. `http://localhost:4317`). Empty = disabled.
    pub otlp_endpoint: String,
    /// The service name reported to the tracing backend.
    pub service_name: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: String::new(),
            service_name: "edgefabric".into(),
        }
    }
}

/// Handles returned by [`init`] so the caller can serve metrics and shut down cleanly.
pub struct TelemetryGuard {
    /// Prometheus handle — call [`render_metrics`] to produce the scrape body.
    pub prometheus: PrometheusHandle,
}

impl TelemetryGuard {
    /// Render the current metrics as a Prometheus text exposition.
    pub fn render_metrics(&self) -> String {
        self.prometheus.render()
    }
}

/// Initialize the full telemetry stack.
///
/// 1. Prometheus metrics recorder (always)
/// 2. `tracing-subscriber` with JSON or pretty formatter + env filter
/// 3. OpenTelemetry OTLP layer (if `telemetry.otlp_endpoint` is non-empty)
///
/// Returns a [`TelemetryGuard`] that must be kept alive for the process lifetime.
pub fn init(
    log_level: &str,
    log_format: &str,
    telemetry: &TelemetryConfig,
) -> TelemetryGuard {
    // ── 1. Prometheus metrics recorder ────────────────────────────
    let prometheus = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // ── 2. Tracing subscriber ─────────────────────────────────────
    let filter =
        EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = match log_format {
        "pretty" => tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .boxed(),
        _ => tracing_subscriber::fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .boxed(),
    };

    // ── 3. Optional OpenTelemetry OTLP layer ──────────────────────
    if !telemetry.otlp_endpoint.is_empty() {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&telemetry.otlp_endpoint)
            .build()
            .expect("failed to build OTLP exporter");

        let provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
            .with_resource(
                opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new(
                        "service.name",
                        telemetry.service_name.clone(),
                    ),
                ]),
            )
            .build();

        let tracer = provider.tracer(telemetry.service_name.clone());
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        Registry::default()
            .with(filter)
            .with(fmt_layer)
            .with(otel_layer)
            .init();
    } else {
        Registry::default()
            .with(filter)
            .with(fmt_layer)
            .init();
    }

    TelemetryGuard { prometheus }
}

// ── Pre-defined metric keys (P12.3) ──────────────────────────────

/// Record that a device transitioned to a new status.
pub fn record_device_status_change(os: &str, kind: &str, new_status: &str) {
    metrics::counter!("ef_device_status_changes_total", "os" => os.to_string(), "kind" => kind.to_string(), "status" => new_status.to_string())
        .increment(1);
}

/// Increment the active session gauge.
pub fn session_started() {
    metrics::gauge!("ef_sessions_active").increment(1.0);
    metrics::counter!("ef_sessions_total").increment(1);
}

/// Decrement the active session gauge.
pub fn session_ended(duration_secs: f64) {
    metrics::gauge!("ef_sessions_active").decrement(1.0);
    metrics::histogram!("ef_session_duration_seconds").record(duration_secs);
}

/// Record a test job submission.
pub fn job_submitted() {
    metrics::counter!("ef_jobs_submitted_total").increment(1);
    metrics::gauge!("ef_jobs_queued").increment(1.0);
}

/// Record that a job started executing.
pub fn job_started() {
    metrics::gauge!("ef_jobs_queued").decrement(1.0);
    metrics::gauge!("ef_jobs_running").increment(1.0);
}

/// Record that a job completed (passed or failed).
pub fn job_completed(status: &str, duration_secs: f64) {
    metrics::gauge!("ef_jobs_running").decrement(1.0);
    metrics::counter!("ef_jobs_completed_total", "status" => status.to_string()).increment(1);
    metrics::histogram!("ef_job_duration_seconds").record(duration_secs);
}

/// Record an HTTP request.
pub fn http_request(method: &str, path: &str, status: u16, duration_secs: f64) {
    metrics::counter!("ef_http_requests_total", "method" => method.to_string(), "path" => path.to_string(), "status" => status.to_string())
        .increment(1);
    metrics::histogram!("ef_http_request_duration_seconds", "method" => method.to_string(), "path" => path.to_string())
        .record(duration_secs);
}

/// Record a device heartbeat.
pub fn heartbeat_received() {
    metrics::counter!("ef_heartbeats_total").increment(1);
}

/// Record test case results.
pub fn test_case_completed(result: &str) {
    metrics::counter!("ef_test_cases_total", "result" => result.to_string()).increment(1);
}

/// Record device pool utilization snapshot.
pub fn pool_utilization(pool_name: &str, ready: f64, busy: f64, total: f64) {
    metrics::gauge!("ef_pool_ready", "pool" => pool_name.to_string()).set(ready);
    metrics::gauge!("ef_pool_busy", "pool" => pool_name.to_string()).set(busy);
    metrics::gauge!("ef_pool_total", "pool" => pool_name.to_string()).set(total);
}
