//! SPDX-License-Identifier: Apache-2.0
//! Node-side hub client with reconnect.

use crate::error::Result;
use crate::proto::{Envelope, IdentityKeys};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub struct HubClient {
    pub tx: mpsc::Sender<Vec<u8>>,
}

impl HubClient {
    pub async fn connect(
        url: &str,
        callsign: &str,
        keys: &IdentityKeys,
        heard: Vec<String>,
        incoming: mpsc::Sender<Envelope>,
        connected: ArcFlag,
    ) -> Result<Self> {
        let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
        let url = url.to_string();
        let callsign = callsign.to_string();
        let pubhex = keys.public_hex();
        let ts = crate::proto::now_ts() as u64;
        let payload = format!("{callsign}|{ts}");
        let sig = hex::encode(keys.sign_bytes(payload.as_bytes()));
        let hello = serde_json::json!({
            "v": 1,
            "callsign": callsign,
            "pubkey": pubhex,
            "sig": sig,
            "ts": ts,
            "heard": heard,
        })
        .to_string();
        tokio::spawn(async move {
            let mut backoff = 1u64;
            loop {
                match connect_async(&url).await {
                    Ok((ws, _)) => {
                        connected.set(true);
                        backoff = 1;
                        let (mut sink, mut stream) = ws.split();
                        if sink
                            .send(Message::Text(hello.clone().into()))
                            .await
                            .is_err()
                        {
                            connected.set(false);
                            continue;
                        }
                        loop {
                            tokio::select! {
                                outgoing = out_rx.recv() => {
                                    match outgoing {
                                        Some(bin) => {
                                            if sink.send(Message::Binary(bin.into())).await.is_err() {
                                                break;
                                            }
                                        }
                                        None => return,
                                    }
                                }
                                incoming_msg = stream.next() => {
                                    match incoming_msg {
                                        Some(Ok(Message::Binary(b))) => {
                                            if let Ok(env) = Envelope::decode(&b) {
                                                let _ = incoming.send(env).await;
                                            }
                                        }
                                        Some(Ok(Message::Text(t))) => {
                                            tracing::debug!("hub text {t}");
                                        }
                                        Some(Ok(Message::Close(_))) | None => break,
                                        Some(Err(_)) => break,
                                        _ => {}
                                    }
                                }
                            }
                        }
                        connected.set(false);
                    }
                    Err(e) => {
                        tracing::warn!("hub connect failed: {e}");
                        connected.set(false);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(30);
            }
        });
        Ok(Self { tx: out_tx })
    }

    pub async fn send(&self, env: &Envelope) -> Result<()> {
        let bytes = env.encode()?;
        self.tx
            .send(bytes)
            .await
            .map_err(|_| crate::error::Error::Net("hub send buffer closed".into()))
    }
}

#[derive(Clone, Default)]
pub struct ArcFlag(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl ArcFlag {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&self, v: bool) {
        self.0.store(v, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn get(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
