use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported operating systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Android,
    Ios,
    Iot,
}

/// Whether the device is real hardware or virtual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Real,
    Virtual,
}

/// Runtime status of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceStatus {
    Available,
    Busy,
    Offline,
    Maintenance,
}

/// A device registered in the EdgeFabric control plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub name: String,
    pub os: Os,
    pub os_version: String,
    pub kind: DeviceKind,
    pub status: DeviceStatus,
    pub region: String,
    pub capabilities: Vec<String>,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

/// Filter criteria used by the scheduler.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceFilter {
    pub os: Option<Os>,
    pub os_version: Option<String>,
    pub kind: Option<DeviceKind>,
    pub region: Option<String>,
    pub capabilities: Option<Vec<String>>,
}
