//! Virtual device pool auto-scaling.
//!
//! Maintains a pool of pre-provisioned virtual devices (Android emulators,
//! iOS simulators, QEMU IoT VMs) that can be allocated to test sessions on demand.
//!
//! The scaler runs as a background task and periodically reconciles the actual
//! pool state against the desired configuration, scaling up or down as needed.

use crate::command::virtual_device::{QemuIotManager, QemuVmConfig, VirtualDeviceManager};
use ef_core::error::{EdgeFabricError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// The type/platform of virtual device in the pool.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum VirtualDeviceType {
    /// Android emulator. `profile` is the system-image package string.
    AndroidEmulator { package: String },
    /// iOS simulator. `device_type` + `runtime` identify the template.
    IosSimulator {
        device_type: String,
        runtime: String,
    },
    /// QEMU IoT VM.
    QemuIot { config: QemuVmConfig },
}

/// Scaling policy for a device type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolPolicy {
    /// Minimum number of ready instances to keep.
    pub min_ready: usize,
    /// Maximum number of instances (ready + busy).
    pub max_total: usize,
    /// Seconds an idle instance can sit before being considered for scale-down.
    pub idle_timeout_secs: u64,
}

impl Default for PoolPolicy {
    fn default() -> Self {
        Self {
            min_ready: 1,
            max_total: 5,
            idle_timeout_secs: 300,
        }
    }
}

/// Current state of a pool instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstanceState {
    Provisioning,
    Ready,
    Busy,
    ShuttingDown,
}

/// A tracked virtual device instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInstance {
    /// Unique identifier (AVD name, simulator UDID, or QEMU VM name).
    pub id: String,
    /// Serial / address used to reach the device.
    pub serial: String,
    pub state: InstanceState,
    /// When the instance became idle (transitioned to Ready from Busy).
    pub idle_since: Option<chrono::DateTime<chrono::Utc>>,
}

/// Auto-scaling pool for virtual devices.
pub struct VirtualDevicePool {
    /// Per-device-type pool entries.
    pools: Arc<RwLock<HashMap<String, PoolEntry>>>,
}

struct PoolEntry {
    device_type: VirtualDeviceType,
    policy: PoolPolicy,
    instances: Vec<PoolInstance>,
    /// Next port to assign (for emulators / QEMU).
    next_port: u16,
}

impl VirtualDevicePool {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a device type with its scaling policy.
    pub async fn register(
        &self,
        name: String,
        device_type: VirtualDeviceType,
        policy: PoolPolicy,
        base_port: u16,
    ) {
        let mut pools = self.pools.write().await;
        pools.insert(
            name,
            PoolEntry {
                device_type,
                policy,
                instances: Vec::new(),
                next_port: base_port,
            },
        );
    }

    /// Get a summary of all pools' instance counts.
    pub async fn status(&self) -> HashMap<String, PoolStatus> {
        let pools = self.pools.read().await;
        pools
            .iter()
            .map(|(name, entry)| {
                let ready = entry
                    .instances
                    .iter()
                    .filter(|i| matches!(i.state, InstanceState::Ready))
                    .count();
                let busy = entry
                    .instances
                    .iter()
                    .filter(|i| matches!(i.state, InstanceState::Busy))
                    .count();
                let total = entry.instances.len();
                (
                    name.clone(),
                    PoolStatus {
                        ready,
                        busy,
                        total,
                        min_ready: entry.policy.min_ready,
                        max_total: entry.policy.max_total,
                    },
                )
            })
            .collect()
    }

    /// Allocate an idle instance from the named pool. Returns the serial.
    pub async fn allocate(&self, pool_name: &str) -> Result<PoolInstance> {
        let mut pools = self.pools.write().await;
        let entry = pools
            .get_mut(pool_name)
            .ok_or_else(|| EdgeFabricError::NotFound(format!("Pool '{pool_name}' not found")))?;

        let instance = entry
            .instances
            .iter_mut()
            .find(|i| matches!(i.state, InstanceState::Ready));

        match instance {
            Some(inst) => {
                inst.state = InstanceState::Busy;
                inst.idle_since = None;
                Ok(inst.clone())
            }
            None => Err(EdgeFabricError::Internal(format!(
                "No ready instances in pool '{pool_name}'"
            ))),
        }
    }

    /// Return an instance to the pool (mark as Ready).
    pub async fn release(&self, pool_name: &str, instance_id: &str) -> Result<()> {
        let mut pools = self.pools.write().await;
        let entry = pools
            .get_mut(pool_name)
            .ok_or_else(|| EdgeFabricError::NotFound(format!("Pool '{pool_name}' not found")))?;

        if let Some(inst) = entry.instances.iter_mut().find(|i| i.id == instance_id) {
            inst.state = InstanceState::Ready;
            inst.idle_since = Some(chrono::Utc::now());
        }
        Ok(())
    }

