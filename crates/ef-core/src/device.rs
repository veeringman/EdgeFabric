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
    Ssh,
}

impl Os {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
            Self::Iot => "iot",
            Self::Ssh => "ssh",
        }
    }
}

/// Whether the device is real hardware or virtual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Real,
    Virtual,
}

impl DeviceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Real => "real",
            Self::Virtual => "virtual",
        }
    }
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
    pub serial: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_as_str_roundtrip() {
        for (os, expected) in [
            (Os::Android, "android"),
            (Os::Ios, "ios"),
            (Os::Iot, "iot"),
            (Os::Ssh, "ssh"),
        ] {
            assert_eq!(os.as_str(), expected);
        }
    }

    #[test]
    fn device_kind_as_str() {
        assert_eq!(DeviceKind::Real.as_str(), "real");
        assert_eq!(DeviceKind::Virtual.as_str(), "virtual");
    }

    #[test]
    fn os_serde_roundtrip() {
        let json = serde_json::to_string(&Os::Android).unwrap();
        assert_eq!(json, "\"android\"");
        let parsed: Os = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Os::Android);
    }

    #[test]
    fn device_kind_serde_roundtrip() {
        let json = serde_json::to_string(&DeviceKind::Virtual).unwrap();
        assert_eq!(json, "\"virtual\"");
        let parsed: DeviceKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DeviceKind::Virtual);
    }

    #[test]
    fn device_status_serde_roundtrip() {
        for status in [
            DeviceStatus::Available,
            DeviceStatus::Busy,
            DeviceStatus::Offline,
            DeviceStatus::Maintenance,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: DeviceStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn device_filter_default_is_empty() {
        let f = DeviceFilter::default();
        assert!(f.os.is_none());
        assert!(f.os_version.is_none());
        assert!(f.kind.is_none());
        assert!(f.region.is_none());
        assert!(f.capabilities.is_none());
    }

    #[test]
    fn device_serialize_deserialize() {
        let device = Device {
            id: Uuid::nil(),
            name: "Pixel 7".into(),
            serial: Some("R5CT900ABCD".into()),
            os: Os::Android,
            os_version: "14".into(),
            kind: DeviceKind::Real,
            status: DeviceStatus::Available,
            region: "us-west".into(),
            capabilities: vec!["adb".into(), "usb".into()],
            registered_at: Utc::now(),
            last_heartbeat: None,
        };
        let json = serde_json::to_string(&device).unwrap();
        let parsed: Device = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Pixel 7");
        assert_eq!(parsed.os, Os::Android);
        assert_eq!(parsed.capabilities.len(), 2);
    }
}
