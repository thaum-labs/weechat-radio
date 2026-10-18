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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use wcr::config::Config;
use wcr::modes::Mode;
use wcr::proto::{Callsign, Envelope, Flags, MsgType, FLAG_GROUP, FLAG_REQ_ACK};
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
/// A warm-up gap is a pace (60 s ± 15 s) and the air queue's carrier-sense
/// backoff.
const WARMUP_GAP_MAX_SECS: u64 = 120;
/// An idle gap is a pace (60 s ± 15 s) plus 4 minutes. The modem's stuck
/// carrier sense moves each key by up to 30 s, and a beacon falling due just
/// after a neighbour's frame decoded waits up to 10 s.
const IDLE_GAP_MAX_SECS: u64 = 420;
/// QA v0.1.95 saw quiet gaps of 285 s. Jitter only adds to the 5 minutes,
/// each counted from when the last beacon reached the modem; the beacon loop
/// looks once a second.
const IDLE_GAP_MIN_SECS: u64 = 298;
/// Most copies of one line a middle relay keys: its relay, one repeat, and
/// one answer to the originator's retry.
const MIDDLE_RELAY_CAP: usize = 3;
/// Longest the traffic case waits for every station's fifth beacon: the
/// warm-up, then two idle rounds after E's line.
const TRAFFIC_PACE_SECS: u64 = 1_380;
/// With traffic on the chain a due beacon waits out one exchange at most
/// (about 100 s at hf-poor), and can then queue behind our own frames. Its
/// gaps stay under 9 minutes: v0.1.94 beaconed only at the 10-minute ceiling.
const TRAFFIC_GAP_MAX_SECS: u64 = 540;
/// A warm-up gap with our own lines queued and awaiting receipts: a pace
/// (60 s), one exchange waited out (about 100 s at hf-poor), the frame
/// already at the modem with its 30 s carrier-sense hold, and up to two
/// frames' wait on a congested channel. Not the queue behind it.
const WARMUP_QUEUE_GAP_MAX_SECS: u64 = 240;

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

    /// Like [`Chain::start_on`], returning as soon as the stations are up,
    /// before any of them has beaconed.
    async fn start_cold(group: Option<&str>, noise_pct: i32) -> Self {
        Self::start_at(group, None, noise_pct, false).await
    }

    async fn start_on(group: Option<&str>, drop_rx: Option<LinkDrop>, noise_pct: i32) -> Self {
        Self::start_at(group, drop_rx, noise_pct, true).await
    }

    async fn start_at(
        group: Option<&str>,
        drop_rx: Option<LinkDrop>,
        noise_pct: i32,
        warm: bool,
    ) -> Self {
        Self::start_modems(group, drop_rx, noise_pct, warm, false).await
    }

    /// Like [`Chain::start_at`]. `rx_after_decode` has each modem report
    /// `rx` only just after a frame decodes, as upstream modem73 does.
    async fn start_modems(
        group: Option<&str>,
        drop_rx: Option<LinkDrop>,
        noise_pct: i32,
        warm: bool,
        rx_after_decode: bool,
    ) -> Self {
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
            rx_after_decode,
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
        if warm {
            tokio::time::sleep(Duration::from_secs(WARMUP_SECS)).await;
        }
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
        trace_of(&self.log, self.t0)
    }

    fn stop(self) {
        for n in self.nodes {
            n.abort();
        }
    }
}

/// Every frame keyed on `log`, with its time from `t0`.
fn trace_of(log: &TimedLog, t0: Instant) -> String {
    let mut out = String::new();
    for k in log.lock().unwrap().iter() {
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
        // A frame the modem held shows how long: a receipt held its whole
        // carrier-sense time keys late, next to its own repeat.
        let held = k.at.saturating_duration_since(k.handed).as_secs_f64();
        out.push_str(&format!(
            "\n  {:7.1}s {} cap={} {what}{}{}",
            k.at.saturating_duration_since(t0).as_secs_f64(),
            CALLS[(k.station - 1) as usize],
            k.cap,
            if held >= 1.0 {
                format!(" (held {held:.0}s)")
            } else {
                String::new()
            },
            if k.dropped { " DROPPED" } else { "" }
        ));
    }
    out
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
    let chain_log = chain.log.clone();
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
    assert_receipts_not_doubled(&chain_log, from, &id, &trace);
    assert_eq!(
        dropped, 0,
        "modem dropped a frame wider than its mode{trace}"
    );
}

