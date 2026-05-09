use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::integrations::{
    CloudSinkCreate, MqttPublisherCreate, MqttSubscriptionCreate, SensorEvent,
};
use crate::middleware::auth_extractor::{AppState, AuthContext};
use crate::middleware::error_response::ApiError;

#[derive(Debug, Deserialize)]
pub struct IngestSensorEventRequest {
    pub device_id: Uuid,
    pub sensor: String,
    pub value: String,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeliveryAttemptsQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn list_mqtt_subscriptions(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let subscriptions = state
        .integrations
        .list_mqtt_subscriptions(auth.tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list MQTT subscriptions: {e}")))?;
    Ok(Json(json!({ "subscriptions": subscriptions, "total": subscriptions.len() })))
}

pub async fn create_mqtt_subscription(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<MqttSubscriptionCreate>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    let created = state
        .integrations
        .create_mqtt_subscription(auth.tenant_id, body)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create MQTT subscription: {e}")))?;

    Ok((StatusCode::CREATED, Json(json!({ "subscription": created }))))
}

pub async fn delete_mqtt_subscription(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if state
        .integrations
        .delete_mqtt_subscription(auth.tenant_id, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete MQTT subscription: {e}")))?
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    Err(ApiError::not_found(format!("MQTT subscription {id} not found")))
}

pub async fn list_mqtt_publishers(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let publishers = state
        .integrations
        .list_mqtt_publishers(auth.tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list MQTT publishers: {e}")))?;
    Ok(Json(json!({ "publishers": publishers, "total": publishers.len() })))
}

pub async fn create_mqtt_publisher(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<MqttPublisherCreate>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    let created = state
        .integrations
        .create_mqtt_publisher(auth.tenant_id, body)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create MQTT publisher: {e}")))?;

    Ok((StatusCode::CREATED, Json(json!({ "publisher": created }))))
}

pub async fn delete_mqtt_publisher(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if state
        .integrations
        .delete_mqtt_publisher(auth.tenant_id, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete MQTT publisher: {e}")))?
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    Err(ApiError::not_found(format!("MQTT publisher {id} not found")))
}

pub async fn list_cloud_sinks(
    auth: AuthContext,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let sinks = state
        .integrations
        .list_cloud_sinks(auth.tenant_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list cloud sinks: {e}")))?;
    Ok(Json(json!({ "sinks": sinks, "total": sinks.len() })))
}

pub async fn create_cloud_sink(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<CloudSinkCreate>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    let created = state
        .integrations
        .create_cloud_sink(auth.tenant_id, body)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create cloud sink: {e}")))?;

    Ok((StatusCode::CREATED, Json(json!({ "sink": created }))))
}

pub async fn delete_cloud_sink(
    auth: AuthContext,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if state
        .integrations
        .delete_cloud_sink(auth.tenant_id, id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete cloud sink: {e}")))?
    {
        return Ok(StatusCode::NO_CONTENT);
    }
    Err(ApiError::not_found(format!("Cloud sink {id} not found")))
}

pub async fn list_delivery_attempts(
    auth: AuthContext,
    Query(query): Query<DeliveryAttemptsQuery>,
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<ApiError>)> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let attempts = state
        .integrations
        .list_delivery_attempts(auth.tenant_id, query.status.as_deref(), limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to list delivery attempts: {e}")))?;

    Ok(Json(json!({
        "attempts": attempts,
        "total": attempts.len()
    })))
}

pub async fn ingest_sensor_event(
    auth: AuthContext,
    State(state): State<AppState>,
    Json(body): Json<IngestSensorEventRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ApiError>)> {
    let event = SensorEvent {
        tenant_id: auth.tenant_id,
        device_id: body.device_id,
        sensor: body.sensor,
        value: body.value,
        source: body.source.unwrap_or_else(|| "external-app".to_string()),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    state
        .integrations
        .ingest_sensor_event(auth.tenant_id, event)
        .await;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "status": "accepted", "message": "sensor event ingested" })),
    ))
}
