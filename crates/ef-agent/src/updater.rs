//! Auto-update mechanism — periodically checks for a new agent binary
//! and replaces the running version if an update is available.

use std::path::PathBuf;
use std::time::Duration;

/// Check for and apply agent updates.
pub struct AutoUpdater {
    update_url: String,
    current_version: String,
    enabled: bool,
}

impl AutoUpdater {
    pub fn new(update_url: &str, enabled: bool) -> Self {
        Self {
            update_url: update_url.to_string(),
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            enabled,
        }
    }

    /// Spawn a background task that checks for updates periodically.
    pub fn spawn_update_checker(
        self,
        check_interval_secs: u64,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if !self.enabled {
                tracing::info!("Auto-update is disabled");
                return;
            }

            let mut interval = tokio::time::interval(Duration::from_secs(check_interval_secs));
            tracing::info!(
                current_version = %self.current_version,
                url = %self.update_url,
                "Auto-update checker started"
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {},
                    _ = shutdown.changed() => {
                        tracing::info!("Update checker received shutdown signal");
                        break;
                    }
                }

                match self.check_for_update().await {
                    Ok(Some(new_version)) => {
                        tracing::info!(
                            current = %self.current_version,
                            new = %new_version,
                            "New version available"
                        );
                        if let Err(e) = self.download_and_apply(&new_version).await {
                            tracing::error!("Auto-update failed: {e}");
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("No update available");
                    }
                    Err(e) => {
                        tracing::warn!("Update check failed: {e}");
                    }
                }
            }
        })
    }

    /// Check if a newer version is available.
    async fn check_for_update(&self) -> anyhow::Result<Option<String>> {
        let url = format!("{}/latest/version.txt", self.update_url);
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Version check returned {}", resp.status());
        }

        let latest = resp.text().await?.trim().to_string();
        if latest != self.current_version {
            Ok(Some(latest))
        } else {
            Ok(None)
        }
    }

    /// Download the new binary and prepare it for replacement.
    ///
    /// On Linux, we download to a temporary file, make it executable,
    /// and replace the current binary. The running process then exits
    /// and is expected to be restarted by a process supervisor (systemd, etc.).
    async fn download_and_apply(&self, version: &str) -> anyhow::Result<()> {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;
        let url = format!(
            "{}/v{version}/ef-agent-{os}-{arch}",
            self.update_url
        );

        tracing::info!(url = %url, "Downloading update");

        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .timeout(Duration::from_secs(300))
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Download failed: {}", resp.status());
        }

        let bytes = resp.bytes().await?;

        // Write to a temporary file next to the current binary
        let current_exe = std::env::current_exe()?;
        let tmp_path = current_exe.with_extension("update");

        tokio::fs::write(&tmp_path, &bytes).await?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&tmp_path, perms)?;
        }

        // Atomic rename to replace current binary
        let backup_path = current_exe.with_extension("bak");
        if backup_path.exists() {
            std::fs::remove_file(&backup_path)?;
        }
        std::fs::rename(&current_exe, &backup_path)?;
        std::fs::rename(&tmp_path, &current_exe)?;

        tracing::info!(
            version,
            "Update applied — restart required (expected via process supervisor)"
        );

        // Signal the process to shut down so the supervisor restarts with
        // the new binary. In practice you'd send SIGTERM to self or set
        // the shutdown watch.
        // For now, just log.

        Ok(())
    }
}
