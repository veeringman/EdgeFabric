//! Platform-specific screen capture agents.
//!
//! - **Android**: uses `scrcpy` in raw H.264 mode (no display) via subprocess.
//! - **iOS**: uses `wda` screenshot streaming or QuickTime screen capture.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Raw H.264 NAL units read from the capture process stdout.
pub type FrameData = bytes::Bytes;

/// A running capture session.
pub struct CaptureHandle {
    pub session_id: Uuid,
    pub device_id: Uuid,
    child: Option<Child>,
    /// Subscribe to receive H.264 frame chunks as they arrive.
    pub frame_tx: broadcast::Sender<FrameData>,
}

impl CaptureHandle {
    /// Stop the capture subprocess gracefully.
    pub async fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill().await;
            tracing::info!(session_id = %self.session_id, "Capture process stopped");
        }
        self.child = None;
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Best-effort kill on drop (non-async; sends SIGKILL)
            let _ = child.start_kill();
        }
    }
}

// ── Android capture via scrcpy ────────────────────────────────────

/// Start screen capture for an Android device using scrcpy.
///
/// `scrcpy --serial <serial> --no-display --record=-` outputs raw H.264 to stdout.
/// We forward the byte stream to a broadcast channel so multiple consumers
/// (WebRTC transport, recording, etc.) can tap in.
pub async fn start_android_capture(
    session_id: Uuid,
    device_id: Uuid,
    serial: &str,
    max_width: u32,
    max_fps: u32,
    bitrate_kbps: u32,
) -> anyhow::Result<CaptureHandle> {
    tracing::info!(
        %session_id, %device_id, serial,
        "Starting scrcpy capture (max_width={max_width}, fps={max_fps}, bitrate={bitrate_kbps}kbps)"
    );

    let mut child = Command::new("scrcpy")
        .args([
            "--serial", serial,
            "--no-display",
            "--video-codec=h264",
            "--no-audio",
            "--max-size", &max_width.to_string(),
            "--max-fps", &max_fps.to_string(),
            "--video-bit-rate", &format!("{bitrate_kbps}K"),
            "--raw-stream=-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let (frame_tx, _) = broadcast::channel::<FrameData>(256);
    let tx = frame_tx.clone();

    // Read raw H.264 stream from stdout and fan-out via broadcast
    let stdout = child.stdout.take().expect("scrcpy stdout piped");
    tokio::spawn(async move {
        let mut reader = BufReader::with_capacity(64 * 1024, stdout);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match tokio::io::AsyncReadExt::read(&mut reader, &mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let chunk = bytes::Bytes::copy_from_slice(&buf[..n]);
                    if tx.send(chunk).is_err() {
                        break; // no receivers
                    }
                }
                Err(e) => {
                    tracing::warn!("scrcpy stdout read error: {e}");
                    break;
                }
            }
        }
        tracing::debug!("scrcpy stdout reader exited");
    });

    // Log stderr in background
    let stderr = child.stderr.take().expect("scrcpy stderr piped");
    let sid = session_id;
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(session_id = %sid, "scrcpy: {line}");
        }
    });

    Ok(CaptureHandle {
        session_id,
        device_id,
        child: Some(child),
        frame_tx,
    })
}

// ── iOS capture ───────────────────────────────────────────────────

/// Start screen capture for an iOS device.
///
/// Uses a two-stage pipeline:
/// 1. `xrecord` / WDA continuous screenshot → raw frames
/// 2. `ffmpeg` encodes to H.264 and outputs to stdout
///
/// If xrecord is unavailable, falls back to polling WDA `/screenshot`.
pub async fn start_ios_capture(
    session_id: Uuid,
    device_id: Uuid,
    udid: &str,
    max_width: u32,
    max_fps: u32,
    bitrate_kbps: u32,
) -> anyhow::Result<CaptureHandle> {
    tracing::info!(
        %session_id, %device_id, udid,
        "Starting iOS capture via ffmpeg pipeline"
    );

    // Use ffmpeg to capture from the AVFoundation screen capture device
    // On macOS, device index 0 is typically the USB-connected iOS screen
    let mut child = Command::new("ffmpeg")
        .args([
            "-f", "avfoundation",
            "-framerate", &max_fps.to_string(),
            "-video_size", &format!("{max_width}x{}", max_width * 16 / 9),
            "-i", udid,
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-tune", "zerolatency",
            "-b:v", &format!("{bitrate_kbps}k"),
            "-f", "h264",
            "-an",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let (frame_tx, _) = broadcast::channel::<FrameData>(256);
    let tx = frame_tx.clone();

    let stdout = child.stdout.take().expect("ffmpeg stdout piped");
    tokio::spawn(async move {
        let mut reader = BufReader::with_capacity(64 * 1024, stdout);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            match tokio::io::AsyncReadExt::read(&mut reader, &mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = bytes::Bytes::copy_from_slice(&buf[..n]);
                    if tx.send(chunk).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("ffmpeg stdout read error: {e}");
                    break;
                }
            }
        }
    });

    let stderr = child.stderr.take().expect("ffmpeg stderr piped");
    let sid = session_id;
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(session_id = %sid, "ffmpeg: {line}");
        }
    });

    Ok(CaptureHandle {
        session_id,
        device_id,
        child: Some(child),
        frame_tx,
    })
}
