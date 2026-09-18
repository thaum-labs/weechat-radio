//! SPDX-License-Identifier: Apache-2.0
//! mDNS LAN discovery (`_wcr._tcp`) plus UDP hello fallback, and TCP mesh links.

use crate::config::DEFAULT_LAN_SERVICE;
use crate::error::Result;
use crate::net::frame;
use crate::proto::Envelope;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;

pub struct LanMesh {
    pub incoming: mpsc::Receiver<Envelope>,
    peers: Arc<Mutex<HashSet<String>>>,
    pub bind_port: u16,
}

impl LanMesh {
    pub async fn start(
        callsign: &str,
        port: u16,
        service: &str,
        hub_advertise: &str,
    ) -> Result<(Self, mpsc::Sender<Envelope>)> {
        let service = if service.trim().is_empty() {
            DEFAULT_LAN_SERVICE
        } else {
            service.trim()
        };
        let (tx_in, rx_in) = mpsc::channel(64);
        let (tx_out, mut rx_out) = mpsc::channel::<Envelope>(64);
        let peers = Arc::new(Mutex::new(HashSet::new()));
        let listener = TcpListener::bind(("0.0.0.0", port)).await?;
        let tx_accept = tx_in.clone();
        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    let tx = tx_accept.clone();
                    tokio::spawn(async move {
                        let _ = handle_peer(stream, tx).await;
                    });
                }
            }
        });

        start_mdns(callsign, port, service, peers.clone(), tx_in.clone());
        start_udp_hello(callsign, port, hub_advertise, peers.clone(), tx_in.clone()).await;

        let peers_out = peers.clone();
        tokio::spawn(async move {
            while let Some(env) = rx_out.recv().await {
                if let Ok(bytes) = env.encode() {
                    let addrs: Vec<String> = peers_out.lock().unwrap().iter().cloned().collect();
                    for a in addrs {
                        let b = bytes.clone();
                        tokio::spawn(async move {
                            if let Ok(mut s) = TcpStream::connect(&a).await {
                                let _ = frame::write_frame(&mut s, &b).await;
                            }
                        });
                    }
                }
            }
        });

        Ok((
            Self {
                incoming: rx_in,
                peers,
                bind_port: port,
            },
            tx_out,
        ))
    }

    pub fn peer_count(&self) -> usize {
        self.peers.lock().unwrap().len()
    }

    pub fn peer_set(&self) -> Arc<Mutex<HashSet<String>>> {
        self.peers.clone()
    }
}

fn start_mdns(
    callsign: &str,
    port: u16,
    service: &str,
    peers: Arc<Mutex<HashSet<String>>>,
    tx_in: mpsc::Sender<Envelope>,
) {
    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("lan mdns: {e}");
            return;
        }
    };
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "wcr".into());
    match ServiceInfo::new(
        service,
        callsign,
        &format!("{hostname}.local."),
        "127.0.0.1",
        port,
        &[("call", callsign)][..],
    ) {
        Ok(info) => {
            let _ = mdns.register(info);
        }
        Err(e) => {
            tracing::warn!("lan mdns register: {e}");
            return;
        }
    }
    let browser = match mdns.browse(service) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("lan mdns browse: {e}");
            return;
        }
    };
    let ours = callsign.to_ascii_uppercase();
    tokio::spawn(async move {
        while let Ok(ev) = browser.recv_async().await {
            if let ServiceEvent::ServiceResolved(info) = ev {
                if let Some(call) = info.get_property_val_str("call") {
                    if call.eq_ignore_ascii_case(&ours) {
                        continue;
                    }
                }
                for addr in info.get_addresses() {
                    let sock = format!("{}:{}", addr, info.get_port());
                    note_peer(&peers, sock, tx_in.clone());
                }
            }
        }
    });
}

async fn start_udp_hello(
    callsign: &str,
    port: u16,
    hub_advertise: &str,
    peers: Arc<Mutex<HashSet<String>>>,
    tx_in: mpsc::Sender<Envelope>,
) {
    let sock = match UdpSocket::bind(("0.0.0.0", port)).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("lan udp hello: {e}");
            return;
        }
    };
    if let Err(e) = sock.set_broadcast(true) {
        tracing::warn!("lan udp broadcast: {e}");
    }
    let sock = Arc::new(sock);
    let hub = if hub_advertise.trim().is_empty() {
        None
    } else {
        Some(hub_advertise.trim())
    };
    let hello = encode_hello(callsign, port, hub);
    let ours = callsign.to_ascii_uppercase();
    let send = sock.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        loop {
            tick.tick().await;
            let _ = send.send_to(&hello, ("255.255.255.255", port)).await;
        }
    });
    let recv = sock;
    tokio::spawn(async move {
        let mut buf = [0u8; 256];
        while let Ok((n, src)) = recv.recv_from(&mut buf).await {
            if let Some(hello) = decode_hello(&buf[..n]) {
                if hello.callsign.eq_ignore_ascii_case(&ours) {
                    continue;
                }
                let sock = format!("{}:{}", src.ip(), hello.port);
                note_peer(&peers, sock, tx_in.clone());
            }
        }
    });
}

fn note_peer(peers: &Arc<Mutex<HashSet<String>>>, sock: String, tx_in: mpsc::Sender<Envelope>) {
    let mut set = peers.lock().unwrap();
    if set.insert(sock.clone()) {
        drop(set);
        tracing::info!("lan peer {sock}");
        tokio::spawn(async move {
            if let Ok(stream) = TcpStream::connect(&sock).await {
                let _ = handle_peer(stream, tx_in).await;
            }
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanHello {
    pub callsign: String,
    pub port: u16,
    pub hub: Option<String>,
}

pub fn encode_hello(callsign: &str, port: u16, hub: Option<&str>) -> Vec<u8> {
    match hub {
        Some(h) if !h.trim().is_empty() => {
            format!("WCR1 {callsign} {port} {}", h.trim()).into_bytes()
        }
        _ => format!("WCR1 {callsign} {port}").into_bytes(),
    }
}

pub fn decode_hello(buf: &[u8]) -> Option<LanHello> {
    let s = std::str::from_utf8(buf).ok()?.trim();
    let mut parts = s.split_whitespace();
    if parts.next()? != "WCR1" {
        return None;
    }
    let callsign = parts.next()?.to_string();
    let port = parts.next()?.parse().ok()?;
    let hub = parts.next().and_then(|h| {
        if h.contains(':') {
            Some(h.to_string())
        } else {
            None
        }
    });
    Some(LanHello {
        callsign,
        port,
        hub,
    })
}

async fn handle_peer(stream: TcpStream, tx: mpsc::Sender<Envelope>) -> Result<()> {
    frame::read_loop(stream, tx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_roundtrip() {
        let raw = encode_hello("~ABC12XY", 7375, None);
        let h = decode_hello(&raw).unwrap();
        assert_eq!(h.callsign, "~ABC12XY");
        assert_eq!(h.port, 7375);
        assert!(h.hub.is_none());
        assert!(decode_hello(b"nope").is_none());
        assert!(decode_hello(b"WCR1 onlyone").is_none());
        let with_hub = encode_hello("~ABC12XY", 7375, Some("192.168.1.5:7376"));
        let h = decode_hello(&with_hub).unwrap();
        assert_eq!(h.hub.as_deref(), Some("192.168.1.5:7376"));
    }
}
