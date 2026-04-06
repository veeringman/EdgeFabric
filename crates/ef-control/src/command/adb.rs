use super::CommandOutput;
use ef_core::error::{EdgeFabricError, Result};
use tokio::process::Command;
use tracing::debug;

/// Low-level Android Debug Bridge (ADB) command driver.
///
/// All methods accept a `serial` — the ADB serial of the target device
/// (e.g. `R5CT900ABCD`, `emulator-5554`, `192.168.1.100:5555`).
///
/// Arguments are passed as separate `Command` args to prevent shell injection.
pub struct AdbDriver;

impl AdbDriver {
    /// Execute an ADB command targeting a specific device.
    async fn exec(serial: &str, args: &[&str]) -> Result<CommandOutput> {
        debug!(serial, ?args, "adb");
        let output = Command::new("adb")
            .arg("-s")
            .arg(serial)
            .args(args)
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Install an APK. Uses `-r` to allow reinstall over existing.
    pub async fn install(serial: &str, apk_path: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["install", "-r", apk_path]).await
    }

    /// Uninstall a package by name.
    pub async fn uninstall(serial: &str, package: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["uninstall", package]).await
    }

    /// Run an ADB shell command (each element is a separate arg — no shell expansion).
    pub async fn shell(serial: &str, cmd: &[&str]) -> Result<CommandOutput> {
        let mut args = vec!["shell"];
        args.extend_from_slice(cmd);
        Self::exec(serial, &args).await
    }

    /// Inject a tap at screen coordinates (x, y).
    pub async fn input_tap(serial: &str, x: u32, y: u32) -> Result<CommandOutput> {
        let xs = x.to_string();
        let ys = y.to_string();
        Self::exec(serial, &["shell", "input", "tap", &xs, &ys]).await
    }

    /// Inject a swipe from (x1, y1) to (x2, y2) over `duration_ms` milliseconds.
    pub async fn input_swipe(
        serial: &str,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> Result<CommandOutput> {
        let vals: Vec<String> = [x1, y1, x2, y2, duration_ms]
            .iter()
            .map(|v| v.to_string())
            .collect();
        let mut args: Vec<&str> = vec!["shell", "input", "swipe"];
        args.extend(vals.iter().map(|s| s.as_str()));
        Self::exec(serial, &args).await
    }

    /// Send a key event (e.g. `KEYCODE_HOME`, `3`, `KEYCODE_BACK`).
    pub async fn input_key(serial: &str, keycode: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["shell", "input", "keyevent", keycode]).await
    }

    /// Type text on the focused input field.
    pub async fn input_text(serial: &str, text: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["shell", "input", "text", text]).await
    }

    /// Capture a screenshot as raw PNG bytes via `exec-out screencap -p`.
    pub async fn screencap(serial: &str) -> Result<Vec<u8>> {
        let output = Command::new("adb")
            .arg("-s")
            .arg(serial)
            .args(["exec-out", "screencap", "-p"])
            .output()
            .await?;

        if !output.status.success() {
            return Err(EdgeFabricError::Internal(format!(
                "screencap failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(output.stdout)
    }

    /// Dump the last `lines` logcat entries.
    pub async fn logcat(serial: &str, lines: u32) -> Result<CommandOutput> {
        let n = lines.to_string();
        Self::exec(serial, &["logcat", "-d", "-t", &n]).await
    }

    /// Clear all logcat buffers.
    pub async fn logcat_clear(serial: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["logcat", "-c"]).await
    }

    /// Reboot the device.
    pub async fn reboot(serial: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["reboot"]).await
    }

    /// Push a local file to the device filesystem.
    pub async fn push(serial: &str, local: &str, remote: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["push", local, remote]).await
    }

    /// Pull a file from the device to a local path.
    pub async fn pull(serial: &str, remote: &str, local: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["pull", remote, local]).await
    }

    /// List all installed packages.
    pub async fn list_packages(serial: &str) -> Result<Vec<String>> {
        let output = Self::exec(serial, &["shell", "pm", "list", "packages"]).await?;
        if !output.success() {
            return Err(EdgeFabricError::Internal(format!(
                "pm list packages failed: {}",
                output.stderr
            )));
        }
        Ok(output
            .stdout
            .lines()
            .filter_map(|l| l.strip_prefix("package:"))
            .map(|s| s.trim().to_string())
            .collect())
    }

    /// Read a system property (`getprop`).
    pub async fn get_prop(serial: &str, prop: &str) -> Result<String> {
        let output = Self::exec(serial, &["shell", "getprop", prop]).await?;
        Ok(output.stdout.trim().to_string())
    }

    /// Kill an emulator instance.
    pub async fn emu_kill(serial: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["emu", "kill"]).await
    }

    /// Forward a local TCP port to a device port.
    pub async fn forward(serial: &str, host_port: u16, device_port: u16) -> Result<CommandOutput> {
        let h = format!("tcp:{host_port}");
        let d = format!("tcp:{device_port}");
        Self::exec(serial, &["forward", &h, &d]).await
    }

    /// Remove all port forwards for a device.
    pub async fn forward_remove_all(serial: &str) -> Result<CommandOutput> {
        Self::exec(serial, &["forward", "--remove-all"]).await
    }
}
