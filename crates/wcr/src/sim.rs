//! SPDX-License-Identifier: Apache-2.0
//! In-process shared-air simulator and mock modem73.

use crate::modem::control::encode_control_frame;
use crate::modem::kiss::{encode_frame, KissDecoder};
use crate::proto::Envelope;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

/// A lossy broadcast medium: every TX is delivered to all other listeners with `loss`.
/// `send_from` tags the transmitter so a linked modem can hide some neighbours.
#[derive(Clone)]
pub struct SharedAir {
    tx: broadcast::Sender<Vec<u8>>,
    linked: broadcast::Sender<(u64, Vec<u8>)>,
    pub loss: f32,
}

impl SharedAir {
    pub fn new(loss: f32) -> Self {
        let (tx, _) = broadcast::channel(256);
        let (linked, _) = broadcast::channel(256);
        Self { tx, linked, loss }
    }

    pub fn send(&self, frame: Vec<u8>) {
        if rand::random::<f32>() < self.loss {
            return;
        }
        let _ = self.tx.send(frame);
    }

    pub fn send_from(&self, station: u64, frame: Vec<u8>) {
        if rand::random::<f32>() < self.loss {
            return;
        }
        let _ = self.linked.send((station, frame));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }

    pub fn subscribe_linked(&self) -> broadcast::Receiver<(u64, Vec<u8>)> {
        self.linked.subscribe()
    }
}

async fn loopback_listener() -> crate::error::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

fn spawn_ctrl(
    listener: TcpListener,
    last_air: Arc<Mutex<Option<Instant>>>,
    tx_count: Arc<AtomicU64>,
) {
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.expect("ctrl accept");
            let last_air = last_air.clone();
            let tx_count = tx_count.clone();
            tokio::spawn(async move {
                let mut stash = Vec::new();
                let mut buf = [0u8; 2048];
                loop {
                    match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            stash.extend_from_slice(&buf[..n]);
                            if let Ok(frames) =
                                crate::modem::control::decode_control_frames(&mut stash)
                            {
                                for v in frames {
                                    let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
                                    let busy = last_air
                                        .lock()
                                        .ok()
                                        .and_then(|g| *g)
                                        .map(|t| t.elapsed() < Duration::from_millis(400))
                                        .unwrap_or(false);
                                    let sent = tx_count.load(Ordering::Relaxed);
                                    let reply = match cmd {
                                        "get_status" => serde_json::json!({
                                            "channel_state": if busy { "rx" } else { "idle" },
                                            "ptt_on": false,
                                            "last_snr": 12.0,
                                            "audio_connected": true,
                                            "occupancy_pct": if busy { 80 } else { 0 },
                                            "input_level_db": if busy { -12.0 } else { -48.0 },
                                            "output_level_db": -80.0,
                                            "tx_frame_count": sent
                                        }),
                                        "set_config" => serde_json::json!({"ok": true}),
                                        _ => serde_json::json!({"ok": true}),
                                    };
                                    if let Ok(f) = encode_control_frame(&reply) {
                                        if stream.write_all(&f).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }
    });
}

fn spawn_open_kiss(
    listener: TcpListener,
    air: SharedAir,
    last_air: Arc<Mutex<Option<Instant>>>,
    tx_count: Arc<AtomicU64>,
) {
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.expect("kiss accept");
            let air = air.clone();
            let last_air = last_air.clone();
            let tx_count = tx_count.clone();
            tokio::spawn(async move {
                let mut dec = KissDecoder::new();
                let mut rx = air.subscribe();
                let mut buf = [0u8; 2048];
                loop {
                    tokio::select! {
                        n = stream.read(&mut buf) => {
                            match n {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    for payload in dec.push(&buf[..n]) {
                                        *last_air.lock().expect("last_air") = Some(Instant::now());
                                        tx_count.fetch_add(1, Ordering::Relaxed);
                                        air.send(payload);
                                    }
                                }
                            }
                        }
                        frame = rx.recv() => {
                            if let Ok(payload) = frame {
                                *last_air.lock().expect("last_air") = Some(Instant::now());
                                let enc = encode_frame(&payload);
                                if stream.write_all(&enc).await.is_err() { break; }
                            }
                        }
                    }
                }
            });
        }
    });
}

fn start_open_modem(kiss: TcpListener, ctrl: TcpListener, air: SharedAir) {
    let last_air = Arc::new(Mutex::new(None::<Instant>));
    let tx_count = Arc::new(AtomicU64::new(0));
    spawn_open_kiss(kiss, air, last_air.clone(), tx_count.clone());
    spawn_ctrl(ctrl, last_air, tx_count);
}

