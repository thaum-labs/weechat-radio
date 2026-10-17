//! Radio-plus mail over a timed modem73: a sender (1) and an internet-radio
//! gateway (2) that hear each other, on hf-poor, with a mock hub that accepts
//! the gateway's post the way Resend would.
//!
//! The mock modem behaves like upstream modem73: `tx_frame_count` stays 0,
//! frames take real airtime from a serial queue, the transmit mode is global,
//! and a station cannot hear while it keys.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use wcr::config::Config;
use wcr::modes::Mode;
use wcr::sim::{LinkDrop, TimedLog, TimedOpts};

const SENDER: &str = "M7SND";
const GATEWAY: &str = "G4GW";

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wcr-mail-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Reserved ports are free again until the nodes bind them, and the mock
/// modems and hub of a pair starting alongside can be given one. A node whose
/// status port is taken runs without it, and its mail never shows as started.
/// Pairs start one at a time.
fn setup_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
    &LOCK
}

async fn reserve_ports(n: usize) -> Vec<u16> {
    let mut held = Vec::with_capacity(n);
    for _ in 0..n {
        held.push(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap());
    }
    let ports = held
        .iter()
        .map(|l| l.local_addr().unwrap().port())
        .collect();
    drop(held);
    ports
}

async fn wait_tcp(port: u16, secs: u64) {
    let addr = format!("127.0.0.1:{port}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(&addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    panic!("timed out waiting for {addr}");
}

#[allow(clippy::too_many_arguments)]
fn station_cfg(
    callsign: &str,
    home: &Path,
    mode: Mode,
    kiss: u16,
    ctrl: u16,
    irc: u16,
    status: u16,
    hub: &str,
) -> Config {
    let mut cfg = Config {
        callsign: callsign.into(),
        grid: "IO91WM".into(),
        mode,
        hub: wcr::config::HubConfig {
            url: hub.into(),
            ..Default::default()
        },
        telemetry: wcr::config::TelemetryConfig {
            url: String::new(),
            ..Default::default()
        },
        lan: wcr::config::LanConfig {
            discovery: false,
            ..Default::default()
        },
        modem: wcr::config::ModemConfig {
            manage: false,
            ptt: "none".into(),
            ..Default::default()
        },
        store: wcr::config::StoreConfig {
            path: home.join("store.db"),
            identity_key: home.join("identity.key"),
            ..Default::default()
        },
        ..Default::default()
    };
    cfg.irc.bind = format!("127.0.0.1:{irc}");
    cfg.status.bind = format!("127.0.0.1:{status}");
    cfg.modem.host = "127.0.0.1".into();
    cfg.modem.kiss_port = kiss;
    cfg.modem.control_port = ctrl;
    cfg.modem.preset = "hf-poor".into();
    cfg.rf.turnaround_ms = 0;
    cfg.rf.retry_jitter = false;
    cfg.rf.frequency_khz = 7_074;
    cfg.gateway.third_party = "allow".into();
    if mode == Mode::RadioPlus {
        cfg.mail.gateway = GATEWAY.into();
    }
    cfg.normalize();
    cfg
}

/// Accepts every `/api/v1/mail/send` after `delay` (the hub's own post to
/// Resend) and keeps the JSON bodies.
async fn mock_hub(posts: Arc<Mutex<Vec<serde_json::Value>>>, delay: Duration) -> u16 {
    use axum::routing::post;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = axum::Router::new().route(
        "/api/v1/mail/send",
        post(move |body: axum::body::Bytes| {
            let posts = posts.clone();
            async move {
                tokio::time::sleep(delay).await;
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body) {
                    posts.lock().unwrap().push(v);
                }
                axum::Json(serde_json::json!({ "ok": true, "id": "re_harness" }))
            }
        }),
    );
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    port
}

/// `(op, idx, count)` of a whole WCRM chunk frame.
fn chunk_of(frame: &[u8]) -> Option<(u8, u16, u16)> {
    let s = std::str::from_utf8(frame).ok()?;
    let mut it = s.strip_prefix("WCRM\t")?.split('\t');
    let op = it.next()?.parse().ok()?;
    let _id = it.next()?;
    let idx = it.next()?.parse().ok()?;
    let count = it.next()?.parse().ok()?;
    Some((op, idx, count))
}

