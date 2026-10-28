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

/// Waits for a node's status port. Fails early with the node's own error if it
/// stopped (an IRC port taken by another test's connection), and after `secs`
/// if it never listened (status port taken, or a slow runner).
async fn wait_listening(
    port: u16,
    node: &mut tokio::task::JoinHandle<wcr::error::Result<()>>,
    secs: u64,
) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < deadline {
        if TcpStream::connect(&addr).await.is_ok() {
            return Ok(());
        }
        if node.is_finished() {
            return Err(match node.await {
                Ok(Ok(())) => format!("node for {addr} stopped before listening"),
                Ok(Err(e)) => format!("node for {addr} failed to start: {e}"),
                Err(e) => format!("node for {addr} panicked: {e}"),
            });
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!("timed out after {secs} s waiting for {addr}"))
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
    ///
    /// Reserved ports are free again until the nodes bind them, so another
    /// test's connection can take one in between. A start that fails is
    /// thrown away and tried again on fresh ports.
    async fn start_tweaked(
        opts: TimedOpts,
        drop_rx: LinkDrop,
        hub_delay: Duration,
        tweak: impl Fn(usize, &mut Config),
    ) -> Self {
        const ATTEMPTS: usize = 3;
        let mut errors = Vec::new();
        for attempt in 1..=ATTEMPTS {
            match Self::try_start(opts, drop_rx.clone(), hub_delay, &tweak).await {
                Ok(pair) => {
                    pair.wait_mail().await;
                    return pair;
                }
                Err(e) => {
                    eprintln!("pair start attempt {attempt}/{ATTEMPTS} failed: {e}");
                    errors.push(e);
                }
            }
        }
        panic!("pair never started: {}", errors.join("; "));
    }

    async fn try_start(
        opts: TimedOpts,
        drop_rx: LinkDrop,
        hub_delay: Duration,
        tweak: &impl Fn(usize, &mut Config),
    ) -> Result<Self, String> {
        let _setup = setup_lock().lock().await;
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
        // 60 s: a loaded msrv runner can take well over 20 s to start a node.
        for (i, p) in [ports[1], ports[3]].into_iter().enumerate() {
            if let Err(e) = wait_listening(p, &mut nodes[i], 60).await {
                for n in &nodes {
                    n.abort();
                }
                return Err(e);
            }
        }
        Ok(Self {
            log,
            posts,
            status: [ports[1], ports[3]],
            irc: [ports[0], ports[2]],
            nodes,
        })
    }

    async fn wait_mail(&self) {
        // Room for a slow runner: msrv CI once gave up at 30 s (#170).
        let deadline = Instant::now() + Duration::from_secs(120);
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
            rx_after_decode: false,
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
            rx_after_decode: false,
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
            rx_after_decode: false,
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
    // The first chunk names the sender, so it is the probe. It is wider
    // than a frame and goes as slices.
    let mut slices = 0u32;
    for k in log.iter().filter(|k| is_mail(&k.payload)) {
        assert!(!k.dropped, "mail frame dropped by the modem\n{trace}");
        if k.station == 2 && chunk_of(&k.payload).is_some_and(|c| c.0 == 7) {
            acks += 1;
        }
        if k.station == 1 {
            if k.payload.starts_with(b"WCRP\t") {
                slices += 1;
            }
            if let Some((op, idx, _)) = chunk_of(&k.payload) {
                if is_data_op(op) {
                    *keyed.entry(idx).or_default() += 1;
                }
            }
        }
    }
    assert!(
        keyed.len() > 4,
        "the mail was too short to prove the probe: {keyed:?}"
    );
    assert!(
        keyed.values().all(|n| *n == 1) && slices == 4,
        "a lost acceptance must cost one probe chunk, not the whole mail: {keyed:?}, {slices} slices\n{trace}"
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
            rx_after_decode: false,
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
            rx_after_decode: false,
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

/// A status page's `beacon_note`: `pending` while a beacon is on its way
/// from the air queue to the modem.
async fn beacon_note(port: u16) -> String {
    let Ok(resp) = reqwest::get(format!("http://127.0.0.1:{port}/status")).await else {
        return String::new();
    };
    resp.json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("beacon_note")?.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Has station `i` heard the other one yet?
async fn heard_someone(port: u16) -> bool {
    let Ok(resp) = reqwest::get(format!("http://127.0.0.1:{port}/status")).await else {
        return false;
    };
    resp.json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("heard")?.as_array().map(|h| !h.is_empty()))
        .unwrap_or(false)
}

/// Wait until station `i`'s next beacon has just been handed to the air
/// queue. The countdown reads `pending` from the handover until the beacon
/// reaches the modem, so a beacon found already pending may be at the modem
/// before the test goes on (msrv CI: the gateway's first beacon was written
/// just before the burst started and keyed between the chunks, which the
/// beacon loop cannot take back). The countdown is seen first, then the
/// handover. The station has heard the other one by then, so with
/// [`beacon_held_a_while`] the beacon waits out carrier sense on the air
/// queue, where the mail can take it back. A warm-up beacon goes a minute
/// after the one before, so the wait is usually one or two of them. The
/// first deadline is for hearing the other station (msrv CI once gave up at
/// 240 s). The two stations' warm-up beacons can collide, and then the
/// station may first hear the other one after its third warm-up beacon
/// (ubuntu CI on fa9c996, #194). Its next beacon is the idle one, up to
/// `BEACON_IDLE_S` plus the jitter and the defer cap later, so once it has
/// heard the other one the deadline moves out to cover that gap.
async fn wait_beacon_pending(pair: &Pair, i: usize) {
    let mut deadline = Instant::now() + Duration::from_secs(360);
    let mut counting_down = false;
    loop {
        let note = beacon_note(pair.status[i]).await;
        if note == "pending" && counting_down {
            return;
        }
        if !counting_down && note != "pending" && heard_someone(pair.status[i]).await {
            counting_down = true;
            let next_beacon = u64::from(wcr::relay::BEACON_IDLE_S) + 120;
            deadline = Instant::now() + Duration::from_secs(next_beacon);
        }
        assert!(
            Instant::now() < deadline,
            "station {}'s beacon never went (heard the other station: {counting_down})",
            i + 1
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The stuck modem reads `busy`, which the air queue's own carrier sense
/// takes as idle, so a beacon taken off the queue goes to the modem on the
/// first p-persistence roll it wins: within a second one time in five, and
/// a beacon at the modem cannot be taken back. With the channel counted as
/// busy for ten minutes after anything heard, a frame taken off the queue
/// waits out the whole defer cap, 20 s, so the beacon is still on the air
/// queue when the mail starts and can be taken back. Six seconds was not
/// enough: the gateway first decodes the mail about 8 s in, its probe's
/// airtime, and on main CI for v0.1.98 (Windows and msrv) the beacon had
/// reached the modem 2 s before that, where nothing can take it back. Mail
/// frames key with carrier sense off and do not wait out this cap.
fn beacon_held_a_while(cfg: &mut Config) {
    cfg.rf.beacon_jitter_s = 0;
    cfg.rf.quiet_ms = 600_000;
    cfg.rf.max_defer_ms = 20_000;
}

/// When `station` keyed a beacon at or after `t0`.
fn beacons_from(log: &[wcr::sim::TimedKey], station: u64, t0: Instant) -> Vec<Instant> {
    log.iter()
        .filter(|k| {
            k.station == station
                && k.at >= t0
                && wcr::proto::Envelope::decode(&k.payload)
                    .is_ok_and(|e| e.kind == wcr::proto::MsgType::Beacon)
        })
        .map(|k| k.at)
        .collect()
}

/// When the gateway's acceptance of the mail ended.
fn acceptance_end(log: &[wcr::sim::TimedKey]) -> Option<Instant> {
    log.iter()
        .find(|k| k.station == 2 && chunk_of(&k.payload).is_some_and(|(op, _, _)| op == 7))
        .map(|k| k.at + k.len)
}

/// A chat frame taken off the air queue waits out carrier sense, which the
/// station's own mail burst keeps busy once it keys, for up to 15 s. It then
/// set chat's modem config, carrier sense on, just before it went, and the
/// rest of the burst keyed under it: two chunks every 38 s on a stuck modem.
/// Here a warm-up beacon of the sender's (no jitter, a minute apart) has just
/// been handed to the air queue when the mail starts. The beacon is taken
/// back while it waits, the burst keeps carrier sense off, and the beacon
/// goes only after the gateway's acceptance.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_beacon_on_its_way_when_a_mail_starts_waits_for_the_acceptance() {
    let drop_rx: LinkDrop = Arc::new(|_, _, _| false);
    let pair = Pair::start_tweaked(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: false,
            collide: false,
            csma_stuck_ms: 30_000,
            rung_airtime: true,
            noise_pct: 99,
            rx_after_decode: false,
        },
        drop_rx,
        Duration::from_secs(2),
        |i, cfg| {
            if i == 0 {
                beacon_held_a_while(cfg);
            }
        },
    )
    .await;
    // The countdown reads pending from when the beacon is handed to the air
    // queue until it reaches the modem.
    wait_beacon_pending(&pair, 0).await;
    let t0 = Instant::now();
    let mut body = long_body();
    body.truncate(800);
    let body = body.trim().to_string();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 300).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let log = pair.log.lock().unwrap().clone();
    pair.stop();

    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    assert_eq!(state, "sent", "row {row}\n{trace}");
    let accepted = acceptance_end(&log).unwrap_or_else(|| panic!("no acceptance\n{trace}"));
    for k in log
        .iter()
        .filter(|k| k.station == 1 && k.at < accepted && is_mail(&k.payload))
    {
        assert!(!k.csma, "a mail frame keyed with carrier sense on\n{trace}");
    }
    let first_chunk = log
        .iter()
        .find(|k| k.station == 1 && is_mail(&k.payload))
        .map(|k| k.at)
        .expect("the sender keyed its mail");
    let beacons = beacons_from(&log, 1, t0);
    assert!(
        !beacons.iter().any(|b| *b < accepted),
        "the sender's beacon keyed {:?} after the first chunk, before the acceptance\n{trace}",
        beacons
            .iter()
            .filter(|b| **b < accepted)
            .map(|b| b.saturating_duration_since(first_chunk))
            .collect::<Vec<_>>()
    );
}

/// QA v0.1.97: the gateway keyed its first beacon between chunks 2 and 3 of
/// a short mail, 64 s after start. The beacon had been taken off the air
/// queue just before the first chunk decoded and was waiting out carrier
/// sense, where the beacon loop could not take it back. Here a warm-up
/// beacon of the gateway's (no jitter, a minute apart) has just been handed
/// to the air queue when the sender's burst starts. The gateway keys no beacon from
/// the first chunk until its acceptance, and the mail is accepted first
/// time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_gateway_takes_its_beacon_back_when_mail_chunks_arrive() {
    let drop_rx: LinkDrop = Arc::new(|_, _, _| false);
    let pair = Pair::start_tweaked(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: false,
            collide: true,
            csma_stuck_ms: 30_000,
            rung_airtime: true,
            noise_pct: 99,
            rx_after_decode: false,
        },
        drop_rx,
        Duration::from_secs(2),
        |i, cfg| {
            if i == 1 {
                beacon_held_a_while(cfg);
            }
        },
    )
    .await;
    wait_beacon_pending(&pair, 1).await;
    let t0 = Instant::now();
    let mut body = long_body();
    body.truncate(800);
    let body = body.trim().to_string();
    let id = pair.compose(&body).await;
    let row = pair.wait_state(&id, "sent", 300).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let log = pair.log.lock().unwrap().clone();
    let posts = pair.posts.lock().unwrap().clone();
    pair.stop();

    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}\n{trace}");
    let first_chunk = log
        .iter()
        .find(|k| k.station == 1 && is_mail(&k.payload))
        .map(|k| k.at)
        .expect("the sender keyed its mail");
    let accepted = acceptance_end(&log).unwrap_or_else(|| panic!("no acceptance\n{trace}"));
    let between: Vec<f64> = beacons_from(&log, 2, first_chunk)
        .into_iter()
        .filter(|b| *b < accepted)
        .map(|b| (b - t0).as_secs_f64())
        .collect();
    assert!(
        between.is_empty(),
        "the gateway keyed a beacon at {between:?} s, between the first chunk and its acceptance\n{trace}"
    );
    let retries = row.get("retries").and_then(|x| x.as_u64()).unwrap_or(0);
    assert_eq!(
        retries, 0,
        "the mail was not accepted first time: row {row}\n{trace}"
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
            rx_after_decode: false,
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
            rx_after_decode: false,
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
    // sender's first beacon, at most the start-up hold. Each copy then waits
    // out the stuck carrier sense, and one keyed while the sender keys a
    // warm-up beacon is lost: room for all three copies.
    let deadline =
        Instant::now() + Duration::from_secs(u64::from(wcr::relay::STARTUP_HOLD_S) + 240);
    while !sender_lines
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.contains("before-the-mail"))
    {
        assert!(
            Instant::now() < deadline,
            "the chat line never reached the sender\n{}",
            pair.trace(t0)
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

/// QA v0.1.95: the 56-chunk mail composed 48 s after start, on a clean
/// channel at real hf-poor airtime with carrier sense stuck. Both stations'
/// warm-up beacons fall due during the burst. The mail must reach Sent with
/// one post, and no chunk keyed more than twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_big_mail_started_just_after_start_reaches_the_gateway() {
    let drop_rx: LinkDrop = Arc::new(|_, _, _| false);
    let started = Instant::now();
    let pair = Pair::start_with(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: false,
            collide: true,
            csma_stuck_ms: 30_000,
            rung_airtime: true,
            noise_pct: 0,
            rx_after_decode: false,
        },
        drop_rx,
        Duration::from_secs(2),
    )
    .await;
    tokio::time::sleep_until((started + Duration::from_secs(48)).into()).await;
    let t0 = Instant::now();
    let id = pair.compose(&long_body()).await;
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
    let mut keyed = std::collections::BTreeMap::<u16, u32>::new();
    for k in log.iter().filter(|k| k.station == 1) {
        if let Some((_, idx, _)) = chunk_of(&k.payload).filter(|c| is_data_op(c.0)) {
            *keyed.entry(idx).or_default() += 1;
        }
    }
    assert!(
        keyed.values().all(|n| *n <= 2),
        "the mail was sent again: {keyed:?}\n{trace}"
    );
}

/// A gateway that missed the first chunk, the only one that names the
/// sender, kept every other chunk but could not address a report, so each
/// probe went unanswered and the mail failed. A slice of the first chunk to
/// the gateway is lost once here, at real hf-poor airtime with carrier sense
/// stuck.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_gateway_that_missed_the_first_chunk_still_gets_the_mail() {
    let lost = Arc::new(AtomicU64::new(0));
    let lost_c = lost.clone();
    let drop_rx: LinkDrop = Arc::new(move |from, to, frame| {
        // The first chunk carries the sender's address and goes as two
        // slices; the burst opens with them.
        let first = from == 1 && to == 2 && frame.starts_with(b"WCRP\t");
        first && lost_c.fetch_add(1, Ordering::SeqCst) == 0
    });
    let pair = Pair::start_with(
        TimedOpts {
            frame_ms: 4_000,
            count_tx: false,
            collide: true,
            csma_stuck_ms: 30_000,
            rung_airtime: true,
            noise_pct: 0,
            rx_after_decode: false,
        },
        drop_rx,
        Duration::from_secs(2),
    )
    .await;
    let t0 = Instant::now();
    let id = pair.compose(&long_body()).await;
    let row = pair.wait_state(&id, "sent", 900).await;
    let trace = pair.trace(t0);
    eprintln!("{trace}");
    let row = row.expect("outbox row");
    let state = row.get("state").and_then(|x| x.as_str()).unwrap_or("");
    let posts = pair.posts.lock().unwrap().clone();
    let log = pair.log.lock().unwrap().clone();
    pair.stop();
    assert!(
        lost.load(Ordering::SeqCst) >= 1,
        "the first chunk never keyed"
    );
    assert_eq!(state, "sent", "row {row}\n{trace}");
    assert_eq!(posts.len(), 1, "gateway posts: {posts:?}");
    let mut keyed = std::collections::BTreeMap::<u16, u32>::new();
    for k in log.iter().filter(|k| k.station == 1) {
        if let Some((_, idx, _)) = chunk_of(&k.payload).filter(|c| is_data_op(c.0)) {
            *keyed.entry(idx).or_default() += 1;
        }
    }
    // The first chunk goes as slices, so it is not counted here.
    assert!(
        keyed.values().all(|n| *n == 1),
        "only the lost first chunk goes again: {keyed:?}\n{trace}"
    );
}
