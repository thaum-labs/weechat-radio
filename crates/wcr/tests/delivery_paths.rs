//! Delivery paths: DM display, hub ACK, bulletin [ok], group [all], relay, refusals.

use std::collections::HashSet;
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
    let p = std::env::temp_dir().join(format!("wcr-deliv-{name}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

async fn port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
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

fn internet_cfg(callsign: &str, home: &Path, hub_ws: &str, irc: u16, status: u16) -> Config {
    let mut cfg = base_cfg(callsign, home, Mode::Internet);
    cfg.hub.url = hub_ws.into();
    cfg.irc.bind = format!("127.0.0.1:{irc}");
    cfg.status.bind = format!("127.0.0.1:{status}");
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
    cfg.rf.emergency_dup_ms = 60_000;
    cfg.rf.frequency_khz = 144_950;
    cfg.normalize();
    cfg
}

async fn start_hub() -> u16 {
    let hub_port = port().await;
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

async fn wait_hub_ok(ports: &[u16]) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
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
    panic!("hub_ok never came up");
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

#[tokio::test]
async fn hub_dm_one_line_and_sender_ack() {
    for (from, to, body) in [
        ("~QAONE1", "~QATWO2", "guest-dm-body"),
        ("G4ABC", "M0XYZ", "licensed-dm-body"),
    ] {
        let hub_port = start_hub().await;
        let hubs = reqwest::get(format!("http://127.0.0.1:{hub_port}/api/v1/hubs"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            !hubs.contains("hub.weechatradio.com"),
            "private hub reported the public id: {hubs}"
        );
        assert!(hubs.contains("local:"), "{hubs}");

        let hub_ws = format!("ws://127.0.0.1:{hub_port}/ws");
        let home_a = scratch("dm-a");
        let home_b = scratch("dm-b");
        let irc_a = port().await;
        let irc_b = port().await;
        let status_a = port().await;
        let status_b = port().await;
        let cfg_a = internet_cfg(from, &home_a, &hub_ws, irc_a, status_a);
        let cfg_b = internet_cfg(to, &home_b, &hub_ws, irc_b, status_b);
        let node_a = tokio::spawn(async move { wcr::node::run_node(cfg_a, false).await });
        let node_b = tokio::spawn(async move { wcr::node::run_node(cfg_b, false).await });
        wait_tcp(irc_a, 20).await;
        wait_tcp(irc_b, 20).await;
        wait_hub_ok(&[status_a, status_b]).await;

        let mut sender = TcpStream::connect(format!("127.0.0.1:{irc_a}"))
            .await
            .unwrap();
        irc_register(&mut sender, from).await;
        let mut receiver = TcpStream::connect(format!("127.0.0.1:{irc_b}"))
            .await
            .unwrap();
        irc_register(&mut receiver, to).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        sender
            .write_all(format!("PRIVMSG {to} :{body}\r\n").as_bytes())
            .await
            .unwrap();

        let mut recv_reader = tokio::io::BufReader::new(&mut receiver);
        let got = read_lines_until(&mut recv_reader, 20, |lines| {
            lines
                .iter()
                .any(|l| privmsg(l).is_some_and(|p| p.2 == body))
        })
        .await;
        let hits: Vec<_> = got
            .iter()
            .filter_map(|l| privmsg(l))
            .filter(|p| p.2 == body)
            .collect();
        assert_eq!(hits.len(), 1, "{from}->{to} lines: {got:?}");
        assert_eq!(hits[0].0, from, "{got:?}");
        assert_eq!(hits[0].1, to, "{got:?}");
        assert_eq!(hits[0].2, body, "{got:?}");

        let mut send_reader = tokio::io::BufReader::new(&mut sender);
        let sent = read_lines_until(&mut send_reader, 20, |lines| {
            lines
                .iter()
                .any(|l| l.contains("+radio/delivery=delivered"))
        })
        .await;
        assert!(
            sent.iter().any(|l| l.contains("+radio/delivery=delivered")),
            "{from} never got [ok]: {sent:?}"
        );
        node_a.abort();
        node_b.abort();
    }
}

#[tokio::test]
async fn internet_bulletin_acks_once_and_not_all() {
    let hub_port = start_hub().await;
    let hub_ws = format!("ws://127.0.0.1:{hub_port}/ws");
    let mut nodes = Vec::new();
    let mut irc_ports = Vec::new();
    let mut status_ports = Vec::new();
    for call in ["G4AAA", "M0BBB", "2E0CCC"] {
        let home = scratch(call);
        let irc = port().await;
        let status = port().await;
        let cfg = internet_cfg(call, &home, &hub_ws, irc, status);
        nodes.push(tokio::spawn(async move {
            wcr::node::run_node(cfg, false).await
        }));
        irc_ports.push(irc);
        status_ports.push(status);
    }
    for p in &irc_ports {
        wait_tcp(*p, 20).await;
    }
    wait_hub_ok(&status_ports).await;

    let mut sender = TcpStream::connect(format!("127.0.0.1:{}", irc_ports[0]))
        .await
        .unwrap();
    irc_register(&mut sender, "G4AAA").await;
    sender.write_all(b"JOIN #bulletin\r\n").await.unwrap();
    let mut others = Vec::new();
    for (call, p) in ["M0BBB", "2E0CCC"].into_iter().zip(irc_ports[1..].iter()) {
        let mut s = TcpStream::connect(format!("127.0.0.1:{p}")).await.unwrap();
        irc_register(&mut s, call).await;
        s.write_all(b"JOIN #bulletin\r\n").await.unwrap();
        others.push(s);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let body = "bulletin-once";
    sender
        .write_all(format!("PRIVMSG #bulletin :{body}\r\n").as_bytes())
        .await
        .unwrap();

    for s in &mut others {
        let mut reader = tokio::io::BufReader::new(&mut *s);
        let got = read_lines_until(&mut reader, 20, |lines| {
            lines
                .iter()
                .any(|l| privmsg(l).is_some_and(|p| p.2 == body))
        })
        .await;
        let n = got
            .iter()
            .filter(|l| privmsg(l).is_some_and(|p| p.2 == body))
            .count();
        assert_eq!(n, 1, "{got:?}");
    }
    let mut send_reader = tokio::io::BufReader::new(&mut sender);
    let mut sent = read_lines_until(&mut send_reader, 20, |lines| {
        lines
            .iter()
            .any(|l| l.contains("+radio/delivery=delivered"))
    })
    .await;
    // Drain a little longer so a second receiver's ACK would show up.
    let drain_until = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut extra = String::new();
    while tokio::time::Instant::now() < drain_until {
        extra.clear();
        if timeout(
            Duration::from_millis(200),
            send_reader.read_line(&mut extra),
        )
        .await
        .is_err()
        {
            continue;
        }
        if extra.is_empty() {
            break;
        }
        sent.push(extra.trim_end().to_string());
    }
    let delivered = sent
        .iter()
        .filter(|l| l.contains("+radio/delivery=delivered"))
        .count();
    let all = sent
        .iter()
        .filter(|l| l.contains("+radio/delivery=all"))
        .count();
    assert_eq!(delivered, 1, "duplicate delivered: {sent:?}");
    assert_eq!(all, 0, "public bulletin must not reach [all]: {sent:?}");
    for n in nodes {
        n.abort();
    }
}

#[tokio::test]
async fn radio_bulletin_reaches_ok_and_dm_ack_cancels_retry() {
    let air = wcr::sim::SharedAir::new(0.0);
    let mut nodes = Vec::new();
    let mut irc = Vec::new();
    let mut status = Vec::new();
    for call in ["G4AAA", "M0BBB"] {
        let home = scratch(call);
        let kiss = port().await;
        let ctrl = port().await;
        let irc_p = port().await;
        let status_p = port().await;
        wcr::sim::mock_modem73(
            &format!("127.0.0.1:{kiss}"),
            &format!("127.0.0.1:{ctrl}"),
            air.clone(),
        )
        .await
        .unwrap();
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
    sender.write_all(b"JOIN #bulletin\r\n").await.unwrap();
    let mut receiver = TcpStream::connect(format!("127.0.0.1:{}", irc[1]))
        .await
        .unwrap();
    irc_register(&mut receiver, "M0BBB").await;
    receiver.write_all(b"JOIN #bulletin\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let body = "rf-bulletin-body";
    sender
        .write_all(format!("PRIVMSG #bulletin :{body}\r\n").as_bytes())
        .await
        .unwrap();
    let mut recv_reader = tokio::io::BufReader::new(&mut receiver);
    let got = read_lines_until(&mut recv_reader, 25, |lines| {
        lines
            .iter()
            .any(|l| privmsg(l).is_some_and(|p| p.2 == body))
    })
    .await;
    assert_eq!(
        got.iter()
            .filter(|l| privmsg(l).is_some_and(|p| p.2 == body))
            .count(),
        1,
        "B did not get the bulletin: {got:?}"
    );

    let mut sent = {
        let mut send_reader = tokio::io::BufReader::new(&mut sender);
        read_lines_until(&mut send_reader, 25, |lines| {
            lines
                .iter()
                .any(|l| l.contains("+radio/delivery=delivered"))
        })
        .await
    };
    assert!(
        sent.iter().any(|l| l.contains("+radio/delivery=delivered")),
        "bulletin never reached [ok]: {sent:?}"
    );
    assert!(
        !sent.iter().any(|l| l.contains("+radio/delivery=all")),
        "bulletin reached [all]: {sent:?}"
    );

    // Emergency DM: retry hold is 8s. After [ok], that retry must not be tagged.
    // The body must not contain the tag text the assertion searches for.
    let dm = "ack-stops-resend";
    sender
        .write_all(format!("PRIVMSG M0BBB :!!{dm}\r\n").as_bytes())
        .await
        .unwrap();
    let mut send_reader = tokio::io::BufReader::new(&mut sender);
    let rest = read_lines_until(&mut send_reader, 14, |lines| {
        lines.iter().any(|l| l.contains("+radio/delivery=retry-"))
    })
    .await;
    sent.extend(rest);
    let delivered = sent
        .iter()
        .filter(|l| l.contains("+radio/delivery=delivered"))
        .count();
    assert!(
        delivered >= 2,
        "DM after the bulletin did not reach [ok]: {sent:?}"
    );
    assert!(
        !sent.iter().any(|l| l.contains("+radio/delivery=retry-")),
        "retry after ACK: {sent:?}"
    );
    for n in nodes {
        n.abort();
    }
}

#[tokio::test]
async fn radio_group_reaches_all() {
    let air = wcr::sim::SharedAir::new(0.0);
    let members = ["G4AAA", "M0BBB", "2E0CCC"];
    let mut nodes = Vec::new();
    let mut irc = Vec::new();
    let mut status = Vec::new();
    for call in members {
        let home = scratch(call);
        let db = home.join("store.db");
        let store = wcr::store::Store::open(&db, 72, 10_000).unwrap();
        store
            .group_create("net", &members.map(|s| s.to_string()))
            .unwrap();
        drop(store);
        let kiss = port().await;
        let ctrl = port().await;
        let irc_p = port().await;
        let status_p = port().await;
        wcr::sim::mock_modem73(
            &format!("127.0.0.1:{kiss}"),
            &format!("127.0.0.1:{ctrl}"),
            air.clone(),
        )
        .await
        .unwrap();
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
    sender.write_all(b"JOIN #net\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    sender
        .write_all(b"PRIVMSG #net :group-all-body\r\n")
        .await
        .unwrap();
    let mut reader = tokio::io::BufReader::new(&mut sender);
    let got = read_lines_until(&mut reader, 30, |lines| {
        lines.iter().any(|l| l.contains("+radio/delivery=all"))
    })
    .await;
    assert!(
        got.iter().any(|l| l.contains("+radio/delivery=all")),
        "group never reached [all]: {got:?}"
    );
    for n in nodes {
        n.abort();
    }
}

#[tokio::test]
async fn relay_through_middle_station_acks_sender() {
    let air = wcr::sim::SharedAir::new(0.0);
    // A (1) hears only B. C (3) hears only B. B hears both.
    let plan = [
        ("G4AAA", 1u64, HashSet::from([2u64])),
        ("M0BBB", 2, HashSet::from([1u64, 3])),
        ("2E0CCC", 3, HashSet::from([2u64])),
    ];
    let mut nodes = Vec::new();
    let mut irc = Vec::new();
    let mut status = Vec::new();
    for (call, station, hear) in plan {
        let home = scratch(call);
        let kiss = port().await;
        let ctrl = port().await;
        let irc_p = port().await;
        let status_p = port().await;
        wcr::sim::mock_modem73_linked(
            &format!("127.0.0.1:{kiss}"),
            &format!("127.0.0.1:{ctrl}"),
            air.clone(),
            station,
            hear,
        )
        .await
        .unwrap();
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
    let mut far = TcpStream::connect(format!("127.0.0.1:{}", irc[2]))
        .await
        .unwrap();
    irc_register(&mut far, "2E0CCC").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let body = "relay-body";
    sender
        .write_all(format!("PRIVMSG 2E0CCC :{body}\r\n").as_bytes())
        .await
        .unwrap();
    let mut far_reader = tokio::io::BufReader::new(&mut far);
    let got = read_lines_until(&mut far_reader, 40, |lines| {
        lines
            .iter()
            .any(|l| privmsg(l).is_some_and(|p| p.2 == body))
    })
    .await;
    let copies = got
        .iter()
        .filter(|l| {
            privmsg(l).is_some_and(|(from, target, text)| {
                from == "G4AAA" && target == "2E0CCC" && text == body
            })
        })
        .count();
    assert_eq!(copies, 1, "C hears only B, so one relayed copy: {got:?}");
    let mut send_reader = tokio::io::BufReader::new(&mut sender);
    let sent = read_lines_until(&mut send_reader, 40, |lines| {
        lines
            .iter()
            .any(|l| l.contains("+radio/delivery=delivered"))
    })
    .await;
    assert!(
        sent.iter().any(|l| l.contains("+radio/delivery=delivered")),
        "A did not get a receipt via the relay: {sent:?}"
    );
    for n in nodes {
        n.abort();
    }
}

#[tokio::test]
async fn dm_to_the_relay_is_not_copied_onward() {
    let air = wcr::sim::SharedAir::new(0.0);
    // C hears only B. A's DM to B must stop at B.
    let plan = [
        ("G4AAA", 1u64, HashSet::from([2u64])),
        ("M0BBB", 2, HashSet::from([1u64, 3])),
        ("2E0CCC", 3, HashSet::from([2u64])),
    ];
    let mut nodes = Vec::new();
    let mut irc = Vec::new();
    let mut status = Vec::new();
    for (call, station, hear) in plan {
        let home = scratch(call);
        let kiss = port().await;
        let ctrl = port().await;
        let irc_p = port().await;
        let status_p = port().await;
        wcr::sim::mock_modem73_linked(
            &format!("127.0.0.1:{kiss}"),
            &format!("127.0.0.1:{ctrl}"),
            air.clone(),
            station,
            hear,
        )
        .await
        .unwrap();
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
    let mut middle = TcpStream::connect(format!("127.0.0.1:{}", irc[1]))
        .await
        .unwrap();
    irc_register(&mut middle, "M0BBB").await;
    let mut far = TcpStream::connect(format!("127.0.0.1:{}", irc[2]))
        .await
        .unwrap();
    irc_register(&mut far, "2E0CCC").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let body = "stop-at-b";
    sender
        .write_all(format!("PRIVMSG M0BBB :{body}\r\n").as_bytes())
        .await
        .unwrap();
    let mut mid_reader = tokio::io::BufReader::new(&mut middle);
    let at_b = read_lines_until(&mut mid_reader, 20, |lines| {
        lines
            .iter()
            .any(|l| privmsg(l).is_some_and(|p| p.2 == body))
    })
    .await;
    assert_eq!(
        at_b.iter()
            .filter(|l| privmsg(l).is_some_and(|(from, target, text)| {
                from == "G4AAA" && target == "M0BBB" && text == body
            }))
            .count(),
        1,
        "B should see the DM once: {at_b:?}"
    );
    let mut far_reader = tokio::io::BufReader::new(&mut far);
    let at_c = read_lines_until(&mut far_reader, 8, |lines| {
        lines
            .iter()
            .any(|l| privmsg(l).is_some_and(|p| p.2 == body))
    })
    .await;
    assert!(
        !at_c.iter().any(|l| privmsg(l).is_some_and(|p| p.2 == body)),
        "C heard a DM that was addressed to B: {at_c:?}"
    );
    for n in nodes {
        n.abort();
    }
}

#[tokio::test]
async fn internet_closed_group_reaches_all() {
    let hub_port = start_hub().await;
    let hub_ws = format!("ws://127.0.0.1:{hub_port}/ws");
    let members = ["G4AAA", "M0BBB", "2E0CCC"];
    let mut nodes = Vec::new();
    let mut irc = Vec::new();
    let mut status = Vec::new();
    for call in members {
        let home = scratch(call);
        let db = home.join("store.db");
        let store = wcr::store::Store::open(&db, 72, 10_000).unwrap();
        store
            .group_create("net", &members.map(|s| s.to_string()))
            .unwrap();
        drop(store);
        let irc_p = port().await;
        let status_p = port().await;
        let cfg = internet_cfg(call, &home, &hub_ws, irc_p, status_p);
        nodes.push(tokio::spawn(async move {
            wcr::node::run_node(cfg, false).await
        }));
        irc.push(irc_p);
        status.push(status_p);
    }
    for p in &irc {
        wait_tcp(*p, 20).await;
    }
    wait_hub_ok(&status).await;
    let mut sender = TcpStream::connect(format!("127.0.0.1:{}", irc[0]))
        .await
        .unwrap();
    irc_register(&mut sender, "G4AAA").await;
    sender.write_all(b"JOIN #net\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    sender
        .write_all(b"PRIVMSG #net :hub-group-all\r\n")
        .await
        .unwrap();
    let mut reader = tokio::io::BufReader::new(&mut sender);
    let got = read_lines_until(&mut reader, 25, |lines| {
        lines.iter().any(|l| l.contains("+radio/delivery=all"))
    })
    .await;
    assert!(
        got.iter().any(|l| l.contains("+radio/delivery=all")),
        "closed group over the hub never reached [all]: {got:?}"
    );
    assert!(
        got.iter().any(|l| l.contains("+radio/delivery=delivered")),
        "closed group over the hub never reached [ok]: {got:?}"
    );
    for n in nodes {
        n.abort();
    }
}

#[tokio::test]
async fn hub_claim_is_a_failed_send_not_sent() {
    let hub_port = start_hub().await;
    let hub_ws = format!("ws://127.0.0.1:{hub_port}/ws");
    let mut nodes = Vec::new();
    let mut irc_ports = Vec::new();
    let mut status_ports = Vec::new();
    for name in ["first", "second"] {
        let home = scratch(name);
        let irc = port().await;
        let status = port().await;
        let cfg = internet_cfg("~QAONE1", &home, &hub_ws, irc, status);
        nodes.push(tokio::spawn(async move {
            wcr::node::run_node(cfg, false).await
        }));
        irc_ports.push(irc);
        status_ports.push(status);
    }
    for p in &irc_ports {
        wait_tcp(*p, 20).await;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut banner = String::new();
    while tokio::time::Instant::now() < deadline {
        if let Some(v) = status_json(status_ports[1]).await {
            banner = v
                .get("hub_banner")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if banner.contains("already claimed") {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    assert!(
        banner.contains("already claimed"),
        "second station banner: {banner}"
    );
    let mut sender = TcpStream::connect(format!("127.0.0.1:{}", irc_ports[1]))
        .await
        .unwrap();
    irc_register(&mut sender, "~QAONE1").await;
    sender
        .write_all(b"PRIVMSG #bulletin :should-fail\r\n")
        .await
        .unwrap();
    let mut reader = tokio::io::BufReader::new(&mut sender);
    let got = read_lines_until(&mut reader, 10, |lines| {
        lines.iter().any(|l| l.contains("+radio/delivery=failed"))
            && lines.iter().any(|l| l.contains("already claimed"))
    })
    .await;
    assert!(got.iter().any(|l| l.contains("already claimed")), "{got:?}");
    assert!(
        got.iter().any(|l| l.contains("+radio/delivery=failed")),
        "{got:?}"
    );
    assert!(
        !got.iter().any(|l| l.contains("+radio/delivery=sent")),
        "refused send was marked sent: {got:?}"
    );
    for n in nodes {
        n.abort();
    }
}

#[tokio::test]
async fn irc_port_in_use_fails_fast() {
    let held = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p = held.local_addr().unwrap().port();
    let home = scratch("irc-busy");
    let mut cfg = base_cfg("G4ABC", &home, Mode::Internet);
    cfg.irc.bind = format!("127.0.0.1:{p}");
    cfg.status.bind = format!("127.0.0.1:{}", port().await);
    let err = wcr::node::run_node(cfg, false)
        .await
        .expect_err("node should refuse a busy IRC port");
    let msg = err.to_string();
    assert!(msg.contains(&p.to_string()), "{msg}");
    assert!(msg.contains("already in use"), "{msg}");
    drop(held);
}
