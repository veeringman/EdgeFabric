use ef_core::config::NatsConfig;

/// Connect to the NATS server.
pub async fn connect(cfg: &NatsConfig) -> Result<async_nats::Client, async_nats::ConnectError> {
    let client = async_nats::connect(&cfg.url).await?;
    tracing::info!("NATS connected");
    Ok(client)
}
