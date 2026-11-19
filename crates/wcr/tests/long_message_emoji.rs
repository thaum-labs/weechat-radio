//! Long messages and emoji over the air (#365): a long line goes as
//! `[i/n]`-tagged pieces, every piece fits its frame and never splits an
//! emoji, the receiver gets them all, and the sender sees one mark for the
//! whole message with how many pieces are on the air.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use wcr::config::Config;
use wcr::modes::Mode;

fn scratch(name: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("wcr-longmsg-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Ports chosen while every probe socket is still held. A later `bind(0)`
/// cannot be given one of these until the probes are dropped, which is why
/// a test reserves the whole set before it starts nodes.
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

fn base_cfg(callsign: &str, home: &Path, mode: Mode) -> Config {
    let mut cfg = Config {
        callsign: callsign.into(),
        grid: "IO91WM".into(),
        mode,
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
    cfg.normalize();
    cfg
}

fn radio_cfg(callsign: &str, home: &Path, kiss: u16, ctrl: u16, irc: u16, status: u16) -> Config {
    let mut cfg = base_cfg(callsign, home, Mode::Radio);
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
    // These modems listen from the first instant: no start-up hold.
    cfg.rf.startup_hold_s = 0;
    cfg.rf.emergency_dup_ms = 60_000;
    cfg.rf.frequency_khz = 144_950;
    cfg.normalize();
    cfg
}

async fn irc_register(stream: &mut TcpStream, nick: &str) {
    let burst = format!(
        "CAP LS\r\nNICK {nick}\r\nUSER {nick} 0 * :wcr test\r\nCAP REQ :message-tags echo-message server-time msgid\r\nCAP END\r\n"
    );
    stream.write_all(burst.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

fn privmsg(line: &str) -> Option<(&str, &str, &str)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let rest = if let Some(tagged) = line.strip_prefix('@') {
        tagged.split_once(' ')?.1
    } else {
        line
    };
    let rest = rest.strip_prefix(':')?;
    let (from, after) = rest.split_once(' ')?;
    let after = after.strip_prefix("PRIVMSG ")?;
    let (target, body) = after.split_once(" :")?;
    Some((from, target, body))
}

async fn read_lines_until(
    reader: &mut tokio::io::BufReader<&mut TcpStream>,
    secs: u64,
    mut done: impl FnMut(&[String]) -> bool,
) -> Vec<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    let mut lines = Vec::new();
    let mut line = String::new();
    while tokio::time::Instant::now() < deadline {
        line.clear();
        let read = timeout(Duration::from_millis(400), reader.read_line(&mut line)).await;
        if read.is_err() {
            if done(&lines) {
                break;
            }
            continue;
        }
        if line.is_empty() {
            break;
        }
        lines.push(line.trim_end().to_string());
        if done(&lines) {
            break;
        }
    }
    lines
}

async fn status_json(port: u16) -> Option<serde_json::Value> {
    let resp = reqwest::get(format!("http://127.0.0.1:{port}/status"))
        .await
        .ok()?;
    resp.json().await.ok()
}

async fn wait_radio(ports: &[u16]) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
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

fn internet_cfg(callsign: &str, home: &Path, hub_ws: &str, irc: u16, status: u16) -> Config {
    let mut cfg = base_cfg(callsign, home, Mode::Internet);
    cfg.hub.url = hub_ws.into();
    cfg.irc.bind = format!("127.0.0.1:{irc}");
    cfg.status.bind = format!("127.0.0.1:{status}");
    cfg.normalize();
    cfg
}

async fn start_hub() -> u16 {
    let hub_port = reserve_ports(1).await[0];
    let hub_bind = format!("127.0.0.1:{hub_port}");
    let store = std::sync::Arc::new(wcr::store::Store::open_memory().unwrap());
    let tel = std::sync::Arc::new(wcr::telemetry::TelemetryDb::open_memory().unwrap());
    let keys = wcr::proto::IdentityKeys::generate();
    tokio::spawn(async move {
        let _ = wcr::net::run_hub(&hub_bind, store, tel, keys).await;
    });
    wait_tcp(hub_port, 15).await;
    hub_port
}

async fn wait_hub_ok(ports: &[u16]) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline {
        let mut ok = true;
        for p in ports {
            let up = status_json(*p)
                .await
                .and_then(|v| v.get("hub_ok").and_then(|x| x.as_bool()))
                .unwrap_or(false);
            ok &= up;
        }
        if ok {
            return;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    panic!("hub_ok never came up (ports {ports:?})");
}

/// The station tags a long line and splits it; the far end joins the pieces
/// back into exactly what was typed, emoji included.
#[tokio::test]
async fn a_long_emoji_line_crosses_the_air_as_tagged_pieces_with_one_mark() {
    let air = wcr::sim::SharedAir::new(0.0);
    let mut nodes = Vec::new();
    let mut irc = Vec::new();
    let mut status = Vec::new();
    let mut prep = Vec::new();
    for call in ["G4AAA", "M0BBB"] {
        let home = scratch(call);
        let (kiss, ctrl) = wcr::sim::mock_modem73_free(air.clone()).await.unwrap();
        prep.push((call, home, kiss, ctrl));
    }
    let ports = reserve_ports(prep.len() * 2).await;
    for (i, (call, home, kiss, ctrl)) in prep.into_iter().enumerate() {
        let irc_p = ports[i * 2];
        let status_p = ports[i * 2 + 1];
        let cfg = radio_cfg(call, &home, kiss, ctrl, irc_p, status_p);
        nodes.push(tokio::spawn(async move {
            wcr::node::run_node(cfg, false).await
        }));
        irc.push(irc_p);
        status.push(status_p);
    }
    for p in &irc {
        wait_tcp(*p, 20).await;
    }
    wait_radio(&status).await;

    let mut sender = TcpStream::connect(format!("127.0.0.1:{}", irc[0]))
        .await
        .unwrap();
    irc_register(&mut sender, "G4AAA").await;
    let mut receiver = TcpStream::connect(format!("127.0.0.1:{}", irc[1]))
        .await
        .unwrap();
    irc_register(&mut receiver, "M0BBB").await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // A short emoji line first: plain, skin tone and family (ZWJ).
    let short = "73 📻 ✅ 👍 👍🏽 👨‍👩‍👧";
    sender
        .write_all(format!("PRIVMSG M0BBB :{short}\r\n").as_bytes())
        .await
        .unwrap();
    // Then a long one: ~700 bytes, with a run of emoji and no spaces so at
    // least one cut has to fall between emoji.
    let long = format!(
        "{} {} {}",
        "Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium, totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo.",
        "👍🏽👨‍👩‍👧📻✅".repeat(14),
        "Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos qui ratione voluptatem sequi nesciunt. 73"
    );
    sender
        .write_all(format!("PRIVMSG M0BBB :{long}\r\n").as_bytes())
        .await
        .unwrap();

    let mut recv_reader = tokio::io::BufReader::new(&mut receiver);
    let got = read_lines_until(&mut recv_reader, 90, |lines| {
        let bodies: Vec<&str> = lines
            .iter()
            .filter_map(|l| privmsg(l))
            .map(|p| p.2)
            .collect();
        let pieces: Vec<&str> = bodies
            .iter()
            .copied()
            .filter(|b| wcr::chat_parts::parse(b).is_some())
            .collect();
        bodies.contains(&short)
            && pieces
                .first()
                .and_then(|b| wcr::chat_parts::parse(b))
                .is_some_and(|(_, n, _)| pieces.len() >= n)
    })
    .await;
    let bodies: Vec<String> = got
        .iter()
        .filter_map(|l| privmsg(l))
        .map(|p| p.2.to_string())
        .collect();
    assert!(
        bodies.iter().any(|b| b == short),
        "short emoji line lost or changed: {bodies:?}"
    );
    let pieces: Vec<&String> = bodies
        .iter()
        .filter(|b| wcr::chat_parts::parse(b).is_some())
        .collect();
    assert!(pieces.len() >= 3, "expected 3+ pieces: {bodies:?}");
    let n = wcr::chat_parts::parse(pieces[0]).unwrap().1;
    assert_eq!(pieces.len(), n, "{bodies:?}");
    let mut parts = wcr::chat_parts::Parts::new(n, 0);
    for p in &pieces {
        assert!(
            p.len() <= wcr::proto::MAX_BODY,
            "piece over {} bytes: {}",
            wcr::proto::MAX_BODY,
            p.len()
        );
        assert!(!p.contains('\u{FFFD}'), "a character was cut in half: {p}");
        let (i, n, text) = wcr::chat_parts::parse(p).unwrap();
        assert!(parts.add(i, n, text, 0), "piece {i}/{n} twice");
    }
    assert_eq!(parts.text(), long, "joined text differs from what was sent");

    // The sender: one mark for the whole message, on the first piece's id,
    // with the pieces on the air counted up to n/n and delivered at the end.
    let mut send_reader = tokio::io::BufReader::new(&mut sender);
    let want = format!("+radio/parts={n}/{n}");
    let marks = read_lines_until(&mut send_reader, 90, |lines| {
        lines
            .iter()
            .any(|l| l.contains(&want) && l.contains("+radio/delivery=delivered"))
    })
    .await;
    let part_marks: Vec<&String> = marks
        .iter()
        .filter(|l| l.contains("+radio/parts="))
        .collect();
    assert!(
        part_marks
            .iter()
            .any(|l| l.contains(&want) && l.contains("+radio/delivery=delivered")),
        "no delivered n/n mark: {marks:?}"
    );
    let ids: std::collections::HashSet<&str> = part_marks
        .iter()
        .filter_map(|l| l.split("+radio/msgid=").nth(1))
        .map(|r| r.split([';', ' ']).next().unwrap_or(""))
        .collect();
    assert_eq!(ids.len(), 1, "marks for more than one id: {part_marks:?}");
    for node in nodes {
        node.abort();
    }
}

/// The same over the hub: emoji and the piece tags arrive unchanged.
#[tokio::test]
async fn a_long_emoji_line_crosses_the_hub_unchanged() {
    let hub_port = start_hub().await;
    let hub_ws = format!("ws://127.0.0.1:{hub_port}/ws");
    let ports = reserve_ports(4).await;
    let mut nodes = Vec::new();
    for (i, call) in ["G4AAA", "M0BBB"].into_iter().enumerate() {
        let cfg = internet_cfg(
            call,
            &scratch(call),
            &hub_ws,
            ports[i * 2],
            ports[i * 2 + 1],
        );
        nodes.push(tokio::spawn(async move {
            wcr::node::run_node(cfg, false).await
        }));
    }
    wait_tcp(ports[0], 20).await;
    wait_tcp(ports[2], 20).await;
    wait_hub_ok(&[ports[1], ports[3]]).await;
    let mut sender = TcpStream::connect(format!("127.0.0.1:{}", ports[0]))
        .await
        .unwrap();
    irc_register(&mut sender, "G4AAA").await;
    let mut receiver = TcpStream::connect(format!("127.0.0.1:{}", ports[2]))
        .await
        .unwrap();
    irc_register(&mut receiver, "M0BBB").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let long = format!("hub test {} end 73 📻", "👍🏽 👨‍👩‍👧 ✅ ".repeat(30));
    sender
        .write_all(format!("PRIVMSG M0BBB :{long}\r\n").as_bytes())
        .await
        .unwrap();
    let mut recv_reader = tokio::io::BufReader::new(&mut receiver);
    let got = read_lines_until(&mut recv_reader, 30, |lines| {
        let pieces: Vec<&str> = lines
            .iter()
            .filter_map(|l| privmsg(l))
            .map(|p| p.2)
            .filter(|b| wcr::chat_parts::parse(b).is_some())
            .collect();
        pieces
            .first()
            .and_then(|b| wcr::chat_parts::parse(b))
            .is_some_and(|(_, n, _)| pieces.len() >= n)
    })
    .await;
    let pieces: Vec<String> = got
        .iter()
        .filter_map(|l| privmsg(l))
        .map(|p| p.2.to_string())
        .filter(|b| wcr::chat_parts::parse(b).is_some())
        .collect();
    assert!(pieces.len() >= 2, "{got:?}");
    let n = wcr::chat_parts::parse(&pieces[0]).unwrap().1;
    let mut parts = wcr::chat_parts::Parts::new(n, 0);
    for p in &pieces {
        let (i, n, text) = wcr::chat_parts::parse(p).unwrap();
        parts.add(i, n, text, 0);
    }
    assert_eq!(parts.text(), long);
    for node in nodes {
        node.abort();
    }
}
