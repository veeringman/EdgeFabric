use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sqlx::PgPool;
use tokio::time::{Instant, interval};
use uuid::Uuid;

use ef_db::repo::device::DeviceRepo;

use crate::sensor_bus::{SensorBus, SensorFrame, publish};

#[derive(Debug, Clone, sqlx::FromRow)]
struct VirtualIotRuntimeRow {
    device_id: Uuid,
    name: String,
    sim_rate_ms: i32,
    status: String,
}

/// Global toggle for the synthetic sensor-frame emitter.
///
/// Disabled by default — users drive sensor changes on demand via
/// `POST /api/v1/devices/{id}/virtual-iot/sensors/inject` or
/// `POST /api/v1/virtual-iot/simulation/start|stop`. When enabled, the
/// runtime publishes a noise-model tick per sensor per heartbeat for
/// every virtual-IoT device that isn't in `maintenance`.
static SIMULATE_SENSORS: AtomicBool = AtomicBool::new(false);

pub fn simulation_enabled() -> bool {
    SIMULATE_SENSORS.load(Ordering::Relaxed)
}

pub fn set_simulation_enabled(enabled: bool) -> bool {
    SIMULATE_SENSORS.swap(enabled, Ordering::Relaxed)
}

/// Sensor kinds that the software noise-model ticker emits on every heartbeat
/// when simulation is explicitly enabled.
const SIMULATED_SENSORS: &[(&str, fn(u64) -> String)] = &[
    ("temperature",     |t| format!("{:.1} C",    24.0 + (t as f64 * 0.013).sin() * 2.0)),
    ("humidity",        |t| format!("{:.0} %",    55.0 + (t as f64 * 0.009).cos() * 5.0)),
    ("pressure",        |t| format!("{:.1} hPa",  1013.0 + (t as f64 * 0.007).sin() * 3.0)),
    ("wifi_rssi",       |t| format!("{:.0} dBm",  -58.0 + (t as f64 * 0.021).sin() * 4.0)),
    ("battery_voltage", |t| format!("{:.2} V",    3.90 - (t as f64) * 0.00001)),
];

pub fn spawn_virtual_iot_runtime(pool: PgPool, sensor_bus: SensorBus) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(1));
        let mut next_heartbeat_due: HashMap<Uuid, Instant> = HashMap::new();
        let mut tick_counter: u64 = 0;

        loop {
            tick.tick().await;
            tick_counter = tick_counter.wrapping_add(1);

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
            let ts = chrono::Utc::now().to_rfc3339();
            let simulate = simulation_enabled();
            for device in &devices {
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

                // Emit simulated sensor frames only when explicitly enabled
                // via /api/v1/virtual-iot/simulation/start. By default the
                // dashboard is quiet; users can inject sensor values on
                // demand through the inject endpoint.
                if simulate {
                    for (sensor, noise_fn) in SIMULATED_SENSORS {
                        publish(
                            &sensor_bus,
                            device.device_id,
                            SensorFrame {
                                sensor: sensor.to_string(),
                                value: noise_fn(tick_counter),
                                source: "simulated".to_string(),
                                ts: ts.clone(),
                            },
                        );
                    }
                }

                *due_at = now + Duration::from_millis(heartbeat_every_ms);
            }
        }
    })
}