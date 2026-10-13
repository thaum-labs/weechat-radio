//! Two internet-mode nodes on a local hub: receiver IRC client sees one tagged PRIVMSG.

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use wcr::config::{self, Config};
use wcr::modes::Mode;
use wcr::proto::IdentityKeys;

/// Bind `n` ports at once so they are a free set, not `base+1` neighbours that
/// another test's `bind(0)` claims the moment the probe socket is dropped.
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

/// True once `port` accepts. Returns as soon as `died` says the owner exited,
/// instead of sitting out the whole timeout on a port that will never open.
async fn wait_listen(port: u16, secs: u64, died: impl Fn() -> bool) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < deadline {
        if died() {
            return Err(format!("{addr} closed before it accepted a connection"));
        }
        match TcpStream::connect(&addr).await {
            Ok(_) => return Ok(()),
            Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
        }
    }
    Err(format!("timed out waiting for {addr}"))
}

fn internet_node_cfg(
    callsign: &str,
    home: &std::path::Path,
    hub_ws: &str,
    hub_report: &str,
    irc_port: u16,
    status_port: u16,
) -> Config {
    let mut cfg = Config {
        callsign: callsign.into(),
        grid: "IO91WM".into(),
        mode: Mode::Internet,
        hub: wcr::config::HubConfig {
            url: hub_ws.into(),
            ..Default::default()
        },
        telemetry: wcr::config::TelemetryConfig {
            url: hub_report.into(),
            interval_secs: 2,
        },
        modem: wcr::config::ModemConfig {
            manage: false,
            ptt: "none".into(),
            ..Default::default()
        },
        irc: wcr::config::IrcConfig {
            bind: format!("127.0.0.1:{irc_port}"),
        },
        status: wcr::config::StatusConfig {
            bind: format!("127.0.0.1:{status_port}"),
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

async fn status_hub_ok(client: &reqwest::Client, port: u16) -> bool {
    let resp = client
        .get(format!("http://127.0.0.1:{port}/status"))
        .send()
        .await
        .ok();
    let Some(resp) = resp else {
        return false;
    };
    let v = resp.json::<serde_json::Value>().await.ok();
    v.and_then(|v| v.get("hub_ok").and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

async fn irc_register(stream: &mut TcpStream, nick: &str) {
    let burst = format!(
        "CAP LS\r\nNICK {nick}\r\nUSER {nick} 0 * :wcr test\r\nCAP REQ :message-tags echo-message server-time msgid\r\nCAP END\r\nJOIN #bulletin\r\n"
    );
    stream.write_all(burst.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}

#[tokio::test]
async fn hub_bulletin_one_privmsg_at_receiver() {
    let base = std::env::temp_dir().join(format!("wcr-hub-irc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::env::set_var("WCR_HOME", &base);
    config::ensure_dirs().unwrap();

    let client = reqwest::Client::new();
    let mut last_err = String::from("no attempt");
    let (irc_a, irc_b, status_a, status_b, node_a, node_b, hub) = 'start: {
        for attempt in 1..=5 {
            let ports = reserve_ports(5).await;
            let hub_port = ports[0];
            let irc_a = ports[1];
            let irc_b = ports[2];
            let status_a = ports[3];
            let status_b = ports[4];
            let hub_bind = format!("127.0.0.1:{hub_port}");
            let store = std::sync::Arc::new(wcr::store::Store::open_memory().unwrap());
            let tel = std::sync::Arc::new(wcr::telemetry::TelemetryDb::open_memory().unwrap());
            let keys = IdentityKeys::generate();
            let hub = tokio::spawn(async move {
                let _ = wcr::net::run_hub(&hub_bind, store, tel, keys).await;
            });
            if let Err(e) = wait_listen(hub_port, 5, || hub.is_finished()).await {
                hub.abort();
                last_err = format!("attempt {attempt} hub: {e}");
                continue;
            }
            let hub_ws = format!("ws://127.0.0.1:{hub_port}/ws");
            let hub_report = format!("http://127.0.0.1:{hub_port}/api/v1/report");
            let home_a = base.join(format!("a-{attempt}"));
            let home_b = base.join(format!("b-{attempt}"));
            std::fs::create_dir_all(&home_a).unwrap();
            std::fs::create_dir_all(&home_b).unwrap();
            let cfg_a =
                internet_node_cfg("~QAONE1", &home_a, &hub_ws, &hub_report, irc_a, status_a);
            let cfg_b =
                internet_node_cfg("~QATWO2", &home_b, &hub_ws, &hub_report, irc_b, status_b);
            let node_a = tokio::spawn(async move { wcr::node::run_node(cfg_a, false).await });
            let node_b = tokio::spawn(async move { wcr::node::run_node(cfg_b, false).await });
            let died = || node_a.is_finished() || node_b.is_finished() || hub.is_finished();
            if let Err(e) = wait_listen(irc_a, 8, &died).await {
                node_a.abort();
                node_b.abort();
                hub.abort();
                last_err = format!("attempt {attempt} irc {irc_a}: {e}");
                continue;
            }
            if let Err(e) = wait_listen(irc_b, 8, &died).await {
                node_a.abort();
                node_b.abort();
                hub.abort();
                last_err = format!("attempt {attempt} irc {irc_b}: {e}");
                continue;
            }
            break 'start (irc_a, irc_b, status_a, status_b, node_a, node_b, hub);
        }
        panic!("bulletin nodes did not open their ports: {last_err}");
    };

    let hub_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < hub_deadline {
        let ok_a = status_hub_ok(&client, status_a).await;
        let ok_b = status_hub_ok(&client, status_b).await;
        if ok_a && ok_b {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let mut sender = TcpStream::connect(format!("127.0.0.1:{irc_a}"))
        .await
        .unwrap();
    irc_register(&mut sender, "~QAONE1").await;

    let mut receiver = TcpStream::connect(format!("127.0.0.1:{irc_b}"))
        .await
        .unwrap();
    irc_register(&mut receiver, "~QATWO2").await;

    let token = "hub-dedupe-qa";
    sender
        .write_all(format!("PRIVMSG #bulletin :{token}\r\n").as_bytes())
        .await
        .unwrap();
    sender.flush().await.unwrap();

    let mut reader = tokio::io::BufReader::new(receiver);
    let mut privmsg_hits = 0u32;
    let mut tagged_ok = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut line = String::new();
    while tokio::time::Instant::now() < deadline {
        line.clear();
        let read = timeout(Duration::from_millis(500), reader.read_line(&mut line)).await;
        if read.is_err() {
            continue;
        }
        if !line.contains(token) {
            continue;
        }
        if line.contains("PRIVMSG #bulletin") || line.contains("PRIVMSG #BULLETIN") {
            privmsg_hits += 1;
            let body = line.trim_end();
            assert!(
                body.ends_with(":hub-dedupe-qa"),
                "exact body missing: {body}"
            );
            assert!(
                !body.contains("hub-dedupe-qaPRIVMSG") && !body.contains("hub-dedupe-qa\r"),
                "doubled IRC line: {body}"
            );
            if line.starts_with('@')
                && line.contains("msgid=")
                && line.contains(":~QAONE1 PRIVMSG")
                && line.contains("+radio/via=inet")
            {
                tagged_ok = true;
            }
            if line.trim_start().starts_with("PRIVMSG") {
                panic!("bare PRIVMSG without nick prefix: {line}");
            }
        }
        if privmsg_hits >= 1 && tagged_ok {
            break;
        }
    }

    node_a.abort();
    node_b.abort();
    hub.abort();
    let _ = node_a.await;
    let _ = node_b.await;
    let _ = hub.await;

    assert!(tagged_ok, "expected one tagged inet PRIVMSG at receiver");
    assert_eq!(
        privmsg_hits, 1,
        "expected exactly one bulletin PRIVMSG line at receiver"
    );
}
