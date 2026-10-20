//! SPDX-License-Identifier: Apache-2.0
//! Node-side hub client with reconnect.

use crate::error::Result;
use crate::proto::{Envelope, IdentityKeys};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

enum HubOut {
    Bin(Vec<u8>),
    Text(String),
}

#[derive(Clone)]
pub struct HubClient {
    tx: mpsc::Sender<HubOut>,
    cancel: CancellationToken,
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
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        tokio::spawn(async move {
            let mut backoff = 1u64;
            // Only a hello the hub accepted. A refused stamp must not be
            // remembered: failed handshakes in one second used to walk it
            // past the hub's future window, and every later hello was then
            // refused as clock skew.
            let mut last_accepted = 0u64;
            let mut prompt_left = 2u8;
            loop {
                if stop.is_cancelled() {
                    connected.set(false);
                    return;
                }
                let now = unix_now();
                let Some(ts) = next_hello_ts(now, last_accepted) else {
                    if !pause(&stop, 1).await {
                        connected.set(false);
                        return;
                    }
                    continue;
                };
                let ws = tokio::select! {
                    _ = stop.cancelled() => {
                        connected.set(false);
                        return;
                    }
                    result = connect_async(&url) => match result {
                        Ok((ws, _)) => ws,
                        Err(e) => {
                            tracing::warn!("hub connect failed: {e}");
                            connected.fail(format!("hub connect failed: {e}"));
                            if !pause(&stop, backoff).await {
                                connected.set(false);
                                return;
                            }
                            backoff = (backoff * 2).min(30);
                            continue;
                        }
                    }
                };
                if stop.is_cancelled() {
                    connected.set(false);
                    return;
                }
                backoff = 1;
                let (mut sink, mut stream) = ws.split();
                let hello = build_hello(&callsign, &keys, &heard, freq_khz, ts);
                if sink.send(Message::Text(hello.into())).await.is_err() {
                    connected.fail("hub hello send failed");
                    if !pause(&stop, backoff).await {
                        connected.set(false);
                        return;
                    }
                    backoff = (backoff * 2).min(30);
                    continue;
                }
                let mut hub_ready = false;
                let mut prompt_retry = false;
                loop {
                    tokio::select! {
                        _ = stop.cancelled() => {
                            connected.set(false);
                            return;
                        }
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
                                None => {
                                    connected.set(false);
                                    return;
                                }
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
                                    if t.contains("\"ok\":true") {
                                        hub_ready = true;
                                        last_accepted = ts;
                                        connected.set(true);
                                    } else if t.contains("\"ok\":false") {
                                        let why = hub_refusal_message(&t, unix_now());
                                        prompt_retry = refusal_retries_promptly(&why) && prompt_left > 0;
                                        if prompt_retry {
                                            prompt_left = prompt_left.saturating_sub(1);
                                        }
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
                if stop.is_cancelled() {
                    connected.set(false);
                    return;
                }
                if hub_ready {
                    prompt_left = 2;
                    connected.fail("hub disconnected");
                } else if prompt_retry {
                    connected.set(false);
                    if !pause_ms(&stop, millis_until_next_second()).await {
                        connected.set(false);
                        return;
                    }
                    continue;
                } else {
                    connected.set(false);
                }
                if !pause(&stop, backoff).await {
                    connected.set(false);
                    return;
                }
                backoff = (backoff * 2).min(30);
            }
        });
        Ok(Self { tx: out_tx, cancel })
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
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

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `false` when `stop` is cancelled.
async fn pause(stop: &CancellationToken, secs: u64) -> bool {
    pause_ms(stop, secs.max(1).saturating_mul(1000)).await
}

async fn pause_ms(stop: &CancellationToken, ms: u64) -> bool {
    tokio::select! {
        _ = stop.cancelled() => false,
        _ = tokio::time::sleep(std::time::Duration::from_millis(ms.max(1))) => true,
    }
}

/// A same-second hello is refused as a replay. Retry on the next second.
/// A pure clock-skew refusal stays on the normal backoff.
pub(crate) fn refusal_retries_promptly(msg: &str) -> bool {
    msg.contains("replay")
}

fn millis_until_next_second() -> u64 {
    let sub = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_millis())
        .unwrap_or(0);
    (1000 - u64::from(sub)).clamp(50, 1000)
}

/// Next hello stamp. `None` means wait for the next second.
///
/// A stamp more than 60s ahead of `now` is ignored. Keeping it and adding
/// one each retry walks further ahead of the hub, which then refuses every
/// hello as clock skew.
pub(crate) fn next_hello_ts(now: u64, last_accepted: u64) -> Option<u64> {
    if last_accepted == 0 || now > last_accepted {
        return Some(now);
    }
    if last_accepted > now.saturating_add(60) {
        return Some(now);
    }
    None
}

pub(crate) fn hub_refusal_message(body: &str, now: u64) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return format!("hub refused: {body}");
    };
    let Some(err) = v.get("error").and_then(|e| e.as_str()) else {
        return format!("hub refused: {body}");
    };
    if err == "clock skew" {
        if let Some(server) = v.get("server_ts").and_then(|t| t.as_u64()) {
            let delta = now.abs_diff(server);
            if delta > 0 {
                let rel = if now > server { "ahead of" } else { "behind" };
                return format!(
                    "hub refused: clock skew — this computer's clock is {delta}s {rel} the hub"
                );
            }
        }
        return "hub refused: clock skew — set this computer's clock".into();
    }
    format!("hub refused: {err}")
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
    use super::{
        build_hello, hub_refusal_message, next_hello_ts, refusal_retries_promptly, websocket_url,
    };
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
        assert_eq!(websocket_url(""), "");
        assert_eq!(websocket_url("   "), "");
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

    #[test]
    fn hello_stamp_stays_on_the_wall_clock() {
        assert_eq!(next_hello_ts(1_000, 0), Some(1_000));
        assert_eq!(next_hello_ts(1_000, 999), Some(1_000));
        assert_eq!(next_hello_ts(1_000, 1_000), None);
        assert_eq!(next_hello_ts(1_000, 1_030), None);
        // A remembered stamp past the hub's future window is not walked forward.
        assert_eq!(next_hello_ts(1_000, 1_061), Some(1_000));
    }

    #[test]
    fn clock_skew_refusal_names_the_offset() {
        let body = r#"{"ok":false,"error":"clock skew","server_ts":1000}"#;
        assert_eq!(
            hub_refusal_message(body, 1_090),
            "hub refused: clock skew — this computer's clock is 90s ahead of the hub"
        );
        assert_eq!(
            hub_refusal_message(body, 900),
            "hub refused: clock skew — this computer's clock is 100s behind the hub"
        );
        assert_eq!(
            hub_refusal_message(r#"{"ok":false,"error":"replay"}"#, 1_000),
            "hub refused: replay"
        );
        assert_eq!(
            hub_refusal_message(r#"{"ok":false,"error":"replay or clock skew"}"#, 1_000),
            "hub refused: replay or clock skew"
        );
        assert!(refusal_retries_promptly("hub refused: replay"));
        assert!(refusal_retries_promptly(
            "hub refused: replay or clock skew"
        ));
        assert!(!refusal_retries_promptly(
            "hub refused: clock skew — this computer's clock is 90s ahead of the hub"
        ));
    }
}
