use std::collections::HashMap;
use std::time::Duration;

use sqlx::PgPool;
use tokio::time::{Instant, interval};
use uuid::Uuid;

use ef_db::repo::device::DeviceRepo;

#[derive(Debug, Clone, sqlx::FromRow)]
struct VirtualIotRuntimeRow {
    device_id: Uuid,
    name: String,
    sim_rate_ms: i32,
    status: String,
}

pub fn spawn_virtual_iot_runtime(pool: PgPool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(1));
        let mut next_heartbeat_due: HashMap<Uuid, Instant> = HashMap::new();

        loop {
            tick.tick().await;

            let devices = match sqlx::query_as::<_, VirtualIotRuntimeRow>(
                r#"SELECT d.id AS device_id, d.name, v.sim_rate_ms, d.status::text AS status
                   FROM devices d
                   JOIN virtual_iot_configs v ON v.device_id = d.id
                   WHERE d.os = 'virtual_iot'::device_os"#,
            )
            .fetch_all(&pool)
            .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::error!("Virtual IoT runtime query failed: {e}");
                    continue;
                }
            };

            next_heartbeat_due.retain(|device_id, _| devices.iter().any(|d| d.device_id == *device_id));

            let now = Instant::now();
            for device in devices {
                if device.status == "maintenance" {
                    continue;
                }

                let heartbeat_every_ms = device.sim_rate_ms.clamp(500, 5_000) as u64;
                let due_at = next_heartbeat_due.entry(device.device_id).or_insert(now);

                if now < *due_at {
                    continue;
                }

                if let Err(e) = DeviceRepo::update_heartbeat(&pool, device.device_id).await {
                    tracing::error!(device_id = %device.device_id, name = %device.name, "Virtual IoT heartbeat failed: {e}");
                    continue;
                }

                if let Err(e) = DeviceRepo::set_status(&pool, device.device_id, "available").await {
                    tracing::error!(device_id = %device.device_id, name = %device.name, "Virtual IoT status refresh failed: {e}");
                    continue;
                }

                *due_at = now + Duration::from_millis(heartbeat_every_ms);
            }
        }
    })
}