    /// Run one reconciliation cycle: scale up if below min, scale down idle surplus.
    pub async fn reconcile(&self) {
        let mut pools = self.pools.write().await;

        for (name, entry) in pools.iter_mut() {
            let ready_count = entry
                .instances
                .iter()
                .filter(|i| matches!(i.state, InstanceState::Ready))
                .count();
            let total = entry.instances.len();

            // ── Scale Up ──
            if ready_count < entry.policy.min_ready && total < entry.policy.max_total {
                let to_create = (entry.policy.min_ready - ready_count)
                    .min(entry.policy.max_total - total);
                for _ in 0..to_create {
                    let port = entry.next_port;
                    entry.next_port += 2; // emulators use consecutive even ports

                    let instance_id = format!("{name}-{port}");
                    info!(pool = name, instance = instance_id, "Scaling up");

                    let result =
                        Self::provision_instance(&entry.device_type, &instance_id, port).await;

                    match result {
                        Ok(serial) => {
                            entry.instances.push(PoolInstance {
                                id: instance_id,
                                serial,
                                state: InstanceState::Ready,
                                idle_since: Some(chrono::Utc::now()),
                            });
                        }
                        Err(e) => {
                            warn!(pool = name, "Scale-up failed: {e}");
                        }
                    }
                }
            }

            // ── Scale Down ──
            let now = chrono::Utc::now();
            let idle_threshold =
                chrono::Duration::seconds(entry.policy.idle_timeout_secs as i64);

            // Only scale down if we're above min_ready
            let idle_surplus: Vec<String> = entry
                .instances
                .iter()
                .filter(|i| matches!(i.state, InstanceState::Ready))
                .filter(|i| {
                    i.idle_since
                        .map(|t| (now - t) > idle_threshold)
                        .unwrap_or(false)
                })
                .skip(entry.policy.min_ready) // keep at least min_ready
                .map(|i| i.id.clone())
                .collect();

            for id in &idle_surplus {
                info!(pool = name, instance = id, "Scaling down idle instance");
                let _ = Self::destroy_instance(&entry.device_type, id).await;
            }

            entry
                .instances
                .retain(|i| !idle_surplus.contains(&i.id));
        }
    }

    /// Provision a single virtual device instance.
    async fn provision_instance(
        device_type: &VirtualDeviceType,
        instance_id: &str,
        port: u16,
    ) -> Result<String> {
        match device_type {
            VirtualDeviceType::AndroidEmulator { package } => {
                VirtualDeviceManager::provision_emulator(
                    instance_id,
                    package,
                    port,
                    120, // 2-minute boot timeout
                )
                .await
            }
            VirtualDeviceType::IosSimulator {
                device_type,
                runtime,
            } => VirtualDeviceManager::provision_simulator(instance_id, device_type, runtime).await,
            VirtualDeviceType::QemuIot { config } => {
                let mut vm_config = config.clone();
                vm_config.name = instance_id.to_string();
                vm_config.serial_port = port;
                vm_config.monitor_port = port + 1;
                QemuIotManager::start_vm(&vm_config).await?;
                Ok(format!("127.0.0.1:{port}"))
            }
        }
    }

    /// Destroy a virtual device instance.
    async fn destroy_instance(device_type: &VirtualDeviceType, instance_id: &str) -> Result<()> {
        match device_type {
            VirtualDeviceType::AndroidEmulator { .. } => {
                let running = VirtualDeviceManager::list_running_emulators().await?;
                if let Some(serial) = running.iter().find(|s| s.contains(instance_id)) {
                    let _ = VirtualDeviceManager::stop_emulator(serial).await;
                }
                let _ = VirtualDeviceManager::delete_avd(instance_id).await;
                Ok(())
            }
            VirtualDeviceType::IosSimulator { .. } => {
                let _ = VirtualDeviceManager::shutdown_simulator(instance_id).await;
                let _ = VirtualDeviceManager::delete_simulator(instance_id).await;
                Ok(())
            }
            VirtualDeviceType::QemuIot { config } => {
                if QemuIotManager::is_vm_running(config.monitor_port).await {
                    let _ = QemuIotManager::stop_vm(instance_id, config.monitor_port).await;
                }
                QemuIotManager::delete_vm(instance_id).await
            }
        }
    }
}

/// Summary status for a pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatus {
    pub ready: usize,
    pub busy: usize,
    pub total: usize,
    pub min_ready: usize,
    pub max_total: usize,
}

/// Spawn a background task that periodically reconciles all device pools.
pub fn spawn_pool_scaler(
    pool: Arc<VirtualDevicePool>,
    interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            tick.tick().await;
            pool.reconcile().await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pool_register_and_status() {
        let pool = VirtualDevicePool::new();
        pool.register(
            "android-api34".into(),
            VirtualDeviceType::AndroidEmulator {
                package: "system-images;android-34;google_apis;x86_64".into(),
            },
            PoolPolicy {
                min_ready: 2,
                max_total: 5,
                idle_timeout_secs: 300,
            },
            5554,
        )
        .await;

        let status = pool.status().await;
        assert!(status.contains_key("android-api34"));
        let s = &status["android-api34"];
        assert_eq!(s.min_ready, 2);
        assert_eq!(s.max_total, 5);
        assert_eq!(s.total, 0);
    }

    #[tokio::test]
    async fn pool_allocate_empty_returns_error() {
        let pool = VirtualDevicePool::new();
        pool.register(
            "test-pool".into(),
            VirtualDeviceType::AndroidEmulator {
                package: "test".into(),
            },
            PoolPolicy::default(),
            5554,
        )
        .await;

        let result = pool.allocate("test-pool").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn pool_not_found() {
        let pool = VirtualDevicePool::new();
        let result = pool.allocate("nonexistent").await;
        assert!(result.is_err());
    }
}
