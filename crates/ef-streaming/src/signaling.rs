//! WebSocket-based signaling protocol for SDP/ICE exchange.
//!
//! Messages are JSON-serialised and exchanged over a single WebSocket
//! connection per streaming session.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages exchanged between the browser and the signaling server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalingMessage {
    /// Browser → Server: request to start streaming a session.
    StartStream {
        session_id: Uuid,
    },

    /// Server → Browser: SDP offer (from the server-side WebRTC peer).
    Offer {
        session_id: Uuid,
        sdp: String,
    },

    /// Browser → Server: SDP answer.
    Answer {
        session_id: Uuid,
        sdp: String,
    },

    /// Bidirectional: ICE candidate exchange.
    IceCandidate {
        session_id: Uuid,
        candidate: String,
    },

    /// Server → Browser: stream is ready.
    StreamReady {
        session_id: Uuid,
    },

    /// Either side: request to stop streaming.
    StopStream {
        session_id: Uuid,
    },

    /// Server → Browser: error during signaling or streaming.
    Error {
        session_id: Uuid,
        message: String,
    },

    /// Browser → Server: input event to relay to the device.
    InputEvent {
        session_id: Uuid,
        event: InputEventPayload,
    },
}

/// Input event types relayed from the browser to the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum InputEventPayload {
    Tap { x: f64, y: f64 },
    Swipe { x1: f64, y1: f64, x2: f64, y2: f64, duration_ms: u32 },
    Key { keycode: String },
    Text { text: String },
    Scroll { x: f64, y: f64, dx: f64, dy: f64 },
    Home,
    Back,
}

/// Tracks the state of a signaling negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NegotiationState {
    /// Waiting for the browser to send an answer.
    WaitingForAnswer,
    /// SDP exchange complete, gathering/exchanging ICE candidates.
    IceExchange,
    /// Connection established, streaming active.
    Connected,
    /// Session ended.
    Closed,
}
