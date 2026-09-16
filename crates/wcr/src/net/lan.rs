//! SPDX-License-Identifier: Apache-2.0
//! mDNS LAN discovery (`_wcr._tcp`) and direct node-to-node links.

use crate::error::Result;
use crate::proto::Envelope;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

const SERVICE: &str = "_wcr._tcp.local.";

pub struct LanMesh {
    pub incoming: mpsc::Receiver<Envelope>,
    peers: Arc<Mutex<HashSet<String>>>,
    pub bind_port: u16,
}

impl LanMesh {
    pub async fn start(callsign: &str, port: u16) -> Result<(Self, mpsc::Sender<Envelope>)> {
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

        let mdns = ServiceDaemon::new().map_err(|e| crate::error::Error::Net(e.to_string()))?;
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "wcr".into());
        let info = ServiceInfo::new(
            SERVICE,
            callsign,
            &format!("{hostname}.local."),
            "127.0.0.1",
            port,
            &[("call", callsign)][..],
        )
        .map_err(|e| crate::error::Error::Net(e.to_string()))?;
        // Prefer unspecified address; mdns-sd fills local IPs.
        let _ = mdns.register(info);

        let browser = mdns
            .browse(SERVICE)
            .map_err(|e| crate::error::Error::Net(e.to_string()))?;
        let peers_b = peers.clone();
        let tx_disc = tx_in.clone();
        tokio::spawn(async move {
            while let Ok(ev) = browser.recv_async().await {
                if let ServiceEvent::ServiceResolved(info) = ev {
                    for addr in info.get_addresses() {
                        let sock = format!("{}:{}", addr, info.get_port());
                        let mut set = peers_b.lock().unwrap();
                        if set.insert(sock.clone()) {
                            drop(set);
                            let tx = tx_disc.clone();
                            tokio::spawn(async move {
                                if let Ok(stream) = TcpStream::connect(&sock).await {
                                    let _ = handle_peer(stream, tx).await;
                                }
                            });
                        }
                    }
                }
            }
        });

        let peers_out = peers.clone();
        tokio::spawn(async move {
            while let Some(env) = rx_out.recv().await {
                if let Ok(bytes) = env.encode() {
                    let addrs: Vec<String> = peers_out.lock().unwrap().iter().cloned().collect();
                    for a in addrs {
                        let b = bytes.clone();
                        tokio::spawn(async move {
                            if let Ok(mut s) = TcpStream::connect(&a).await {
                                let len = (b.len() as u32).to_be_bytes();
                                let _ = s.write_all(&len).await;
                                let _ = s.write_all(&b).await;
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
}

async fn handle_peer(mut stream: TcpStream, tx: mpsc::Sender<Envelope>) -> Result<()> {
    loop {
        let mut lenb = [0u8; 4];
        if stream.read_exact(&mut lenb).await.is_err() {
            break;
        }
        let len = u32::from_be_bytes(lenb) as usize;
        if len == 0 || len > 8192 {
            break;
        }
        let mut buf = vec![0u8; len];
        if stream.read_exact(&mut buf).await.is_err() {
            break;
        }
        if let Ok(env) = Envelope::decode(&buf) {
            if tx.send(env).await.is_err() {
                break;
            }
        }
    }
    Ok(())
}
