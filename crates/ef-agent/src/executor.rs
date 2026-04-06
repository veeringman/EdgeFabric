//! Command executor — receives commands from the control plane and
//! dispatches them to the local device via `CommandEngine`.

use std::sync::Arc;
use std::time::Duration;

use ef_control::command::CommandEngine;
use ef_core::device::Device;

use crate::client::{CommandResult, ControlPlaneClient, PendingCommand};
use crate::discovery::DeviceRegistry;

/// Spawn a background task that polls for and executes pending commands.
pub fn spawn_command_executor(
    client: Arc<ControlPlaneClient>,
    registry: Arc<DeviceRegistry>,
    poll_interval_secs: u64,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(poll_interval_secs));
        let mut shutdown = shutdown;

        tracing::info!(poll_interval_secs, "Command executor started");

        loop {
            tokio::select! {
                _ = interval.tick() => {},
                _ = shutdown.changed() => {
                    tracing::info!("Command executor received shutdown signal");
                    break;
                }
            }

            let commands = match client.poll_commands().await {
                Ok(cmds) => cmds,
                Err(e) => {
                    tracing::debug!("Failed to poll commands: {e}");
                    continue;
                }
            };

            for cmd in commands {
                let device = match registry.device_by_id(cmd.device_id).await {
                    Some(d) => d,
                    None => {
                        tracing::warn!(
                            device_id = %cmd.device_id,
                            "Command for unknown device, skipping"
                        );
                        continue;
                    }
                };

                let result = execute_command(&device, &cmd).await;

                if let Err(e) = client.report_command_result(&result).await {
                    tracing::warn!(command_id = %cmd.id, "Failed to report result: {e}");
                }
            }
        }
    })
}

async fn execute_command(device: &Device, cmd: &PendingCommand) -> CommandResult {
    tracing::info!(
        command_id = %cmd.id,
        device_id = %cmd.device_id,
        command_type = %cmd.command_type,
        "Executing command"
    );

    let result = match cmd.command_type.as_str() {
        "install_app" => {
            let path = cmd.params.get("app_path").and_then(|v| v.as_str()).unwrap_or("");
            CommandEngine::install_app(device, path).await
        }
        "uninstall_app" => {
            let pkg = cmd.params.get("package").and_then(|v| v.as_str()).unwrap_or("");
            CommandEngine::uninstall_app(device, pkg).await
        }
        "screenshot" => {
            match CommandEngine::screenshot(device).await {
                Ok(data) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: true,
                        stdout: base64_encode(&data),
                        stderr: String::new(),
                        exit_code: 0,
                    };
                }
                Err(e) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: false,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        exit_code: 1,
                    };
                }
            }
        }
        "tap" => {
            let x = cmd.params.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let y = cmd.params.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            match CommandEngine::tap(device, x, y).await {
                Ok(()) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: true,
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: 0,
                    };
                }
                Err(e) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: false,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        exit_code: 1,
                    };
                }
            }
        }
        "text" => {
            let text = cmd.params.get("text").and_then(|v| v.as_str()).unwrap_or("");
            match CommandEngine::text(device, text).await {
                Ok(()) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: true,
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: 0,
                    };
                }
                Err(e) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: false,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        exit_code: 1,
                    };
                }
            }
        }
        "logs" => {
            let lines = cmd.params.get("lines").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
            match CommandEngine::logs(device, lines).await {
                Ok(output) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: true,
                        stdout: output,
                        stderr: String::new(),
                        exit_code: 0,
                    };
                }
                Err(e) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: false,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        exit_code: 1,
                    };
                }
            }
        }
        "reset" => {
            match CommandEngine::reset(device).await {
                Ok(()) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: true,
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: 0,
                    };
                }
                Err(e) => {
                    return CommandResult {
                        command_id: cmd.id,
                        success: false,
                        stdout: String::new(),
                        stderr: e.to_string(),
                        exit_code: 1,
                    };
                }
            }
        }
        other => {
            tracing::warn!(command_type = other, "Unknown command type");
            return CommandResult {
                command_id: cmd.id,
                success: false,
                stdout: String::new(),
                stderr: format!("Unknown command type: {other}"),
                exit_code: 127,
            };
        }
    };

    match result {
        Ok(output) => CommandResult {
            command_id: cmd.id,
            success: output.success(),
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
        },
        Err(e) => CommandResult {
            command_id: cmd.id,
            success: false,
            stdout: String::new(),
            stderr: e.to_string(),
            exit_code: 1,
        },
    }
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}
