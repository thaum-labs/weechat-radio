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

/// Per-link loss for a linked modem: `(from, to, frame)` returns true to drop
/// that copy for that receiver only. Lets a test lose one hop of a chain.
pub type LinkDrop = Arc<dyn Fn(u64, u64, &[u8]) -> bool + Send + Sync>;

fn spawn_linked_kiss(
    listener: TcpListener,
    air: SharedAir,
    station: u64,
    hear: Arc<std::collections::HashSet<u64>>,
    drop_rx: Option<LinkDrop>,
    last_air: Arc<Mutex<Option<Instant>>>,
    tx_count: Arc<AtomicU64>,
) {
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.expect("kiss accept");
            let air = air.clone();
            let last_air = last_air.clone();
            let hear = hear.clone();
            let drop_rx = drop_rx.clone();
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
                                if drop_rx.as_ref().is_some_and(|d| d(from, station, &payload)) {
                                    continue;
                                }
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
    drop_rx: Option<LinkDrop>,
) {
    let last_air = Arc::new(Mutex::new(None::<Instant>));
    let tx_count = Arc::new(AtomicU64::new(0));
    spawn_linked_kiss(
        kiss,
        air,
        station,
        Arc::new(hear),
        drop_rx,
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
    start_linked_modem(kiss, ctrl, air, station, hear, None);
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
    start_linked_modem(kiss, ctrl, air, station, hear, None);
    Ok((kiss_port, ctrl_port))
}

/// [`mock_modem73_linked_free`] with a per-link drop rule on what this station hears.
pub async fn mock_modem73_chain_free(
    air: SharedAir,
    station: u64,
    hear: std::collections::HashSet<u64>,
    drop_rx: LinkDrop,
) -> crate::error::Result<(u16, u16)> {
    let (kiss, kiss_port) = loopback_listener().await?;
    let (ctrl, ctrl_port) = loopback_listener().await?;
    start_linked_modem(kiss, ctrl, air, station, hear, Some(drop_rx));
    Ok((kiss_port, ctrl_port))
}

pub fn encode_over_air(env: &Envelope) -> Vec<u8> {
    env.encode().unwrap_or_default()
}

/// One frame a [`mock_modem73_timed`] station took off its queue.
#[derive(Debug, Clone)]
pub struct TimedKey {
    pub station: u64,
    pub at: Instant,
    pub payload: Vec<u8>,
    /// Carrier sense was on when the frame keyed.
    pub csma: bool,
    /// Frame capacity of the mode current when the frame keyed.
    pub cap: usize,
    /// Wider than `cap`. modem73 drops it instead of keying.
    pub dropped: bool,
}

pub type TimedLog = Arc<Mutex<Vec<TimedKey>>>;

/// How a [`mock_modem73_timed`] station reports its transmitter.
#[derive(Debug, Clone, Copy)]
pub struct TimedOpts {
    /// Airtime of every keyed frame.
    pub frame_ms: u64,
    /// Upstream modem73 reports `tx_frame_count` 0 and only `tx_queue` /
    /// `ptt_on` move. True counts each frame as it starts.
    pub count_tx: bool,
    /// Two stations this receiver hears keyed over each other: neither frame
    /// decodes here. The keyers may not hear each other (hidden terminal).
    pub collide: bool,
    /// Non-zero: the noise floor sits above the carrier threshold, as on the
    /// PulseAudio RF sim. Carrier sense is always busy, so each queued frame
    /// waits this long, then keys anyway with one burst continuation.
    pub csma_stuck_ms: u64,
}

struct TimedState {
    queue: std::collections::VecDeque<Vec<u8>>,
    keying: bool,
    keyed_until: Option<Instant>,
    heard_at: Option<Instant>,
    cap: usize,
    csma: bool,
    tx_count: u64,
}

/// A neighbour's frame is on the air right now. Carrier sense hears it
/// before the frame has finished, which is when a receiver decodes it.
fn timed_hearing(log: &TimedLog, hear: &std::collections::HashSet<u64>, frame: Duration) -> bool {
    let log = log.lock().expect("timed log");
    log.iter()
        .rev()
        .take_while(|k| k.at.elapsed() < frame * 2)
        .any(|k| !k.dropped && hear.contains(&k.station) && k.at.elapsed() < frame)
}

/// Another station this receiver hears was keying while `from` sent `payload`.
fn timed_collided(
    log: &TimedLog,
    hear: &std::collections::HashSet<u64>,
    from: u64,
    payload: &[u8],
    frame: Duration,
) -> bool {
    let log = log.lock().expect("timed log");
    let Some(start) = log
        .iter()
        .rev()
        .find(|k| k.station == from && !k.dropped && k.payload == payload)
        .map(|k| k.at)
    else {
        return false;
    };
    let edge = Duration::from_millis(20);
    log.iter().rev().take_while(|k| k.at + frame * 4 > start).any(|k| {
        k.station != from
            && !k.dropped
            && hear.contains(&k.station)
            && k.at + edge < start + frame
            && k.at + frame > start + edge
    })
}

fn timed_cap(cfg: &serde_json::Value, prev: usize) -> usize {
    match cfg.get("modem_type").and_then(|v| v.as_i64()) {
        Some(0) => 512,
        Some(1) => 55,
        Some(2) => 170,
        _ => prev,
    }
}

/// modem73 as upstream behaves: one global TX mode, a serial TX queue with
/// real airtime, frames over the current mode's capacity dropped when they
/// reach the transmitter, and a receiver that is deaf while it keys.
/// Every frame this station takes off its queue is appended to `log`.
pub async fn mock_modem73_timed(
    air: SharedAir,
    station: u64,
    hear: std::collections::HashSet<u64>,
    drop_rx: Option<LinkDrop>,
    opts: TimedOpts,
    log: TimedLog,
) -> crate::error::Result<(u16, u16)> {
    let (kiss, kiss_port) = loopback_listener().await?;
    let (ctrl, ctrl_port) = loopback_listener().await?;
    let st = Arc::new(Mutex::new(TimedState {
        queue: Default::default(),
        keying: false,
        keyed_until: None,
        heard_at: None,
        cap: 170,
        csma: true,
        tx_count: 0,
    }));
    let frame = Duration::from_millis(opts.frame_ms);
    let hear = Arc::new(hear);

    let tx_st = st.clone();
    let tx_air = air.clone();
    let tx_hear = hear.clone();
    let tx_log = log.clone();
    let stuck = Duration::from_millis(opts.csma_stuck_ms);
    tokio::spawn(async move {
        let mut burst_left = 0u8;
        loop {
            let next = tx_st.lock().expect("timed").queue.pop_front();
            let Some(payload) = next else {
                burst_left = 0;
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            };
            let waited = Instant::now();
            let cap = if stuck.is_zero() {
                Duration::from_secs(10)
            } else {
                stuck
            };
            let mut deferred = false;
            loop {
                let clear = {
                    let g = tx_st.lock().expect("timed");
                    if !g.csma || burst_left > 0 {
                        true
                    } else if !stuck.is_zero() {
                        false
                    } else {
                        g.heard_at
                            .is_none_or(|t| t.elapsed() >= Duration::from_millis(300))
                            && !timed_hearing(&tx_log, &tx_hear, frame)
                    }
                };
                if clear || waited.elapsed() >= cap {
                    break;
                }
                deferred = true;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            if deferred && !stuck.is_zero() {
                burst_left = 1;
            } else {
                burst_left = burst_left.saturating_sub(1);
            }
            let (cap, csma) = {
                let mut g = tx_st.lock().expect("timed");
                g.keying = true;
                if opts.count_tx {
                    g.tx_count += 1;
                }
                (g.cap, g.csma)
            };
            let dropped = payload.len() > cap;
            tx_log.lock().expect("timed log").push(TimedKey {
                station,
                at: Instant::now(),
                payload: payload.clone(),
                csma,
                cap,
                dropped,
            });
            if !dropped {
                tokio::time::sleep(frame).await;
                tx_air.send_from(station, payload);
            }
            let mut g = tx_st.lock().expect("timed");
            g.keying = false;
            g.keyed_until = Some(Instant::now());
        }
    });

    let rx_st = st.clone();
    let rx_hear = hear.clone();
    let rx_log = log.clone();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = kiss.accept().await.expect("kiss accept");
            let st = rx_st.clone();
            let air = air.clone();
            let hear = rx_hear.clone();
            let drop_rx = drop_rx.clone();
            let log = rx_log.clone();
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
                                        st.lock().expect("timed").queue.push_back(payload);
                                    }
                                }
                            }
                        }
                        got = rx.recv() => {
                            let Ok((from, payload)) = got else { continue };
                            if from == station || !hear.contains(&from) {
                                continue;
                            }
                            let deaf = {
                                let mut g = st.lock().expect("timed");
                                g.heard_at = Some(Instant::now());
                                g.keying || g.keyed_until.is_some_and(|t| t.elapsed() < frame)
                            };
                            let collided = opts.collide
                                && timed_collided(&log, &hear, from, &payload, frame);
                            if deaf
                                || collided
                                || drop_rx.as_ref().is_some_and(|d| d(from, station, &payload))
                            {
                                continue;
                            }
                            if stream.write_all(&encode_frame(&payload)).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });

    tokio::spawn(async move {
        loop {
            let (mut stream, _) = ctrl.accept().await.expect("ctrl accept");
            let st = st.clone();
            let log = log.clone();
            let hear = hear.clone();
            tokio::spawn(async move {
                let mut stash = Vec::new();
                let mut buf = [0u8; 2048];
                loop {
                    let n = match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    stash.extend_from_slice(&buf[..n]);
                    let Ok(frames) = crate::modem::control::decode_control_frames(&mut stash)
                    else {
                        continue;
                    };
                    for v in frames {
                        let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
                        let on_air = timed_hearing(&log, &hear, frame);
                        let reply = {
                            let mut g = st.lock().expect("timed");
                            match cmd {
                                "get_status" => {
                                    let rx = on_air
                                        || g.heard_at.is_some_and(|t| {
                                            t.elapsed() < Duration::from_millis(400)
                                        });
                                    let state = if g.keying {
                                        "tx"
                                    } else if rx {
                                        "rx"
                                    } else if opts.csma_stuck_ms > 0 {
                                        "busy"
                                    } else {
                                        "idle"
                                    };
                                    serde_json::json!({
                                        "channel_state": state,
                                        "ptt_on": g.keying,
                                        "tx_queue": g.queue.len(),
                                        "tx_frame_count": g.tx_count,
                                        "last_snr": 12.0,
                                        "audio_connected": true,
                                        "occupancy_pct": if rx { 80 } else { 0 },
                                        "input_level_db": if rx { -12.0 } else { -48.0 },
                                        "output_level_db": -80.0,
                                    })
                                }
                                "set_config" => {
                                    g.cap = timed_cap(&v, g.cap);
                                    if let Some(on) =
                                        v.get("csma_enabled").and_then(|x| x.as_bool())
                                    {
                                        g.csma = on;
                                    }
                                    serde_json::json!({"ok": true})
                                }
                                _ => serde_json::json!({"ok": true}),
                            }
                        };
                        if let Ok(f) = encode_control_frame(&reply) {
                            if stream.write_all(&f).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    Ok((kiss_port, ctrl_port))
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
