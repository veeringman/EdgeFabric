//! Streaming forwarder — initialises local screen capture and
//! coordinates with the control plane to set up WebRTC streams.

use std::sync::Arc;

use ef_core::config::StreamingConfig;
use ef_core::device::Device;
use ef_streaming::StreamManager;
use uuid::Uuid;

use crate::discovery::DeviceRegistry;

/// Wrapper around `ef_streaming::StreamManager` that the agent uses
/// to manage streams for devices on this edge node.
pub struct StreamingForwarder {
    manager: StreamManager,
    registry: Arc<DeviceRegistry>,
}

impl StreamingForwarder {
    pub fn new(config: StreamingConfig, registry: Arc<DeviceRegistry>) -> Self {
        let manager = StreamManager::new(config);
        Self { manager, registry }
    }

    /// Start a stream for a device belonging to the given session.
    pub async fn start(
        &self,
        session_id: Uuid,
        device_id: Uuid,
    ) -> anyhow::Result<()> {
        let device = self
            .registry
            .device_by_id(device_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("Device {device_id} not found in local registry"))?;

        tracing::info!(%session_id, %device_id, "Starting stream via forwarder");

        let (_signal_tx, _signal_rx) = self.manager.start_stream(session_id, device).await?;

        // In full implementation, the signal channels would be connected
        // to the control plane's signaling relay so that the browser's
        // SDP/ICE messages reach this agent's WebRTC peer.

        Ok(())
    }

    /// Stop a stream.
    pub async fn stop(&self, session_id: Uuid) -> anyhow::Result<()> {
        self.manager.stop_stream(session_id).await
    }

    /// Number of active streams on this agent.
    pub fn active_count(&self) -> usize {
        self.manager.active_count()
    }
}