fn is_mail(frame: &[u8]) -> bool {
    frame.starts_with(b"WCRM\t") || frame.starts_with(b"WCRP\t")
}

/// `MailOp::Data` and `MailOp::VoxData` on the wire.
fn is_data_op(op: u8) -> bool {
    op == 0 || op == 5
}

struct Pair {
    log: TimedLog,
    posts: Arc<Mutex<Vec<serde_json::Value>>>,
    status: [u16; 2],
    irc: [u16; 2],
    nodes: Vec<tokio::task::JoinHandle<wcr::error::Result<()>>>,
}

impl Pair {
    async fn start(opts: TimedOpts, drop_rx: LinkDrop) -> Self {
        Self::start_with(opts, drop_rx, Duration::ZERO).await
    }

    async fn start_with(opts: TimedOpts, drop_rx: LinkDrop, hub_delay: Duration) -> Self {
        Self::start_tweaked(opts, drop_rx, hub_delay, |_, _| {}).await
    }

    /// `tweak` adjusts station `i`'s config (0 the sender, 1 the gateway).
    async fn start_tweaked(
        opts: TimedOpts,
        drop_rx: LinkDrop,
        hub_delay: Duration,
        tweak: impl Fn(usize, &mut Config),
    ) -> Self {
        let setup = setup_lock().lock().await;
        let air = wcr::sim::SharedAir::new(0.0);
        let log: TimedLog = Arc::new(Mutex::new(Vec::new()));
        let posts = Arc::new(Mutex::new(Vec::new()));
        let hub = mock_hub(posts.clone(), hub_delay).await;
        let hub_url = format!("ws://127.0.0.1:{hub}/ws");
        let ports = reserve_ports(4).await;
        let mut nodes = Vec::new();
        for (i, (call, mode)) in [(SENDER, Mode::RadioPlus), (GATEWAY, Mode::InternetRadio)]
            .into_iter()
            .enumerate()
        {
            let me = i as u64 + 1;
            let other = 3 - me;
            let (kiss, ctrl) = wcr::sim::mock_modem73_timed(
                air.clone(),
                me,
                [other].into_iter().collect(),
                Some(drop_rx.clone()),
                opts,
                log.clone(),
            )
            .await
            .unwrap();
            let home = scratch(call);
            let mut cfg = station_cfg(
                call,
                &home,
                mode,
                kiss,
                ctrl,
                ports[i * 2],
                ports[i * 2 + 1],
                &hub_url,
            );
            tweak(i, &mut cfg);
            nodes.push(tokio::spawn(async move {
                wcr::node::run_node(cfg, false).await
            }));
        }
        for p in [ports[1], ports[3]] {
            wait_tcp(p, 20).await;
        }
        drop(setup);
        let pair = Self {
            log,
            posts,
            status: [ports[1], ports[3]],
            irc: [ports[0], ports[2]],
            nodes,
        };
        pair.wait_mail().await;
        pair
    }

