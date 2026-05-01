//! Per-device sensor event bus.
//!
//! Every virtual-IoT device gets a `broadcast::Sender<SensorFrame>`.
//! Any number of WebSocket clients subscribe to it; `inject_virtual_iot_sensor`
//! and the software noise-model ticker both publish to it.  The map entry is
//! created lazily on first publish and never explicitly removed — inactive
//! senders simply have no receivers and drop their messages harmlessly.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

/// A single sensor reading event forwarded to all live-viewer clients.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SensorFrame {
    /// The sensor kind key (e.g. `"temperature"`, `"gpio_d2"`, `"gps_lat"`).
    pub sensor: String,
    /// String-encoded value (`"24.5"`, `"1"`, `"-33.8688"`).
    pub value: String,
    /// Who produced the event: `"ui-inject"`, `"simulated"`, `"fold-vm"`.
    pub source: String,
    /// RFC-3339 timestamp.
    pub ts: String,
}

/// Capacity of each device's broadcast channel.
const CHANNEL_CAPACITY: usize = 256;

/// Map from device_id → broadcast sender.
pub type SensorBus = Arc<DashMap<Uuid, broadcast::Sender<SensorFrame>>>;

/// Create a fresh, empty bus.
pub fn new_bus() -> SensorBus {
    Arc::new(DashMap::new())
}

/// Publish a `SensorFrame` to the bus for a device.  
/// Creates the channel entry on first use.  Silently drops the frame when
/// there are no active receivers (all `RecvError::Lagged` / closed).
pub fn publish(bus: &SensorBus, device_id: Uuid, frame: SensorFrame) {
    let sender = bus
        .entry(device_id)
        .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
        .clone();
    let _ = sender.send(frame); // ignore SendError (no receivers) — not an error
}

/// Subscribe to sensor events for a device.
/// Returns `None` when there is no existing sender for that device yet.
/// Callers should call `publish` at least once first, or use
/// `subscribe_or_create` for lazy initialisation.
pub fn subscribe(bus: &SensorBus, device_id: Uuid) -> broadcast::Receiver<SensorFrame> {
    bus.entry(device_id)
        .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
        .subscribe()
}
