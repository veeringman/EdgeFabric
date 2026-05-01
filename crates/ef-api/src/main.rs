use anyhow::Result;
use axum::{Router, middleware as axum_mw, routing::get};
use ef_core::config::AppConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod auth;
mod grpc;
mod middleware;
mod openapi;
mod proto;
mod routes;
mod session_manager;
mod virtual_iot_runtime;
mod sensor_bus;
mod veeros_proxy;

#[cfg(test)]
mod api_tests;

use middleware::auth_extractor::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration (default.toml → {env}.toml → EF_ env vars)
    let cfg = AppConfig::load().expect("Failed to load configuration");

    // ── P12.1 + P12.4 Unified telemetry init ─────────────────────
    let telemetry_guard = ef_core::telemetry::init(
        &cfg.logging.level,
        &cfg.logging.format,
        &cfg.telemetry,
    );
    let prometheus = Arc::new(telemetry_guard);

    tracing::info!("Starting EdgeFabric API server");

    // Connect to PostgreSQL
    let pool = ef_db::pool::connect(&cfg.database).await?;

    // Run migrations
    ef_db::pool::run_migrations(&pool).await?;

    // Connect to NATS (optional — session events degrade gracefully if unavailable)
    let nats = match ef_db::nats::connect(&cfg.nats).await {
        Ok(nc) => {
            tracing::info!("NATS connected — session events enabled");
            Some(nc)
        }
        Err(e) => {
            tracing::warn!("NATS unavailable ({e}), session events disabled");
            None
        }
    };

    // Spawn session reaper (P5.3 — auto-expire timed-out sessions every 30s)
    session_manager::spawn_session_reaper(pool.clone(), nats.clone(), 30);

    // Spawn heartbeat-based stale device checker — mark devices offline if no
    // heartbeat received within 60 s (checked every 15 s).
    ef_control::state::spawn_stale_device_checker(pool.clone(), 60, 15);

    // Minimal Virtual IoT runtime — periodically heartbeats saved virtual devices
    // so they behave like software-backed controllers instead of going stale.
    let bus = sensor_bus::new_bus();
    virtual_iot_runtime::spawn_virtual_iot_runtime(pool.clone(), bus.clone());

    let state = AppState {
        pool,
        jwt_secret: cfg.auth.jwt_secret.clone(),
        veeros: cfg.veeros.clone(),
        sensor_bus: bus,
    };

    // Rate limiter: 50 requests/sec per IP
    let rate_limiter = middleware::rate_limit::build_rate_limiter(50);

    let prom = Arc::clone(&prometheus);
    let app = Router::new()
        // Swagger UI
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi::ApiDoc::openapi()))
        // Public endpoints (no auth required)
        .route("/health", get(routes::health))
        .route("/metrics", get(move || {
            let p = Arc::clone(&prom);
            async move { p.render_metrics() }
        }))
        .route("/api/v1/auth/login", axum::routing::post(routes::login))
        // Protected API routes
        .nest("/api/v1", routes::api_router())
        // Middleware stack (applied bottom-up)
        .layer(middleware::request_id::propagate_request_id_layer())
        .layer(middleware::request_id::trace_layer())
        .layer(middleware::request_id::cors_layer())
        .layer(axum_mw::from_fn(middleware::rate_limit::rate_limit_middleware))
        .layer(axum::Extension(rate_limiter))
        .layer(middleware::request_id::set_request_id_layer())
        .with_state(state.clone());

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("HTTP listening on {}", listener.local_addr()?);

    // ──── gRPC server on port 50051 ────
    let grpc_pool = state.pool.clone();
    let grpc_secret = cfg.auth.jwt_secret.clone();
    let grpc_host = cfg.server.host.clone();
    tokio::spawn(async move {
        if let Err(e) = start_grpc_server(grpc_pool, grpc_secret, &grpc_host).await {
            tracing::error!("gRPC server error: {e}");
        }
    });

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;

    Ok(())
}

async fn start_grpc_server(pool: sqlx::PgPool, jwt_secret: String, host: &str) -> Result<()> {
    use proto::device::device_service_server::DeviceServiceServer;
    use proto::session::session_service_server::SessionServiceServer;
    use proto::test::test_service_server::TestServiceServer;
    use proto::agent::agent_service_server::AgentServiceServer;

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(proto::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<DeviceServiceServer<grpc::device_service::DeviceServiceImpl>>()
        .await;
    health_reporter
        .set_serving::<SessionServiceServer<grpc::session_service::SessionServiceImpl>>()
        .await;
    health_reporter
        .set_serving::<TestServiceServer<grpc::test_service::TestServiceImpl>>()
        .await;
    health_reporter
        .set_serving::<AgentServiceServer<grpc::agent_service::AgentServiceImpl>>()
        .await;

    let interceptor = grpc::auth_interceptor::AuthInterceptor::new(pool.clone(), jwt_secret);

    let device_svc = DeviceServiceServer::with_interceptor(
        grpc::device_service::DeviceServiceImpl::new(pool.clone()),
        interceptor.clone(),
    );
    let session_svc = SessionServiceServer::with_interceptor(
        grpc::session_service::SessionServiceImpl::new(pool.clone()),
        interceptor.clone(),
    );
    let test_svc = TestServiceServer::with_interceptor(
        grpc::test_service::TestServiceImpl::new(pool.clone()),
        interceptor,
    );

    // Prototype: AgentService has no auth interceptor yet — the RegisterAgent
    // RPC returns a session token that future revisions will bind via a
    // dedicated interceptor.
    let agent_hub = grpc::agent_service::AgentHub::new();
    let agent_svc = AgentServiceServer::new(
        grpc::agent_service::AgentServiceImpl::new(agent_hub.clone()),
    );

    let grpc_addr: SocketAddr = format!("{host}:50051").parse()?;
    tracing::info!("gRPC listening on {grpc_addr}");

    tonic::transport::Server::builder()
        .add_service(reflection_service)
        .add_service(health_service)
        .add_service(device_svc)
        .add_service(session_svc)
        .add_service(test_svc)
        .add_service(agent_svc)
        .serve(grpc_addr)
        .await?;

    Ok(())
}