    async fn wait_mail(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let mut ok = true;
            for p in self.status {
                let r = reqwest::get(format!("http://127.0.0.1:{p}/mail/tab")).await;
                ok &= r.is_ok_and(|r| r.status().is_success());
            }
            if ok {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        panic!("mail never started");
    }

    async fn compose(&self, body: &str) -> String {
        let v: serde_json::Value = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/mail/compose", self.status[0]))
            .json(&serde_json::json!({
                "to": "operator@example.com",
                "subject": "long report",
                "body": body,
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        v.get("id").and_then(|x| x.as_str()).unwrap().to_string()
    }

    async fn row(&self, id: &str) -> Option<serde_json::Value> {
        for folder in ["outbox", "sent"] {
            let rows: Vec<serde_json::Value> = reqwest::get(format!(
                "http://127.0.0.1:{}/mail/list?folder={folder}",
                self.status[0]
            ))
            .await
            .ok()?
            .json()
            .await
            .ok()?;
            if let Some(r) = rows
                .into_iter()
                .find(|r| r.get("id").and_then(|x| x.as_str()) == Some(id))
            {
                return Some(r);
            }
        }
        None
    }

    async fn wait_state(&self, id: &str, want: &str, secs: u64) -> Option<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(secs);
        let mut last = None;
        while Instant::now() < deadline {
            last = self.row(id).await;
            let state = last
                .as_ref()
                .and_then(|r| r.get("state").and_then(|x| x.as_str()))
                .unwrap_or("");
            if state == want || state == "failed" {
                return last;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        last
    }

    fn trace(&self, t0: Instant) -> String {
        let mut out = String::new();
        for k in self.log.lock().unwrap().iter() {
            let what = match chunk_of(&k.payload) {
                Some((op, idx, count)) => format!("WCRM op{op} {}/{count}", idx + 1),
                None if k.payload.starts_with(b"WCRP\t") => "WCRP slice".to_string(),
                None => format!("chat {} B", k.payload.len()),
            };
            out.push_str(&format!(
                "{:7.1}s st{} {what} csma={} cap={}{}\n",
                k.at.saturating_duration_since(t0).as_secs_f64(),
                k.station,
                k.csma,
                k.cap,
                if k.dropped { " DROPPED" } else { "" }
            ));
        }
        out
    }

    fn stop(self) {
        for n in self.nodes {
            n.abort();
        }
    }
}

fn long_body() -> String {
    let para = "Field report from the hill: power steady, antenna up, band noisy but usable.\n\
                Next sked at the top of the hour on the usual channel.\n\n";
    let mut body = para.repeat(3900 / para.len() + 1);
    body.truncate(3900);
    body.trim().to_string()
}

/// The final chunk of a 3.9 KB mail is lost once on the way to the gateway.
/// The sender must key only that chunk again, the gateway must post the whole
/// body once, and the outbox row must reach Sent. Every mail frame keys inside
/// the mail lease: carrier sense off, and never after chat's config is back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn long_mail_recovers_a_lost_final_chunk() {
    let lost = Arc::new(AtomicU64::new(0));
    let lost_c = lost.clone();
    let drop_rx: LinkDrop = Arc::new(move |from, to, frame| {
        let Some((op, idx, count)) = chunk_of(frame) else {
            return false;
        };
        let last = from == 1 && to == 2 && is_data_op(op) && count > 1 && idx + 1 == count;
        last && lost_c.fetch_add(1, Ordering::SeqCst) == 0
    });
    let pair = Pair::start(
        TimedOpts {
            frame_ms: 2_000,
            count_tx: false,
            collide: false,
            csma_stuck_ms: 0,
            rung_airtime: false,
            noise_pct: 0,
        },
        drop_rx,
    )
    .await;
    let body = long_body();
    let t0 = Instant::now();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 220).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    let posts = pair.posts.lock().unwrap().clone();
    let log = pair.log.lock().unwrap().clone();
    pair.stop();

    assert!(
        lost.load(Ordering::SeqCst) >= 1,
        "the final chunk was never keyed"
    );
    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}");
    assert_eq!(
        posts[0].get("body").and_then(|x| x.as_str()),
        Some(body.as_str())
    );

    let sender: Vec<_> = log
        .iter()
        .filter(|k| k.station == 1 && is_mail(&k.payload))
        .collect();
    for k in &sender {
        assert!(!k.dropped, "mail frame dropped by the modem\n{trace}");
        assert!(!k.csma, "mail frame keyed after the lease ended\n{trace}");
    }
    let count = sender
        .iter()
        .find_map(|k| {
            chunk_of(&k.payload)
                .filter(|c| is_data_op(c.0))
                .map(|c| c.2)
        })
        .expect("a data chunk");
    let mut keyed = vec![0u32; count as usize];
    for k in &sender {
        if let Some((op, idx, _)) = chunk_of(&k.payload) {
            if is_data_op(op) {
                keyed[idx as usize] += 1;
            }
        }
    }
    let last = count as usize - 1;
    assert_eq!(keyed[last], 2, "final chunk keys: {keyed:?}\n{trace}");
    assert!(
        keyed[1..last].iter().all(|n| *n == 1),
        "only the lost chunk is keyed again: {keyed:?}\n{trace}"
    );
}

/// Chunks 8, 31 and 48 of 48 are lost once each, on a modem that does count
/// transmitted frames. The gateway's have-report lands while the sender may
/// still hold the radio; the resend must key exactly the three it lacks, not
/// a set planned from older progress.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resend_keys_only_what_the_latest_report_lacks() {
    let lost = Arc::new(Mutex::new(std::collections::HashSet::new()));
    let lost_c = lost.clone();
    let drop_rx: LinkDrop = Arc::new(move |from, to, frame| {
        let Some((op, idx, count)) = chunk_of(frame) else {
            return false;
        };
        let target = idx == 7 || idx == 30 || idx + 1 == count;
        from == 1
            && to == 2
            && is_data_op(op)
            && count > 1
            && target
            && lost_c.lock().unwrap().insert(idx)
    });
    let pair = Pair::start(
        TimedOpts {
            frame_ms: 2_000,
            count_tx: true,
            collide: false,
            csma_stuck_ms: 0,
            rung_airtime: false,
            noise_pct: 0,
        },
        drop_rx,
    )
    .await;
    let body = long_body();
    let t0 = Instant::now();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 220).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    let posts = pair.posts.lock().unwrap().clone();
    let log = pair.log.lock().unwrap().clone();
    pair.stop();

