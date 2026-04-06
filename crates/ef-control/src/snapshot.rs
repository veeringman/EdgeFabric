//! Unified snapshot management for virtual devices.
//!
//! Provides a cross-platform abstraction over:
//! - Android emulator snapshots (`avd snapshot save/load`)
//! - iOS simulator state (`simctl erase` for reset; clone for duplication)
//! - QEMU VM snapshots (`savevm` / `loadvm` via monitor)

use crate::command::virtual_device::{QemuIotManager, VirtualDeviceManager};
use ef_core::error::{EdgeFabricError, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

/// The platform a snapshot belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotPlatform {
    AndroidEmulator,
    IosSimulator,
    QemuIot,
}

/// A unified snapshot descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub name: String,
    pub platform: SnapshotPlatform,
    /// The device identifier (ADB serial, simulator UDID, or QEMU monitor port).
    pub device_id: String,
}

/// Cross-platform snapshot operations.
pub struct SnapshotManager;

impl SnapshotManager {
    /// Save a snapshot of the current device state.
    pub async fn save(
        platform: &SnapshotPlatform,
        device_id: &str,
        snapshot_name: &str,
    ) -> Result<()> {
        info!(platform = ?platform, device_id, snapshot_name, "Saving snapshot");
        match platform {
            SnapshotPlatform::AndroidEmulator => {
                let out =
                    VirtualDeviceManager::emulator_snapshot_save(device_id, snapshot_name).await?;
                if !out.success() {
                    return Err(EdgeFabricError::Internal(format!(
                        "Android snapshot save failed: {}",
                        out.stderr
                    )));
                }
                Ok(())
            }
            SnapshotPlatform::IosSimulator => {
                // iOS doesn't have named snapshots — we clone the simulator as a "snapshot"
                let clone_name = format!("{device_id}-snap-{snapshot_name}");
                VirtualDeviceManager::clone_simulator(device_id, &clone_name).await?;
                Ok(())
            }
            SnapshotPlatform::QemuIot => {
                let port: u16 = device_id.parse().map_err(|_| {
                    EdgeFabricError::Internal(
                        "QEMU device_id must be the monitor port number".into(),
                    )
                })?;
                QemuIotManager::snapshot_save(port, snapshot_name).await?;
                Ok(())
            }
        }
    }

    /// Restore a device to a previously saved snapshot.
    pub async fn restore(
        platform: &SnapshotPlatform,
        device_id: &str,
        snapshot_name: &str,
    ) -> Result<()> {
        info!(platform = ?platform, device_id, snapshot_name, "Restoring snapshot");
        match platform {
            SnapshotPlatform::AndroidEmulator => {
                let out =
                    VirtualDeviceManager::emulator_snapshot_load(device_id, snapshot_name).await?;
                if !out.success() {
                    return Err(EdgeFabricError::Internal(format!(
                        "Android snapshot restore failed: {}",
                        out.stderr
                    )));
                }
                Ok(())
            }
            SnapshotPlatform::IosSimulator => {
                // "Restoring" an iOS sim means erasing it back to clean state.
                // If the user saved a clone, they should boot that clone instead.
                let out = VirtualDeviceManager::erase_simulator(device_id).await?;
                if !out.success() {
                    return Err(EdgeFabricError::Internal(format!(
                        "iOS simulator erase failed: {}",
                        out.stderr
                    )));
                }
                Ok(())
            }
            SnapshotPlatform::QemuIot => {
                let port: u16 = device_id.parse().map_err(|_| {
                    EdgeFabricError::Internal(
                        "QEMU device_id must be the monitor port number".into(),
                    )
                })?;
                QemuIotManager::snapshot_load(port, snapshot_name).await?;
                Ok(())
            }
        }
    }

    /// List available snapshots for a device.
    pub async fn list(
        platform: &SnapshotPlatform,
        device_id: &str,
    ) -> Result<Vec<String>> {
        match platform {
            SnapshotPlatform::AndroidEmulator => {
                VirtualDeviceManager::emulator_snapshot_list(device_id).await
            }
            SnapshotPlatform::IosSimulator => {
                // iOS: list simulators whose name starts with "{udid}-snap-"
                let output = VirtualDeviceManager::list_simulators().await?;
                let prefix = format!("{device_id}-snap-");
                let json: serde_json::Value =
                    serde_json::from_str(&output.stdout).unwrap_or_default();
                let mut names = Vec::new();
                if let Some(devices) = json.get("devices").and_then(|d| d.as_object()) {
                    for (_runtime, sims) in devices {
                        if let Some(arr) = sims.as_array() {
                            for sim in arr {
                                if let Some(name) = sim.get("name").and_then(|n| n.as_str()) {
                                    if let Some(snap) = name.strip_prefix(&prefix) {
                                        names.push(snap.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(names)
            }
            SnapshotPlatform::QemuIot => {
                let port: u16 = device_id.parse().map_err(|_| {
                    EdgeFabricError::Internal(
                        "QEMU device_id must be the monitor port number".into(),
                    )
                })?;
                let output = QemuIotManager::snapshot_list(port).await?;
                Ok(output
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect())
            }
        }
    }

    /// Delete a snapshot.
    pub async fn delete(
        platform: &SnapshotPlatform,
        device_id: &str,
        snapshot_name: &str,
    ) -> Result<()> {
        info!(platform = ?platform, device_id, snapshot_name, "Deleting snapshot");
        match platform {
            SnapshotPlatform::AndroidEmulator => {
                let out = VirtualDeviceManager::emulator_snapshot_delete(device_id, snapshot_name)
                    .await?;
                if !out.success() {
                    return Err(EdgeFabricError::Internal(format!(
                        "Android snapshot delete failed: {}",
                        out.stderr
                    )));
                }
                Ok(())
            }
            SnapshotPlatform::IosSimulator => {
                // Delete the cloned simulator acting as a snapshot
                let clone_name = format!("{device_id}-snap-{snapshot_name}");
                // We need to find the UDID of the clone by name — search simulators
                let output = VirtualDeviceManager::list_simulators().await?;
                let json: serde_json::Value =
                    serde_json::from_str(&output.stdout).unwrap_or_default();
                if let Some(devices) = json.get("devices").and_then(|d| d.as_object()) {
                    for (_runtime, sims) in devices {
                        if let Some(arr) = sims.as_array() {
                            for sim in arr {
                                if sim.get("name").and_then(|n| n.as_str()) == Some(&clone_name) {
                                    if let Some(udid) = sim.get("udid").and_then(|u| u.as_str()) {
                                        let _ =
                                            VirtualDeviceManager::delete_simulator(udid).await;
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
                Err(EdgeFabricError::NotFound(format!(
                    "Snapshot clone '{clone_name}' not found"
                )))
            }
            SnapshotPlatform::QemuIot => {
                let port: u16 = device_id.parse().map_err(|_| {
                    EdgeFabricError::Internal(
                        "QEMU device_id must be the monitor port number".into(),
                    )
                })?;
                QemuIotManager::snapshot_delete(port, snapshot_name).await?;
                Ok(())
            }
        }
    }
}