/// QA v0.1.94: B passed on 10 receipts from further down the chain for one
/// group line. A receipt a station carries goes again only a retry interval
/// after its last copy, never because another copy of it came in, and next
/// to the originator at most twice (its relay and one repeat), plus once
/// for each of the originator's copies keyed after it.
fn assert_receipts_not_doubled(log: &TimedLog, from: usize, line: &str, trace: &str) {
    let log = log.lock().unwrap();
    let origin = from as u64 + 1;
    let line_keys: Vec<Instant> = log
        .iter()
        .filter(|k| k.station == origin && !k.dropped)
        .filter(|k| {
            Envelope::decode(&k.payload)
                .is_ok_and(|e| e.kind != MsgType::Ack && e.msg_id.hex() == line)
        })
        .map(|k| k.at)
        .collect();
    for (i, call) in CALLS.iter().enumerate() {
        let st = i as u64 + 1;
        let mut carried: HashMap<String, (String, Vec<Instant>)> = HashMap::new();
        for k in log.iter().filter(|k| k.station == st && !k.dropped) {
            let Ok(e) = Envelope::decode(&k.payload) else {
                continue;
            };
            if e.kind != MsgType::Ack
                || e.origin.as_str() == *call
                || e.acked_id().map(|id| id.hex()).as_deref() != Some(line)
            {
                continue;
            }
            carried
                .entry(e.msg_id.hex())
                .or_insert_with(|| (e.origin.to_string(), Vec::new()))
                .1
                .push(k.at);
        }
        for (by, times) in carried.values() {
            for w in times.windows(2) {
                let gap = w[1].duration_since(w[0]).as_secs_f64();
                assert!(
                    gap >= 30.0,
                    "{call} keyed {by}'s receipt twice {gap:.1}s apart{trace}"
                );
            }
            if st.abs_diff(origin) == 1 {
                let asked = line_keys.iter().filter(|t| **t > times[0]).count();
                assert!(
                    times.len() <= 2 + asked,
                    "{call} carried {by}'s receipt {} times, with {asked} copies of the line after its first{trace}",
                    times.len()
                );
            }
        }
    }
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

/// G1 on ubuntu CI for the v0.1.94 release. Each of D's beacons was lost at
/// C (C keying, or B keying over D), so C knew only B. B's modem keyed its
/// relay of A's line and its own receipt back to back. The receipt reached C
/// while C's first relay was still in its air queue, and C, taking itself
/// for the end of the chain, dropped that copy: D and E never got the line.
/// Only C runs here, as a relay outside the group, so it keys no receipt of
/// its own over B's (on main CI C's receipt did, C never learnt B, and it
/// rightly kept relaying). B's relay is played to it, then B's receipt once
/// C's copy is queued. C's modem reads its noise floor busy, so that copy
/// waits out a congested slot of at least 2 s and the receipt, a second
/// later, lands while it is still queued. At the end of a chain it is one
/// relay and no repeats.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_relay_that_knows_one_neighbour_still_relays_once() {
    let setup = setup_lock().lock().await;
    let air = wcr::sim::SharedAir::new(0.0);
    let log: TimedLog = Arc::new(Mutex::new(Vec::new()));
    let opts = TimedOpts {
        frame_ms: FRAME_MS,
        count_tx: true,
        collide: true,
        csma_stuck_ms: 0,
        rung_airtime: false,
        noise_pct: 99,
        rx_after_decode: false,
    };
    let c = 3;
    let home = scratch(CALLS[2]);
    let store = wcr::store::Store::open(&home.join("store.db"), 72, 10_000).unwrap();
    let members: Vec<String> = CALLS
        .iter()
        .filter(|s| **s != CALLS[2])
        .map(|s| s.to_string())
        .collect();
    store.group_create("net", &members).unwrap();
    drop(store);
    let (kiss, ctrl) =
        wcr::sim::mock_modem73_timed(air.clone(), c, hear_set(c), None, opts, log.clone())
            .await
            .unwrap();
    let ports = reserve_ports(2).await;
    let cfg = radio_cfg(CALLS[2], &home, kiss, ctrl, ports[0], ports[1]);
    let node = tokio::spawn(async move { wcr::node::run_node(cfg, false).await });
    wait_tcp(ports[0], 20).await;
    wait_radio(&ports[1..]).await;
    drop(setup);
    let t0 = Instant::now();

    // C uses its neighbour list once every neighbour has had time to beacon.
    // B's frames are played just after one of C's own beacons, while C
    // listens: its next one is at least 45 s away, and none follows traffic.
    let warm = t0 + Duration::from_secs(u64::from(wcr::relay::NEIGHBOUR_WARMUP_S) + 5);
    let deadline = warm + Duration::from_secs(400);
    loop {
        let end = log
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.station == c)
            .map(|k| k.at + k.len)
            .max();
        if end.is_some_and(|e| e > warm && Instant::now() >= e + Duration::from_secs(1)) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "C never beaconed after its warm-up{}",
            trace_of(&log, t0)
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let line = Envelope::new_msg(
        Callsign::parse(CALLS[0]).unwrap(),
        Callsign::from_raw("NET"),
        1,
        b"g1-past-a-neighbour-never-heard".to_vec(),
        6,
        Flags::new().with(FLAG_REQ_ACK).with(FLAG_GROUP),
    )
    .unwrap();
    let mut relayed = line.clone();
    relayed.hops_left = 5;
    let mut receipt = Envelope::ack_for(&line, Callsign::parse(CALLS[1]).unwrap(), 1, None);
    receipt.hops_left = 6;
    let frame = Duration::from_millis(FRAME_MS);
    key_as(&air, &log, 2, &relayed, frame).await;
    let queued = Instant::now() + Duration::from_secs(15);
    loop {
        let depth = status_json(ports[1])
            .await
            .and_then(|v| v.get("queue_air").and_then(|x| x.as_u64()))
            .unwrap_or(0);
        if depth > 0 {
            break;
        }
        assert!(
            Instant::now() < queued,
            "C never queued its relay{}",
            trace_of(&log, t0)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    key_as(&air, &log, 2, &receipt, Duration::from_secs(1)).await;

    // Its slot and the congested wait, then long enough for a repeat a retry
    // interval later.
    let copies = || {
        log.lock()
            .unwrap()
            .iter()
            .filter(|k| {
                k.station == c
                    && Envelope::decode(&k.payload).is_ok_and(|e| e.msg_id == line.msg_id)
            })
            .count()
    };
    let deadline = Instant::now() + Duration::from_secs(90);
    while copies() == 0 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    tokio::time::sleep(Duration::from_secs(90)).await;
    let n = copies();
    let trace = trace_of(&log, t0);
    eprintln!("{trace}");
    node.abort();
    assert_eq!(n, 1, "C keyed the line {n} times, not one relay{trace}");
}

/// Play `env` as if `station` keyed it: the receivers hear the channel busy
/// for `len`, then get it.
async fn key_as(
    air: &wcr::sim::SharedAir,
    log: &TimedLog,
    station: u64,
    env: &Envelope,
    len: Duration,
) {
    let payload = wcr::sim::encode_over_air(env);
    log.lock().unwrap().push(wcr::sim::TimedKey {
        station,
        at: Instant::now(),
        handed: Instant::now(),
        payload: payload.clone(),
        csma: true,
        cap: 170,
        dropped: false,
        len,
    });
    tokio::time::sleep(len).await;
    air.send_from(station, payload);
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

/// G1 on main CI for v0.1.95: D had its own receipt on the air and its relay
/// of the line queued behind it when it handed over E's receipt. The line
/// moved the modem's frame counter, D turned carrier sense back on, and the
/// modem held E's receipt its whole 30 s: it keyed 24 s before D's repeat of
/// it. A receipt keeps carrier sense off until it has left the modem's
/// queue, so it keys straight after the frames ahead of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_receipt_behind_queued_frames_keys_straight_after_them() {
    use wcr::modem::{ControlClient, KissClient};

    async fn key_receipt(kiss: &KissClient, control: &ControlClient, frame: &[u8]) -> bool {
        let frames = [frame.to_vec()];
        wcr::air::transmit_ack(wcr::air::AckTx {
            frames: &frames,
            preset: wcr::presets::Preset::HfPoor,
            air: None,
            sense: None,
            control: Some(control),
            kiss: Some(kiss),
            cap: Duration::from_secs(3),
            slot: Duration::from_millis(100),
            quiet: Duration::from_millis(500),
            csma: true,
            drain_cap: Duration::from_secs(5),
        })
        .await
    }

    let log: TimedLog = Arc::new(Mutex::new(Vec::new()));
    let opts = TimedOpts {
        frame_ms: FRAME_MS,
        count_tx: true,
        collide: false,
        csma_stuck_ms: CSMA_STUCK_MS,
        rung_airtime: false,
        noise_pct: 99,
        rx_after_decode: false,
    };
    let air = wcr::sim::SharedAir::new(0.0);
    let (kiss_port, ctrl_port) =
        wcr::sim::mock_modem73_timed(air, 4, HashSet::new(), None, opts, log.clone())
            .await
            .unwrap();
    let (kiss, _heard) = KissClient::connect(&format!("127.0.0.1:{kiss_port}"))
        .await
        .unwrap();
    let (control, _events) = ControlClient::connect(&format!("127.0.0.1:{ctrl_port}"))
        .await
        .unwrap();
    let t0 = Instant::now();
    let (own, line, carried) = (
        b"D's own receipt".as_slice(),
        b"D's relay of the line".as_slice(),
        b"E's receipt carried by D".as_slice(),
    );
    assert!(key_receipt(&kiss, &control, own).await);
    kiss.send(line).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while control
        .get_status()
        .await
        .is_ok_and(|st| st.tx_queue != Some(1))
    {
        assert!(Instant::now() < deadline, "the line never queued");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(key_receipt(&kiss, &control, carried).await);
    let deadline = Instant::now() + Duration::from_secs(60);
    while log.lock().unwrap().len() < 3 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let keys = log.lock().unwrap().clone();
    let trace: String = keys
        .iter()
        .map(|k| {
            format!(
                "\n  {:6.1}s {} (handed over at {:.1}s, carrier sense {})",
                k.at.duration_since(t0).as_secs_f64(),
                String::from_utf8_lossy(&k.payload),
                k.handed.duration_since(t0).as_secs_f64(),
                if k.csma { "on" } else { "off" }
            )
        })
        .collect();
    let order: Vec<&[u8]> = keys.iter().map(|k| k.payload.as_slice()).collect();
    assert_eq!(order, vec![own, line, carried], "keyed out of order{trace}");
    let after = keys[2].at.duration_since(keys[1].at).as_secs_f64();
    assert!(
        after <= FRAME_MS as f64 / 1000.0 + 1.5,
        "the receipt keyed {after:.1}s after the line ahead of it{trace}"
    );
    assert!(
        !keys[2].csma,
        "the receipt keyed with carrier sense on{trace}"
    );
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
/// falls due just after a neighbour's frame waits until that frame is 10 s
/// old, and its countdown says so (`busy`). Nothing keys before the countdown,
/// and the beacon comes at the idle pace (about 5 minutes after the one
/// before).
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
        if gap.is_some_and(|g| !(240..=IDLE_GAP_MAX_SECS).contains(&g)) {
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
/// every 5 minutes, and never more than 10 minutes apart. QA v0.1.95 saw
/// quiet gaps of 285 s: one never goes sooner than 5 minutes after the last
/// reached the modem.
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
            .map(|k| k.handed)
            .collect()
    };
    let idle_keyed = |st: usize| beacons(st).iter().filter(|t| **t > warm_end).count();
    chain
        .wait(NOISY_CASE_SECS, |_| {
            (0..CALLS.len()).all(|i| beacons(i).len() >= 5 && idle_keyed(i) >= 2)
        })
        .await;
    let mut report = String::new();
    let mut problems = Vec::new();
    for (i, call) in CALLS.iter().enumerate() {
        let keyed = beacons(i);
        let gaps: Vec<u64> = keyed
            .windows(2)
            .map(|w| w[1].duration_since(w[0]).as_secs())
            .collect();
        report.push_str(&format!(
            "\n  {call}: {} beacons, gaps {gaps:?}s",
            keyed.len()
        ));
        // The first three go a pace apart (60 s ± 15), each counted from
        // when the one before reached the modem, then one about every 5
        // minutes.
        if keyed.len() < 5 {
            problems.push(format!("{call} keyed {} beacons", keyed.len()));
        }
        if let Some(g) = gaps.iter().take(2).find(|g| **g > WARMUP_GAP_MAX_SECS) {
            problems.push(format!("{call} warm-up gap {g}s, not about a minute"));
        }
        if let Some(g) = gaps.iter().find(|g| **g > MAX_BEACON_GAP_SECS) {
            problems.push(format!("{call} went {g}s between beacons"));
        }
        let idle: Vec<u64> = gaps.iter().skip(2).copied().collect();
        if idle
            .iter()
            .any(|g| !(IDLE_GAP_MIN_SECS..=IDLE_GAP_MAX_SECS).contains(g))
        {
            problems.push(format!("{call} idle gaps {idle:?}s, not about 5 minutes"));
        }
    }
    let trace = chain.trace();
    chain.stop();
    eprintln!("{report}");
    assert!(problems.is_empty(), "{problems:?}{report}{trace}");
}

/// Unix time of `at`, to set against the node's countdown.
fn unix_of(at: Instant) -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let now_at = Instant::now();
    if at <= now_at {
        now - now_at.duration_since(at).as_secs_f64()
    } else {
        now + at.duration_since(now_at).as_secs_f64()
    }
}

/// Sleep until unix time `t`.
async fn sleep_until_unix(t: f64) {
    let wait = t - unix_of(Instant::now());
    if wait > 0.0 {
        tokio::time::sleep(Duration::from_secs_f64(wait)).await;
    }
}

/// The beacon countdown a node shows: `(due, span, note)`, `due` in unix
/// seconds, `span` the seconds it counts down from.
async fn countdown(status: u16) -> Option<(f64, u64, String)> {
    let snap = status_json(status).await?;
    let due = snap.get("beacon_due").and_then(|x| x.as_u64())?;
    let span = snap
        .get("beacon_span")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let note = snap
        .get("beacon_note")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Some((due as f64, span, note))
}

/// One real node at station `st` on its own air. The harness plays its
/// neighbours with [`key_as`].
struct Solo {
    node: tokio::task::JoinHandle<wcr::error::Result<()>>,
    st: u64,
    air: wcr::sim::SharedAir,
    log: TimedLog,
    irc: u16,
    status: u16,
    t0: Instant,
}

impl Solo {
    /// Its modem reads `noise_pct` busy with nothing on the air and holds
    /// each frame `CSMA_STUCK_MS`, as on QA's chain. `tweak` sets the config.
    async fn start(st: u64, noise_pct: i32, tweak: impl FnOnce(&mut Config)) -> Self {
        let setup = setup_lock().lock().await;
        let air = wcr::sim::SharedAir::new(0.0);
        let log: TimedLog = Arc::new(Mutex::new(Vec::new()));
        let opts = TimedOpts {
            frame_ms: FRAME_MS,
            count_tx: true,
            collide: true,
            csma_stuck_ms: CSMA_STUCK_MS,
            rung_airtime: false,
            noise_pct,
            rx_after_decode: false,
        };
        let call = CALLS[(st - 1) as usize];
        let home = scratch(call);
        let (kiss, ctrl) =
            wcr::sim::mock_modem73_timed(air.clone(), st, hear_set(st), None, opts, log.clone())
                .await
                .unwrap();
        let ports = reserve_ports(2).await;
        let mut cfg = radio_cfg(call, &home, kiss, ctrl, ports[0], ports[1]);
        tweak(&mut cfg);
        let node = tokio::spawn(async move { wcr::node::run_node(cfg, false).await });
        wait_tcp(ports[0], 20).await;
        wait_radio(&ports[1..]).await;
        drop(setup);
        Self {
            node,
            st,
            air,
            log,
            irc: ports[0],
            status: ports[1],
            t0: Instant::now(),
        }
    }

    /// When each of this station's beacons reached its modem.
    fn beacons(&self) -> Vec<Instant> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.station == self.st && !k.dropped)
            .filter(|k| Envelope::decode(&k.payload).is_ok_and(|e| e.kind == MsgType::Beacon))
            .map(|k| k.handed)
            .collect()
    }

    /// When this station's copies of `id` (not its receipts) reached its modem.
    fn copies_of(&self, id: &wcr::proto::MsgId) -> Vec<Instant> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.station == self.st)
            .filter(|k| {
                Envelope::decode(&k.payload)
                    .is_ok_and(|e| e.kind != MsgType::Ack && e.msg_id == *id)
            })
            .map(|k| k.handed)
            .collect()
    }

    /// The unix time of the first idle beacon's countdown: three warm-up
    /// beacons have gone and the next is minutes away.
    async fn idle_due(&self) -> f64 {
        let deadline = Instant::now() + Duration::from_secs(600);
        loop {
            if self.beacons().len() >= 3 {
                if let Some((due, _, note)) = countdown(self.status).await {
                    if note.is_empty() && due > unix_of(Instant::now()) + 120.0 {
                        return due;
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "no idle beacon countdown{}",
                self.trace()
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    fn trace(&self) -> String {
        trace_of(&self.log, self.t0)
    }

    fn stop(self) {
        self.node.abort();
    }
}

fn beacon_from(st: usize) -> Envelope {
    let mut env = Envelope::new_msg(
        Callsign::parse(CALLS[st - 1]).unwrap(),
        Callsign::from_raw("BEACON"),
        1,
        b"B|radio|7074".to_vec(),
        1,
        Flags::new(),
    )
    .unwrap();
    env.kind = MsgType::Beacon;
    env
}

/// QA v0.1.94: C beaconed 19 s after G3 went past it and 50 s before G3's
/// receipt came back, and B beaconed between a mail and its acceptance. A
/// beacon keyed then leaves the station deaf to the answer, or lands on it at
/// a neighbour. Here C's idle beacon falls due while a line B passed it waits
/// for its receipt from D: the beacon waits for the receipt, then goes, at
/// hf-poor timing on a channel that reads 99% busy with carrier sense stuck.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_due_beacon_waits_for_the_receipt() {
    let solo = Solo::start(3, 99, |_| {}).await;
    let due = solo.idle_due().await;
    let line = Envelope::new_msg(
        Callsign::parse(CALLS[0]).unwrap(),
        Callsign::parse(CALLS[4]).unwrap(),
        1,
        b"a-line-waits-for-its-receipt".to_vec(),
        6,
        Flags::new().with(FLAG_REQ_ACK),
    )
    .unwrap();
    let mut relayed = line.clone();
    relayed.hops_left = 5;
    sleep_until_unix(due - 15.0).await;
    key_as(
        &solo.air,
        &solo.log,
        2,
        &relayed,
        Duration::from_millis(FRAME_MS),
    )
    .await;
    // C passes the line on. E's receipt comes back through D after that has
    // keyed, and at least 45 s after C's beacon fell due.
    let deadline = Instant::now() + Duration::from_secs(120);
    let relay_end = loop {
        let keyed = solo
            .log
            .lock()
            .unwrap()
            .iter()
            .find(|k| {
                k.station == solo.st
                    && Envelope::decode(&k.payload).is_ok_and(|e| e.msg_id == line.msg_id)
            })
            .map(|k| k.at + k.len);
        if let Some(end) = keyed {
            break end;
        }
        assert!(
            Instant::now() < deadline,
            "C never relayed the line{}",
            solo.trace()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    sleep_until_unix((due + 45.0).max(unix_of(relay_end) + 1.0)).await;
    let mut receipt = Envelope::ack_for(&line, Callsign::parse(CALLS[4]).unwrap(), 1, None);
    receipt.hops_left = 2;
    key_as(&solo.air, &solo.log, 4, &receipt, Duration::from_secs(1)).await;
    let answered = unix_of(Instant::now());
    let deadline = Instant::now() + Duration::from_secs(300);
    let beacon = loop {
        if let Some(b) = solo
            .beacons()
            .into_iter()
            .find(|b| unix_of(*b) > due - 30.0)
        {
            break unix_of(b);
        }
        assert!(
            Instant::now() < deadline,
            "C never beaconed{}",
            solo.trace()
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let trace = solo.trace();
    eprintln!("due {due:.0}, receipt {answered:.0}, beacon {beacon:.0}{trace}");
    solo.stop();
    assert!(
        beacon >= answered,
        "C's beacon reached the modem {:.0}s before the receipt it waited for{trace}",
        answered - beacon
    );
    assert!(
        beacon <= answered + 90.0,
        "C held its beacon {:.0}s past the receipt{trace}",
        beacon - answered
    );
}

/// QA v0.1.94: a station's countdown restarted when it decided to beacon,
/// while the beacon could reach the modem up to 39 s later (A's first, 7
/// minutes later, behind its invites). The countdown now says `pending` until
/// the beacon reaches the modem, and the next one counts from there. Here B
/// keys a long frame as C's idle beacon falls due, so C's carrier sense holds
/// the beacon off its modem for the air queue's 15 s cap.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_countdown_says_pending_until_the_beacon_reaches_the_modem() {
    let solo = Solo::start(3, 99, |_| {}).await;
    let due = solo.idle_due().await;
    sleep_until_unix(due - 3.0).await;
    let long = Duration::from_secs(25);
    let air = solo.air.clone();
    let log = solo.log.clone();
    let play = tokio::spawn(async move { key_as(&air, &log, 2, &beacon_from(2), long).await });
    let mut samples = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    let handed = loop {
        if let Some(c) = countdown(solo.status).await {
            samples.push((unix_of(Instant::now()), c));
        }
        if let Some(b) = solo
            .beacons()
            .into_iter()
            .find(|b| unix_of(*b) > due - 30.0)
        {
            let b = unix_of(b);
            if unix_of(Instant::now()) > b + 5.0 {
                break b;
            }
        }
        assert!(
            Instant::now() < deadline,
            "C never beaconed{}",
            solo.trace()
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    let _ = play.await;
    let trace = solo.trace();
    solo.stop();
    let shown: Vec<String> = samples
        .iter()
        .filter(|(at, _)| *at > due - 2.0)
        .map(|(at, (d, span, note))| {
            format!("{:.1}: due {:.0} span {span} {note:?}", at - due, d - due)
        })
        .collect();
    let report = format!(
        "\n  handed {:.1}s after due\n  {}",
        handed - due,
        shown.join("\n  ")
    );
    assert!(
        handed - due >= 10.0,
        "the beacon was not held off the modem{report}{trace}"
    );
    let early = samples.iter().find(|(at, (d, _, note))| {
        *at > due + 1.0 && *at < handed - 1.0 && note.is_empty() && *d > *at + 60.0
    });
    assert!(
        early.is_none(),
        "the next countdown started before the beacon reached the modem{report}"
    );
    assert!(
        samples
            .iter()
            .any(|(at, (_, _, note))| *at > due && *at < handed && note == "pending"),
        "no pending countdown while the beacon waited{report}"
    );
    let (d, span, _) = samples
        .iter()
        .find(|(at, (d, _, note))| *at > handed + 1.0 && note.is_empty() && *d > *at + 60.0)
        .map(|(_, c)| c.clone())
        .expect("a countdown after the beacon");
    let from = d - span as f64;
    assert!(
        (from - handed).abs() <= 2.0,
        "the next countdown counts from {:.1}s, the beacon reached the modem at {:.1}s{report}",
        from - due,
        handed - due
    );
}

/// QA v0.1.94: A sent four invites 10-25 s after start and B decoded none:
/// a neighbour started with A was not listening yet. A station's own lines
/// now wait until another station has been heard, at most two minutes after
/// start. Here A starts alone, its client sends a line 15 s in, and B's
/// first beacon is played 35 s in: the line reaches A's modem only after it,
/// and the line's first retry does not follow straight behind it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_line_sent_just_after_start_waits_for_another_station() {
    let solo = Solo::start(1, 99, |cfg| cfg.rf.beacon_jitter_s = 0).await;
    let mut client = Client::connect(solo.irc, CALLS[0]).await;
    tokio::time::sleep_until((solo.t0 + Duration::from_secs(15)).into()).await;
    client
        .send(&format!("PRIVMSG {} :sent-just-after-start", CALLS[1]))
        .await;
    let deadline = Instant::now() + Duration::from_secs(30);
    let id = loop {
        if let Some(id) = first_msgid(&client) {
            break wcr::proto::MsgId::parse_hex(&id).expect("msgid");
        }
        assert!(Instant::now() < deadline, "the line was never tagged");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    tokio::time::sleep_until((solo.t0 + Duration::from_secs(35)).into()).await;
    key_as(
        &solo.air,
        &solo.log,
        2,
        &beacon_from(2),
        Duration::from_millis(FRAME_MS),
    )
    .await;
    let heard = Instant::now();
    let deadline = heard + Duration::from_secs(150);
    while solo.copies_of(&id).len() < 2 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let copies = solo.copies_of(&id);
    let trace = solo.trace();
    eprintln!("{trace}");
    solo.stop();
    let first = *copies.first().expect("A never keyed its line");
    assert!(
        first >= heard,
        "A's line reached its modem {:.0}s before it heard another station{trace}",
        heard.duration_since(first).as_secs_f64()
    );
    assert!(
        first <= heard + Duration::from_secs(30),
        "A held its line {:.0}s after hearing B{trace}",
        first.duration_since(heard).as_secs_f64()
    );
    if let Some(second) = copies.get(1) {
        assert!(
            second.duration_since(first) >= Duration::from_secs(30),
            "A's first retry followed {:.0}s behind its first copy{trace}",
            second.duration_since(first).as_secs_f64()
        );
    }
}

/// QA v0.1.94 on a channel that read 79-99% busy with carrier sense stuck:
/// traffic from the start held every beacon after the first for ten
/// minutes, so A, B and C keyed one warm-up beacon and every station then
/// beaconed only at the 10-minute ceiling. Here every modem reads 99% busy,
/// A sends a group line just after start and E another six minutes in.
/// Each station must still key three warm-up beacons in its first nine
/// minutes, each waiting out one exchange at most, then beacon about 5
/// minutes after the last, and under 9 minutes even while it carries lines.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn beacons_keep_their_pace_through_traffic() {
    let mut chain = Chain::start_cold(Some("net"), 99).await;
    chain.join("#net").await;
    let t0 = chain.t0;
    tokio::time::sleep_until((t0 + Duration::from_secs(20)).into()).await;
    chain.clients[0].send("PRIVMSG #net :g1-at-start").await;
    tokio::time::sleep_until((t0 + Duration::from_secs(360)).into()).await;
    chain.clients[4]
        .send("PRIVMSG #net :g2-six-minutes-in")
        .await;
    let beacons = |st: usize| -> Vec<Instant> {
        chain
            .log
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.station == st as u64 + 1 && !k.dropped)
            .filter(|k| Envelope::decode(&k.payload).is_ok_and(|e| e.kind == MsgType::Beacon))
            .map(|k| k.handed)
            .collect()
    };
    let left = TRAFFIC_PACE_SECS.saturating_sub(t0.elapsed().as_secs());
    chain
        .wait(left, |_| (0..CALLS.len()).all(|i| beacons(i).len() >= 5))
        .await;
    let mut report = String::new();
    let mut problems = Vec::new();
    for (i, call) in CALLS.iter().enumerate() {
        let handed = beacons(i);
        let at: Vec<u64> = handed
            .iter()
            .map(|b| b.saturating_duration_since(t0).as_secs())
            .collect();
        let gaps: Vec<u64> = handed
            .windows(2)
            .map(|w| w[1].duration_since(w[0]).as_secs())
            .collect();
        report.push_str(&format!("\n  {call}: beacons at {at:?}s, gaps {gaps:?}s"));
        let warm = at.iter().filter(|t| **t <= TRAFFIC_GAP_MAX_SECS).count();
        if warm < 3 {
            problems.push(format!(
                "{call} keyed {warm} beacons in its first 9 minutes"
            ));
        }
        let idle: Vec<u64> = gaps.iter().skip(2).copied().collect();
        if idle.len() < 2 {
            problems.push(format!(
                "{call} keyed {} beacons after its warm-up",
                idle.len()
            ));
        }
        if !idle.iter().any(|g| *g <= IDLE_GAP_MAX_SECS) {
            problems.push(format!("{call} idle gaps {idle:?}s, never about 5 minutes"));
        }
        if let Some(g) = gaps.iter().find(|g| **g > TRAFFIC_GAP_MAX_SECS) {
            problems.push(format!(
                "{call} went {g}s between beacons, more than one exchange past 5 minutes"
            ));
        }
    }
    let trace = chain.trace();
    chain.stop();
    eprintln!("{report}");
    assert!(problems.is_empty(), "{problems:?}{report}{trace}");
}

/// QA v0.1.95, a fresh start on a channel that reads busy: A's client sent
/// invites 10-25 s after start and B decoded none of their first copies.
/// B got A's invite only on A's second retry, keyed its receipt three times,
/// and A heard none: the invite ended failed although B had it. Here A sends
/// a direct message to B and invites B..E in its first 25 s. The modems
/// report `rx` as upstream modem73 does: only just after a frame decodes,
/// not while the sender's next frame is on the air. B must get both, A must
/// hear B's receipts, and neither line may ever show failed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_early_line_to_the_neighbour_is_delivered() {
    let mut chain = Chain::start_modems(None, None, 99, false, true).await;
    let started = chain.t0;
    chain.clients[0].send("JOIN #qa95a").await;
    tokio::time::sleep_until((started + Duration::from_secs(10)).into()).await;
    let invite_b = send_tracked(&mut chain, 0, "INVITE M0BBB #qa95a").await;
    tokio::time::sleep_until((started + Duration::from_secs(15)).into()).await;
    let dm = send_tracked(&mut chain, 0, "PRIVMSG M0BBB :early-dm-to-b").await;
    for (n, call) in CALLS[2..].iter().enumerate() {
        tokio::time::sleep_until((started + Duration::from_secs(18 + 3 * n as u64)).into()).await;
        chain.clients[0]
            .send(&format!("INVITE {call} #qa95a"))
            .await;
    }
    let delivered = |c: &Chain, id: &str| {
        marks_for(&c.clients[0], id)
            .iter()
            .any(|m| m == "delivered" || m == "all")
    };
    chain
        .wait(CASE_SECS, |c| {
            c.clients[1].shows("early-dm-to-b") >= 1
                && got_invite(&c.clients[1], "#qa95a")
                && delivered(c, &dm)
                && delivered(c, &invite_b)
        })
        .await;
    let trace = chain.trace();
    let mut problems = Vec::new();
    let mut report = String::new();
    for (what, id) in [("dm", &dm), ("invite to B", &invite_b)] {
        let marks = marks_for(&chain.clients[0], id);
        let keys = chain.keys_of(id);
        report.push_str(&format!("\n  {what}: marks {marks:?} keys {keys:?}"));
        if !delivered(&chain, id) {
            problems.push(format!("A never saw B's receipt for the {what}"));
        }
        if marks.iter().any(|m| m == "failed") {
            problems.push(format!("the {what} showed failed although B is next to A"));
        }
        if keys[0].1 > 2 {
            problems.push(format!("A keyed the {what} {} times", keys[0].1));
        }
    }
    if chain.clients[1].shows("early-dm-to-b") == 0 {
        problems.push("B never showed the direct message".into());
    }
    if !got_invite(&chain.clients[1], "#qa95a") {
        problems.push("B never got the invite".into());
    }
    eprintln!("{trace}{report}");
    chain.stop();
    assert!(problems.is_empty(), "{problems:?}{report}{trace}");
}

/// QA v0.1.95: the busiest stations' third warm-up beacon waited behind
/// their own queued frames. A's beacons went 14:14:16, 14:15:30, then
/// 14:24:17, and B's third was taken back from the air queue four times. Here
/// A hears B's beacon 5 s after start and its client queues eight direct
/// messages to B, which never answers, at hf-poor timing with carrier sense
/// stuck. A's warm-up beacons must each wait out one exchange at most, then
/// go ahead of the queued lines.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn warm_up_beacons_go_between_queued_frames() {
    let solo = Solo::start(1, 99, |cfg| cfg.rf.beacon_jitter_s = 0).await;
    let mut client = Client::connect(solo.irc, CALLS[0]).await;
    tokio::time::sleep_until((solo.t0 + Duration::from_secs(5)).into()).await;
    key_as(
        &solo.air,
        &solo.log,
        2,
        &beacon_from(2),
        Duration::from_millis(FRAME_MS),
    )
    .await;
    for n in 0..8 {
        client
            .send(&format!("PRIVMSG {} :queued-line-{n}", CALLS[1]))
            .await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let deadline = solo.t0 + Duration::from_secs(900);
    while solo.beacons().len() < 3 && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let beacons = solo.beacons();
    let trace = solo.trace();
    let t0 = solo.t0;
    solo.stop();
    let at: Vec<u64> = beacons
        .iter()
        .map(|b| b.saturating_duration_since(t0).as_secs())
        .collect();
    let gaps: Vec<u64> = beacons
        .windows(2)
        .map(|w| w[1].duration_since(w[0]).as_secs())
        .collect();
    eprintln!("{trace}\n  beacons at {at:?}s, gaps {gaps:?}s");
    assert!(
        beacons.len() >= 3,
        "A keyed {} warm-up beacons{trace}",
        beacons.len()
    );
    assert!(
        at[0] <= WARMUP_QUEUE_GAP_MAX_SECS
            && gaps.iter().take(2).all(|g| *g <= WARMUP_QUEUE_GAP_MAX_SECS),
        "warm-up beacons at {at:?}s, gaps {gaps:?}s: a beacon waited behind A's queue{trace}"
    );
}