    assert_eq!(
        lost.lock().unwrap().len(),
        3,
        "the lost chunks were never keyed"
    );
    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}");
    assert_eq!(
        posts[0].get("body").and_then(|x| x.as_str()),
        Some(body.as_str())
    );
    let sender: Vec<_> = log
        .iter()
        .filter(|k| k.station == 1 && is_mail(&k.payload))
        .collect();
    for k in &sender {
        assert!(!k.dropped, "mail frame dropped by the modem\n{trace}");
        assert!(!k.csma, "mail frame keyed after the lease ended\n{trace}");
    }
    let mut keyed = std::collections::BTreeMap::<u16, u32>::new();
    for k in &sender {
        if let Some((op, idx, _)) = chunk_of(&k.payload) {
            if is_data_op(op) {
                *keyed.entry(idx).or_default() += 1;
            }
        }
    }
    let again: Vec<u16> = keyed
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(i, _)| *i)
        .collect();
    assert_eq!(again, vec![7, 30, 47], "keys per chunk: {keyed:?}\n{trace}");
    assert!(
        keyed.values().all(|n| *n <= 2),
        "keys per chunk: {keyed:?}\n{trace}"
    );
}

/// The gateway posts the mail and keys the acceptance, and that one frame is
/// lost. The sender must not key the whole mail again: one probe chunk, the
/// gateway repeats the acceptance it already has, and the row reaches Sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_lost_acceptance_is_answered_by_one_probe() {
    let lost = Arc::new(AtomicU64::new(0));
    let lost_c = lost.clone();
    let drop_rx: LinkDrop = Arc::new(move |from, to, frame| {
        let Some((op, _, _)) = chunk_of(frame) else {
            return false;
        };
        from == 2 && to == 1 && op == 7 && lost_c.fetch_add(1, Ordering::SeqCst) == 0
    });
    let pair = Pair::start(
        TimedOpts {
            frame_ms: 400,
            count_tx: false,
            collide: false,
            csma_stuck_ms: 0,
            rung_airtime: false,
            noise_pct: 0,
        },
        drop_rx,
    )
    .await;
    // Compose trims the body, and the gateway posts what it reassembled.
    let body = "Field report, power steady, next sked on the hour. "
        .repeat(12)
        .trim_end()
        .to_string();
    let t0 = Instant::now();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 180).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    let posts = pair.posts.lock().unwrap().clone();
    let log = pair.log.lock().unwrap().clone();
    pair.stop();

    assert!(
        lost.load(Ordering::SeqCst) >= 1,
        "the acceptance was never keyed"
    );
    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}");
    assert_eq!(
        posts[0].get("body").and_then(|x| x.as_str()),
        Some(body.as_str())
    );
    let mut keyed = std::collections::BTreeMap::<u16, u32>::new();
    let mut acks = 0u32;
    for k in log.iter().filter(|k| is_mail(&k.payload)) {
        assert!(!k.dropped, "mail frame dropped by the modem\n{trace}");
        if k.station == 2 && chunk_of(&k.payload).is_some_and(|c| c.0 == 7) {
            acks += 1;
        }
        if k.station == 1 {
            if let Some((op, idx, _)) = chunk_of(&k.payload) {
                if is_data_op(op) {
                    *keyed.entry(idx).or_default() += 1;
                }
            }
        }
    }
    let again: Vec<u16> = keyed
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(i, _)| *i)
        .collect();
    assert!(
        keyed.len() > 4,
        "the mail was too short to prove the probe: {keyed:?}"
    );
    assert_eq!(
        again.len(),
        1,
        "a lost acceptance must cost one probe chunk, not the whole mail: {keyed:?}\n{trace}"
    );
    assert!(
        acks >= 2,
        "gateway keyed the acceptance {acks} times\n{trace}"
    );
}

