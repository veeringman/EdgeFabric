use axum::http::HeaderName;
use tower_http::{
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Layer that sets a UUID `x-request-id` on incoming requests.
pub fn set_request_id_layer() -> SetRequestIdLayer<MakeRequestUuid> {
    SetRequestIdLayer::new(X_REQUEST_ID.clone(), MakeRequestUuid)
}

/// Layer that propagates `x-request-id` to the response.
pub fn propagate_request_id_layer() -> PropagateRequestIdLayer {
    PropagateRequestIdLayer::new(X_REQUEST_ID.clone())
}

/// HTTP tracing layer — logs requests/responses using `tracing`.
pub fn trace_layer() -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
}

/// CORS layer — permissive for development, should be tightened for production.
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([HeaderName::from_static("x-request-id")])
}
