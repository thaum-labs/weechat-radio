//! Five stations in a line on hf-poor, each hearing only its neighbours:
//! A(1) - B(2) - C(3) - D(4) - E(5). Unlike `chain_harness`, every frame
//! takes airtime on a serial modem queue, a station is deaf while it keys,
//! and two neighbours keyed over each other are lost at the station between
//! them (B and D at C), as on the live RF chain. Carrier sense is stuck
//! busy on the noise floor, so every queued frame waits a fixed time and
//! then keys anyway, which lines up neighbours triggered by the same frame.
//!
//! Lines start in the middle (C) and at the far end (E), so a fix that only
//! works for traffic flowing away from A cannot pass.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use wcr::config::Config;
use wcr::modes::Mode;
use wcr::proto::{Envelope, MsgType};
use wcr::sim::{LinkDrop, TimedLog, TimedOpts};

const CALLS: [&str; 5] = ["G4AAA", "M0BBB", "2E0CCC", "M0DDD", "G4EEE"];
/// One hf-poor (RDM-600S) frame on the air. The node's own timers (ACK cap,
/// relay slots, retry holds) are set for this airtime, so the harness keeps it.
const FRAME_MS: u64 = 4_000;
/// The live sim's "busy, transmitting anyway" after 30 s of carrier sense.
const CSMA_STUCK_MS: u64 = 30_000;
/// Longest a case may take: the originator's three retries and the ack wait.
const CASE_SECS: u64 = 900;
/// Invites walk four hops out to E on a quiet chain.
const BUSY_INVITE_SECS: u64 = 600;
/// Three group lines and a bulletin on one chain, after the last is sent.
const BUSY_CASE_SECS: u64 = 1_200;
/// Stations listen this long before the first line, as on a chain that has
/// been up a while: past the node's neighbour warm-up, with every first
/// beacon on the air.
const WARMUP_SECS: u64 = 300;
/// The beacon case on a channel that reads busy: two idle rounds.
const NOISY_CASE_SECS: u64 = 900;
/// Longest gap between two beacons on the air: the node's 10-minute ceiling,
/// plus the modem's carrier-sense hold and a little queue time.
const MAX_BEACON_GAP_SECS: u64 = 600 + 45;
/// An idle gap is a pace (60 s ± 15 s) plus 4 minutes. A beacon falling due
/// just after a neighbour's frame decoded waits one more pace.
const IDLE_GAP_MAX_SECS: u64 = 315 + 75 + 30;
/// Most copies of one line a middle relay keys: its relay, one repeat, and
/// one answer to the originator's retry.
const MIDDLE_RELAY_CAP: usize = 3;

/// Reserved ports are free again until the nodes bind them, so chains start
/// one at a time. Once up, the cases run side by side on their own air.
fn setup_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
    &LOCK
}

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wcr-timed-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
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

/// Shipping RF defaults on hf-poor: ACK dither, retry jitter and beacons on.
fn radio_cfg(callsign: &str, home: &Path, kiss: u16, ctrl: u16, irc: u16, status: u16) -> Config {
    let mut cfg = Config {
        callsign: callsign.into(),
        grid: "IO91WM".into(),
        mode: Mode::Radio,
        hub: wcr::config::HubConfig {
            url: String::new(),
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
    cfg.rf.frequency_khz = 7_074;
    cfg.normalize();
    cfg
}

async fn status_json(port: u16) -> Option<serde_json::Value> {
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/status"))
        .await
        .ok()?;
    resp.json().await.ok()
}

async fn wait_radio(ports: &[u16]) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        let mut ok = true;
        for p in ports {
            let label = status_json(*p)
                .await
                .and_then(|v| {
                    v.get("audio_label")
                        .and_then(|x| x.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_default();
            ok &= matches!(label.as_str(), "low" | "good" | "hot");
        }
        if ok {
            return;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    panic!("radio modems never reported a level");
}

fn hear_set(st: u64) -> HashSet<u64> {
    let mut h = HashSet::new();
    if st > 1 {
        h.insert(st - 1);
    }
    if st < CALLS.len() as u64 {
        h.insert(st + 1);
    }
    h
}

struct Client {
    lines: Arc<Mutex<Vec<String>>>,
    w: OwnedWriteHalf,
}

impl Client {
    async fn connect(port: u16, nick: &str) -> Self {
        let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        let (r, mut w) = stream.into_split();
        let burst = format!(
            "CAP LS\r\nNICK {nick}\r\nUSER {nick} 0 * :wcr chain\r\nCAP REQ :message-tags echo-message server-time msgid\r\nCAP END\r\n"
        );
        w.write_all(burst.as_bytes()).await.unwrap();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(r);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => sink.lock().unwrap().push(line.trim_end().to_string()),
                }
            }
        });
        Self { lines, w }
    }

    async fn send(&mut self, line: &str) {
        self.w
            .write_all(format!("{line}\r\n").as_bytes())
            .await
            .unwrap();
    }

    fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }

    fn shows(&self, body: &str) -> usize {
        self.lines()
            .iter()
            .filter(|l| privmsg_body(l).is_some_and(|b| b.contains(body)))
            .count()
    }

    fn tags(&self) -> Vec<String> {
        self.lines()
            .into_iter()
            .filter(|l| l.contains("+radio/delivery="))
            .collect()
    }
}

