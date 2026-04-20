use axum::{
    Json,
    extract::FromRequestParts,
    http::{StatusCode, header, request::Parts},
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{api_key, jwt};
use crate::middleware::error_response::ApiError;
use ef_core::config::VeerOsConfig;

/// Authenticated tenant context extracted from either:
/// - `Authorization: Bearer <jwt>` header
/// - `X-Api-Key: <key>` header
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub tenant_id: Uuid,
    pub tenant_name: String,
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Try Bearer JWT first
        if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION) {
            let auth_str = auth_header
                .to_str()
                .map_err(|_| ApiError::unauthorized("Invalid authorization header"))?;

            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let claims = jwt::validate_token(token, &state.jwt_secret)
                    .map_err(|_| ApiError::unauthorized("Invalid or expired token"))?;

                return Ok(AuthContext {
                    tenant_id: claims.sub,
                    tenant_name: claims.tenant,
                });
            }
        }

        // Try X-Api-Key header
        if let Some(key_header) = parts.headers.get("x-api-key") {
            let key = key_header
                .to_str()
                .map_err(|_| ApiError::unauthorized("Invalid API key header"))?
                .to_owned();

            let tenant = api_key::validate_api_key(&state.pool, &key)
                .await
                .map_err(|_| ApiError::internal("Database error during authentication"))?
                .ok_or_else(|| ApiError::unauthorized("Invalid API key"))?;

            return Ok(AuthContext {
                tenant_id: tenant.id,
                tenant_name: tenant.name,
            });
        }

        Err(ApiError::unauthorized(
            "Missing Authorization header or X-Api-Key",
        ))
    }
}

/// Shared application state available to all handlers and extractors.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: String,
    pub veeros: VeerOsConfig,
}

// Allow handlers to extract PgPool directly from AppState
impl std::ops::Deref for AppState {
    type Target = PgPool;
    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl axum::extract::FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}
