//! Two stations that hear each other send a small file in a direct message
//! (`/file`). The receiver must end up with the exact bytes, and the
//! sender's file line must reach `[ok]` only once the whole file is there.

#[path = "common/watchdog.rs"]
mod common_watchdog;
use common_watchdog::{watchdog, TEST_LIMIT};
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
use wcr::proto::file::Piece;
use wcr::proto::{Envelope, MsgType};
use wcr::sim::LinkDrop;

const A: &str = "G4AAA";
const B: &str = "M0BBB";

fn suite_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));
    &LOCK
}

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wcr-file-{name}-{}-{n}", std::process::id()));
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
    cfg.rf.startup_hold_s = 0;
    cfg.rf.emergency_dup_ms = 60_000;
    cfg.rf.frequency_khz = 144_950;
    cfg.normalize();
    cfg
}

async fn wait_radio(ports: &[u16]) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        let mut ok = true;
        for p in ports {
            let label = match reqwest::get(format!("http://127.0.0.1:{p}/status")).await {
                Ok(r) => r
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v.get("audio_label")?.as_str().map(str::to_string))
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            ok &= matches!(label.as_str(), "low" | "good" | "hot");
        }
        if ok {
            return;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    panic!("radio modems never reported a level");
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
            "CAP LS\r\nNICK {nick}\r\nUSER {nick} 0 * :wcr file\r\nCAP REQ :message-tags echo-message server-time msgid\r\nCAP END\r\n"
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

/// Msgid tag of the sender's own file line.
fn file_line_id(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .find(|l| l.contains("PRIVMSG") && l.contains("📎") && l.contains(" piece"))
        .and_then(|l| {
            l.strip_prefix('@')?
                .split(' ')
                .next()?
                .split(';')
                .find_map(|t| t.strip_prefix("msgid=").map(str::to_string))
        })
}

fn delivered(lines: &[String], id: &str) -> bool {
    lines
        .iter()
        .any(|l| l.contains("+radio/delivery=delivered") && l.contains(id))
}

struct Pair {
    nodes: Vec<tokio::task::JoinHandle<wcr::error::Result<()>>>,
    clients: Vec<Client>,
    homes: Vec<PathBuf>,
    tx: Arc<Mutex<Vec<(u64, Envelope)>>>,
}

impl Pair {
    /// `drop` sees `(from, to, envelope)` and returns true to lose that copy.
    async fn start(drop: impl Fn(u64, u64, &Envelope) -> bool + Send + Sync + 'static) -> Self {
        let air = wcr::sim::SharedAir::new(0.0);
        let tx = Arc::new(Mutex::new(Vec::new()));
        {
            let mut rx = air.subscribe_linked();
            let tx = tx.clone();
            tokio::spawn(async move {
                while let Ok((from, frame)) = rx.recv().await {
                    if let Ok(env) = Envelope::decode(&frame) {
                        tx.lock().unwrap().push((from, env));
                    }
                }
            });
        }
        let drop: LinkDrop = Arc::new(move |from, to, frame| {
            Envelope::decode(frame).is_ok_and(|env| drop(from, to, &env))
        });
        let mut prep = Vec::new();
        for (i, call) in [A, B].into_iter().enumerate() {
            let st = i as u64 + 1;
            let hear: HashSet<u64> = [3 - st].into_iter().collect();
            let home = scratch(call);
            let (kiss, ctrl) =
                wcr::sim::mock_modem73_chain_free(air.clone(), st, hear, drop.clone())
                    .await
                    .unwrap();
            prep.push((call, home, kiss, ctrl));
        }
        let ports = reserve_ports(4).await;
        let mut nodes = Vec::new();
        let mut homes = Vec::new();
        for (i, (call, home, kiss, ctrl)) in prep.into_iter().enumerate() {
            let cfg = radio_cfg(call, &home, kiss, ctrl, ports[i * 2], ports[i * 2 + 1]);
            homes.push(home);
            nodes.push(tokio::spawn(async move {
                wcr::node::run_node(cfg, false).await
            }));
        }
        wait_tcp(ports[0], 20).await;
        wait_tcp(ports[2], 20).await;
        wait_radio(&[ports[1], ports[3]]).await;
        let clients = vec![
            Client::connect(ports[0], A).await,
            Client::connect(ports[2], B).await,
        ];
        tokio::time::sleep(Duration::from_millis(400)).await;
        Self {
            nodes,
            clients,
            homes,
            tx,
        }
    }

    fn trace(&self) -> String {
        let mut out = String::new();
        for (from, env) in self.tx.lock().unwrap().iter() {
            let what = match env.kind {
                MsgType::Ack => format!(
                    "ack-of {}",
                    env.acked_id().map(|i| i.hex()).unwrap_or_default()
                ),
                MsgType::File => Piece::decode(&env.body)
                    .map(|p| format!("piece {}/{}", p.idx + 1, p.count))
                    .unwrap_or_default(),
                MsgType::Beacon => continue,
                _ => env.body_text(),
            };
            out.push_str(&format!(
                "\n  tx {from} {:?} {} {}>{} {what}",
                env.kind,
                env.msg_id.hex(),
                env.origin,
                env.dest
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

fn sample(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 253) as u8).collect()
}

/// A 2 KB file arrives whole, and the sender sees `[ok]` on its file line.
#[tokio::test]
async fn a_small_file_arrives_whole() {
    let _hang = watchdog(TEST_LIMIT);
    let _suite = suite_lock().lock().await;
    let mut pair = Pair::start(|_, _, _| false).await;
    let src = scratch("src").join("field notes.bin");
    let body = sample(2000);
    std::fs::write(&src, &body).unwrap();
    pair.clients[0]
        .send(&format!("RADIO file {B} {}", src.display()))
        .await;
    let got = pair.clients[1]
        .wait_for(120, |lines| lines.iter().any(|l| l.contains("saved to")))
        .await;
    let ok = pair.clients[0]
        .wait_for(60, |lines| {
            file_line_id(lines).is_some_and(|id| delivered(lines, &id))
        })
        .await;
    let saved = pair.homes[1].join("files").join(A).join("field_notes.bin");
    let trace = pair.trace();
    let a_lines = pair.clients[0].lines().join("\n");
    pair.stop();
    assert!(got, "B never saved the file{trace}");
    assert_eq!(
        std::fs::read(&saved).ok(),
        Some(body),
        "saved bytes differ{trace}"
    );
    assert!(ok, "A's file line never reached [ok]\n{a_lines}{trace}");
}

/// The first copy of a middle piece is lost. B must hold back the receipt
/// for the last piece until the retry fills the gap, and A reaches `[ok]`
/// only after that.
#[tokio::test]
async fn a_lost_piece_is_sent_again_and_the_last_receipt_waits_for_it() {
    let _hang = watchdog(TEST_LIMIT);
    let _suite = suite_lock().lock().await;
    let dropped: Arc<Mutex<HashMap<[u8; 8], u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let d = dropped.clone();
    let mut pair = Pair::start(move |from, to, env| {
        if from != 1 || to != 2 || env.kind != MsgType::File {
            return false;
        }
        let Ok(p) = Piece::decode(&env.body) else {
            return false;
        };
        if p.idx != 1 {
            return false;
        }
        let mut g = d.lock().unwrap();
        let n = g.entry(env.msg_id.0).or_insert(0);
        *n += 1;
        *n == 1
    })
    .await;
    let src = scratch("src").join("route.gpx");
    let body = sample(1000);
    std::fs::write(&src, &body).unwrap();
    pair.clients[0]
        .send(&format!("RADIO file {B} {}", src.display()))
        .await;
    let got = pair.clients[1]
        .wait_for(240, |lines| lines.iter().any(|l| l.contains("saved to")))
        .await;
    let ok = pair.clients[0]
        .wait_for(120, |lines| {
            file_line_id(lines).is_some_and(|id| delivered(lines, &id))
        })
        .await;
    let saved = pair.homes[1].join("files").join(A).join("route.gpx");
    let trace = pair.trace();
    let a_lines = pair.clients[0].lines().join("\n");
    // B's receipt for the last piece must come after the second copy of piece 2.
    let order: Vec<String> = pair
        .tx
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(from, env)| match env.kind {
            MsgType::File if *from == 1 => {
                Piece::decode(&env.body).ok().map(|p| format!("p{}", p.idx))
            }
            MsgType::Ack if *from == 2 => Some("ack".into()),
            _ => None,
        })
        .collect();
    let file_id = file_line_id(&pair.clients[0].lines()).unwrap_or_default();
    let last_ack_at = pair.tx.lock().unwrap().iter().position(|(from, env)| {
        *from == 2 && env.kind == MsgType::Ack && env.acked_id().is_some_and(|i| i.hex() == file_id)
    });
    let second_p1_at = pair
        .tx
        .lock()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, (from, env))| {
            *from == 1
                && env.kind == MsgType::File
                && Piece::decode(&env.body).is_ok_and(|p| p.idx == 1)
        })
        .nth(1)
        .map(|(i, _)| i);
    pair.stop();
    assert!(
        dropped.lock().unwrap().values().any(|n| *n >= 1),
        "piece 2 was never dropped{trace}"
    );
    assert!(got, "B never saved the file{trace}");
    assert_eq!(
        std::fs::read(&saved).ok(),
        Some(body),
        "saved bytes differ{trace}"
    );
    assert!(ok, "A's file line never reached [ok]\n{a_lines}{trace}");
    let (Some(last_ack), Some(retry)) = (last_ack_at, second_p1_at) else {
        panic!("missing last-piece receipt or piece-2 retry {order:?}{trace}");
    };
    assert!(
        last_ack > retry,
        "B answered the last piece before it had piece 2 {order:?}{trace}"
    );
}
