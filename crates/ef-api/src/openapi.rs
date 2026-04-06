use utoipa::OpenApi;

use crate::middleware::error_response::{ApiError, ApiErrorBody};
use crate::routes::LoginRequest;

/// EdgeFabric API — OpenAPI specification.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "EdgeFabric API",
        version = "0.1.0",
        description = "Unified device cloud platform — test, automate, scale across real/virtual devices",
        license(name = "MIT"),
    ),
    paths(
        crate::routes::health,
        crate::routes::login,
    ),
    components(
        schemas(LoginRequest, ApiError, ApiErrorBody),
    ),
    tags(
        (name = "health", description = "Health check"),
        (name = "auth",   description = "Authentication"),
        (name = "devices", description = "Device management"),
        (name = "sessions", description = "Session management"),
    )
)]
pub struct ApiDoc;
