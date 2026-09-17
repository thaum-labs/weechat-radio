//! SPDX-License-Identifier: Apache-2.0
//! In-process shared-air simulator and mock modem73.

use crate::modem::control::encode_control_frame;
use crate::modem::kiss::{encode_frame, KissDecoder};
use crate::proto::Envelope;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

/// A lossy broadcast medium: every TX is delivered to all other listeners with `loss`.
#[derive(Clone)]
pub struct SharedAir {
    tx: broadcast::Sender<Vec<u8>>,
    pub loss: f32,
}

impl SharedAir {
    pub fn new(loss: f32) -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx, loss }
    }

    pub fn send(&self, frame: Vec<u8>) {
        if rand::random::<f32>() < self.loss {
            return;
        }
        let _ = self.tx.send(frame);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.tx.subscribe()
    }
}

/// Fake modem73: KISS TCP + length-prefixed JSON control, bridged to SharedAir.
pub async fn mock_modem73(
    kiss_bind: &str,
    ctrl_bind: &str,
    air: SharedAir,
) -> crate::error::Result<()> {
    let last_air = Arc::new(Mutex::new(None::<Instant>));
    let air_k = air.clone();
    let last_k = last_air.clone();
    let kiss_bind = kiss_bind.to_string();
    tokio::spawn(async move {
        let listener = TcpListener::bind(&kiss_bind).await.expect("kiss bind");
        loop {
            let (mut stream, _) = listener.accept().await.expect("kiss accept");
            let air = air_k.clone();
            let last_air = last_k.clone();
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

    let ctrl_bind = ctrl_bind.to_string();
    tokio::spawn(async move {
        let listener = TcpListener::bind(&ctrl_bind).await.expect("ctrl bind");
        loop {
            let (mut stream, _) = listener.accept().await.expect("ctrl accept");
            let last_air = last_air.clone();
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
                                    let reply = match cmd {
                                        "get_status" => serde_json::json!({
                                            "channel_state": if busy { "rx" } else { "idle" },
                                            "ptt_on": false,
                                            "last_snr": 12.0,
                                            "audio_connected": true,
                                            "occupancy_pct": if busy { 80 } else { 0 }
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
    Ok(())
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
        assert_eq!(
            crate::band::beacon_khz(b"B|internet-radio|7045"),
            Some(7045)
        );
        assert_eq!(crate::band::to_bands([144950, 7045]), vec!["2m", "40m"]);
    }

    #[tokio::test]
    async fn mock_kiss_roundtrip() {
        let air = SharedAir::new(0.0);
        mock_modem73("127.0.0.1:18001", "127.0.0.1:18073", air.clone())
            .await
            .unwrap();
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
