use ef_core::device::{Device, DeviceStatus};
use std::collections::HashMap;
use uuid::Uuid;

/// In-memory device & session state (will be backed by PostgreSQL + Redis).
pub struct StateManager {
    devices: HashMap<Uuid, Device>,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
        }
    }

    pub fn register_device(&mut self, device: Device) {
        tracing::info!(device_id = %device.id, name = %device.name, "Device registered");
        self.devices.insert(device.id, device);
    }

    pub fn get_device(&self, id: &Uuid) -> Option<&Device> {
        self.devices.get(id)
    }

    pub fn list_available(&self) -> Vec<&Device> {
        self.devices
            .values()
            .filter(|d| d.status == DeviceStatus::Available)
            .collect()
    }

    pub fn set_status(&mut self, id: &Uuid, status: DeviceStatus) {
        if let Some(device) = self.devices.get_mut(id) {
            device.status = status;
        }
    }
}