/// The live v0.1.89 big mail, at real hf-poor airtime: 4 s frames, a slower
/// frame on a retry rung, and a hub that takes 2 s to accept the post. Nothing
/// is lost on the air. The gateway's acceptance used to land after the
/// sender's retry had already keyed, so the sender never heard it and keyed
/// the whole mail again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_long_mail_at_real_airtime_is_accepted_on_the_first_pass() {
    let drop_rx: LinkDrop = Arc::new(|_, _, _| false);
    let pair = Pair::start_with(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: false,
            collide: false,
            csma_stuck_ms: 0,
            rung_airtime: true,
            noise_pct: 0,
        },
        drop_rx,
        Duration::from_secs(2),
    )
    .await;
    let mut body = long_body();
    body.truncate(800);
    let body = body.trim().to_string();
    let t0 = Instant::now();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 240).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    let posts = pair.posts.lock().unwrap().clone();
    let log = pair.log.lock().unwrap().clone();
    pair.stop();

    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}");
    let mut keyed = std::collections::BTreeMap::<u16, u32>::new();
    for k in log.iter().filter(|k| k.station == 1 && is_mail(&k.payload)) {
        assert!(!k.dropped, "mail frame dropped by the modem\n{trace}");
        if let Some((op, idx, _)) = chunk_of(&k.payload) {
            if is_data_op(op) {
                *keyed.entry(idx).or_default() += 1;
            }
        }
    }
    assert!(
        keyed.len() > 4,
        "the mail was too short to show a resend: {keyed:?}"
    );
    assert!(
        keyed.values().all(|n| *n == 1),
        "the sender keyed over the acceptance and sent the mail again: {keyed:?}\n{trace}"
    );
}

/// macOS CI on #50: the sender's first beacon after start keyed the moment
/// the gateway keyed its acceptance, so the sender never heard it and sent the
/// whole mail again. Its first beacon now waits after its own mail, as later
/// ones do. Here the first beacon (no jitter, a minute after start) is due
/// between the end of the burst and the gateway's reply.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_first_beacon_waits_for_the_reply_to_our_mail() {
    let drop_rx: LinkDrop = Arc::new(|_, _, _| false);
    let pair = Pair::start_tweaked(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: false,
            collide: false,
            csma_stuck_ms: 0,
            rung_airtime: true,
            noise_pct: 0,
        },
        drop_rx,
        Duration::from_secs(2),
        |i, cfg| {
            if i == 0 {
                cfg.rf.beacon_jitter_s = 0;
            }
        },
    )
    .await;
    // The burst takes about 48 s and the reply comes about 10 s after it.
    tokio::time::sleep(Duration::from_secs(6)).await;
    let mut body = long_body();
    body.truncate(800);
    let body = body.trim().to_string();
    let t0 = Instant::now();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 240).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let log = pair.log.lock().unwrap().clone();
    pair.stop();

    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    assert_eq!(state, "sent", "row {row}\n{trace}");
    let reply = log
        .iter()
        .find(|k| {
            k.station == 2 && chunk_of(&k.payload).is_some_and(|(op, _, _)| op == 6 || op == 7)
        })
        .map(|k| k.at + k.len)
        .expect("the gateway replied");
    let keyed_before: Vec<_> = log
        .iter()
        .filter(|k| k.station == 1 && k.at >= t0 && k.at < reply && !is_mail(&k.payload))
        .map(|k| (k.at - t0).as_secs_f64())
        .collect();
    assert!(
        keyed_before.is_empty(),
        "the sender keyed chat at {keyed_before:?} s before the gateway's reply ended\n{trace}"
    );
}

