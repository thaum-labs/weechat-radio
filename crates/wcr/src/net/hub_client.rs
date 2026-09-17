//! SPDX-License-Identifier: Apache-2.0
//! Node-side hub client with reconnect.

use crate::error::Result;
use crate::proto::{Envelope, IdentityKeys};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

enum HubOut {
    Bin(Vec<u8>),
    Text(String),
}

#[derive(Clone)]
pub struct HubClient {
    tx: mpsc::Sender<HubOut>,
}

/// Nodes speak WebSocket on `/ws`. A host-only URL gets that path appended.
pub fn websocket_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.ends_with("/ws") || trimmed.contains("/ws/") {
        return trimmed.to_string();
    }
    if let Some((_, rest)) = trimmed.split_once("://") {
        if !rest.contains('/') {
            return format!("{trimmed}/ws");
        }
    }
    trimmed.to_string()
}

impl HubClient {
    pub async fn connect(
        url: &str,
        callsign: &str,
        keys: &IdentityKeys,
        heard: Vec<String>,
        freq_khz: u32,
        incoming: mpsc::Sender<Envelope>,
        connected: ArcFlag,
    ) -> Result<Self> {
        let (out_tx, mut out_rx) = mpsc::channel::<HubOut>(64);
        let url = websocket_url(url);
        let callsign = callsign.to_string();
        let keys = keys.clone();
        tokio::spawn(async move {
            let mut backoff = 1u64;
            loop {
                match connect_async(&url).await {
                    Ok((ws, _)) => {
                        connected.set(true);
                        backoff = 1;
                        let (mut sink, mut stream) = ws.split();
                        let ts = crate::proto::now_ts() as u64;
                        let hello = build_hello(&callsign, &keys, &heard, freq_khz, ts);
                        if sink.send(Message::Text(hello.into())).await.is_err() {
                            connected.fail("hub hello send failed");
                            continue;
                        }
                        loop {
                            tokio::select! {
                                outgoing = out_rx.recv() => {
                                    match outgoing {
                                        Some(HubOut::Bin(bin)) => {
                                            if sink.send(Message::Binary(bin.into())).await.is_err() {
                                                break;
                                            }
                                        }
                                        Some(HubOut::Text(t)) => {
                                            if sink.send(Message::Text(t.into())).await.is_err() {
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
                                            if t.contains("\"ok\":false") {
                                                let why = hub_error_text(&t);
                                                connected.fail(why);
                                                break;
                                            }
                                        }
                                        Some(Ok(Message::Close(_))) | None => break,
                                        Some(Err(_)) => break,
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if connected.get() {
                            connected.fail("hub disconnected");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("hub connect failed: {e}");
                        connected.fail(format!("hub connect failed: {e}"));
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
            .send(HubOut::Bin(bytes))
            .await
            .map_err(|_| crate::error::Error::Net("hub send buffer closed".into()))
    }

    pub async fn send_text(&self, text: &str) -> Result<()> {
        self.tx
            .send(HubOut::Text(text.to_string()))
            .await
            .map_err(|_| crate::error::Error::Net("hub send buffer closed".into()))
    }
}

#[derive(Clone, Default)]
pub struct ArcFlag {
    ok: std::sync::Arc<std::sync::atomic::AtomicBool>,
    err: std::sync::Arc<parking_lot::Mutex<String>>,
}

impl ArcFlag {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&self, v: bool) {
        self.ok.store(v, std::sync::atomic::Ordering::SeqCst);
        if v {
            self.err.lock().clear();
        }
    }
    pub fn fail(&self, msg: impl Into<String>) {
        self.ok.store(false, std::sync::atomic::Ordering::SeqCst);
        *self.err.lock() = msg.into();
    }
    pub fn get(&self) -> bool {
        self.ok.load(std::sync::atomic::Ordering::SeqCst)
    }
    pub fn error(&self) -> String {
        self.err.lock().clone()
    }
}

fn hub_error_text(t: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return format!("hub refused: {e}");
        }
    }
    format!("hub refused: {t}")
}

pub(crate) fn build_hello(
    callsign: &str,
    keys: &IdentityKeys,
    heard: &[String],
    freq_khz: u32,
    ts: u64,
) -> String {
    let payload = format!("{callsign}|{ts}");
    let sig = hex::encode(keys.sign_bytes(payload.as_bytes()));
    serde_json::json!({
        "v": 1,
        "callsign": callsign,
        "pubkey": keys.public_hex(),
        "sig": sig,
        "ts": ts,
        "heard": heard,
        "freq_khz": freq_khz,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::{build_hello, websocket_url};
    use crate::proto::IdentityKeys;

    #[test]
    fn adds_ws_path_when_missing() {
        assert_eq!(
            websocket_url("wss://hub.weechatradio.com"),
            "wss://hub.weechatradio.com/ws"
        );
        assert_eq!(
            websocket_url("wss://hub.weechatradio.com/"),
            "wss://hub.weechatradio.com/ws"
        );
        assert_eq!(
            websocket_url("wss://hub.weechatradio.com/ws"),
            "wss://hub.weechatradio.com/ws"
        );
    }

    #[test]
    fn hello_ts_is_monotonic_across_reconnects() {
        let keys = IdentityKeys::generate();
        let a = build_hello("G4ABC", &keys, &[], 0, 1_700_000_000);
        let b = build_hello("G4ABC", &keys, &[], 0, 1_700_000_001);
        let ta = serde_json::from_str::<serde_json::Value>(&a)
            .unwrap()
            .get("ts")
            .and_then(|v| v.as_u64())
            .unwrap();
        let tb = serde_json::from_str::<serde_json::Value>(&b)
            .unwrap()
            .get("ts")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert!(tb > ta);
        assert_ne!(
            serde_json::from_str::<serde_json::Value>(&a)
                .unwrap()
                .get("sig"),
            serde_json::from_str::<serde_json::Value>(&b)
                .unwrap()
                .get("sig")
        );
    }
}
