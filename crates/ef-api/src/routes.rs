use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

pub fn api_router() -> Router {
    Router::new()
        .route("/devices", get(list_devices))
        .route("/sessions", get(list_sessions))
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn list_devices() -> Json<Value> {
    Json(json!({ "devices": [], "total": 0 }))
}

async fn list_sessions() -> Json<Value> {
    Json(json!({ "sessions": [], "total": 0 }))
}
