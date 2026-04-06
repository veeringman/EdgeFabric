//! TURN / STUN server configuration for NAT traversal.

use serde::{Deserialize, Serialize};

/// Configuration for a single ICE server (STUN or TURN).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServerConfig {
    /// One or more URLs, e.g. `["stun:stun.l.google.com:19302"]` or
    /// `["turn:turn.example.com:3478"]`.
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

/// Build the ICE server list from the application config.
pub fn build_ice_servers(cfg: &ef_core::config::StreamingConfig) -> Vec<IceServerConfig> {
    let mut servers = Vec::new();

    // STUN servers (no auth)
    for url in &cfg.stun_servers {
        servers.push(IceServerConfig {
            urls: vec![url.clone()],
            username: None,
            credential: None,
        });
    }

    // TURN server (with auth)
    if !cfg.turn_server.is_empty() {
        servers.push(IceServerConfig {
            urls: vec![cfg.turn_server.clone()],
            username: Some(cfg.turn_username.clone()),
            credential: Some(cfg.turn_password.clone()),
        });
    }

    servers
}

/// Return the ICE servers in the format expected by the browser (RTCIceServer JSON).
/// This is sent to the client so it can configure its own RTCPeerConnection.
pub fn ice_servers_for_client(servers: &[IceServerConfig]) -> serde_json::Value {
    serde_json::json!(
        servers
            .iter()
            .map(|s| {
                let mut obj = serde_json::json!({ "urls": s.urls });
                if let Some(ref u) = s.username {
                    obj["username"] = serde_json::json!(u);
                }
                if let Some(ref c) = s.credential {
                    obj["credential"] = serde_json::json!(c);
                }
                obj
            })
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ice_servers_includes_stun_and_turn() {
        let cfg = ef_core::config::StreamingConfig {
            signaling_port: 8443,
            turn_server: "turn:localhost:3478".into(),
            turn_username: "user".into(),
            turn_password: "pass".into(),
            stun_servers: vec!["stun:stun.l.google.com:19302".into()],
            max_resolution_width: 1920,
            max_resolution_height: 1080,
            max_fps: 30,
            target_bitrate_kbps: 2000,
            hardware_accel: true,
            recording_enabled: false,
            recording_dir: "/tmp".into(),
            latency_target_ms: 200,
        };

        let servers = build_ice_servers(&cfg);
        assert_eq!(servers.len(), 2);
        assert!(servers[0].urls[0].starts_with("stun:"));
        assert!(servers[1].urls[0].starts_with("turn:"));
        assert_eq!(servers[1].username.as_deref(), Some("user"));
    }
}
