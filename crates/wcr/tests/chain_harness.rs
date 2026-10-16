//! Five stations in a line, each hearing only its neighbours, with per-link
//! frame loss: A(1) - B(2) - C(3) - D(4) - E(5).
//!
//! Every drop rule is deterministic (the first copy of a chosen frame on a
//! chosen link) so a failure is the protocol, not the dice.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use wcr::config::Config;
use wcr::modes::Mode;
use wcr::proto::{Envelope, MsgType};
use wcr::sim::LinkDrop;

const CALLS: [&str; 5] = ["G4AAA", "M0BBB", "2E0CCC", "M0DDD", "G4EEE"];

fn suite_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
    &LOCK
}

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wcr-chain-{name}-{}-{n}", std::process::id()));
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
    cfg.modem.preset = "vhf-fm".into();
    cfg.rf.turnaround_ms = 0;
    cfg.rf.slot_ms = 40;
    cfg.rf.quiet_ms = 40;
    cfg.rf.ack_dither_ms = 0;
    cfg.rf.retry_jitter = false;
    cfg.rf.beacon_jitter_s = 0;
    cfg.rf.emergency_dup_ms = 60_000;
    cfg.rf.frequency_khz = 144_950;
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

/// Station index on the chain (1-based, matching the sim station ids).
fn station(call: &str) -> u64 {
    CALLS.iter().position(|c| *c == call).unwrap() as u64 + 1
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

type FramePred = Arc<dyn Fn(&Envelope) -> bool + Send + Sync>;

/// Drop the first `copies` copies of every frame matching `pred` heard on
/// `from -> to`. Later copies of the same msg_id get through.
#[derive(Clone)]
struct Rule {
    from: u64,
    to: u64,
    copies: u32,
    pred: FramePred,
}

/// Copies already dropped, per rule and message id.
type Seen = Arc<Mutex<HashMap<(usize, [u8; 8]), u32>>>;

fn link_drop(rules: Vec<Rule>, log: Arc<Mutex<Vec<String>>>) -> LinkDrop {
    let seen: Seen = Arc::new(Mutex::new(HashMap::new()));
    Arc::new(move |from, to, frame| {
        let Ok(env) = Envelope::decode(frame) else {
            return false;
        };
        for (i, r) in rules.iter().enumerate() {
            if r.from == from && r.to == to && (r.pred)(&env) {
                let mut g = seen.lock().unwrap();
                let n = g.entry((i, env.msg_id.0)).or_insert(0);
                if *n < r.copies {
                    *n += 1;
                    log.lock().unwrap().push(format!(
                        "drop {from}->{to} {:?} {} from {} hops {}",
                        env.kind,
                        env.msg_id.hex(),
                        env.origin,
                        env.hops_left
                    ));
                    return true;
                }
            }
        }
        false
    })
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

    async fn wait_for(&self, secs: u64, pred: impl Fn(&[String]) -> bool) -> bool {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
        while tokio::time::Instant::now() < deadline {
            if pred(&self.lines()) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        pred(&self.lines())
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

/// Members in the last `353` reply for `chan`.
fn names_of(lines: &[String], chan: &str) -> Option<Vec<String>> {
    let needle = " 353 ";
    let tail = format!("= {chan} :");
    lines
        .iter()
        .rev()
        .find(|l| l.contains(needle) && l.contains(&tail))
        .map(|l| {
            l.split_once(&tail)
                .map(|(_, b)| b.split_whitespace().map(|s| s.to_string()).collect())
                .unwrap_or_default()
        })
}

struct Chain {
    nodes: Vec<tokio::task::JoinHandle<wcr::error::Result<()>>>,
    clients: Vec<Client>,
    /// Every frame keyed, tagged with the transmitting station and ms since start.
    tx: Arc<Mutex<Vec<(u64, Envelope, u128)>>>,
    drops: Arc<Mutex<Vec<String>>>,
}

impl Chain {
    async fn start(rules: Vec<Rule>, group: Option<(&str, &[&str])>) -> Self {
        let air = wcr::sim::SharedAir::new(0.0);
        let tx = Arc::new(Mutex::new(Vec::new()));
        {
            let mut rx = air.subscribe_linked();
            let tx = tx.clone();
            let t0 = std::time::Instant::now();
            tokio::spawn(async move {
                while let Ok((from, frame)) = rx.recv().await {
                    if let Ok(env) = Envelope::decode(&frame) {
                        tx.lock()
                            .unwrap()
                            .push((from, env, t0.elapsed().as_millis()));
                    }
                }
            });
        }
        let drops = Arc::new(Mutex::new(Vec::new()));
        let drop_fn = link_drop(rules, drops.clone());
        let mut prep = Vec::new();
        for call in CALLS {
            let home = scratch(call);
            if let Some((name, members)) = group {
                let store = wcr::store::Store::open(&home.join("store.db"), 72, 10_000).unwrap();
                let members: Vec<String> = members.iter().map(|s| s.to_string()).collect();
                store.group_create(name, &members).unwrap();
            }
            let st = station(call);
            let (kiss, ctrl) =
                wcr::sim::mock_modem73_chain_free(air.clone(), st, hear_set(st), drop_fn.clone())
                    .await
                    .unwrap();
            prep.push((call, home, kiss, ctrl));
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
        Self {
            nodes,
            clients,
            tx,
            drops,
        }
    }

    fn keyed_by(&self, st: u64, pred: impl Fn(&Envelope) -> bool) -> usize {
        self.tx
            .lock()
            .unwrap()
            .iter()
            .filter(|(from, env, _)| *from == st && pred(env))
            .count()
    }

    fn trace(&self) -> String {
        let tx = self.tx.lock().unwrap();
        let mut out = String::new();
        for (from, env, ms) in tx.iter() {
            if env.kind == MsgType::Beacon {
                continue;
            }
            let what = if env.kind == MsgType::Ack {
                format!(
                    "ack-of {}",
                    env.acked_id().map(|i| i.hex()).unwrap_or_default()
                )
            } else {
                env.body_text()
            };
            out.push_str(&format!(
                "\n  {ms:>6} tx {} {:?} {} {}>{} hops {} {}",
                CALLS[(*from - 1) as usize],
                env.kind,
                env.msg_id.hex(),
                env.origin,
                env.dest,
                env.hops_left,
                what
            ));
        }
        for d in self.drops.lock().unwrap().iter() {
            out.push_str(&format!("\n  {d}"));
        }
        out
    }

    fn stop(self) {
        for n in self.nodes {
            n.abort();
        }
    }
}

fn is_roster(env: &Envelope) -> bool {
    env.kind == MsgType::Status && env.body.starts_with(b"WCRMETA members=")
}

/// Issue 1: A invites B..E one at a time. Every roster frame loses its first
/// copy between C and D. NAMES must still list all five on every station.
#[tokio::test]
async fn roster_reaches_every_station_on_a_lossy_chain() {
    let _suite = suite_lock().lock().await;
    let rules = vec![Rule {
        from: 3,
        to: 4,
        copies: 1,
        pred: Arc::new(is_roster),
    }];
    let mut chain = Chain::start(rules, None).await;
    chain.clients[0].send("JOIN #net").await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    for target in &CALLS[1..] {
        chain.clients[0]
            .send(&format!("INVITE {target} #net"))
            .await;
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let want: HashSet<String> = CALLS.iter().map(|c| c.to_string()).collect();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let mut last: Vec<Option<Vec<String>>> = vec![None; CALLS.len()];
    loop {
        for (i, c) in chain.clients.iter_mut().enumerate() {
            c.send("NAMES #net").await;
            let _ = i;
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
        let mut all = true;
        for (i, c) in chain.clients.iter().enumerate() {
            last[i] = names_of(&c.lines(), "#net");
            let got: HashSet<String> = last[i].clone().unwrap_or_default().into_iter().collect();
            all &= got == want;
        }
        if all || tokio::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let report: Vec<String> = CALLS
        .iter()
        .zip(&last)
        .map(|(c, n)| format!("{c}: {:?}", n.clone().unwrap_or_default()))
        .collect();
    let complete = last.iter().all(|n| {
        n.as_ref()
            .map(|n| n.iter().cloned().collect::<HashSet<_>>() == want)
            .unwrap_or(false)
    });
    let trace = chain.trace();
    eprintln!("{trace}");
    chain.stop();
    assert!(
        complete,
        "roster incomplete on far stations: {report:?}{trace}"
    );
}

/// Issue 2: closed group of all five. A sends; the first copy of D's and
/// E's receipts is lost between C and B. A must still reach [all], with one
/// [rl], no [no], and every member seeing the line once.
#[tokio::test]
async fn far_group_receipts_return_through_lossy_relays() {
    let _suite = suite_lock().lock().await;
    let rules = vec![Rule {
        from: 3,
        to: 2,
        copies: 1,
        pred: Arc::new(|env: &Envelope| {
            env.kind == MsgType::Ack && matches!(env.origin.as_str(), "M0DDD" | "G4EEE")
        }),
    }];
    let mut chain = Chain::start(rules, Some(("net", &CALLS))).await;
    for c in chain.clients.iter_mut() {
        c.send("JOIN #net").await;
    }
    tokio::time::sleep(Duration::from_millis(600)).await;
    let body = "g1-far-receipts";
    chain.clients[0]
        .send(&format!("PRIVMSG #net :!!{body}"))
        .await;
    let got_all = chain.clients[0]
        .wait_for(120, |lines| {
            lines.iter().any(|l| l.contains("+radio/delivery=all"))
        })
        .await;
    let a_lines = chain.clients[0].lines();
    let mut seen = Vec::new();
    for (i, c) in chain.clients.iter().enumerate().skip(1) {
        let n = c
            .lines()
            .iter()
            .filter(|l| privmsg_body(l).is_some_and(|b| b.contains(body)))
            .count();
        seen.push((CALLS[i], n));
    }
    let trace = chain.trace();
    eprintln!("{trace}");
    chain.stop();
    let tags: Vec<&String> = a_lines
        .iter()
        .filter(|l| l.contains("+radio/delivery="))
        .collect();
    assert!(got_all, "A never reached [all]: {tags:?}{trace}");
    assert!(
        seen.iter().all(|(_, n)| *n == 1),
        "each member should show the line once: {seen:?}{trace}"
    );
    assert!(
        tags.iter()
            .filter(|l| l.contains("+radio/delivery=relayed"))
            .count()
            <= 1,
        "[rl] painted more than once: {tags:?}"
    );
    assert!(
        !tags.iter().any(|l| l.contains("+radio/delivery=failed")),
        "false [no]: {tags:?}"
    );
}

/// Issue 2, the other end: E sends to the group; A's and B's receipts lose
/// their first copy between C and D.
#[tokio::test]
async fn far_group_receipts_return_to_the_far_end_sender() {
    let _suite = suite_lock().lock().await;
    let rules = vec![Rule {
        from: 3,
        to: 4,
        copies: 1,
        pred: Arc::new(|env: &Envelope| {
            env.kind == MsgType::Ack && matches!(env.origin.as_str(), "G4AAA" | "M0BBB")
        }),
    }];
    let mut chain = Chain::start(rules, Some(("net", &CALLS))).await;
    for c in chain.clients.iter_mut() {
        c.send("JOIN #net").await;
    }
    tokio::time::sleep(Duration::from_millis(600)).await;
    chain.clients[4].send("PRIVMSG #net :!!g2-from-e").await;
    let got_all = chain.clients[4]
        .wait_for(120, |lines| {
            lines.iter().any(|l| l.contains("+radio/delivery=all"))
        })
        .await;
    let tags: Vec<String> = chain.clients[4]
        .lines()
        .into_iter()
        .filter(|l| l.contains("+radio/delivery="))
        .collect();
    let trace = chain.trace();
    eprintln!("{trace}");
    chain.stop();
    assert!(got_all, "E never reached [all]: {tags:?}{trace}");
    assert!(
        !tags.iter().any(|l| l.contains("+radio/delivery=failed")),
        "false [no]: {tags:?}"
    );
}

/// Issue 3: a bulletin from A loses its first copy between C and D. C must
/// key it again until it hears D carry it on, so D and E still get it once.
#[tokio::test]
async fn bulletin_keeps_walking_past_the_second_relay() {
    let _suite = suite_lock().lock().await;
    let rules = vec![Rule {
        from: 3,
        to: 4,
        copies: 1,
        pred: Arc::new(|env: &Envelope| {
            env.kind == MsgType::Msg && env.dest.as_str() == "BULLETIN"
        }),
    }];
    let mut chain = Chain::start(rules, None).await;
    for c in chain.clients.iter_mut() {
        c.send("JOIN #bulletin").await;
    }
    tokio::time::sleep(Duration::from_millis(600)).await;
    let body = "b1-walks-on";
    chain.clients[0]
        .send(&format!("PRIVMSG #bulletin :!!{body}"))
        .await;
    let mut ok = true;
    for i in [3usize, 4] {
        ok &= chain.clients[i]
            .wait_for(90, |lines| {
                lines
                    .iter()
                    .any(|l| privmsg_body(l).is_some_and(|b| b.contains(body)))
            })
            .await;
    }
    tokio::time::sleep(Duration::from_secs(4)).await;
    let counts: Vec<(&str, usize)> = chain
        .clients
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, c)| {
            (
                CALLS[i],
                c.lines()
                    .iter()
                    .filter(|l| privmsg_body(l).is_some_and(|b| b.contains(body)))
                    .count(),
            )
        })
        .collect();
    let is_b1 = |env: &Envelope| {
        env.kind == MsgType::Msg
            && env.dest.as_str() == "BULLETIN"
            && env.body_text().contains(body)
    };
    let c_keys = chain.keyed_by(3, is_b1);
    let trace = chain.trace();
    eprintln!("{trace}");
    chain.stop();
    assert!(
        ok,
        "bulletin died past C: {counts:?} C keyed {c_keys}{trace}"
    );
    assert!(c_keys >= 2, "C keyed the bulletin {c_keys} times{trace}");
    assert!(
        counts.iter().all(|(_, n)| *n == 1),
        "duplicate or missing bulletin lines: {counts:?}"
    );
}

/// QA v0.1.92: B, next to A, keyed the far receipts of A's lines four or five
/// times each. A got each the first time and never retried. Every extra copy
/// is airtime in which B cannot hear A's next line. On a quiet chain B
/// carries D's receipt to A once.
#[tokio::test]
async fn the_station_next_to_the_sender_carries_a_receipt_once() {
    let _suite = suite_lock().lock().await;
    let mut chain = Chain::start(Vec::new(), None).await;
    chain.clients[0].send("PRIVMSG M0DDD :hello-d").await;
    let ok = chain.clients[0]
        .wait_for(120, |lines| {
            lines
                .iter()
                .any(|l| l.contains("+radio/delivery=delivered"))
        })
        .await;
    // Long enough for B's receipt walk to run three retry intervals.
    tokio::time::sleep(Duration::from_secs(300)).await;
    let line_keys = chain.keyed_by(1, |e| {
        e.kind == MsgType::Msg && e.body_text().contains("hello-d")
    });
    let b_receipts = chain.keyed_by(2, |e| {
        e.kind == MsgType::Ack && e.origin.as_str() == "M0DDD"
    });
    let trace = chain.trace();
    chain.stop();
    eprintln!("A keyed the line {line_keys}x, B keyed D's receipt {b_receipts}x{trace}");
    assert!(ok, "A never reached [ok]{trace}");
    assert_eq!(line_keys, 1, "A retried a line that was answered{trace}");
    assert_eq!(b_receipts, 1, "B repeated D's receipt next to A{trace}");
}
