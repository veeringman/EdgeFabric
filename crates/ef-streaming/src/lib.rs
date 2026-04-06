//! # ef-streaming
//!
//! WebRTC streaming engine for real-time device screen capture and remote input.
//!
//! ## Architecture
//!
//! ```text
//! Device → capture (scrcpy / ffmpeg) → H.264 frames → broadcast channel
//!                                                    ↓              ↓
//!                                            WebRTC transport   recording
//!                                                    ↓
//!                                            Browser (SDP/ICE via WS signaling)
//!                                                    ↓
//!                                            Data channel → input relay → device
//! ```

pub mod capture;
pub mod encode;
pub mod input;
pub mod recording;
pub mod signaling;
pub mod transport;
pub mod turn;

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use ef_core::config::StreamingConfig;
use ef_core::device::{Device, Os};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::capture::CaptureHandle;
use crate::input::spawn_input_relay;
use crate::recording::RecordingHandle;
use crate::signaling::SignalingMessage;
use crate::transport::PeerSession;
use crate::turn::{IceServerConfig, build_ice_servers};

/// A live streaming session combining capture, transport, input, and recording.
pub struct LiveStream {
    pub session_id: Uuid,
    pub device_id: Uuid,
    pub capture: CaptureHandle,
    pub peer: PeerSession,
    pub recording: Option<RecordingHandle>,
    pub input_task: tokio::task::JoinHandle<()>,
}

/// Central manager for all active streaming sessions.
pub struct StreamManager {
    config: StreamingConfig,
    ice_servers: Vec<IceServerConfig>,
    streams: Arc<DashMap<Uuid, StreamMeta>>,
}

/// Lightweight metadata stored in the stream registry (the heavy resources
/// live inside the tasks; this tracks what's alive).
struct StreamMeta {
    device_id: Uuid,
    device_os: Os,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl StreamManager {
    pub fn new(config: StreamingConfig) -> Self {
        let ice_servers = build_ice_servers(&config);
        Self {
            config,
            ice_servers,
            streams: Arc::new(DashMap::new()),
        }
    }

    /// Start a new streaming session for a device.
    ///
    /// 1. Launches the platform-specific capture subprocess.
    /// 2. Creates a WebRTC peer connection with a video track.
    /// 3. Optionally starts recording.
    /// 4. Spawns the input relay task.
    ///
    /// Returns a signaling channel for the WebSocket handler to drive
    /// the SDP/ICE negotiation.
    pub async fn start_stream(
        &self,
        session_id: Uuid,
        device: Device,
    ) -> anyhow::Result<(mpsc::Sender<SignalingMessage>, mpsc::Receiver<SignalingMessage>)> {
        let device_id = device.id;
        let serial = device.serial.as_deref().unwrap_or("unknown");

        tracing::info!(%session_id, %device_id, serial, "Starting stream");

        // 1. Start capture
        let capture = match device.os {
            Os::Android => {
                capture::start_android_capture(
                    session_id,
                    device_id,
                    serial,
                    self.config.max_resolution_width,
                    self.config.max_fps,
                    self.config.target_bitrate_kbps,
                )
                .await?
            }
            Os::Ios => {
                capture::start_ios_capture(
                    session_id,
                    device_id,
                    serial,
                    self.config.max_resolution_width,
                    self.config.max_fps,
                    self.config.target_bitrate_kbps,
                )
                .await?
            }
            Os::Iot => {
                anyhow::bail!("Streaming is not supported for IoT devices");
            }
        };

        // 2. Create WebRTC peer
        let (signal_tx, signal_rx) = mpsc::channel::<SignalingMessage>(64);
        let (inbound_tx, _inbound_rx) = mpsc::channel::<SignalingMessage>(64);
        let peer = transport::create_peer_session(session_id, &self.ice_servers, signal_tx.clone()).await?;

        // 3. Optionally start recording
        let recording = if self.config.recording_enabled {
            let dir = PathBuf::from(&self.config.recording_dir);
            let rx = capture.frame_tx.subscribe();
            Some(recording::start_recording(session_id, &dir, rx)?)
        } else {
            None
        };

        // 4. Input relay
        let input_task = spawn_input_relay(session_id, device.clone(), peer.input_rx);

        // Track the stream
        self.streams.insert(session_id, StreamMeta {
            device_id,
            device_os: device.os.clone(),
            started_at: chrono::Utc::now(),
        });

        tracing::info!(
            %session_id,
            active_streams = self.streams.len(),
            "Stream started"
        );

        Ok((inbound_tx, signal_rx))
    }

    /// Stop a streaming session and release all resources.
    pub async fn stop_stream(&self, session_id: Uuid) -> anyhow::Result<()> {
        if self.streams.remove(&session_id).is_none() {
            anyhow::bail!("No active stream for session {session_id}");
        }

        tracing::info!(%session_id, "Stream stopped");
        Ok(())
    }

    /// Check if a session has an active stream.
    pub fn is_streaming(&self, session_id: &Uuid) -> bool {
        self.streams.contains_key(session_id)
    }

    /// Number of currently active streams.
    pub fn active_count(&self) -> usize {
        self.streams.len()
    }

    /// Get the ICE server configuration (for sending to browser clients).
    pub fn ice_servers_json(&self) -> serde_json::Value {
        turn::ice_servers_for_client(&self.ice_servers)
    }
}

/// Latency budget breakdown (P7.8).
///
/// Target: < 200 ms end-to-end. Budget allocation:
///
/// | Stage                | Budget  |
/// |----------------------|---------|
/// | Device capture       | ~30 ms  |
/// | H.264 encode         | ~15 ms  |
/// | Network (LAN)        | ~5 ms   |
/// | WebRTC jitter buffer | ~50 ms  |
/// | Decode + render      | ~20 ms  |
/// | **Headroom**         | ~80 ms  |
///
/// Optimisations applied:
/// - `ultrafast` / `zerolatency` preset for software encoding
/// - `ull` (ultra-low-latency) mode for NVENC
/// - Baseline H.264 profile (simpler decode)
/// - 2-second keyframe interval (fast recovery, low overhead)
/// - Direct scrcpy raw stream (no re-encode for Android)
/// - Broadcast channel with 256-frame buffer (drop-if-lagged)
pub const LATENCY_TARGET_MS: u32 = 200;

