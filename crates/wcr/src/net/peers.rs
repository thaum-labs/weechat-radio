//! SPDX-License-Identifier: Apache-2.0
//! Configured direct peer links (`hub.peers`), same framing as LAN mesh.

use crate::error::Result;
use crate::proto::Envelope;
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

type WriterTx = mpsc::Sender<Vec<u8>>;

pub struct DirectPeers {
    writers: Arc<Mutex<Vec<WriterTx>>>,
}

impl DirectPeers {
    pub async fn start(
        peers: Vec<String>,
    ) -> Result<(Self, mpsc::Receiver<Envelope>, mpsc::Sender<Envelope>)> {
        let (tx_in, rx_in) = mpsc::channel(64);
        let (tx_out, mut rx_out) = mpsc::channel::<Envelope>(64);
        let writers: Arc<Mutex<Vec<WriterTx>>> = Arc::new(Mutex::new(Vec::new()));

        for raw in peers {
            let peer = raw.trim().to_string();
            if peer.is_empty() {
                continue;
            }
            let tx = tx_in.clone();
            let writers_c = writers.clone();
            tokio::spawn(async move {
                let mut backoff = 1u64;
                loop {
                    match TcpStream::connect(&peer).await {
                        Ok(stream) => {
                            backoff = 1;
                            let (mut read, mut write) = stream.into_split();
                            let (wtx, mut wrx) = mpsc::channel::<Vec<u8>>(32);
                            writers_c.lock().push(wtx);
                            let tx_r = tx.clone();
                            let mut read_done = tokio::spawn(async move {
                                loop {
                                    let mut lenb = [0u8; 4];
                                    if read.read_exact(&mut lenb).await.is_err() {
                                        break;
                                    }
                                    let len = u32::from_be_bytes(lenb) as usize;
                                    if len == 0 || len > 8192 {
                                        break;
                                    }
                                    let mut buf = vec![0u8; len];
                                    if read.read_exact(&mut buf).await.is_err() {
                                        break;
                                    }
                                    if let Ok(env) = Envelope::decode(&buf) {
                                        if tx_r.send(env).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            });
                            loop {
                                tokio::select! {
                                    bytes = wrx.recv() => {
                                        match bytes {
                                            Some(b) => {
                                                let len = (b.len() as u32).to_be_bytes();
                                                if write.write_all(&len).await.is_err()
                                                    || write.write_all(&b).await.is_err()
                                                {
                                                    break;
                                                }
                                            }
                                            None => break,
                                        }
                                    }
                                    _ = &mut read_done => break,
                                }
                            }
                        }
                        Err(e) => tracing::debug!("peer {peer}: {e}"),
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(30);
                }
            });
        }

        let writers_out = writers.clone();
        tokio::spawn(async move {
            while let Some(env) = rx_out.recv().await {
                if let Ok(bytes) = env.encode() {
                    let writers: Vec<WriterTx> = writers_out.lock().clone();
                    for w in writers {
                        let b = bytes.clone();
                        let _ = w.send(b).await;
                    }
                }
            }
        });

        Ok((Self { writers }, rx_in, tx_out))
    }

    pub fn connected(&self) -> usize {
        self.writers.lock().len()
    }
}
