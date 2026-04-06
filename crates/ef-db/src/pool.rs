use ef_core::config::DatabaseConfig;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Create a PostgreSQL connection pool from config.
pub async fn connect(cfg: &DatabaseConfig) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .connect(&cfg.url)
        .await?;

    tracing::info!("PostgreSQL pool connected (max_conns={})", cfg.max_connections);
    Ok(pool)
}

/// Run all pending SQLx migrations.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    tracing::info!("Running database migrations...");
    sqlx::migrate!("../../migrations").run(pool).await?;
    tracing::info!("Migrations complete");
    Ok(())
}
