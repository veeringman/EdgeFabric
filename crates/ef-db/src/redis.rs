use ef_core::config::RedisConfig;

/// Create a Redis connection manager (multiplexed async connection).
pub async fn connect(
    cfg: &RedisConfig,
) -> Result<::redis::aio::MultiplexedConnection, ::redis::RedisError> {
    let client = ::redis::Client::open(cfg.url.as_str())?;
    let conn = client.get_multiplexed_tokio_connection().await?;
    tracing::info!("Redis connected");
    Ok(conn)
}
