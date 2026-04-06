//! # ef-streaming
//!
//! WebRTC streaming engine for real-time device screen capture and remote input.

use ef_core::error::Result;
use uuid::Uuid;

pub struct StreamingEngine;

impl StreamingEngine {
    /// Start capturing and streaming from a device.
    pub async fn start(device_id: Uuid, session_id: Uuid) -> Result<()> {
        tracing::info!(%device_id, %session_id, "Starting WebRTC stream");
        // TODO: init frame capture → H.264 encode → WebRTC transport
        Ok(())
    }

    /// Stop an active stream.
    pub async fn stop(session_id: Uuid) -> Result<()> {
        tracing::info!(%session_id, "Stopping stream");
        Ok(())
    }
}
