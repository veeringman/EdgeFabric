//! Device command execution engine.
//!
//! Platform-specific drivers:
//! - [`adb`] — Android Debug Bridge commands
//! - [`ios`] — iOS via WebDriverAgent + xcrun toolchain
//! - [`iot`] — IoT via SSH, SCP, MQTT
//! - [`virtual_device`] — Emulator & simulator lifecycle management

pub mod adb;
pub mod ios;
pub mod iot;
pub mod virtual_device;

use ef_core::device::{Device, DeviceKind, Os};
use ef_core::error::{EdgeFabricError, Result};

/// Output from a shell command execution.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Unified command engine that dispatches operations to the correct
/// platform driver based on the device's OS type.
///
/// For Android, the `serial` field is the ADB serial (e.g. `emulator-5554`).
/// For iOS, the `serial` is the UDID or WDA base URL depending on the operation.
/// For IoT, the `serial` is the hostname / IP address.
pub struct CommandEngine;

impl CommandEngine {
    // ── Helpers ────────────────────────────────────────────────────

    fn serial(device: &Device) -> Result<&str> {
        device
            .serial
            .as_deref()
            .ok_or_else(|| EdgeFabricError::Internal(format!("device {} has no serial", device.id)))
    }

    // ── App Installation ──────────────────────────────────────────

    /// Install an application on the device.
    pub async fn install_app(device: &Device, app_path: &str) -> Result<CommandOutput> {
        let serial = Self::serial(device)?;
        match device.os {
            Os::Android => adb::AdbDriver::install(serial, app_path).await,
            Os::Ios => ios::IosDriver::install(serial, app_path).await,
            Os::Iot => iot::IotDriver::scp_push(serial, "root", app_path, "/tmp/app").await,
        }
    }

    /// Uninstall / remove an application.
    pub async fn uninstall_app(device: &Device, package: &str) -> Result<CommandOutput> {
        let serial = Self::serial(device)?;
        match device.os {
            Os::Android => adb::AdbDriver::uninstall(serial, package).await,
            Os::Ios => ios::IosDriver::terminate_app(serial, package)
                .await
                .map(|()| CommandOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: 0,
                }),
            Os::Iot => iot::IotDriver::ssh_exec(serial, "root", &["rm", "-f", package]).await,
        }
    }

    // ── Screenshot ────────────────────────────────────────────────

    /// Capture a screenshot. Returns raw PNG bytes.
    pub async fn screenshot(device: &Device) -> Result<Vec<u8>> {
        let serial = Self::serial(device)?;
        match (device.os, device.kind) {
            (Os::Android, _) => adb::AdbDriver::screencap(serial).await,
            (Os::Ios, DeviceKind::Virtual) => {
                virtual_device::VirtualDeviceManager::simulator_screenshot(serial).await
            }
            (Os::Ios, DeviceKind::Real) => ios::IosDriver::screenshot(serial).await,
            (Os::Iot, _) => Err(EdgeFabricError::Internal(
                "IoT devices do not support screenshots".into(),
            )),
        }
    }

    // ── Input Injection ───────────────────────────────────────────

    /// Tap at screen coordinates.
    pub async fn tap(device: &Device, x: u32, y: u32) -> Result<()> {
        let serial = Self::serial(device)?;
        match device.os {
            Os::Android => {
                adb::AdbDriver::input_tap(serial, x, y).await?;
                Ok(())
            }
            Os::Ios => ios::IosDriver::tap(serial, x as f64, y as f64).await,
            Os::Iot => Err(EdgeFabricError::Internal(
                "IoT devices do not support tap input".into(),
            )),
        }
    }

    /// Swipe from (x1,y1) to (x2,y2) over a duration.
    pub async fn swipe(
        device: &Device,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> Result<()> {
        let serial = Self::serial(device)?;
        match device.os {
            Os::Android => {
                adb::AdbDriver::input_swipe(serial, x1, y1, x2, y2, duration_ms).await?;
                Ok(())
            }
            Os::Ios => {
                ios::IosDriver::swipe(
                    serial,
                    x1 as f64,
                    y1 as f64,
                    x2 as f64,
                    y2 as f64,
                    duration_ms as f64 / 1000.0,
                )
                .await
            }
            Os::Iot => Err(EdgeFabricError::Internal(
                "IoT devices do not support swipe input".into(),
            )),
        }
    }

    /// Send a key event (Android keycode).
    pub async fn key(device: &Device, keycode: &str) -> Result<()> {
        let serial = Self::serial(device)?;
        match device.os {
            Os::Android => {
                adb::AdbDriver::input_key(serial, keycode).await?;
                Ok(())
            }
            Os::Ios => Err(EdgeFabricError::Internal(
                "iOS key events require WDA session; use text() instead".into(),
            )),
            Os::Iot => Err(EdgeFabricError::Internal(
                "IoT devices do not support key input".into(),
            )),
        }
    }

    /// Type text on the focused input field.
    pub async fn text(device: &Device, text: &str) -> Result<()> {
        let serial = Self::serial(device)?;
        match device.os {
            Os::Android => {
                adb::AdbDriver::input_text(serial, text).await?;
                Ok(())
            }
            Os::Ios => ios::IosDriver::input_text(serial, text).await,
            Os::Iot => Err(EdgeFabricError::Internal(
                "IoT devices do not support text input".into(),
            )),
        }
    }

    // ── Log Capture ───────────────────────────────────────────────

    /// Capture recent device logs.
    pub async fn logs(device: &Device, lines: u32) -> Result<String> {
        let serial = Self::serial(device)?;
        match device.os {
            Os::Android => {
                let output = adb::AdbDriver::logcat(serial, lines).await?;
                Ok(output.stdout)
            }
            Os::Ios => {
                let output = ios::IosDriver::syslog(serial, lines).await?;
                Ok(output.stdout)
            }
            Os::Iot => {
                let output = iot::IotDriver::journal_logs(serial, "root", lines).await?;
                Ok(output.stdout)
            }
        }
    }

    // ── Device Reset ──────────────────────────────────────────────

    /// Reset/wipe the device to a clean state.
    pub async fn reset(device: &Device) -> Result<()> {
        let serial = Self::serial(device)?;
        match (device.os, device.kind) {
            (Os::Android, DeviceKind::Virtual) => {
                // Load clean snapshot for emulators
                let _ = virtual_device::VirtualDeviceManager::emulator_snapshot_load(
                    serial, "clean",
                )
                .await;
                Ok(())
            }
            (Os::Android, DeviceKind::Real) => {
                // Reboot real devices (non-destructive)
                adb::AdbDriver::reboot(serial).await?;
                Ok(())
            }
            (Os::Ios, DeviceKind::Virtual) => {
                virtual_device::VirtualDeviceManager::erase_simulator(serial).await?;
                Ok(())
            }
            (Os::Ios, DeviceKind::Real) => {
                ios::IosDriver::reboot(serial).await?;
                Ok(())
            }
            (Os::Iot, _) => {
                iot::IotDriver::reboot(serial, "root").await?;
                Ok(())
            }
        }
    }
}
