use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct InfrastructureSimulationRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub identity: String,
    pub simulation_kind: String,
    pub scenario_id: String,
    pub engine: String,
    pub engine_version: String,
    pub seed: i64,
    pub source_manifest_revision: Option<String>,
    pub document: Value,
    pub created_at: DateTime<Utc>,
}

pub struct InfrastructureSimulationRepo;

impl InfrastructureSimulationRepo {
    pub async fn create(
        pool: &PgPool,
        tenant_id: Uuid,
        identity: &str,
        simulation_kind: &str,
        scenario_id: &str,
        engine: &str,
        engine_version: &str,
        seed: i64,
        source_manifest_revision: Option<&str>,
        document: &Value,
    ) -> Result<InfrastructureSimulationRow, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureSimulationRow>(
            r#"INSERT INTO infrastructure_simulations
                   (tenant_id, identity, simulation_kind, scenario_id, engine,
                    engine_version, seed, source_manifest_revision, document)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               RETURNING id, tenant_id, identity, simulation_kind, scenario_id,
                         engine, engine_version, seed, source_manifest_revision,
                         document, created_at"#,
        )
        .bind(tenant_id)
        .bind(identity)
        .bind(simulation_kind)
        .bind(scenario_id)
        .bind(engine)
        .bind(engine_version)
        .bind(seed)
        .bind(source_manifest_revision)
        .bind(document)
        .fetch_one(pool)
        .await
    }

    pub async fn list(
        pool: &PgPool,
        tenant_id: Uuid,
        scenario_id: Option<&str>,
    ) -> Result<Vec<InfrastructureSimulationRow>, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureSimulationRow>(
            r#"SELECT id, tenant_id, identity, simulation_kind, scenario_id, engine,
                      engine_version, seed, source_manifest_revision, document, created_at
               FROM infrastructure_simulations
               WHERE tenant_id = $1 AND ($2::text IS NULL OR scenario_id = $2)
               ORDER BY created_at DESC"#,
        )
        .bind(tenant_id)
        .bind(scenario_id)
        .fetch_all(pool)
        .await
    }

    pub async fn get(
        pool: &PgPool,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<InfrastructureSimulationRow>, sqlx::Error> {
        sqlx::query_as::<_, InfrastructureSimulationRow>(
            r#"SELECT id, tenant_id, identity, simulation_kind, scenario_id, engine,
                      engine_version, seed, source_manifest_revision, document, created_at
               FROM infrastructure_simulations
               WHERE tenant_id = $1 AND id = $2"#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(pool)
        .await
    }
}
