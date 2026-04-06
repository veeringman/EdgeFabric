use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

/// Standard JSON error envelope returned by all API endpoints.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiError {
    pub error: ApiErrorBody,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorBody {
                code,
                message: message.into(),
                details: None,
            },
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.error.details = Some(details);
        self
    }

    // -- Common constructors --

    pub fn unauthorized(msg: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::UNAUTHORIZED, Json(Self::new("unauthorized", msg)))
    }

    pub fn forbidden(msg: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::FORBIDDEN, Json(Self::new("forbidden", msg)))
    }

    pub fn not_found(msg: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::NOT_FOUND, Json(Self::new("not_found", msg)))
    }

    pub fn bad_request(msg: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::BAD_REQUEST, Json(Self::new("bad_request", msg)))
    }

    pub fn rate_limited() -> (StatusCode, Json<Self>) {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(Self::new("rate_limited", "Too many requests")),
        )
    }

    pub fn internal(msg: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Self::new("internal_error", msg)),
        )
    }
}

/// Convert `ef_core::error::EdgeFabricError` into an HTTP response.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Default to 500; callers should use the tuple constructors above for correct status codes
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

impl ApiError {
    /// Convert an `EdgeFabricError` into a status + JSON error pair.
    pub fn from_domain(err: ef_core::error::EdgeFabricError) -> (StatusCode, Json<ApiError>) {
        use ef_core::error::EdgeFabricError::*;
        match &err {
            DeviceNotFound(_) | SessionNotFound(_) | NotFound(_) => ApiError::not_found(err.to_string()),
            NoMatchingDevices => ApiError::not_found(err.to_string()),
            DeviceUnavailable(_) => (
                StatusCode::CONFLICT,
                Json(ApiError::new("device_unavailable", err.to_string())),
            ),
            InvalidSessionTransition { .. } => ApiError::bad_request(err.to_string()),
            SessionLimitExceeded(_, _) => (
                StatusCode::TOO_MANY_REQUESTS,
                Json(ApiError::new("session_limit_exceeded", err.to_string())),
            ),
            AuthenticationFailed => ApiError::unauthorized(err.to_string()),
            PermissionDenied => ApiError::forbidden(err.to_string()),
            Timeout => (
                StatusCode::GATEWAY_TIMEOUT,
                Json(ApiError::new("timeout", err.to_string())),
            ),
            Internal(_) | Database(_) | Io(_) => ApiError::internal(err.to_string()),
        }
    }
}
