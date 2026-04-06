use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting EdgeFabric Agent");

    // TODO: load config, connect to control plane, start device discovery loop
    tracing::info!("Agent running — waiting for control plane connection");

    // Keep the agent alive
    tokio::signal::ctrl_c().await?;
    tracing::info!("Agent shutting down");

    Ok(())
}
