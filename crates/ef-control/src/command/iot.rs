use super::CommandOutput;
use ef_core::error::{EdgeFabricError, Result};
use tokio::process::Command;
use tracing::debug;

/// IoT device command driver.
///
/// Supports:
/// - SSH command execution (via system `ssh`)
/// - SCP file transfer
/// - MQTT message publishing (via `rumqttc`)
pub struct IotDriver;

impl IotDriver {
    /// Execute a command on a remote device via SSH.
    ///
    /// Uses `StrictHostKeyChecking=accept-new` for convenience.
    /// Production deployments should use proper known_hosts management.
    pub async fn ssh_exec(host: &str, user: &str, cmd: &[&str]) -> Result<CommandOutput> {
        debug!(host, user, ?cmd, "SSH exec");

        let target = format!("{user}@{host}");
        let output = Command::new("ssh")
            .args([
                "-o", "StrictHostKeyChecking=accept-new",
                "-o", "ConnectTimeout=10",
                "-o", "BatchMode=yes",
                &target,
            ])
            .args(cmd)
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Push a local file to a remote device via SCP.
    pub async fn scp_push(
        host: &str,
        user: &str,
        local_path: &str,
        remote_path: &str,
    ) -> Result<CommandOutput> {
        debug!(host, user, local_path, remote_path, "SCP push");

        let target = format!("{user}@{host}:{remote_path}");
        let output = Command::new("scp")
            .args([
                "-o", "StrictHostKeyChecking=accept-new",
                "-o", "ConnectTimeout=10",
                local_path,
                &target,
            ])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Pull a file from a remote device via SCP.
    pub async fn scp_pull(
        host: &str,
        user: &str,
        remote_path: &str,
        local_path: &str,
    ) -> Result<CommandOutput> {
        debug!(host, user, remote_path, local_path, "SCP pull");

        let target = format!("{user}@{host}:{remote_path}");
        let output = Command::new("scp")
            .args([
                "-o", "StrictHostKeyChecking=accept-new",
                "-o", "ConnectTimeout=10",
                &target,
                local_path,
            ])
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Publish an MQTT message to a broker.
    pub async fn mqtt_publish(
        broker_host: &str,
        broker_port: u16,
        topic: &str,
        payload: &[u8],
    ) -> Result<()> {
        use rumqttc::{AsyncClient, MqttOptions, QoS};

        debug!(broker_host, broker_port, topic, "MQTT publish");

        let mut opts = MqttOptions::new("ef-control", broker_host, broker_port);
        opts.set_keep_alive(std::time::Duration::from_secs(5));

        let (client, mut eventloop) = AsyncClient::new(opts, 10);

        // Spawn the event loop to drive the connection
        let el_handle = tokio::spawn(async move {
            loop {
                if eventloop.poll().await.is_err() {
                    break;
                }
            }
        });

        client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
            .map_err(|e| EdgeFabricError::Internal(format!("MQTT publish failed: {e}")))?;

        // Allow the publish to flush, then tear down
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        el_handle.abort();

        Ok(())
    }

    /// Reboot a device via SSH.
    pub async fn reboot(host: &str, user: &str) -> Result<CommandOutput> {
        Self::ssh_exec(host, user, &["sudo", "reboot"]).await
    }

    /// Read system logs via `journalctl` over SSH.
    pub async fn journal_logs(host: &str, user: &str, lines: u32) -> Result<CommandOutput> {
        let n = lines.to_string();
        Self::ssh_exec(host, user, &["journalctl", "-n", &n, "--no-pager"]).await
    }
}
