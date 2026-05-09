use std::{collections::HashMap, sync::Arc, time::Duration};

use aes_gcm::{Aes256Gcm, Nonce, aead::Aead, KeyInit};
use base64::Engine;
use chrono::Utc;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS, Transport};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{RwLock, watch};
use uuid::Uuid;

use ef_db::repo::device::DeviceRepo;
use ef_db::repo::integration::{IntegrationDeliveryAttemptRow, IntegrationEndpointRow, IntegrationRepo};
use ef_db::repo::virtual_iot::VirtualIotConfigRepo;

const TRANSPORT_MQTT_SUBSCRIPTION: &str = "mqtt_subscription";
const TRANSPORT_MQTT_PUBLISHER: &str = "mqtt_publisher";
const TRANSPORT_CLOUD_SINK: &str = "cloud_sink";

const KIND_MQTT_SUBSCRIPTION: &str = "mqtt_subscription";
const KIND_MQTT_PUBLISHER: &str = "mqtt_publisher";
const KIND_AWS_S3_PRESIGNED_PUT: &str = "aws_s3_presigned_put";
const KIND_AWS_IOT_CORE_HTTPS: &str = "aws_iot_core_https";
const KIND_AZURE_IOT_HUB: &str = "azure_iot_hub";
const ENC_PREFIX: &str = "enc:v1:";

#[derive(Clone)]
pub struct IntegrationManager {
    inner: Arc<IntegrationInner>,
}

struct IntegrationInner {
    pool: PgPool,
    http: reqwest::Client,
    mqtt_subscriptions: RwLock<HashMap<Uuid, SubscriptionRuntime>>,
}