/// Text of a PRIVMSG line, without tags or prefix.
fn privmsg_body(line: &str) -> Option<&str> {
    let rest = if let Some(tagged) = line.strip_prefix('@') {
        tagged.split_once(' ')?.1
    } else {
        line
    };
    let after = rest.strip_prefix(':')?.split_once(' ')?.1;
    let after = after.strip_prefix("PRIVMSG ")?;
    Some(after.split_once(" :")?.1)
}

struct Chain {
    nodes: Vec<tokio::task::JoinHandle<wcr::error::Result<()>>>,
    clients: Vec<Client>,
    log: TimedLog,
    t0: Instant,
    status: Vec<u16>,
}

impl Chain {
    async fn start(group: Option<&str>) -> Self {
        Self::start_with(group, None).await
    }

    /// Like [`Chain::start`], with `drop_rx` losing chosen frames per link.
    async fn start_with(group: Option<&str>, drop_rx: Option<LinkDrop>) -> Self {
        Self::start_on(group, drop_rx, 0).await
    }

    /// Like [`Chain::start`], on a channel whose modems read `noise_pct`
    /// occupancy with nothing on the air.
    async fn start_noisy(noise_pct: i32) -> Self {
        Self::start_on(None, None, noise_pct).await
    }

    async fn start_on(group: Option<&str>, drop_rx: Option<LinkDrop>, noise_pct: i32) -> Self {
        let _setup = setup_lock().lock().await;
        let air = wcr::sim::SharedAir::new(0.0);
        let log: TimedLog = Arc::new(Mutex::new(Vec::new()));
        let opts = TimedOpts {
            frame_ms: FRAME_MS,
            count_tx: true,
            collide: true,
            csma_stuck_ms: CSMA_STUCK_MS,
            rung_airtime: false,
            noise_pct,
        };
        let mut prep = Vec::new();
        for (i, call) in CALLS.iter().enumerate() {
            let home = scratch(call);
            if let Some(name) = group {
                let store = wcr::store::Store::open(&home.join("store.db"), 72, 10_000).unwrap();
                let members: Vec<String> = CALLS.iter().map(|s| s.to_string()).collect();
                store.group_create(name, &members).unwrap();
            }
            let st = i as u64 + 1;
            let (kiss, ctrl) = wcr::sim::mock_modem73_timed(
                air.clone(),
                st,
                hear_set(st),
                drop_rx.clone(),
                opts,
                log.clone(),
            )
            .await
            .unwrap();
            prep.push((*call, home, kiss, ctrl));
        }
        let ports = reserve_ports(prep.len() * 2).await;
        let mut nodes = Vec::new();
        let mut irc = Vec::new();
        let mut status = Vec::new();
        for (i, (call, home, kiss, ctrl)) in prep.into_iter().enumerate() {
            let cfg = radio_cfg(call, &home, kiss, ctrl, ports[i * 2], ports[i * 2 + 1]);
            nodes.push(tokio::spawn(async move {
                wcr::node::run_node(cfg, false).await
            }));
            irc.push(ports[i * 2]);
            status.push(ports[i * 2 + 1]);
        }
        for p in &irc {
            wait_tcp(*p, 20).await;
        }
        wait_radio(&status).await;
        let mut clients = Vec::new();
        for (i, call) in CALLS.iter().enumerate() {
            clients.push(Client::connect(irc[i], call).await);
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
        let chain = Self {
            nodes,
            clients,
            log,
            t0: Instant::now(),
            status,
        };
        // A live chain has been up a while: every station has beaconed and
        // knows its neighbours before the first line.
        drop(_setup);
        tokio::time::sleep(Duration::from_secs(WARMUP_SECS)).await;
        chain
    }

    async fn join(&mut self, chan: &str) {
        for c in self.clients.iter_mut() {
            c.send(&format!("JOIN {chan}")).await;
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
    }

    /// Wait until `done` holds, polling every 500 ms.
    async fn wait(&self, secs: u64, done: impl Fn(&Self) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            if done(self) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        done(self)
    }

    /// Wait until no station has keyed anything but a beacon for `quiet`
    /// seconds, or `max` seconds pass.
    async fn wait_quiet(&self, quiet: u64, max: u64) {
        let deadline = Instant::now() + Duration::from_secs(max);
        while Instant::now() < deadline {
            let last = self
                .log
                .lock()
                .unwrap()
                .iter()
                .filter(|k| {
                    Envelope::decode(&k.payload).map_or(true, |e| e.kind != MsgType::Beacon)
                })
                .map(|k| k.at)
                .max();
            if last.is_none_or(|t| t.elapsed() >= Duration::from_secs(quiet)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// How many times each station keyed the line `id` (not its receipts).
    fn keys_of(&self, id: &str) -> Vec<(&'static str, usize)> {
        let log = self.log.lock().unwrap();
        CALLS
            .iter()
            .enumerate()
            .map(|(i, call)| {
                let n = log
                    .iter()
                    .filter(|k| k.station == i as u64 + 1 && !k.dropped)
                    .filter(|k| {
                        Envelope::decode(&k.payload)
                            .is_ok_and(|e| e.kind != MsgType::Ack && e.msg_id.hex() == id)
                    })
                    .count();
                (*call, n)
            })
            .collect()
    }

    fn dropped(&self) -> usize {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.dropped)
            .count()
    }

    fn trace(&self) -> String {
        let mut out = String::new();
        for k in self.log.lock().unwrap().iter() {
            let what = match Envelope::decode(&k.payload) {
                Ok(env) if env.kind == MsgType::Ack => format!(
                    "ack {} by {} of {} hops {}",
                    env.msg_id.hex(),
                    env.origin,
                    env.acked_id().map(|i| i.hex()).unwrap_or_default(),
                    env.hops_left
                ),
                Ok(env) if env.kind == MsgType::Beacon => format!("beacon {}", env.origin),
                Ok(env) => format!(
                    "{:?} {} {}>{} hops {} {}",
                    env.kind,
                    env.msg_id.hex(),
                    env.origin,
                    env.dest,
                    env.hops_left,
                    env.body_text().chars().take(24).collect::<String>()
                ),
                Err(_) => format!("{} B", k.payload.len()),
            };
            out.push_str(&format!(
                "\n  {:7.1}s {} cap={} {what}{}",
                k.at.saturating_duration_since(self.t0).as_secs_f64(),
                CALLS[(k.station - 1) as usize],
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

/// Delivery states for one line, in the order the sender's client got them.
fn marks_for(client: &Client, id: &str) -> Vec<String> {
    client
        .tags()
        .iter()
        .filter(|l| l.contains(&format!("+radio/msgid={id}")))
        .filter_map(|l| {
            let rest = l.split("+radio/delivery=").nth(1)?;
            Some(rest.split([';', ' ']).next()?.to_string())
        })
        .collect()
}

/// The msgid of the first line `client` sent.
fn first_msgid(client: &Client) -> Option<String> {
    client.tags().iter().find_map(|l| {
        let rest = l.split("+radio/msgid=").nth(1)?;
        Some(rest.split([';', ' ']).next()?.to_string())
    })
}

/// Marks only move forward: `retry-n` only before anyone carried or
/// answered the line, and no `waiting`. `[no]` never comes after `[ok]`, and
/// only a real receipt follows it: on a busy chain a receipt can arrive after
/// the last wait, and the store lets it lift `[no]` to `[ok]`.
fn marks_move_forward(marks: &[String]) -> bool {
    let rank = |m: &str| match m {
        "sent" => Some(1),
        m if m.starts_with("retry-") => Some(1),
        "relayed" => Some(2),
        "gateway" => Some(3),
        "delivered" => Some(5),
        "all" => Some(6),
        _ => None,
    };
    let mut best = 0;
    let mut failed = false;
    for m in marks {
        if m == "failed" {
            if best >= 5 || failed {
                return false;
            }
            failed = true;
            continue;
        }
        let Some(r) = rank(m) else {
            return false;
        };
        if r < best || (failed && r < 5) {
            return false;
        }
        best = r;
    }
    true
}

#[test]
fn marks_check_takes_a_late_receipt_after_no() {
    let forward = |s: &str| marks_move_forward(&s.split(' ').map(String::from).collect::<Vec<_>>());
    assert!(forward("sent relayed delivered all"));
    assert!(forward("sent retry-3/3 relayed failed"));
    // A's invite to E in the busy replay on #48's msrv run.
    assert!(forward(
        "sent retry-1/3 retry-2/3 retry-3/3 relayed failed delivered"
    ));
    assert!(forward("sent failed delivered all"));
    assert!(!forward("sent delivered failed"));
    assert!(!forward("sent failed relayed"));
    assert!(!forward("sent failed retry-3/3"));
    assert!(!forward("sent relayed sent"));
}

/// Every station but `from` shows `body` exactly once.
fn seen_once(chain: &Chain, from: usize, body: &str) -> Vec<(&'static str, usize)> {
    chain
        .clients
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != from)
        .map(|(i, c)| (CALLS[i], c.shows(body)))
        .collect()
}

/// The line reaches every station once and its sender gets `[ok]`. `[all]`
/// is reported, not held: under blind keying the two neighbours of a station
/// can answer the same retry at once, and a far receipt can be lost on every
/// round (G3 about 1 run in 9, G2 rarer). `chain_harness` and
/// `delivery_paths` hold `[all]` itself.
async fn group_line_reaches_everyone(from: usize, body: &str) {
    let mut chain = Chain::start(Some("net")).await;
    chain.join("#net").await;
    chain.clients[from]
        .send(&format!("PRIVMSG #net :{body}"))
        .await;
    let reached = |c: &Chain, mark: &str| {
        c.clients[from]
            .lines()
            .iter()
            .any(|l| l.contains(&format!("+radio/delivery={mark}")))
    };
    let answered = |c: &Chain| reached(c, "all") || reached(c, "delivered");
    let got_ok = chain
        .wait(CASE_SECS, |c| {
            answered(c) && seen_once(c, from, body).iter().all(|(_, n)| *n >= 1)
        })
        .await;
    // Let every relay finish, so a station that keeps repeating is counted.
    chain.wait_quiet(90, 600).await;
    let seen = seen_once(&chain, from, body);
    let tags = chain.clients[from].tags();
    let id = first_msgid(&chain.clients[from]).unwrap_or_default();
    let marks = marks_for(&chain.clients[from], &id);
    let keys = chain.keys_of(&id);
    let dropped = chain.dropped();
    let trace = chain.trace();
    eprintln!("{trace}\n  marks {marks:?}\n  keys {keys:?}");
    chain.stop();
    assert!(
        seen.iter().all(|(_, n)| *n == 1),
        "{} line not shown once everywhere: {seen:?}{trace}",
        CALLS[from]
    );
    assert!(
        got_ok,
        "{} never reached [ok]: {tags:?}{trace}",
        CALLS[from]
    );
    assert!(
        !tags.iter().any(|l| l.contains("+radio/delivery=failed")),
        "false [no]: {tags:?}"
    );
    assert!(marks_move_forward(&marks), "marks moved back: {marks:?}");
    assert_relays_capped(from, &keys, &trace);
    assert_eq!(
        dropped, 0,
        "modem dropped a frame wider than its mode{trace}"
    );
}

/// A station at the end of the chain has nobody to pass a line on to: one
/// relay at most. A relay in the middle stops once its neighbours have it.
fn assert_relays_capped(from: usize, keys: &[(&'static str, usize)], trace: &str) {
    for (i, (call, n)) in keys.iter().enumerate() {
        if i == from {
            continue;
        }
        let cap = if i == 0 || i == CALLS.len() - 1 {
            1
        } else {
            MIDDLE_RELAY_CAP
        };
        assert!(
            *n <= cap,
            "{call} keyed the line {n} times (cap {cap}): {keys:?}{trace}"
        );
    }
}

/// G1 on the live chain: A keys a group line and it reaches everyone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_line_from_the_near_end_reaches_all() {
    group_line_reaches_everyone(0, "g1-from-the-near-end").await;
}

/// G1 when B has never heard C (every C beacon lost at B, as on ubuntu CI
/// for #45) and B's first relay is lost at C. B hears only the originator,
/// so it cannot tell it is not the end of the chain: it keeps relaying.
/// Before the fix it relayed once and dropped A's retries. Delivery is
/// reported, not held: C's and D's idle beacons can still take B's later
/// copies, or A's copies at B.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_relay_that_never_heard_beyond_keeps_relaying() {
    let body = "g1-past-an-unheard-neighbour";
    let copies_to_c = AtomicU64::new(0);
    let drop_rx: LinkDrop = Arc::new(move |from, to, frame| {
        let Ok(env) = Envelope::decode(frame) else {
            return false;
        };
        match (from, to) {
            (3, 2) => env.kind == MsgType::Beacon,
            (2, 3) => {
                env.kind == MsgType::Msg
                    && env.body_text().contains(body)
                    && copies_to_c.fetch_add(1, Ordering::SeqCst) == 0
            }
            _ => false,
        }
    });
    let mut chain = Chain::start_with(Some("net"), Some(drop_rx)).await;
    chain.join("#net").await;
    chain.clients[0]
        .send(&format!("PRIVMSG #net :{body}"))
        .await;
    chain
        .wait(CASE_SECS, |c| {
            seen_once(c, 0, body).iter().all(|(_, n)| *n >= 1)
        })
        .await;
    chain.wait_quiet(90, 600).await;
    let seen = seen_once(&chain, 0, body);
    let id = first_msgid(&chain.clients[0]).unwrap_or_default();
    let marks = marks_for(&chain.clients[0], &id);
    let keys = chain.keys_of(&id);
    let dropped = chain.dropped();
    let trace = chain.trace();
    eprintln!("{trace}\n  seen {seen:?}\n  marks {marks:?}\n  keys {keys:?}");
    chain.stop();
    // 0: A's copies never reached B, so the case did not happen.
    let b_keys = keys[1].1;
    assert!(b_keys != 1, "B relayed once and stopped: {keys:?}{trace}");
    assert!(
        seen.iter().all(|(_, n)| *n <= 1),
        "line shown twice: {seen:?}{trace}"
    );
    assert!(marks_move_forward(&marks), "marks moved back: {marks:?}");
    assert_relays_capped(0, &keys, &trace);
    assert_eq!(
        dropped, 0,
        "modem dropped a frame wider than its mode{trace}"
    );
}

/// G3 on the live chain: C keys a group line, B and D both relay it and
/// both answer at once. Their receipts collide at C.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_line_from_the_middle_reaches_all() {
    group_line_reaches_everyone(2, "g3-from-the-middle").await;
}

/// G2 on the live chain: E's line reached D and C only, and B and A never
/// saw it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn group_line_from_the_far_end_reaches_all() {
    group_line_reaches_everyone(4, "g2-from-the-far-end").await;
}

/// B2 on the live chain: E's bulletin reached D only.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bulletin_from_the_far_end_reaches_the_other_end() {
    let mut chain = Chain::start(None).await;
    chain.join("#bulletin").await;
    let body = "b2-from-the-far-end";
    chain.clients[4]
        .send(&format!("PRIVMSG #bulletin :{body}"))
        .await;
    let ok = chain
        .wait(CASE_SECS, |c| {
            seen_once(c, 4, body).iter().all(|(_, n)| *n >= 1)
        })
        .await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    let seen = seen_once(&chain, 4, body);
    let dropped = chain.dropped();
    let trace = chain.trace();
    eprintln!("{trace}");
    chain.stop();
    assert!(ok, "bulletin from E did not reach A: {seen:?}{trace}");
    assert!(
        seen.iter().all(|(_, n)| *n == 1),
        "duplicate bulletin lines: {seen:?}"
    );
    assert_eq!(
        dropped, 0,
        "modem dropped a frame wider than its mode{trace}"
    );
}

/// Distinct msgids of `client`'s own lines, in the order first tagged.
fn own_msgids(client: &Client) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for l in client.tags() {
        let Some(rest) = l.split("+radio/msgid=").nth(1) else {
            continue;
        };
        let id = rest
            .split([';', ' '])
            .next()
            .unwrap_or_default()
            .to_string();
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Send `line` from client `i` and return the msgid its first delivery tag
/// carries: the first of its own msgids not seen before.
async fn send_tracked(chain: &mut Chain, i: usize, line: &str) -> String {
    let before = own_msgids(&chain.clients[i]);
    chain.clients[i].send(line).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if let Some(id) = own_msgids(&chain.clients[i])
            .into_iter()
            .find(|id| !before.contains(id))
        {
            return id;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    String::new()
}

/// Members in the last `NAMES` reply for `chan`.
fn names_reply(client: &Client, chan: &str) -> Vec<String> {
    let mut names = Vec::new();
    for l in client.lines() {
        let mut parts = l.splitn(2, " 353 ");
        let (_, Some(rest)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !rest.contains(&format!(" {chan} :")) {
            continue;
        }
        names = rest
            .split_once(" :")
            .map(|(_, n)| {
                n.split_whitespace()
                    .map(|s| s.trim_start_matches(['@', '+']).to_ascii_uppercase())
                    .collect()
            })
            .unwrap_or_default();
    }
    names.sort();
    names
}

/// The v0.1.90 live retest: nothing pre-seeded. A opens #qa and invites
/// B..E over the air, and everyone joins. Group lines from A, C and E go out
/// two minutes apart, then a bulletin from A. E is four hops from A, past a
/// direct message's three.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn busy_chain_with_invites_reaches_everyone() {
    let mut chain = Chain::start(None).await;
    chain.join("#bulletin").await;
    chain.clients[0].send("JOIN #qa").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    for call in &CALLS[1..] {
        chain.clients[0].send(&format!("INVITE {call} #qa")).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    for c in chain.clients[1..].iter_mut() {
        c.send("JOIN #qa").await;
    }
    let invited = |c: &Chain| {
        c.clients[1..]
            .iter()
            .filter(|cl| got_invite(cl, "#qa"))
            .count()
    };
    // Give the invites a head start. Under this load one can still be lost
    // on every copy; `invite_reaches_the_far_end` holds the hop budget.
    chain
        .wait(BUSY_INVITE_SECS, |c| invited(c) == CALLS.len() - 1)
        .await;

    let lines = [
        (0, "qa-g1-from-a"),
        (2, "qa-g3-from-c"),
        (4, "qa-g2-from-e"),
    ];
    let mut ids = Vec::new();
    for (from, body) in lines {
        ids.push(send_tracked(&mut chain, from, &format!("PRIVMSG #qa :{body}")).await);
        tokio::time::sleep(Duration::from_secs(120)).await;
    }
    let bulletin = "qa-b1-from-a";
    let bulletin_id = send_tracked(&mut chain, 0, &format!("PRIVMSG #bulletin :{bulletin}")).await;

    // Wait for every line shown everywhere and every group line answered,
    // then let late receipts settle. This is a soak replay of the live
    // retest: under this load any copy can be lost on every try (an invite,
    // a bulletin's first hop, a far receipt), so delivery, invites, [ok] and
    // [all] are reported. Held: marks only move forward, member lists match,
    // no line shows twice, and the ends of the chain relay a line once. The
    // quiet-chain cases above hold delivery.
    let done = |c: &Chain| {
        lines.iter().zip(&ids).all(|((from, body), id)| {
            seen_once(c, *from, body).iter().all(|(_, n)| *n >= 1)
                && marks_for(&c.clients[*from], id)
                    .iter()
                    .any(|m| m == "delivered" || m == "all")
        }) && seen_once(c, 0, bulletin).iter().all(|(_, n)| *n >= 1)
    };
    chain.wait(BUSY_CASE_SECS, done).await;
    chain.wait_quiet(90, 600).await;
    for c in chain.clients.iter_mut() {
        c.send("NAMES #qa").await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;

    let trace = chain.trace();
    let mut report = String::new();
    let mut problems = Vec::new();
    report.push_str(&format!(
        "\n  invites arrived {}/{}",
        invited(&chain),
        CALLS.len() - 1
    ));
    let all_ids = ids.iter().cloned().chain([bulletin_id]);
    for ((from, body), id) in lines.iter().copied().chain([(0, bulletin)]).zip(all_ids) {
        let seen = seen_once(&chain, from, body);
        let marks = marks_for(&chain.clients[from], &id);
        let keys = chain.keys_of(&id);
        report.push_str(&format!(
            "\n  {body}: seen {seen:?} marks {marks:?} keys {keys:?}"
        ));
        if seen.iter().any(|(_, n)| *n > 1) {
            problems.push(format!("{body} shown twice somewhere"));
        }
        for (i, (call, n)) in keys.iter().enumerate() {
            let end = i == 0 || i == CALLS.len() - 1;
            if i != from && end && *n > 1 {
                problems.push(format!("{call} at the chain end keyed {body} {n} times"));
            }
        }
    }
    for (i, c) in chain.clients.iter().enumerate() {
        for id in own_msgids(c) {
            let marks = marks_for(c, &id);
            if !marks_move_forward(&marks) {
                problems.push(format!("{} marks moved back on {id}: {marks:?}", CALLS[i]));
            }
        }
        let names = names_reply(c, "#qa");
        report.push_str(&format!("\n  {} NAMES #qa {names:?}", CALLS[i]));
        if names.len() != CALLS.len() {
            problems.push(format!("{} lists {names:?}", CALLS[i]));
        }
    }
    // Receipts B carried to A. B is next to A, so each goes once unless A
    // retried its line. Reported: QA v0.1.92 saw four or five of each.
    let to_a: Vec<usize> = {
        let log = chain.log.lock().unwrap();
        let mut per = std::collections::HashMap::<String, usize>::new();
        for k in log.iter().filter(|k| k.station == 2 && !k.dropped) {
            if let Ok(e) = Envelope::decode(&k.payload) {
                if e.kind == MsgType::Ack
                    && e.dest.as_str() == CALLS[0]
                    && e.origin.as_str() != CALLS[1]
                {
                    *per.entry(e.msg_id.hex()).or_default() += 1;
                }
            }
        }
        let mut v: Vec<usize> = per.into_values().collect();
        v.sort_unstable();
        v
    };
    report.push_str(&format!("\n  B keyed each far receipt to A {to_a:?} times"));
    // A roster queued behind an ARQ copy on a narrow rung can be dropped by
    // the modem when the mode switches. Reported here, not held to zero.
    report.push_str(&format!("\n  dropped {}", chain.dropped()));
    eprintln!("{trace}{report}");
    chain.stop();
    assert!(problems.is_empty(), "{problems:?}{report}{trace}");
}

/// The client got an over-air INVITE to `chan`.
fn got_invite(client: &Client, chan: &str) -> bool {
    client
        .lines()
        .iter()
        .any(|l| l.contains(" INVITE ") && l.contains(chan))
}

/// A→E is four hops, one past a direct message's three. An invite carries
/// its group's six, so E gets it and E's receipt comes back to A. No copy is
/// lost at A's own modem.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invite_reaches_the_far_end() {
    let mut chain = Chain::start(None).await;
    chain.clients[0].send("JOIN #qa").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let id = send_tracked(&mut chain, 0, "INVITE G4EEE #qa").await;
    let ok = chain
        .wait(CASE_SECS, |c| {
            got_invite(&c.clients[4], "#qa")
                && marks_for(&c.clients[0], &id)
                    .iter()
                    .any(|m| m == "delivered")
        })
        .await;
    let marks = marks_for(&chain.clients[0], &id);
    let dropped = chain.dropped();
    let trace = chain.trace();
    eprintln!("{trace}\n  marks {marks:?}");
    chain.stop();
    assert!(
        ok,
        "E never got the invite, or A never got E's receipt: {marks:?}{trace}"
    );
    assert!(marks_move_forward(&marks), "marks moved back: {marks:?}");
    // QA v0.1.93: the invite's first copy waited out the stuck carrier sense
    // at A's modem while a roster repeat switched it to MFSK, and was dropped.
    assert_eq!(
        dropped, 0,
        "modem dropped a frame wider than its mode{trace}"
    );
}

/// QA could not score the beacon pace on v0.1.92: the countdown looked as if
/// it reset every minute. On a quiet chain at hf-poor timing, each station's
/// first idle beacon after the warm-up (the first countdown more than 2
/// minutes ahead; warm-up and busy holds are never that long) must key within
/// the carrier-sense hold of the countdown showing when it fell due. One that
/// falls due while a neighbour is on the air waits a pace, and its countdown
/// says so (`busy`). Nothing keys before the countdown, and the beacon comes
/// at the idle pace (about 5 minutes after the one before).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn each_beacon_keys_when_its_countdown_says() {
    let chain = Chain::start(None).await;
    let sampled_at = Instant::now();
    let slack = Duration::from_secs(2);
    let beacons = |st: usize| -> Vec<Instant> {
        chain
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.station == st as u64 + 1 && !k.dropped)
            .filter(|k| Envelope::decode(&k.payload).is_ok_and(|e| e.kind == MsgType::Beacon))
            .map(|k| k.at)
            .collect()
    };
    // Every countdown each station showed: when we read it, when it said the
    // beacon was due, and its note.
    let mut seen: Vec<Vec<(Instant, Instant, String)>> = vec![Vec::new(); CALLS.len()];
    let idle = |(at, due, _): &(Instant, Instant, String)| *due > *at + Duration::from_secs(120);
    let give_up = sampled_at + Duration::from_secs(900);
    loop {
        for (i, p) in chain.status.iter().enumerate() {
            let Some(snap) = status_json(*p).await else {
                continue;
            };
            let due = snap.get("beacon_due").and_then(|x| x.as_u64()).unwrap_or(0);
            if due == 0 {
                continue;
            }
            let at = Instant::now();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let due_at = if due >= now {
                at + Duration::from_secs(due - now)
            } else {
                at - Duration::from_secs(now - due)
            };
            let note = snap
                .get("beacon_note")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            seen[i].push((at, due_at, note));
        }
        let all_keyed = (0..CALLS.len()).all(|i| {
            seen[i]
                .iter()
                .find(|s| idle(s))
                .is_some_and(|(_, d, _)| beacons(i).iter().any(|t| *t + slack >= *d))
        });
        if all_keyed || Instant::now() > give_up {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let mut report = String::new();
    let mut problems = Vec::new();
    for (i, call) in CALLS.iter().enumerate() {
        let keyed = beacons(i);
        let warm = keyed.iter().filter(|t| **t < sampled_at).count();
        let Some(&(shown_at, due_at, _)) = seen[i].iter().find(|s| idle(s)) else {
            problems.push(format!("{call} showed no idle countdown"));
            continue;
        };
        let ahead = due_at.saturating_duration_since(shown_at).as_secs();
        // The beacon the first idle countdown names, and the one keyed before
        // it. A beacon handed to the modem just before that countdown showed
        // can still key up to the carrier-sense hold later; only after that is
        // a beacon ahead of the countdown early.
        let next = keyed.iter().find(|t| **t + slack >= due_at).copied();
        let prev = keyed.iter().rev().find(|t| **t + slack < due_at).copied();
        let in_flight = shown_at + Duration::from_secs(60);
        let early = keyed.iter().any(|t| *t >= in_flight && *t + slack < due_at);
        // The countdown showing when it fell due: the latest one due by the
        // time it keyed (a busy hold moves it on; the next round's is later).
        let named = next.and_then(|n| {
            seen[i]
                .iter()
                .map(|(_, d, _)| *d)
                .filter(|d| *d <= n + slack)
                .max()
        });
        let busy = seen[i]
            .iter()
            .any(|(_, d, note)| note == "busy" && *d + slack >= due_at);
        let off = next
            .zip(named)
            .map(|(n, d)| n.saturating_duration_since(d).as_secs_f64());
        let gap = next.zip(prev).map(|(n, l)| n.duration_since(l).as_secs());
        let held = if busy { ", held busy" } else { "" };
        report.push_str(&format!(
            "\n  {call}: {warm} warm-up beacons, countdown {ahead}s ahead{held}, keyed {off:?}s after it, idle gap {gap:?}s"
        ));
        if !(2..=4).contains(&warm) {
            problems.push(format!("{call} keyed {warm} beacons in the warm-up"));
        }
        match off {
            None => problems.push(format!("{call} never keyed the beacon its countdown named")),
            Some(s) if s > 60.0 => {
                problems.push(format!("{call} keyed {s:.0}s after its countdown"))
            }
            _ => {}
        }
        if early {
            problems.push(format!("{call} keyed a beacon before its countdown"));
        }
        // A busy hold adds a pace (at most 75 s) to the idle gap.
        let max_gap = if busy { 500 } else { 420 };
        if gap.is_some_and(|g| !(240..=max_gap).contains(&g)) {
            problems.push(format!("{call} idle gap {gap:?}s, not about 5 minutes"));
        }
    }
    let trace = chain.trace();
    chain.stop();
    eprintln!("{report}");
    assert!(problems.is_empty(), "{problems:?}{report}{trace}");
}

/// QA v0.1.93: every modem on the live chain read 99% busy for 45 minutes,
/// quiet spells included, and no station keyed a single beacon. Each one fell
/// due, was held a pace for the busy channel, then held again. On a quiet
/// chain whose modems read 99% from the noise floor, with carrier sense stuck
/// busy, every station must still beacon through the warm-up, then about
/// every 5 minutes, and never more than 10 minutes apart.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn beacons_go_on_a_channel_that_reads_busy() {
    let chain = Chain::start_noisy(99).await;
    let warm_end = Instant::now();
    let beacons = |st: usize| -> Vec<Instant> {
        chain
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.station == st as u64 + 1 && !k.dropped)
            .filter(|k| Envelope::decode(&k.payload).is_ok_and(|e| e.kind == MsgType::Beacon))
            .map(|k| k.at)
            .collect()
    };
    let idle_keyed = |st: usize| beacons(st).iter().filter(|t| **t > warm_end).count();
    chain
        .wait(NOISY_CASE_SECS, |_| {
            (0..CALLS.len()).all(|i| idle_keyed(i) >= 2)
        })
        .await;
    let mut report = String::new();
    let mut problems = Vec::new();
    for (i, call) in CALLS.iter().enumerate() {
        let keyed = beacons(i);
        let warm = keyed.iter().filter(|t| **t <= warm_end).count();
        let gaps: Vec<u64> = keyed
            .windows(2)
            .map(|w| w[1].duration_since(w[0]).as_secs())
            .collect();
        // Gaps that end after the harness warm-up. The node's own warm-up
        // beacons go a pace apart in its first 3 minutes, and each keys within
        // the 30 s carrier-sense hold, so all of them are on the air by then.
        let idle: Vec<u64> = keyed
            .windows(2)
            .filter(|w| w[1] > warm_end)
            .map(|w| w[1].duration_since(w[0]).as_secs())
            .collect();
        report.push_str(&format!(
            "\n  {call}: {warm} warm-up beacons, {} after, gaps {gaps:?}s",
            keyed.len() - warm
        ));
        if !(2..=4).contains(&warm) {
            problems.push(format!("{call} keyed {warm} beacons in the warm-up"));
        }
        if keyed.len() - warm < 2 {
            problems.push(format!(
                "{call} keyed {} beacons after the warm-up",
                keyed.len() - warm
            ));
        }
        if let Some(g) = gaps.iter().find(|g| **g > MAX_BEACON_GAP_SECS) {
            problems.push(format!("{call} went {g}s between beacons"));
        }
        if idle.iter().any(|g| !(240..=IDLE_GAP_MAX_SECS).contains(g)) {
            problems.push(format!("{call} idle gaps {idle:?}s, not about 5 minutes"));
        }
    }
    let trace = chain.trace();
    chain.stop();
    eprintln!("{report}");
    assert!(problems.is_empty(), "{problems:?}{report}{trace}");
}