/// Fake modem73: KISS TCP + length-prefixed JSON control, bridged to SharedAir.
/// Both listeners are bound before this returns, so a lost probe port cannot
/// be claimed by another test between the call and a later spawned bind.
pub async fn mock_modem73(
    kiss_bind: &str,
    ctrl_bind: &str,
    air: SharedAir,
) -> crate::error::Result<()> {
    let kiss = TcpListener::bind(kiss_bind).await?;
    let ctrl = TcpListener::bind(ctrl_bind).await?;
    start_open_modem(kiss, ctrl, air);
    Ok(())
}

/// Same as [`mock_modem73`], on two fresh loopback ports that stay bound.
pub async fn mock_modem73_free(air: SharedAir) -> crate::error::Result<(u16, u16)> {
    let (kiss, kiss_port) = loopback_listener().await?;
    let (ctrl, ctrl_port) = loopback_listener().await?;
    start_open_modem(kiss, ctrl, air);
    Ok((kiss_port, ctrl_port))
}

fn spawn_linked_kiss(
    listener: TcpListener,
    air: SharedAir,
    station: u64,
    hear: Arc<std::collections::HashSet<u64>>,
    last_air: Arc<Mutex<Option<Instant>>>,
    tx_count: Arc<AtomicU64>,
) {
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.expect("kiss accept");
            let air = air.clone();
            let last_air = last_air.clone();
            let hear = hear.clone();
            let tx_count = tx_count.clone();
            tokio::spawn(async move {
                let mut dec = KissDecoder::new();
                let mut rx = air.subscribe_linked();
                let mut buf = [0u8; 2048];
                loop {
                    tokio::select! {
                        n = stream.read(&mut buf) => {
                            match n {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    for payload in dec.push(&buf[..n]) {
                                        *last_air.lock().expect("last_air") = Some(Instant::now());
                                        tx_count.fetch_add(1, Ordering::Relaxed);
                                        air.send_from(station, payload);
                                    }
                                }
                            }
                        }
                        frame = rx.recv() => {
                            if let Ok((from, payload)) = frame {
                                if from == station || !hear.contains(&from) {
                                    continue;
                                }
                                *last_air.lock().expect("last_air") = Some(Instant::now());
                                let enc = encode_frame(&payload);
                                if stream.write_all(&enc).await.is_err() { break; }
                            }
                        }
                    }
                }
            });
        }
    });
}

fn start_linked_modem(
    kiss: TcpListener,
    ctrl: TcpListener,
    air: SharedAir,
    station: u64,
    hear: std::collections::HashSet<u64>,
) {
    let last_air = Arc::new(Mutex::new(None::<Instant>));
    let tx_count = Arc::new(AtomicU64::new(0));
    spawn_linked_kiss(
        kiss,
        air,
        station,
        Arc::new(hear),
        last_air.clone(),
        tx_count.clone(),
    );
    spawn_ctrl(ctrl, last_air, tx_count);
}

/// Like [`mock_modem73`], but this station only hears `hear` and tags its own TX.
/// Use this when A must not hear C directly and B is the relay.
/// Both listeners are bound before this returns.
pub async fn mock_modem73_linked(
    kiss_bind: &str,
    ctrl_bind: &str,
    air: SharedAir,
    station: u64,
    hear: std::collections::HashSet<u64>,
) -> crate::error::Result<()> {
    let kiss = TcpListener::bind(kiss_bind).await?;
    let ctrl = TcpListener::bind(ctrl_bind).await?;
    start_linked_modem(kiss, ctrl, air, station, hear);
    Ok(())
}

/// Same as [`mock_modem73_linked`], on two fresh loopback ports that stay bound.
pub async fn mock_modem73_linked_free(
    air: SharedAir,
    station: u64,
    hear: std::collections::HashSet<u64>,
) -> crate::error::Result<(u16, u16)> {
    let (kiss, kiss_port) = loopback_listener().await?;
    let (ctrl, ctrl_port) = loopback_listener().await?;
    start_linked_modem(kiss, ctrl, air, station, hear);
    Ok((kiss_port, ctrl_port))
}