struct SubscriptionRuntime {
    config: MqttSubscriptionConfig,
    shutdown_tx: watch::Sender<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorEvent {
    pub tenant_id: Uuid,
    pub device_id: Uuid,
    pub sensor: String,
    pub value: String,
    pub source: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttSubscriptionCreate {
    pub name: String,
    pub broker_host: String,
    #[serde(default = "default_mqtt_port")]
    pub broker_port: u16,
    pub topic_filter: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default = "default_reconnect_secs")]
    pub reconnect_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MqttSubscriptionDbConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub topic_filter: String,
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_tls: bool,
    pub reconnect_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttSubscriptionConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub broker_host: String,
    pub broker_port: u16,
    pub topic_filter: String,
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(skip_serializing)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_tls: bool,
    pub reconnect_secs: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttPublisherCreate {
    pub name: String,
    pub broker_host: String,
    #[serde(default = "default_mqtt_port")]
    pub broker_port: u16,
    #[serde(default)]
    pub topic_root: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MqttPublisherDbConfig {
    pub broker_host: String,
    pub broker_port: u16,
    pub topic_root: String,
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttPublisherConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub broker_host: String,
    pub broker_port: u16,
    pub topic_root: String,
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(skip_serializing)]
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub use_tls: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudSinkCreate {
    AwsS3PresignedPut {
        name: String,
        presigned_put_url: String,
    },
    AwsIotCoreHttps {
        name: String,
        publish_url: String,
        #[serde(default)]
        authorization_header: Option<String>,
    },
    AzureIotHub {
        name: String,
        host_name: String,
        device_id: String,
        sas_token: String,
        #[serde(default = "default_azure_api_version")]
        api_version: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CloudSinkDbConfig {
    AwsS3PresignedPut {
        presigned_put_url: String,
    },
    AwsIotCoreHttps {
        publish_url: String,
        #[serde(default)]
        authorization_header: Option<String>,
    },
    AzureIotHub {
        host_name: String,
        device_id: String,
        sas_token: String,
        api_version: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudSinkConfig {
    AwsS3PresignedPut {
        id: Uuid,
        tenant_id: Uuid,
        name: String,
        presigned_put_url: String,
        created_at: String,
    },
    AwsIotCoreHttps {
        id: Uuid,
        tenant_id: Uuid,
        name: String,
        publish_url: String,
        #[serde(skip_serializing)]
        #[serde(default)]
        authorization_header: Option<String>,
        created_at: String,
    },
    AzureIotHub {
        id: Uuid,
        tenant_id: Uuid,
        name: String,
        host_name: String,
        device_id: String,
        #[serde(skip_serializing)]
        sas_token: String,
        api_version: String,
        created_at: String,
    },
}

#[derive(Debug, Deserialize)]
struct SensorEventMessage {
    device_id: Uuid,
    sensor: String,
    value: Value,
    #[serde(default)]
    source: Option<String>,
}

fn default_mqtt_port() -> u16 {
    1883
}

fn default_reconnect_secs() -> u64 {
    5
}

fn default_azure_api_version() -> String {
    "2021-04-12".to_string()
}

impl IntegrationManager {
    pub fn new(pool: PgPool) -> Self {
        Self {
            inner: Arc::new(IntegrationInner {
                pool,
                http: reqwest::Client::new(),
                mqtt_subscriptions: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub async fn init(&self) {
        let all_subs = match IntegrationRepo::list_enabled_by_transport(
            &self.inner.pool,
            TRANSPORT_MQTT_SUBSCRIPTION,
        )
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!("failed loading persisted mqtt subscriptions: {e}");
                return;
            }
        };

        for row in all_subs {
            match endpoint_to_subscription(&row) {
                Ok(cfg) => {
                    if let Err(e) = self.start_subscription_runtime(cfg).await {
                        tracing::warn!(endpoint_id = %row.id, "failed starting subscription worker: {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!(endpoint_id = %row.id, "invalid subscription config: {e}");
                }
            }
        }
    }

    pub async fn list_mqtt_subscriptions(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<MqttSubscriptionConfig>> {
        let rows = IntegrationRepo::list_by_tenant_transport(
            &self.inner.pool,
            tenant_id,
            TRANSPORT_MQTT_SUBSCRIPTION,
        )
        .await?;
        rows.iter().map(endpoint_to_subscription).collect()
    }

    pub async fn create_mqtt_subscription(
        &self,
        tenant_id: Uuid,
        req: MqttSubscriptionCreate,
    ) -> anyhow::Result<MqttSubscriptionConfig> {
        let id = Uuid::new_v4();
        let client_id = req
            .client_id
            .unwrap_or_else(|| format!("ef-api-sub-{}", &id.to_string()[..8]));

        let db_cfg = MqttSubscriptionDbConfig {
            broker_host: req.broker_host,
            broker_port: req.broker_port,
            topic_filter: req.topic_filter,
            client_id,
            username: req.username,
            password: encrypt_optional_secret(req.password)?,
            use_tls: req.use_tls,
            reconnect_secs: req.reconnect_secs,
        };
        let cfg_json = serde_json::to_value(&db_cfg)?;

        let row = IntegrationRepo::create_endpoint(
            &self.inner.pool,
            tenant_id,
            &req.name,
            TRANSPORT_MQTT_SUBSCRIPTION,
            KIND_MQTT_SUBSCRIPTION,
            &cfg_json,
        )
        .await?;

        let cfg = endpoint_to_subscription(&row)?;
        self.start_subscription_runtime(cfg.clone()).await?;
        Ok(cfg)
    }

    pub async fn delete_mqtt_subscription(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<bool> {
        if let Some(rt) = self.inner.mqtt_subscriptions.write().await.remove(&id) {
            let _ = rt.shutdown_tx.send(true);
        }
        Ok(IntegrationRepo::delete_by_tenant(&self.inner.pool, tenant_id, id).await?)
    }

    pub async fn list_mqtt_publishers(
        &self,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<MqttPublisherConfig>> {
        let rows = IntegrationRepo::list_by_tenant_transport(
            &self.inner.pool,
            tenant_id,
            TRANSPORT_MQTT_PUBLISHER,
        )
        .await?;
        rows.iter().map(endpoint_to_publisher).collect()
    }

    pub async fn create_mqtt_publisher(
        &self,
        tenant_id: Uuid,
        req: MqttPublisherCreate,
    ) -> anyhow::Result<MqttPublisherConfig> {
        let id = Uuid::new_v4();
        let db_cfg = MqttPublisherDbConfig {
            broker_host: req.broker_host,
            broker_port: req.broker_port,
            topic_root: req
                .topic_root
                .unwrap_or_else(|| "edgefabric/sensors".to_string()),
            client_id: req
                .client_id
                .unwrap_or_else(|| format!("ef-api-pub-{}", &id.to_string()[..8])),
            username: req.username,
            password: encrypt_optional_secret(req.password)?,
            use_tls: req.use_tls,
        };
        let cfg_json = serde_json::to_value(&db_cfg)?;

        let row = IntegrationRepo::create_endpoint(
            &self.inner.pool,
            tenant_id,
            &req.name,
            TRANSPORT_MQTT_PUBLISHER,
            KIND_MQTT_PUBLISHER,
            &cfg_json,
        )
        .await?;

        endpoint_to_publisher(&row)
    }

    pub async fn delete_mqtt_publisher(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<bool> {
        Ok(IntegrationRepo::delete_by_tenant(&self.inner.pool, tenant_id, id).await?)
    }

    pub async fn list_cloud_sinks(&self, tenant_id: Uuid) -> anyhow::Result<Vec<CloudSinkConfig>> {
        let rows = IntegrationRepo::list_by_tenant_transport(
            &self.inner.pool,
            tenant_id,
            TRANSPORT_CLOUD_SINK,
        )
        .await?;
        rows.iter().map(endpoint_to_cloud_sink).collect()
    }

    pub async fn create_cloud_sink(
        &self,
        tenant_id: Uuid,
        req: CloudSinkCreate,
    ) -> anyhow::Result<CloudSinkConfig> {
        let (name, kind, cfg_json) = match req {
            CloudSinkCreate::AwsS3PresignedPut {
                name,
                presigned_put_url,
            } => (
                name,
                KIND_AWS_S3_PRESIGNED_PUT,
                serde_json::to_value(CloudSinkDbConfig::AwsS3PresignedPut { presigned_put_url })?,
            ),
            CloudSinkCreate::AwsIotCoreHttps {
                name,
                publish_url,
                authorization_header,
            } => (
                name,
                KIND_AWS_IOT_CORE_HTTPS,
                serde_json::to_value(CloudSinkDbConfig::AwsIotCoreHttps {
                    publish_url,
                    authorization_header: encrypt_optional_secret(authorization_header)?,
                })?,
            ),
            CloudSinkCreate::AzureIotHub {
                name,
                host_name,
                device_id,
                sas_token,
                api_version,
            } => (
                name,
                KIND_AZURE_IOT_HUB,
                serde_json::to_value(CloudSinkDbConfig::AzureIotHub {
                    host_name,
                    device_id,
                    sas_token: encrypt_secret(&sas_token)?,
                    api_version,
                })?,
            ),
        };

        let row = IntegrationRepo::create_endpoint(
            &self.inner.pool,
            tenant_id,
            &name,
            TRANSPORT_CLOUD_SINK,
            kind,
            &cfg_json,
        )
        .await?;

        endpoint_to_cloud_sink(&row)
    }

    pub async fn delete_cloud_sink(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<bool> {
        Ok(IntegrationRepo::delete_by_tenant(&self.inner.pool, tenant_id, id).await?)
    }

    pub async fn list_delivery_attempts(
        &self,
        tenant_id: Uuid,
        status: Option<&str>,
        limit: i64,
    ) -> anyhow::Result<Vec<IntegrationDeliveryAttemptRow>> {
        Ok(IntegrationRepo::list_delivery_attempts_by_tenant(
            &self.inner.pool,
            tenant_id,
            status,
            limit,
        )
        .await?)
    }

    pub async fn ingest_sensor_event(&self, tenant_id: Uuid, event: SensorEvent) {
        if let Err(e) = self.persist_sensor_event(&event).await {
            tracing::warn!(device_id = %event.device_id, sensor = %event.sensor, "failed to persist mqtt sensor event: {e}");
        }

        self.forward_sensor_event(tenant_id, event).await;
    }

    pub async fn forward_sensor_event(&self, tenant_id: Uuid, event: SensorEvent) {
        let payload = json!({
            "tenant_id": event.tenant_id,
            "device_id": event.device_id,
            "sensor": event.sensor,
            "value": event.value,
            "source": event.source,
            "timestamp": event.timestamp,
        });

        if let Ok(rows) = IntegrationRepo::list_enabled_by_tenant_transport(
            &self.inner.pool,
            tenant_id,
            TRANSPORT_MQTT_PUBLISHER,
        )
        .await
        {
            for row in rows {
                let _ = self
                    .deliver_with_retry(&row, &payload, &event, 3)
                    .await;
            }
        }

        if let Ok(rows) = IntegrationRepo::list_enabled_by_tenant_transport(
            &self.inner.pool,
            tenant_id,
            TRANSPORT_CLOUD_SINK,
        )
        .await
        {
            for row in rows {
                let _ = self
                    .deliver_with_retry(&row, &payload, &event, 3)
                    .await;
            }
        }
    }

    async fn start_subscription_runtime(&self, cfg: MqttSubscriptionConfig) -> anyhow::Result<()> {
        if self.inner.mqtt_subscriptions.read().await.contains_key(&cfg.id) {
            return Ok(());
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        self.inner.mqtt_subscriptions.write().await.insert(
            cfg.id,
            SubscriptionRuntime {
                config: cfg.clone(),
                shutdown_tx,
            },
        );

        let manager = self.clone();
        tokio::spawn(async move {
            manager.run_subscription_worker(cfg, shutdown_rx).await;
        });

        Ok(())
    }

    async fn run_subscription_worker(
        &self,
        cfg: MqttSubscriptionConfig,
        mut shutdown_rx: watch::Receiver<bool>,
    ) {
        loop {
            if *shutdown_rx.borrow() {
                break;
            }

            let mut opts = MqttOptions::new(cfg.client_id.clone(), cfg.broker_host.clone(), cfg.broker_port);
            opts.set_keep_alive(Duration::from_secs(30));
            if cfg.use_tls {
                opts.set_transport(Transport::tls_with_default_config());
            }
            if let Some(username) = cfg.username.clone() {
                opts.set_credentials(username, cfg.password.clone().unwrap_or_default());
            }

            let (client, mut eventloop) = AsyncClient::new(opts, 100);
            if let Err(e) = client.subscribe(cfg.topic_filter.clone(), QoS::AtLeastOnce).await {
                tracing::warn!(name = %cfg.name, "mqtt subscribe failed: {e}");
                tokio::time::sleep(Duration::from_secs(cfg.reconnect_secs)).await;
                continue;
            }

            loop {
                tokio::select! {
                    changed = shutdown_rx.changed() => {
                        if changed.is_ok() && *shutdown_rx.borrow() {
                            let _ = client.disconnect().await;
                            return;
                        }
                    }
                    polled = eventloop.poll() => {
                        match polled {
                            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                                let message = String::from_utf8_lossy(&publish.payload).to_string();
                                match serde_json::from_str::<SensorEventMessage>(&message) {
                                    Ok(parsed) => {
                                        let source = parsed
                                            .source
                                            .unwrap_or_else(|| format!("mqtt-sub:{}", cfg.name));
                                        let event = SensorEvent {
                                            tenant_id: cfg.tenant_id,
                                            device_id: parsed.device_id,
                                            sensor: parsed.sensor,
                                            value: normalize_sensor_value(parsed.value),
                                            source,
                                            timestamp: Utc::now().to_rfc3339(),
                                        };
                                        self.ingest_sensor_event(cfg.tenant_id, event).await;
                                    }
                                    Err(e) => {
                                        tracing::warn!(subscription = %cfg.name, topic = %publish.topic, "invalid sensor event payload: {e}");
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(subscription = %cfg.name, "mqtt eventloop error: {e}");
                                break;
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(cfg.reconnect_secs)).await;
        }
    }

    async fn persist_sensor_event(&self, event: &SensorEvent) -> anyhow::Result<()> {
        let Some(config) = VirtualIotConfigRepo::find_by_device_id(&self.inner.pool, event.device_id).await? else {
            return Ok(());
        };

        let mut sensor_state = config.sensor_state.as_object().cloned().unwrap_or_default();
        sensor_state.insert(event.sensor.clone(), Value::String(event.value.clone()));

        let mut sensor_log = config.sensor_log.as_array().cloned().unwrap_or_default();
        sensor_log.push(json!({
            "ts": event.timestamp,
            "sensor": event.sensor,
            "value": event.value,
            "source": event.source,
        }));
        if sensor_log.len() > 200 {
            sensor_log.drain(0..sensor_log.len().saturating_sub(200));
        }

        let _ = VirtualIotConfigRepo::update_sensor_runtime(
            &self.inner.pool,
            event.device_id,
            &Value::Object(sensor_state),
            &Value::Array(sensor_log),
        )
        .await?;

        let _ = DeviceRepo::update_heartbeat(&self.inner.pool, event.device_id).await;
        Ok(())
    }

    async fn deliver_with_retry(
        &self,
        endpoint: &IntegrationEndpointRow,
        payload: &Value,
        event: &SensorEvent,
        max_attempts: i32,
    ) -> anyhow::Result<()> {
        for attempt in 1..=max_attempts {
            let result = self.deliver_once(endpoint, payload, event).await;
            match result {
                Ok(()) => {
                    let _ = IntegrationRepo::create_delivery_attempt(
                        &self.inner.pool,
                        endpoint.id,
                        Some(endpoint.tenant_id),
                        Some(event.device_id),
                        Some(&event.sensor),
                        payload,
                        "success",
                        attempt,
                        None,
                    )
                    .await;
                    let _ = IntegrationRepo::mark_delivery_success(&self.inner.pool, endpoint.id).await;
                    return Ok(());
                }
                Err(e) => {
                    let err = e.to_string();
                    let _ = IntegrationRepo::create_delivery_attempt(
                        &self.inner.pool,
                        endpoint.id,
                        Some(endpoint.tenant_id),
                        Some(event.device_id),
                        Some(&event.sensor),
                        payload,
                        "failed",
                        attempt,
                        Some(&err),
                    )
                    .await;

                    if attempt == max_attempts {
                        let _ = IntegrationRepo::create_delivery_attempt(
                            &self.inner.pool,
                            endpoint.id,
                            Some(endpoint.tenant_id),
                            Some(event.device_id),
                            Some(&event.sensor),
                            payload,
                            "dead_letter",
                            attempt,
                            Some(&err),
                        )
                        .await;
                        let _ = IntegrationRepo::mark_delivery_error(&self.inner.pool, endpoint.id, &err).await;
                        return Err(anyhow::anyhow!(err));
                    }

                    let backoff_ms = 250u64.saturating_mul(2u64.saturating_pow((attempt - 1) as u32));
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }

        Ok(())
    }

    async fn deliver_once(
        &self,
        endpoint: &IntegrationEndpointRow,
        payload: &Value,
        event: &SensorEvent,
    ) -> anyhow::Result<()> {
        match endpoint.transport.as_str() {
            TRANSPORT_MQTT_PUBLISHER => {
                let publisher = endpoint_to_publisher(endpoint)?;
                publish_to_mqtt(&publisher, event, payload).await
            }
            TRANSPORT_CLOUD_SINK => {
                let sink = endpoint_to_cloud_sink(endpoint)?;
                self.push_to_cloud_sink(&sink, payload).await
            }
            _ => Err(anyhow::anyhow!("unsupported transport: {}", endpoint.transport)),
        }
    }

    async fn push_to_cloud_sink(&self, sink: &CloudSinkConfig, payload: &Value) -> anyhow::Result<()> {
        match sink {
            CloudSinkConfig::AwsS3PresignedPut {
                presigned_put_url, ..
            } => {
                self.inner
                    .http
                    .put(presigned_put_url)
                    .header("content-type", "application/json")
                    .body(payload.to_string())
                    .send()
                    .await?
                    .error_for_status()?;
            }
            CloudSinkConfig::AwsIotCoreHttps {
                publish_url,
                authorization_header,
                ..
            } => {
                let mut req = self.inner.http.post(publish_url).json(payload);
                if let Some(auth) = authorization_header {
                    req = req.header("authorization", auth);
                }
                req.send().await?.error_for_status()?;
            }
            CloudSinkConfig::AzureIotHub {
                host_name,
                device_id,
                sas_token,
                api_version,
                ..
            } => {
                let url = format!(
                    "https://{host_name}/devices/{device_id}/messages/events?api-version={api_version}"
                );
                self.inner
                    .http
                    .post(url)
                    .header("authorization", sas_token)
                    .header("content-type", "application/json")
                    .body(payload.to_string())
                    .send()
                    .await?
                    .error_for_status()?;
            }
        }

        Ok(())
    }
}

fn endpoint_to_subscription(row: &IntegrationEndpointRow) -> anyhow::Result<MqttSubscriptionConfig> {
    let cfg: MqttSubscriptionDbConfig = serde_json::from_value(row.config.clone())?;
    Ok(MqttSubscriptionConfig {
        id: row.id,
        tenant_id: row.tenant_id,
        name: row.name.clone(),
        broker_host: cfg.broker_host,
        broker_port: cfg.broker_port,
        topic_filter: cfg.topic_filter,
        client_id: cfg.client_id,
        username: cfg.username,
        password: decrypt_optional_secret(cfg.password),
        use_tls: cfg.use_tls,
        reconnect_secs: cfg.reconnect_secs,
        created_at: row.created_at.to_rfc3339(),
    })
}

fn endpoint_to_publisher(row: &IntegrationEndpointRow) -> anyhow::Result<MqttPublisherConfig> {
    let cfg: MqttPublisherDbConfig = serde_json::from_value(row.config.clone())?;
    Ok(MqttPublisherConfig {
        id: row.id,
        tenant_id: row.tenant_id,
        name: row.name.clone(),
        broker_host: cfg.broker_host,
        broker_port: cfg.broker_port,
        topic_root: cfg.topic_root,
        client_id: cfg.client_id,
        username: cfg.username,
        password: decrypt_optional_secret(cfg.password),
        use_tls: cfg.use_tls,
        created_at: row.created_at.to_rfc3339(),
    })
}

fn endpoint_to_cloud_sink(row: &IntegrationEndpointRow) -> anyhow::Result<CloudSinkConfig> {
    let cfg: CloudSinkDbConfig = serde_json::from_value(row.config.clone())?;
    let created_at = row.created_at.to_rfc3339();

    match cfg {
        CloudSinkDbConfig::AwsS3PresignedPut { presigned_put_url } => Ok(
            CloudSinkConfig::AwsS3PresignedPut {
                id: row.id,
                tenant_id: row.tenant_id,
                name: row.name.clone(),
                presigned_put_url,
                created_at,
            },
        ),
        CloudSinkDbConfig::AwsIotCoreHttps {
            publish_url,
            authorization_header,
        } => Ok(CloudSinkConfig::AwsIotCoreHttps {
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name.clone(),
            publish_url,
            authorization_header: decrypt_optional_secret(authorization_header),
            created_at,
        }),
        CloudSinkDbConfig::AzureIotHub {
            host_name,
            device_id,
            sas_token,
            api_version,
        } => Ok(CloudSinkConfig::AzureIotHub {
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name.clone(),
            host_name,
            device_id,
            sas_token: decrypt_secret(&sas_token).unwrap_or_else(|_| String::new()),
            api_version,
            created_at,
        }),
    }
}

fn encrypt_optional_secret(value: Option<String>) -> anyhow::Result<Option<String>> {
    value.map(|v| encrypt_secret(&v)).transpose()
}

fn decrypt_optional_secret(value: Option<String>) -> Option<String> {
    value.and_then(|v| decrypt_secret(&v).ok())
}

fn encrypt_secret(plain: &str) -> anyhow::Result<String> {
    if plain.is_empty() {
        return Ok(String::new());
    }
    let key = integration_secret_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("invalid encryption key: {e}"))?;

    let mut nonce_bytes = [0u8; 12];
    nonce_bytes.copy_from_slice(&Uuid::new_v4().as_bytes()[..12]);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plain.as_bytes())
        .map_err(|e| anyhow::anyhow!("secret encryption failed: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD;
    Ok(format!(
        "{ENC_PREFIX}{}:{}",
        b64.encode(nonce_bytes),
        b64.encode(ciphertext)
    ))
}

fn decrypt_secret(value: &str) -> anyhow::Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    if !value.starts_with(ENC_PREFIX) {
        return Ok(value.to_string());
    }

    let payload = &value[ENC_PREFIX.len()..];
    let mut parts = payload.splitn(2, ':');
    let nonce_b64 = parts.next().ok_or_else(|| anyhow::anyhow!("invalid encrypted secret format"))?;
    let ct_b64 = parts.next().ok_or_else(|| anyhow::anyhow!("invalid encrypted secret format"))?;

    let b64 = base64::engine::general_purpose::STANDARD;
    let nonce = b64
        .decode(nonce_b64)
        .map_err(|e| anyhow::anyhow!("invalid encrypted nonce: {e}"))?;
    let ciphertext = b64
        .decode(ct_b64)
        .map_err(|e| anyhow::anyhow!("invalid encrypted payload: {e}"))?;

    if nonce.len() != 12 {
        return Err(anyhow::anyhow!("invalid encrypted nonce length"));
    }

    let key = integration_secret_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| anyhow::anyhow!("invalid encryption key: {e}"))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|e| anyhow::anyhow!("secret decryption failed: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("invalid decrypted secret encoding: {e}"))
}

fn integration_secret_key() -> anyhow::Result<[u8; 32]> {
    let raw = std::env::var("EF_INTEGRATIONS_SECRET_KEY").map_err(|_| {
        anyhow::anyhow!(
            "EF_INTEGRATIONS_SECRET_KEY is required for integration secret encryption/decryption"
        )
    })?;

    parse_secret_key(&raw)
}

fn parse_secret_key(raw: &str) -> anyhow::Result<[u8; 32]> {
    if let Some(hex) = raw.strip_prefix("hex:") {
        let decoded = decode_hex(hex)?;
        return slice_to_32(&decoded);
    }

    if let Some(b64) = raw.strip_prefix("base64:") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| anyhow::anyhow!("invalid base64 integration key: {e}"))?;
        return slice_to_32(&decoded);
    }

    if raw.len() == 32 {
        return slice_to_32(raw.as_bytes());
    }

    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(raw) {
        if decoded.len() == 32 {
            return slice_to_32(&decoded);
        }
    }

    Err(anyhow::anyhow!(
        "EF_INTEGRATIONS_SECRET_KEY must be 32-byte raw, base64, base64:<...>, or hex:<64-hex>"
    ))
}

fn slice_to_32(bytes: &[u8]) -> anyhow::Result<[u8; 32]> {
    if bytes.len() != 32 {
        return Err(anyhow::anyhow!("integration secret key must be 32 bytes"));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(out)
}

fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return Err(anyhow::anyhow!("hex integration key must have even length"));
    }

    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = from_hex_nibble(bytes[i])?;
        let lo = from_hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn from_hex_nibble(b: u8) -> anyhow::Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(10 + (b - b'a')),
        b'A'..=b'F' => Ok(10 + (b - b'A')),
        _ => Err(anyhow::anyhow!("invalid hex byte in integration key")),
    }
}

fn normalize_sensor_value(value: Value) -> String {
    match value {
        Value::String(v) => v,
        other => other.to_string(),
    }
}

async fn publish_to_mqtt(
    publisher: &MqttPublisherConfig,
    event: &SensorEvent,
    payload: &Value,
) -> anyhow::Result<()> {
    let mut opts = MqttOptions::new(
        publisher.client_id.clone(),
        publisher.broker_host.clone(),
        publisher.broker_port,
    );
    opts.set_keep_alive(Duration::from_secs(15));
    if publisher.use_tls {
        opts.set_transport(Transport::tls_with_default_config());
    }
    if let Some(username) = publisher.username.clone() {
        opts.set_credentials(username, publisher.password.clone().unwrap_or_default());
    }

    let (client, mut eventloop) = AsyncClient::new(opts, 10);
    let topic = format!(
        "{}/{}/{}",
        publisher.topic_root.trim_end_matches('/'),
        event.device_id,
        event.sensor
    );

    client
        .publish(topic, QoS::AtLeastOnce, false, payload.to_string())
        .await?;

    let _ = tokio::time::timeout(Duration::from_secs(1), eventloop.poll()).await;
    let _ = client.disconnect().await;

    Ok(())
}
