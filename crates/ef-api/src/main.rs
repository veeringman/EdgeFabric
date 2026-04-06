use anyhow::Result;
use axum::{Router, routing::get};
use tracing_subscriber::EnvFilter;

mod routes;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .json()
        .init();

    tracing::info!("Starting EdgeFabric API server");

    let app = Router::new()
        .route("/health", get(routes::health))
        .nest("/api/v1", routes::api_router());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("Listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
