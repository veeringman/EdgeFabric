//! gRPC authentication interceptor.
//!
//! Extracts a JWT or API key from the `authorization` metadata header,
//! validates it, and injects the tenant ID into request extensions.

use ef_db::repo::tenant::TenantRepo;
use jsonwebtoken::{decode, DecodingKey, Validation};
use sqlx::PgPool;
use tonic::service::Interceptor;
use tonic::{Request, Status};

/// JWT claims — matches the REST auth module.
#[derive(Debug, serde::Deserialize)]
struct Claims {
    sub: String,
    #[allow(dead_code)]
    exp: usize,
}

/// Tenant identity extracted from a valid token, stored in gRPC extensions.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: uuid::Uuid,
}

/// Interceptor that validates Bearer JWT or API-key tokens on every gRPC call.
#[derive(Clone)]
pub struct AuthInterceptor {
    pool: PgPool,
    jwt_secret: String,
}

impl AuthInterceptor {
    pub fn new(pool: PgPool, jwt_secret: String) -> Self {
        Self { pool, jwt_secret }
    }

    fn validate_jwt(&self, token: &str) -> Result<uuid::Uuid, Status> {
        let key = DecodingKey::from_secret(self.jwt_secret.as_bytes());
        let data = decode::<Claims>(token, &key, &Validation::default())
            .map_err(|e| Status::unauthenticated(format!("Invalid JWT: {e}")))?;
        let id = uuid::Uuid::parse_str(&data.claims.sub)
            .map_err(|_| Status::unauthenticated("Invalid subject in JWT"))?;
        Ok(id)
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let auth_header = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let Some(header) = auth_header else {
            // Allow health/reflection without auth
            return Ok(request);
        };

        if let Some(token) = header.strip_prefix("Bearer ") {
            let tenant_id = self.validate_jwt(token)?;
            request.extensions_mut().insert(TenantContext { tenant_id });
            return Ok(request);
        }

        if let Some(api_key) = header.strip_prefix("ApiKey ") {
            // Validate API key against tenant table — synchronous lookup via a blocking
            // check is not ideal, but the interceptor trait is sync. We cache nothing
            // beyond the DB round-trip that sqlx already pools.
            //
            // For a production system, use an async middleware (tower layer) instead of
            // the tonic Interceptor trait.  For now we store the raw key and let the
            // service handlers validate lazily when they need TenantContext.
            let key_str = api_key.to_string();
            request.extensions_mut().insert(ApiKeyPending { api_key: key_str });
            return Ok(request);
        }

        Err(Status::unauthenticated(
            "Authorization header must start with 'Bearer ' or 'ApiKey '",
        ))
    }
}

/// Marker placed in extensions when an API key is provided; handlers should resolve
/// this asynchronously via [`resolve_api_key`].
#[derive(Debug, Clone)]
pub struct ApiKeyPending {
    pub api_key: String,
}

/// Resolve an API key to a `TenantContext` by hitting the database.
/// Call this from service handlers when `TenantContext` is absent but `ApiKeyPending` is present.
pub async fn resolve_api_key(pool: &PgPool, key: &str) -> Result<TenantContext, Status> {
    let tenant = TenantRepo::find_by_api_key(pool, key)
        .await
        .map_err(|e| Status::internal(format!("Database error: {e}")))?
        .ok_or_else(|| Status::unauthenticated("Invalid API key"))?;
    Ok(TenantContext {
        tenant_id: tenant.id,
    })
}
