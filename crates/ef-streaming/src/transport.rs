//! WebRTC peer connection management using webrtc-rs.
//!
//! Each streaming session gets its own `PeerSession` that manages:
//! - A video track fed from the capture pipeline
//! - A data channel for input relay
//! - ICE negotiation with STUN/TURN

use std::sync::Arc;

use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocal;

use crate::signaling::SignalingMessage;
use crate::turn::IceServerConfig;

/// A live WebRTC peer session for one streaming viewer.
pub struct PeerSession {
    pub session_id: Uuid,
    pub peer_connection: Arc<RTCPeerConnection>,
    pub video_track: Arc<TrackLocalStaticRTP>,
    /// Receive input events from the browser data channel.
    pub input_rx: mpsc::Receiver<String>,
    /// Send signaling messages (ICE candidates, etc.) back to the browser.
    pub signal_tx: mpsc::Sender<SignalingMessage>,
}

/// Create a new WebRTC peer connection with H.264 video capability.
pub async fn create_peer_session(
    session_id: Uuid,
    ice_servers: &[IceServerConfig],
    signal_tx: mpsc::Sender<SignalingMessage>,
) -> anyhow::Result<PeerSession> {
    // Configure media engine for H.264
    let mut media_engine = MediaEngine::default();
    media_engine.register_default_codecs()?;

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)?;

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .build();

    let rtc_ice_servers: Vec<RTCIceServer> = ice_servers
        .iter()
        .map(|s| RTCIceServer {
            urls: s.urls.clone(),
            username: s.username.clone().unwrap_or_default(),
            credential: s.credential.clone().unwrap_or_default(),
            ..Default::default()
        })
        .collect();

    let config = RTCConfiguration {
        ice_servers: rtc_ice_servers,
        ..Default::default()
    };

    let peer_connection = Arc::new(api.new_peer_connection(config).await?);

    // Create video track (H.264)
    let video_track = Arc::new(TrackLocalStaticRTP::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            clock_rate: 90000,
            channels: 0,
            sdp_fmtp_line: "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f".into(),
            rtcp_feedback: vec![],
        },
        "video".into(),
        format!("ef-stream-{session_id}"),
    ));

    peer_connection
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    // Create data channel for input relay
    let (input_tx, input_rx) = mpsc::channel::<String>(128);
    let data_channel = peer_connection.create_data_channel("input", None).await?;

    let itx = input_tx.clone();
    data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
        let tx = itx.clone();
        Box::pin(async move {
            if let Ok(text) = String::from_utf8(msg.data.to_vec()) {
                let _ = tx.send(text).await;
            }
        })
    }));

    let dc = Arc::clone(&data_channel);
    data_channel.on_open(Box::new(move || {
        tracing::info!("Data channel '{}' opened", dc.label());
        Box::pin(async {})
    }));

    // Forward ICE candidates to the signaling channel
    let sig_tx = signal_tx.clone();
    let sid = session_id;
    peer_connection.on_ice_candidate(Box::new(move |candidate| {
        let tx = sig_tx.clone();
        Box::pin(async move {
            if let Some(c) = candidate {
                let json = c.to_json().expect("ICE candidate serialization");
                let _ = tx
                    .send(SignalingMessage::IceCandidate {
                        session_id: sid,
                        candidate: serde_json::to_string(&json).unwrap_or_default(),
                    })
                    .await;
            }
        })
    }));

    // Log connection state changes
    let sid = session_id;
    peer_connection.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
        tracing::info!(session_id = %sid, ?state, "Peer connection state changed");
        Box::pin(async {})
    }));

    Ok(PeerSession {
        session_id,
        peer_connection,
        video_track,
        input_rx,
        signal_tx,
    })
}

/// Handle an SDP offer from the remote peer and generate an answer.
pub async fn handle_offer(
    peer: &PeerSession,
    offer: RTCSessionDescription,
) -> anyhow::Result<RTCSessionDescription> {
    peer.peer_connection.set_remote_description(offer).await?;
    let answer = peer.peer_connection.create_answer(None).await?;
    let mut gather_complete = peer.peer_connection.gathering_complete_promise().await;
    peer.peer_connection
        .set_local_description(answer)
        .await?;
    // Wait for ICE gathering to complete
    let _ = gather_complete.recv().await;
    let local_desc = peer
        .peer_connection
        .local_description()
        .await
        .ok_or_else(|| anyhow::anyhow!("No local description after answering"))?;
    Ok(local_desc)
}

/// Add a remote ICE candidate to the peer connection.
pub async fn add_ice_candidate(
    peer: &PeerSession,
    candidate_json: &str,
) -> anyhow::Result<()> {
    let candidate: webrtc::ice_transport::ice_candidate::RTCIceCandidateInit =
        serde_json::from_str(candidate_json)?;
    peer.peer_connection.add_ice_candidate(candidate).await?;
    Ok(())
}

/// Close the peer connection and release all resources.
pub async fn close_peer(peer: &PeerSession) -> anyhow::Result<()> {
    peer.peer_connection.close().await?;
    tracing::info!(session_id = %peer.session_id, "Peer connection closed");
    Ok(())
}