/// QA's v0.1.92 big mail: about 3.9 KB (56 chunks) from a radio-plus sender to
/// the gateway at real hf-poor airtime, with modem carrier sense stuck busy
/// (it holds a frame up to 30 s, then keys anyway), as in QA's simulator.
/// The mail must go out in one burst and reach Sent without a full resend.
/// QA v0.1.94: the gateway beaconed 27 s into the burst. Neither station
/// beacons from the first chunk until the gateway's acceptance.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_big_mail_with_stuck_carrier_sense_goes_in_one_burst() {
    let drop_rx: LinkDrop = Arc::new(|_, _, _| false);
    let pair = Pair::start_with(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: true,
            collide: true,
            csma_stuck_ms: 30_000,
            rung_airtime: true,
            noise_pct: 0,
        },
        drop_rx,
        Duration::from_secs(2),
    )
    .await;
    let body = long_body();
    let t0 = Instant::now();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 900).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    let posts = pair.posts.lock().unwrap().clone();
    let log = pair.log.lock().unwrap().clone();
    pair.stop();
    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}");
    let burst = log
        .iter()
        .find(|k| k.station == 1 && is_mail(&k.payload))
        .map(|k| k.at)
        .expect("the burst");
    let accepted = log
        .iter()
        .find(|k| k.station == 2 && chunk_of(&k.payload).is_some_and(|(op, _, _)| op == 7))
        .map(|k| k.at + k.len)
        .expect("the gateway's acceptance");
    let beacons: Vec<String> = log
        .iter()
        .filter(|k| k.at >= burst && k.at < accepted)
        .filter(|k| {
            wcr::proto::Envelope::decode(&k.payload)
                .is_ok_and(|e| e.kind == wcr::proto::MsgType::Beacon)
        })
        .map(|k| format!("station {} at {:.0}s", k.station, (k.at - t0).as_secs_f64()))
        .collect();
    assert!(
        beacons.is_empty(),
        "beacons between the first chunk and the acceptance: {beacons:?}\n{trace}"
    );
}

/// An IRC client on `port` as `nick` that keeps every line it reads.
async fn irc(port: u16, nick: &str) -> (Arc<Mutex<Vec<String>>>, tokio::net::tcp::OwnedWriteHalf) {
    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .unwrap();
    let (r, mut w) = stream.into_split();
    w.write_all(format!("NICK {nick}\r\nUSER {nick} 0 * :wcr mail\r\n").as_bytes())
        .await
        .unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(r);
        let mut line = String::new();
        while reader.read_line(&mut line).await.is_ok_and(|n| n > 0) {
            sink.lock().unwrap().push(line.trim_end().to_string());
            line.clear();
        }
    });
    (lines, w)
}

/// The gateway sends the sender a chat line, and the sender composes a big
/// mail the moment it shows. The sender's receipt for that line is keyed
/// while the mail lease starts. In v0.1.92 the receipt switched the modem to
/// its own config and, 5 s later, restored chat's with carrier sense on, so
/// the rest of the burst keyed at chat's rung and waited out the stuck
/// carrier sense: QA's six-minute stall in the middle of a 56-chunk mail.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_receipt_keyed_as_a_mail_starts_leaves_the_burst_alone() {
    let drop_rx: LinkDrop = Arc::new(|_, _, _| false);
    let pair = Pair::start_with(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: false,
            collide: true,
            csma_stuck_ms: 30_000,
            rung_airtime: true,
            noise_pct: 0,
        },
        drop_rx,
        Duration::from_secs(2),
    )
    .await;
    let (sender_lines, _sender_w) = irc(pair.irc[0], SENDER).await;
    let (_gw_lines, mut gw_w) = irc(pair.irc[1], GATEWAY).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let t0 = Instant::now();
    gw_w.write_all(format!("PRIVMSG {SENDER} :before-the-mail\r\n").as_bytes())
        .await
        .unwrap();
    // Sent just after start, the line waits until the gateway hears the
    // sender's first beacon, at most the start-up hold.
    let deadline =
        Instant::now() + Duration::from_secs(u64::from(wcr::relay::STARTUP_HOLD_S) + 120);
    while !sender_lines
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.contains("before-the-mail"))
    {
        assert!(
            Instant::now() < deadline,
            "the chat line never reached the sender"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let id = pair.compose(&long_body()).await;
    let row = pair.wait_state(&id, "sent", 900).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    let posts = pair.posts.lock().unwrap().clone();
    let log = pair.log.lock().unwrap().clone();
    pair.stop();
    for k in log.iter().filter(|k| k.station == 1 && is_mail(&k.payload)) {
        assert!(!k.csma, "a mail frame keyed with carrier sense on\n{trace}");
    }
    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}");
}
