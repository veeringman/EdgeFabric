use super::CommandOutput;
use ef_core::error::{EdgeFabricError, Result};
use tokio::process::Command;
use tracing::debug;

/// iOS device command driver.
///
/// Uses:
/// - `xcrun devicectl` (Xcode 15+) / `ios-deploy` for app installation
/// - WebDriverAgent (WDA) HTTP API for UI automation
/// - `ideviceinfo` / `idevicesyslog` (libimobiledevice) for device info & logs
pub struct IosDriver;

impl IosDriver {
    /// Install an app (.ipa / .app) on a real device.
    ///
    /// Tries `xcrun devicectl` first, falls back to `ios-deploy`.
    pub async fn install(udid: &str, app_path: &str) -> Result<CommandOutput> {
        debug!(udid, app_path, "iOS install");

        let output = Command::new("xcrun")
            .args([
                "devicectl", "device", "install", "app",
                "--device", udid, app_path,
            ])
            .output()
            .await;

        if let Ok(o) = output {
            if o.status.success() {
                return Ok(CommandOutput {
                    stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
                    exit_code: 0,
                });
            }
        }

        // Fallback: ios-deploy
        let output = Command::new("ios-deploy")
            .args(["--id", udid, "--bundle", app_path])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Capture a screenshot via WDA HTTP API.
    ///
    /// `wda_base_url` is typically `http://localhost:8100`.
    /// WDA returns `{ "value": "<base64-png>", ... }`.
    pub async fn screenshot(wda_base_url: &str) -> Result<Vec<u8>> {
        let url = format!("{wda_base_url}/screenshot");
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA screenshot failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(EdgeFabricError::Internal(format!(
                "WDA screenshot HTTP {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA parse error: {e}")))?;

        let b64 = body["value"]
            .as_str()
            .ok_or_else(|| EdgeFabricError::Internal("WDA screenshot: missing value".into()))?;

        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| EdgeFabricError::Internal(format!("base64 decode error: {e}")))
    }

    /// Tap at (x, y) via WDA.
    pub async fn tap(wda_base_url: &str, x: f64, y: f64) -> Result<()> {
        let url = format!("{wda_base_url}/wda/tap/0");
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&serde_json::json!({ "x": x, "y": y }))
            .send()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA tap failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(EdgeFabricError::Internal(format!(
                "WDA tap HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// Swipe gesture via WDA drag-from-to.
    pub async fn swipe(
        wda_base_url: &str,
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        duration: f64,
    ) -> Result<()> {
        let url = format!("{wda_base_url}/wda/dragfromtoforduration");
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&serde_json::json!({
                "fromX": from_x, "fromY": from_y,
                "toX": to_x, "toY": to_y,
                "duration": duration
            }))
            .send()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA swipe failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(EdgeFabricError::Internal(format!(
                "WDA swipe HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// Type text via WDA keys endpoint.
    pub async fn input_text(wda_base_url: &str, text: &str) -> Result<()> {
        let url = format!("{wda_base_url}/wda/keys");
        let client = reqwest::Client::new();
        let chars: Vec<String> = text.chars().map(|c| c.to_string()).collect();
        let resp = client
            .post(&url)
            .json(&serde_json::json!({ "value": chars }))
            .send()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA keys failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(EdgeFabricError::Internal(format!(
                "WDA keys HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// Press the home button via WDA.
    pub async fn press_home(wda_base_url: &str) -> Result<()> {
        let url = format!("{wda_base_url}/wda/homescreen");
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .send()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA homescreen failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(EdgeFabricError::Internal(format!(
                "WDA homescreen HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// Launch an app by bundle ID via WDA.
    pub async fn launch_app(wda_base_url: &str, bundle_id: &str) -> Result<()> {
        let url = format!("{wda_base_url}/wda/apps/launch");
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&serde_json::json!({ "bundleId": bundle_id }))
            .send()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA launch failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(EdgeFabricError::Internal(format!(
                "WDA launch HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// Terminate an app by bundle ID via WDA.
    pub async fn terminate_app(wda_base_url: &str, bundle_id: &str) -> Result<()> {
        let url = format!("{wda_base_url}/wda/apps/terminate");
        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .json(&serde_json::json!({ "bundleId": bundle_id }))
            .send()
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("WDA terminate failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(EdgeFabricError::Internal(format!(
                "WDA terminate HTTP {}",
                resp.status()
            )));
        }
        Ok(())
    }

    /// Query device info via `ideviceinfo`.
    pub async fn device_info(udid: &str) -> Result<CommandOutput> {
        let output = Command::new("ideviceinfo")
            .args(["-u", udid])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Capture syslog lines via `idevicesyslog` with a timeout.
    pub async fn syslog(udid: &str, lines: u32) -> Result<CommandOutput> {
        let output = Command::new("timeout")
            .args(["2", "idevicesyslog", "-u", udid])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let truncated: String = stdout
            .lines()
            .take(lines as usize)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(CommandOutput {
            stdout: truncated,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: 0,
        })
    }

    /// Reboot a real device via `idevicediagnostics`.
    pub async fn reboot(udid: &str) -> Result<CommandOutput> {
        let output = Command::new("idevicediagnostics")
            .args(["restart", "-u", udid])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}
