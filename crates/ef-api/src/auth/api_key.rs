use ef_db::repo::tenant::{Tenant, TenantRepo};
use sqlx::PgPool;
use uuid::Uuid;

/// Create a new API key for a tenant. Returns the generated key.
pub async fn create_api_key(
    pool: &PgPool,
    tenant_name: &str,
) -> Result<Tenant, sqlx::Error> {
    let key = format!("ef-{}", Uuid::new_v4().as_simple());
    TenantRepo::create(pool, tenant_name, &key).await
}

/// Look up a tenant by API key. Returns None if the key is invalid or revoked.
pub async fn validate_api_key(
    pool: &PgPool,
    api_key: &str,
) -> Result<Option<Tenant>, sqlx::Error> {
    TenantRepo::find_by_api_key(pool, api_key).await
}

/// Rotate the API key for a tenant — generates a new key and invalidates the old one.
pub async fn rotate_api_key(pool: &PgPool, tenant_id: Uuid) -> Result<Option<String>, sqlx::Error> {
    let new_key = format!("ef-{}", Uuid::new_v4().as_simple());
    let result = sqlx::query_scalar::<_, String>(
        r#"UPDATE tenants SET api_key = $2 WHERE id = $1 RETURNING api_key"#,
    )
    .bind(tenant_id)
    .bind(&new_key)
    .fetch_optional(pool)
    .await?;
    Ok(result)
}

/// Revoke (delete) a tenant's API key by setting it to a dead value.
pub async fn revoke_api_key(pool: &PgPool, tenant_id: Uuid) -> Result<bool, sqlx::Error> {
    let revoked = format!("revoked-{}", Uuid::new_v4().as_simple());
    let result = sqlx::query(r#"UPDATE tenants SET api_key = $2 WHERE id = $1"#)
        .bind(tenant_id)
        .bind(&revoked)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
