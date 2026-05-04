//! VeerOS Host Advertisement Discovery listener.
//!
//! Listens on UDP multicast 224.0.0.251:5354 for `VeerOsAdvertisement` JSON
//! packets broadcast by VeerOS instances.  Maintains an in-memory table of
//! recently seen hosts and exposes them via GET /veeros/discovery/hosts.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ─── Wire format ─────────────────────────────────────────────────────────────

pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
pub const MULTICAST_PORT: u16 = 5354;
/// Hosts not seen within this window are pruned from the in-memory table.
pub const STALE_SECS: u64 = 180;

/// Advertisement packet sent by VeerOS hosts over UDP multicast.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VeerOsAdvertisement {
    /// Stable host identity — ed25519 pubkey fingerprint or a hostname-based ID.
    pub host_id: String,
    pub hostname: String,
    pub ip: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub os_version: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub quantum: Option<QuantumInfo>,
    /// Unix timestamp of packet generation (used for replay protection).
    pub timestamp: u64,
    /// Optional ed25519 signature (hex) over the canonical JSON without this field.
    pub signature: Option<String>,
}

fn default_port() -> u16 { 2323 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuantumInfo {
    pub qubits: Option<u32>,
    #[serde(default)]
    pub backends: Vec<String>,
}

// ─── In-memory host table ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredHost {
    pub host_id: String,
    pub hostname: String,
    pub ip_address: String,
    pub port: u16,
    pub os_version: Option<String>,
    pub capabilities: Vec<String>,
    pub quantum_capable: bool,
    pub quantum_qubits: Option<u32>,
    pub quantum_backends: Vec<String>,
    pub verified: bool,
    pub last_seen_secs_ago: u64,
    #[serde(skip)]
    pub(crate) last_seen: Instant,
}

impl DiscoveredHost {
    fn from_advert(advert: &VeerOsAdvertisement, verified: bool) -> Self {
        let quantum_capable = advert.capabilities.iter().any(|c| c.starts_with("quantum"));
        let (qubits, backends) = advert.quantum.as_ref()
            .map(|q| (q.qubits, q.backends.clone()))
            .unwrap_or((None, vec![]));

        Self {
            host_id: advert.host_id.clone(),
            hostname: advert.hostname.clone(),
            ip_address: advert.ip.clone(),
            port: advert.port,
            os_version: advert.os_version.clone(),
            capabilities: advert.capabilities.clone(),
            quantum_capable,
            quantum_qubits: qubits,
            quantum_backends: backends,
            verified,
            last_seen_secs_ago: 0,
            last_seen: Instant::now(),
        }
    }
}

pub type HostTable = Arc<RwLock<HashMap<String, DiscoveredHost>>>;

pub fn new_host_table() -> HostTable {
    Arc::new(RwLock::new(HashMap::new()))
}

// ─── UDP listener task ───────────────────────────────────────────────────────

/// Spawns a background task that listens for VeerOS advertisement packets.
pub async fn start_listener(hosts: HostTable) -> anyhow::Result<()> {
    let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MULTICAST_PORT);
    let socket = UdpSocket::bind(SocketAddr::V4(bind_addr)).await
        .map_err(|e| anyhow::anyhow!("Failed to bind UDP discovery socket {bind_addr}: {e}"))?;

    // Join multicast group on all interfaces
    socket.join_multicast_v4(MULTICAST_ADDR, Ipv4Addr::UNSPECIFIED)
        .map_err(|e| anyhow::anyhow!("Failed to join multicast {MULTICAST_ADDR}: {e}"))?;

    info!(
        addr = %MULTICAST_ADDR,
        port = MULTICAST_PORT,
        "VeerOS discovery listener started"
    );

    // Stale prune task
    let prune_hosts = hosts.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut table = prune_hosts.write().await;
            let cutoff = Duration::from_secs(STALE_SECS);
            let before = table.len();
            table.retain(|_, h| h.last_seen.elapsed() < cutoff);
            let pruned = before - table.len();
            if pruned > 0 {
                debug!("Pruned {pruned} stale VeerOS discovery entries");
            }
        }
    });

    // Receive loop
    let mut buf = vec![0u8; 4096];
    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(r) => r,
            Err(e) => {
                warn!("UDP recv error: {e}");
                continue;
            }
        };

        let data = &buf[..len];
        match serde_json::from_slice::<VeerOsAdvertisement>(data) {
            Ok(advert) => {
                debug!(
                    host_id = %advert.host_id,
                    src = %src,
                    "Received VeerOS advertisement"
                );

                // Basic replay protection: reject packets older than 5 minutes
                let now_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if advert.timestamp + 300 < now_unix {
                    warn!(host_id = %advert.host_id, "Dropping stale advertisement (timestamp too old)");
                    continue;
                }

                // Rate limit: max 100 unique hosts per agent
                {
                    let table = hosts.read().await;
                    if table.len() >= 100 && !table.contains_key(&advert.host_id) {
                        warn!("Discovery table full (100 hosts); dropping advertisement from {src}");
                        continue;
                    }
                }

                // Signature verification placeholder: mark verified=false if no signature
                let verified = advert.signature.is_some();

                let host = DiscoveredHost::from_advert(&advert, verified);
                let mut table = hosts.write().await;
                table.insert(advert.host_id.clone(), host);
            }
            Err(e) => {
                debug!(src = %src, err = %e, "Ignoring non-VeerOS UDP packet");
            }
        }
    }
}

// ─── HTTP handler ─────────────────────────────────────────────────────────────

/// Returns the current discovered host table as a JSON list.
/// Exposed at GET /veeros/discovery/hosts
pub async fn list_discovered_hosts(
    hosts: &HostTable,
    quantum_only: bool,
) -> Vec<serde_json::Value> {
    let table = hosts.read().await;
    table.values()
        .filter(|h| !quantum_only || h.quantum_capable)
        .map(|h| {
            let secs_ago = h.last_seen.elapsed().as_secs();
            serde_json::json!({
                "host_id":         h.host_id,
                "hostname":        h.hostname,
                "ip_address":      h.ip_address,
                "port":            h.port,
                "os_version":      h.os_version,
                "capabilities":    h.capabilities,
                "quantum_capable": h.quantum_capable,
                "quantum_qubits":  h.quantum_qubits,
                "quantum_backends":h.quantum_backends,
                "verified":        h.verified,
                "last_seen_secs_ago": secs_ago,
            })
        })
        .collect()
}