pub fn encode_over_air(env: &Envelope) -> Vec<u8> {
    env.encode().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Callsign, Envelope, Flags};
    use crate::relay::{decide, Action};
    use crate::store::{Delivery, Store};

    #[test]
    fn multi_hop_ttl() {
        let store_a = Store::open_memory().unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("M0ZZZ").unwrap(),
            1,
            b"hello".to_vec(),
            3,
            Flags::new().with(crate::proto::FLAG_INET_OK),
        )
        .unwrap();
        assert_eq!(
            decide(&store_a, &env, false).unwrap().action,
            Action::Accept
        );
        store_a.insert(&env, Delivery::Queued).unwrap();
        // B hears it
        let store_b = Store::open_memory().unwrap();
        let mut hop = env.clone();
        hop.hops_left -= 1;
        assert_eq!(
            decide(&store_b, &hop, false).unwrap().action,
            Action::Accept
        );
        store_b.insert(&hop, Delivery::Queued).unwrap();
        hop.hops_left = 0;
        let store_c = Store::open_memory().unwrap();
        assert_eq!(
            decide(&store_c, &hop, false).unwrap().action,
            Action::DropRelay
        );
    }

    #[test]
    fn offline_catchup_have_want() {
        let store = Store::open_memory().unwrap();
        store
            .group_create("net", &["G4AAA".into(), "M0ZZZ".into()])
            .unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("NET"),
            1,
            b"missed".to_vec(),
            3,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        store.insert(&env, Delivery::Sent).unwrap();
        let have = store.have_digest("NET", 50, 24).unwrap();
        assert_eq!(have.len(), 1);
        assert!(!store.group_all_received("net", &env.msg_id).unwrap());
        store.receipt("net", &env.msg_id, "G4AAA").unwrap();
        store.receipt("net", &env.msg_id, "M0ZZZ").unwrap();
        assert!(store.group_all_received("net", &env.msg_id).unwrap());
    }

    #[test]
    fn gateway_bridge_two_frequencies() {
        use crate::relay::may_rf_egress;
        use crate::store::Store;
        // Node on freq A is a gateway, dest recently heard on A
        assert!(may_rf_egress(true, true, true, false, false, true, false));
        // Same frame must not go on air from a radio-only node
        assert!(!may_rf_egress(false, true, true, false, false, true, false));

        let store = Store::open_memory().unwrap();
        store
            .heard_touch(
                "G4AAA",
                None,
                Some("internet-radio"),
                true,
                "rf",
                Some(144950),
            )
            .unwrap();
        store
            .heard_touch("M0ZZZ", None, Some("radio-plus"), false, "inet", Some(7045))
            .unwrap();
        let list = store.heard_list().unwrap();
        let vhf = list.iter().find(|h| h.callsign == "G4AAA").unwrap();
        let hf = list.iter().find(|h| h.callsign == "M0ZZZ").unwrap();
        assert_eq!(vhf.band.as_deref(), Some("2m"));
        assert_eq!(hf.band.as_deref(), Some("40m"));
        assert!(store.recently_heard_rf("G4AAA", 600, 144950).unwrap());
        assert!(!store.recently_heard_rf("M0ZZZ", 600, 7045).unwrap());
        assert!(store.recently_heard_any_rf(600, 144950).unwrap());
        assert!(!store.recently_heard_any_rf(600, 7045).unwrap());
        assert_eq!(
            crate::band::beacon_khz(b"B|internet-radio|7045"),
            Some(7045)
        );
        assert_eq!(crate::band::to_bands([144950, 7045]), vec!["2m", "40m"]);
    }

    #[tokio::test]
    async fn mock_kiss_roundtrip() {
        let air = SharedAir::new(0.0);
        mock_modem73_free(air.clone()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let env = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("M0ZZZ").unwrap(),
            1,
            b"sim".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        air.send(encode_over_air(&env));
        let mut rx = air.subscribe();
        air.send(encode_over_air(&env));
        let got = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let back = Envelope::decode(&got).unwrap();
        assert_eq!(back.body, b"sim");
    }

    fn delivery_rate(loss: f32, retries: u32, trials: u32) -> f32 {
        let mut delivered = 0u32;
        for i in 0..trials {
            let air = SharedAir::new(loss);
            let mut rx = air.subscribe();
            let env = Envelope::new_msg(
                Callsign::parse("G4AAA").unwrap(),
                Callsign::parse("M0ZZZ").unwrap(),
                i,
                format!("m{i}").into_bytes(),
                3,
                Flags::new(),
            )
            .unwrap();
            let bytes = encode_over_air(&env);
            let mut got = false;
            for _ in 0..=retries {
                air.send(bytes.clone());
                if rx.try_recv().is_ok() {
                    got = true;
                    break;
                }
            }
            if got {
                delivered += 1;
            }
        }
        delivered as f32 / trials as f32
    }

    #[test]
    fn arq_retries_raise_delivery_under_loss() {
        let none = delivery_rate(0.5, 0, 80);
        let with = delivery_rate(0.5, 3, 80);
        assert!(
            with > none,
            "retries should improve delivery ({with} vs {none})"
        );
        assert!(
            with > 0.7,
            "3 retries at 50% loss should usually get through"
        );
    }
}
