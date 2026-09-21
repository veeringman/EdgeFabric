//! Length-prefixed LAN collective frames used by the M6 Mini dataplane.
//! Payload is JSON; optional raw float32 follows for activations.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub const OPS_FORWARD: &str = "forward";
pub const OPS_BACKWARD: &str = "backward";
pub const OPS_DILOCO: &str = "diloco_sync";
pub const OPS_METRICS: &str = "metrics";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FrameHeader {
    pub op: String,
    pub seq: u64,
    pub rank: i32,
    pub hidden_elems: usize,
    pub hop_ms: f64,
    pub bytes: usize,
}

pub fn encode_frame(header: &FrameHeader, payload: &[u8]) -> Vec<u8> {
    let hdr = serde_json::to_vec(header).expect("header json");
    let mut out = Vec::with_capacity(8 + hdr.len() + payload.len());
    out.extend_from_slice(&(hdr.len() as u32).to_be_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&hdr);
    out.extend_from_slice(payload);
    out
}

pub fn decode_frame(buf: &[u8]) -> Option<(FrameHeader, Vec<u8>)> {
    if buf.len() < 8 {
        return None;
    }
    let hlen = u32::from_be_bytes(buf[0..4].try_into().ok()?) as usize;
    let plen = u32::from_be_bytes(buf[4..8].try_into().ok()?) as usize;
    if buf.len() < 8 + hlen + plen {
        return None;
    }
    let header: FrameHeader = serde_json::from_slice(&buf[8..8 + hlen]).ok()?;
    let payload = buf[8 + hlen..8 + hlen + plen].to_vec();
    Some((header, payload))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_forward_frame() {
        let header = FrameHeader {
            op: OPS_FORWARD.into(),
            seq: 7,
            rank: 0,
            hidden_elems: 4,
            hop_ms: 1.5,
            bytes: 16,
        };
        let payload = 1.0f32.to_le_bytes().repeat(4);
        let encoded = encode_frame(&header, &payload);
        let (h2, p2) = decode_frame(&encoded).expect("decode");
        assert_eq!(h2, header);
        assert_eq!(p2, payload);
    }
}
