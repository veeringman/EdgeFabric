//! Stream recording — save the raw H.264 byte stream to a file for later
//! playback or artifact collection.

use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::capture::FrameData;

/// Handle to a running recording. Drop or call `stop()` to finalise.
pub struct RecordingHandle {
    pub session_id: Uuid,
    pub path: PathBuf,
    cancel: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<u64>>,
}

impl RecordingHandle {
    /// Stop recording and return the total bytes written.
    pub async fn stop(self) -> anyhow::Result<u64> {
        let _ = self.cancel.send(()); // signal the writer loop to exit
        self.task.await?
    }
}

/// Start recording the H.264 frame stream to a file.
///
/// The recording runs until `stop()` is called on the returned handle.
pub fn start_recording(
    session_id: Uuid,
    recording_dir: &Path,
    mut frame_rx: broadcast::Receiver<FrameData>,
) -> anyhow::Result<RecordingHandle> {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("{session_id}_{timestamp}.h264");
    let path = recording_dir.join(&filename);

    tracing::info!(%session_id, path = %path.display(), "Starting stream recording");

    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let out_path = path.clone();

    let task = tokio::spawn(async move {
        // Ensure the directory exists
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(&out_path).await?;
        let mut total_bytes: u64 = 0;

        loop {
            tokio::select! {
                biased;
                _ = &mut cancel_rx => {
                    tracing::debug!(session_id = %session_id, "Recording stop signal received");
                    break;
                }
                frame = frame_rx.recv() => {
                    match frame {
                        Ok(data) => {
                            file.write_all(&data).await?;
                            total_bytes += data.len() as u64;
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(%session_id, "Recording lagged, skipped {n} frames");
                        }
                    }
                }
            }
        }

        file.flush().await?;
        tracing::info!(
            %session_id,
            path = %out_path.display(),
            total_bytes,
            "Recording saved"
        );
        Ok(total_bytes)
    });

    Ok(RecordingHandle {
        session_id,
        path,
        cancel: cancel_tx,
        task,
    })
}

/// Convert a raw H.264 file to MP4 using ffmpeg (post-recording).
pub async fn convert_h264_to_mp4(h264_path: &Path) -> anyhow::Result<PathBuf> {
    let mp4_path = h264_path.with_extension("mp4");
    let status = tokio::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel", "warning",
            "-y",
            "-i", &h264_path.to_string_lossy(),
            "-c:v", "copy",
            &mp4_path.to_string_lossy(),
        ])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!("ffmpeg conversion failed with exit code {:?}", status.code());
    }

    tracing::info!(
        src = %h264_path.display(),
        dst = %mp4_path.display(),
        "Converted recording to MP4"
    );
    Ok(mp4_path)
}
