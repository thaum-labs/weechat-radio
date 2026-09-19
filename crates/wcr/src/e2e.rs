//! SPDX-License-Identifier: Apache-2.0
//! Paired LAN end-to-end test: two machines run `wcr e2e lan`.

use crate::config::{
    self, Config, E2E_HUB_PORT, E2E_IRC_BIND, E2E_LAN_PORT, E2E_LAN_SERVICE, E2E_STATUS_BIND,
};
use crate::error::{Error, Result};
use crate::modes::Mode;
use crate::net::lan::{decode_hello, encode_hello};
use crate::proto::Callsign;
use crate::status::StatusSnapshot;
use crate::ui_style;
use rand::Rng;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpStream, UdpSocket};

const TOKEN_PREFIX: &str = "WCR-E2E";
/// Keep sending after we PASS so the slower machine still hears our token.
const PEER_LINGER: Duration = Duration::from_secs(20);

pub async fn run_lan(timeout_secs: u64, port: u16, hub_port: u16) -> Result<()> {
    ui_style::panel("WEECHAT RADIO", "E2E LAN");
    println!(
        "  {}",
        ui_style::dim().apply_to("instructions: wcr help e2e")
    );
    let home = e2e_home();
    let _guard = HomeGuard(home.clone());
    std::env::set_var("WCR_HOME", &home);
    config::ensure_dirs()?;

    let host = hostname();
    let callsign = guest_callsign_from_host(&host);
    let grid = grid_from_host(&host);
    let nonce = hex::encode(rand::thread_rng().gen::<[u8; 8]>());
    let token = format!("{TOKEN_PREFIX} {callsign} {nonce}");
    let lan_port = if port == 0 { E2E_LAN_PORT } else { port };
    let hub_port = if hub_port == 0 {
        E2E_HUB_PORT
    } else {
        hub_port
    };
    let timeout = Duration::from_secs(timeout_secs);

    println!(
        "  {}",
        ui_style::dim().apply_to(format!("callsign {callsign}  grid {grid}"))
    );
    println!(
        "  {}",
        ui_style::dim().apply_to("run the same command on the other machine")
    );

    let elected = elect_hub(&callsign, lan_port, hub_port, timeout).await?;
    let mut hub_task = None;
    if elected.host {
        hub_task = Some(spawn_local_hub(&format!("0.0.0.0:{hub_port}")).await?);
        println!(
            "  {}",
            ui_style::dim().apply_to(format!("this machine is the e2e hub {}", elected.map_http))
        );
    } else {
        println!(
            "  {}",
            ui_style::dim().apply_to(format!("joining e2e hub {}", elected.map_http))
        );
    }
    wait_http_ok(&format!("{}/healthz", elected.local_http), timeout).await?;

    if elected.ws.contains("weechatradio.com") || elected.report.contains("weechatradio.com") {
        return Err(Error::Msg(
            "e2e refused to use the public hub; aborting".into(),
        ));
    }

    let mut cfg = Config {
        callsign: callsign.clone(),
        grid: grid.clone(),
        mode: Mode::Internet,
        hub: crate::config::HubConfig {
            url: elected.ws.clone(),
            ..Default::default()
        },
        telemetry: crate::config::TelemetryConfig {
            url: elected.report.clone(),
            interval_secs: 2,
        },
        lan: crate::config::LanConfig {
            discovery: true,
            port: lan_port,
            service: E2E_LAN_SERVICE.into(),
            hub_advertise: elected.advertise.clone(),
        },
        irc: crate::config::IrcConfig {
            bind: E2E_IRC_BIND.into(),
        },
        status: crate::config::StatusConfig {
            bind: E2E_STATUS_BIND.into(),
        },
        modem: crate::config::ModemConfig {
            manage: false,
            ptt: "none".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    cfg.normalize();
    cfg.save(&Config::default_path())?;

    if !cfg.dials_hub() || !cfg.reports_telemetry() {
        return Err(Error::Msg("e2e hub URLs were empty after normalize".into()));
    }

    println!(
        "  {}",
        ui_style::dim().apply_to(format!(
            "LAN :{lan_port}  hub {}  IRC {}",
            elected.local_http, cfg.irc.bind
        ))
    );

    let cfg_n = cfg.clone();
    let node = tokio::spawn(async move { crate::node::run_node(cfg_n, false).await });

    let deadline = Instant::now() + timeout;
    let outcome = match run_exchange(&cfg, &callsign, &token, deadline).await {
        Ok(ex) => {
            match wait_hub_nodes(&elected.local_http, &callsign, &ex.peer, &grid, deadline).await {
                Ok(hub) => Ok((ex, hub)),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    };
    let snap = fetch_status(&cfg.status.bind).await.ok();

    match outcome {
        Ok((mut ex, hub)) => {
            let peers = snap.as_ref().map(|s| s.lan_peers).unwrap_or(0);
            println!(
                "{} local={callsign} grid={grid} peer={} peer_grid={} lan_peers={peers} hub_ok=true",
                ui_style::ok().apply_to("PASS"),
                ex.peer,
                hub.peer_grid
            );
            println!("  map  (from repo/web)  python -m http.server 5173");
            println!(
                "       then open http://127.0.0.1:5173/?api={}",
                elected.map_http
            );
            println!(
                "  {}",
                ui_style::dim().apply_to("leave this running until the other machine prints PASS")
            );
            ex.linger(PEER_LINGER).await;
            shutdown_e2e(node, hub_task).await;
            Ok(())
        }
        Err(e) => {
            shutdown_e2e(node, hub_task).await;
            if let Some(snap) = snap {
                println!(
                    "{} {e}  lan_peers={} hub_ok={} heard={}",
                    ui_style::err().apply_to("FAIL"),
                    snap.lan_peers,
                    snap.hub_ok,
                    snap.heard
                        .iter()
                        .map(|h| h.callsign.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            } else {
                println!("{} {e}", ui_style::err().apply_to("FAIL"));
            }
            Err(e)
        }
    }
}

async fn shutdown_e2e(
    node: tokio::task::JoinHandle<Result<()>>,
    hub_task: Option<tokio::task::JoinHandle<Result<()>>>,
) {
    node.abort();
    let _ = node.await;
    if let Some(h) = hub_task {
        h.abort();
        let _ = h.await;
    }
}

struct ElectedHub {
    host: bool,
    ws: String,
    report: String,
    local_http: String,
    map_http: String,
    advertise: String,
}

async fn elect_hub(
    callsign: &str,
    lan_port: u16,
    hub_port: u16,
    timeout: Duration,
) -> Result<ElectedHub> {
    let sock = UdpSocket::bind(("0.0.0.0", lan_port)).await?;
    sock.set_broadcast(true)?;
    let lan_ip = lan_ipv4().unwrap_or_else(|| "127.0.0.1".into());
    let hello = encode_hello(callsign, lan_port, None);
    let ours = callsign.to_ascii_uppercase();
    let start = Instant::now();
    let listen = timeout.min(Duration::from_secs(8));
    let mut heard: HashMap<String, String> = HashMap::new();
    let mut found_hub = None::<String>;
    let mut buf = [0u8; 256];

    while start.elapsed() < listen {
        let _ = sock.send_to(&hello, ("255.255.255.255", lan_port)).await;
        match tokio::time::timeout(Duration::from_millis(400), sock.recv_from(&mut buf)).await {
            Ok(Ok((n, src))) => {
                if let Some(h) = decode_hello(&buf[..n]) {
                    if !h.callsign.eq_ignore_ascii_case(&ours) {
                        heard.insert(h.callsign.to_ascii_uppercase(), src.ip().to_string());
                    }
                    if let Some(hub) = h.hub {
                        if !h.callsign.eq_ignore_ascii_case(&ours) {
                            found_hub = Some(rewrite_hub_host(&hub, src.ip()));
                        }
                    }
                }
            }
            _ => {}
        }
        if found_hub.is_some() {
            break;
        }
        if start.elapsed() >= Duration::from_secs(3) && !heard.is_empty() {
            let mut names: Vec<String> = heard.keys().cloned().collect();
            names.push(ours.clone());
            names.sort();
            if names[0] == ours {
                break;
            }
        }
    }
    drop(sock);

    if let Some(hub) = found_hub {
        return Ok(join_hub(&hub));
    }

    let we_host = if heard.is_empty() {
        true
    } else {
        let mut names: Vec<String> = heard.keys().cloned().collect();
        names.push(ours.clone());
        names.sort();
        names[0] == ours
    };

    if we_host {
        let advertise = format!("{lan_ip}:{hub_port}");
        Ok(ElectedHub {
            host: true,
            ws: format!("ws://127.0.0.1:{hub_port}/ws"),
            report: format!("http://127.0.0.1:{hub_port}/api/v1/report"),
            local_http: format!("http://127.0.0.1:{hub_port}"),
            map_http: format!("http://{lan_ip}:{hub_port}"),
            advertise,
        })
    } else {
        let mut names: Vec<String> = heard.keys().cloned().collect();
        names.sort();
        let winner = names
            .first()
            .ok_or_else(|| Error::Msg("hub election heard no winner".into()))?;
        let ip = heard
            .get(winner)
            .ok_or_else(|| Error::Msg("hub election missing winner address".into()))?;
        Ok(join_hub(&format!("{ip}:{hub_port}")))
    }
}

fn rewrite_hub_host(hub: &str, src: IpAddr) -> String {
    if let Some((_, port)) = hub.rsplit_once(':') {
        if hub.starts_with("127.") || hub.starts_with("localhost") {
            return format!("{src}:{port}");
        }
    }
    hub.to_string()
}

fn join_hub(hostport: &str) -> ElectedHub {
    let http = format!("http://{hostport}");
    ElectedHub {
        host: false,
        ws: format!("ws://{hostport}/ws"),
        report: format!("http://{hostport}/api/v1/report"),
        local_http: http.clone(),
        map_http: http,
        advertise: String::new(),
    }
}

async fn spawn_local_hub(bind: &str) -> Result<tokio::task::JoinHandle<Result<()>>> {
    let store = std::sync::Arc::new(crate::store::Store::open(
        &config::default_data_dir().join("hub.db"),
        72,
        50_000,
    )?);
    let tel = std::sync::Arc::new(crate::telemetry::TelemetryDb::open(
        &config::default_data_dir().join("telemetry.db"),
    )?);
    let keys = crate::proto::load_or_create(&Config::key_path())?;
    let bind = bind.to_string();
    Ok(tokio::spawn(async move {
        crate::net::run_hub(&bind, store, tel, keys).await
    }))
}

async fn wait_http_ok(url: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout.min(Duration::from_secs(10));
    let client = reqwest::Client::new();
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Msg(format!("hub {url} never became ready")));
        }
        if let Ok(res) = client.get(url).send().await {
            if res.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

struct HubSighting {
    peer_grid: String,
}

async fn wait_hub_nodes(
    api: &str,
    us: &str,
    peer: &str,
    our_grid: &str,
    deadline: Instant,
) -> Result<HubSighting> {
    let client = reqwest::Client::new();
    let url = format!("{api}/api/v1/nodes");
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Msg(
                "hub /api/v1/nodes never showed both stations with different grids".into(),
            ));
        }
        if let Ok(res) = client.get(&url).send().await {
            if let Ok(v) = res.json::<serde_json::Value>().await {
                if let Some(hit) = nodes_show_pair(&v, us, peer, our_grid) {
                    return Ok(hit);
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

fn nodes_show_pair(
    body: &serde_json::Value,
    us: &str,
    peer: &str,
    our_grid: &str,
) -> Option<HubSighting> {
    let nodes = body.get("nodes")?.as_array()?;
    let ours = find_node(nodes, us)?;
    let theirs = find_node(nodes, peer)?;
    let our_g = node_grid(ours);
    let their_g = node_grid(theirs);
    if our_g.is_empty() || their_g.is_empty() || our_g.eq_ignore_ascii_case(&their_g) {
        return None;
    }
    if !our_g.eq_ignore_ascii_case(our_grid) {
        return None;
    }
    let our_ll = node_lat_lon(ours)?;
    let their_ll = node_lat_lon(theirs)?;
    if (our_ll.0 - their_ll.0).abs() < 0.05 && (our_ll.1 - their_ll.1).abs() < 0.05 {
        return None;
    }
    Some(HubSighting { peer_grid: their_g })
}

fn find_node<'a>(nodes: &'a [serde_json::Value], call: &str) -> Option<&'a serde_json::Value> {
    nodes.iter().find(|n| {
        n.get("callsign")
            .and_then(|c| c.as_str())
            .is_some_and(|c| c.eq_ignore_ascii_case(call))
    })
}

fn node_grid(n: &serde_json::Value) -> String {
    n.get("grid")
        .and_then(|g| g.as_str())
        .unwrap_or("")
        .to_ascii_uppercase()
}

fn node_lat_lon(n: &serde_json::Value) -> Option<(f64, f64)> {
    Some((n.get("lat")?.as_f64()?, n.get("lon")?.as_f64()?))
}

struct Exchange {
    peer: String,
    token: String,
    last_send: Instant,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

impl Exchange {
    async fn linger(&mut self, dur: Duration) {
        let end = Instant::now() + dur;
        while Instant::now() < end {
            if self.last_send.elapsed() >= Duration::from_secs(2) {
                let _ = self
                    .writer
                    .write_all(format!("PRIVMSG #bulletin :{}\r\n", self.token).as_bytes())
                    .await;
                self.last_send = Instant::now();
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

async fn run_exchange(
    cfg: &Config,
    callsign: &str,
    token: &str,
    deadline: Instant,
) -> Result<Exchange> {
    let mut stream = wait_irc(&cfg.irc.bind, deadline).await?;
    irc_register(&mut stream, callsign).await?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    writer
        .write_all(format!("PRIVMSG #bulletin :{token}\r\n").as_bytes())
        .await?;
    let mut last_send = Instant::now();
    let mut peer = None::<String>;
    let mut hub_ok = false;

    while Instant::now() < deadline {
        let wait = Duration::from_millis(250);
        let line = tokio::time::timeout(wait, lines.next_line()).await;
        match line {
            Ok(Ok(Some(line))) => {
                if let Some(found) = peer_from_irc_line(&line, callsign) {
                    peer = Some(found);
                }
            }
            Ok(Ok(None)) => {
                return Err(Error::Msg("IRC connection closed".into()));
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {}
        }
        if last_send.elapsed() >= Duration::from_secs(2) {
            writer
                .write_all(format!("PRIVMSG #bulletin :{token}\r\n").as_bytes())
                .await?;
            last_send = Instant::now();
        }
        if let Ok(snap) = fetch_status(&cfg.status.bind).await {
            hub_ok = snap.hub_ok;
        }
        if peer.is_some() && hub_ok {
            break;
        }
    }

    let Some(peer) = peer else {
        return Err(Error::Msg(format!(
            "no peer token in {}s (allow UDP/TCP on the LAN port if Windows asked)",
            deadline
                .saturating_duration_since(Instant::now())
                .as_secs()
                .max(1)
        )));
    };
    if !hub_ok {
        return Err(Error::Msg(
            "local hub never connected (status hub_ok=false)".into(),
        ));
    }
    Ok(Exchange {
        peer,
        token: token.to_string(),
        last_send,
        writer,
    })
}

async fn wait_irc(bind: &str, deadline: Instant) -> Result<TcpStream> {
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Msg(format!("IRC {bind} never accepted")));
        }
        match TcpStream::connect(bind).await {
            Ok(s) => return Ok(s),
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
}

async fn irc_register(stream: &mut TcpStream, nick: &str) -> Result<()> {
    stream
        .write_all(
            format!(
                "CAP LS\r\nNICK {nick}\r\nUSER {nick} 0 * :wcr e2e\r\nCAP REQ :message-tags echo-message server-time msgid\r\nCAP END\r\nJOIN #bulletin\r\n"
            )
            .as_bytes(),
        )
        .await?;
    Ok(())
}

async fn fetch_status(bind: &str) -> Result<StatusSnapshot> {
    let url = format!("http://{bind}/status");
    let res = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if !res.status().is_success() {
        return Err(Error::Net(format!("status HTTP {}", res.status())));
    }
    res.json().await.map_err(|e| Error::Net(e.to_string()))
}

/// Guest callsign `~` + 7 chars from a hostname hash (max 8 including `~`).
pub fn guest_callsign_from_host(host: &str) -> String {
    let hash = blake3::hash(host.trim().to_ascii_uppercase().as_bytes());
    let bytes = hash.as_bytes();
    const ALPH: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut s = String::from("~");
    for i in 0..7 {
        s.push(ALPH[bytes[i] as usize % ALPH.len()] as char);
    }
    s
}

/// Distinct Maidenhead square per hostname so two LAN stations plot apart on the map.
pub fn grid_from_host(host: &str) -> String {
    let hash = blake3::hash(host.trim().to_ascii_uppercase().as_bytes());
    let b = hash.as_bytes();
    let lat = (b[0] as f64 / 255.0) * 120.0 - 60.0;
    let lon = (b[1] as f64 / 255.0) * 360.0 - 180.0;
    crate::grid::from_lat_lon(lat, lon, 6).unwrap_or_else(|_| "IO91WM".into())
}

fn hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| format!("wcr{}", std::process::id()))
}

fn e2e_home() -> PathBuf {
    std::env::temp_dir().join(format!("wcr-e2e-{}", std::process::id()))
}

fn lan_ipv4() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("1.1.1.1:80").ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() => Some(ip.to_string()),
        _ => None,
    }
}

/// `WCR-E2E <call> <nonce>` from a peer (not our own echo).
pub fn peer_from_irc_line(line: &str, our_call: &str) -> Option<String> {
    let payload = line.rsplit_once(" :").map(|(_, t)| t).unwrap_or(line);
    let mut parts = payload.split_whitespace();
    if parts.next()? != TOKEN_PREFIX {
        return None;
    }
    let call = parts.next()?;
    if Callsign::parse(call).is_err() {
        return None;
    }
    if call.eq_ignore_ascii_case(our_call) {
        return None;
    }
    Some(call.to_ascii_uppercase())
}

struct HomeGuard(PathBuf);

impl Drop for HomeGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        if std::env::var_os("WCR_HOME").as_deref() == Some(self.0.as_os_str()) {
            std::env::remove_var("WCR_HOME");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_callsign_is_eight_and_parses() {
        let c = guest_callsign_from_host("DESKTOP-ABCDEFG");
        assert_eq!(c.len(), 8);
        assert!(c.starts_with('~'));
        assert!(Callsign::parse(&c).unwrap().is_guest());
        assert_eq!(c, guest_callsign_from_host("desktop-abcdefg"));
        assert_ne!(c, guest_callsign_from_host("other-pc"));
    }

    #[test]
    fn grids_differ_by_hostname() {
        let a = grid_from_host("laptop-alpha");
        let b = grid_from_host("laptop-bravo");
        assert_eq!(a.len(), 6);
        assert_ne!(a, b);
        crate::grid::normalize(&a).unwrap();
        crate::grid::normalize(&b).unwrap();
        let (la, loa) = crate::grid::to_lat_lon(&a).unwrap();
        let (lb, lob) = crate::grid::to_lat_lon(&b).unwrap();
        assert!((la - lb).abs() > 0.05 || (loa - lob).abs() > 0.05);
    }

    #[test]
    fn peer_token_ignores_echo() {
        let ours = "~ABC12XY";
        assert!(peer_from_irc_line("PRIVMSG #bulletin :WCR-E2E ~ABC12XY deadbeef", ours).is_none());
        assert_eq!(
            peer_from_irc_line(
                ":~Z9Y8X7W PRIVMSG #bulletin :WCR-E2E ~Z9Y8X7W cafe1234",
                ours
            )
            .as_deref(),
            Some("~Z9Y8X7W")
        );
        assert!(peer_from_irc_line("PRIVMSG #bulletin :hello", ours).is_none());
    }

    #[test]
    fn nodes_payload_requires_distinct_grids() {
        let body = serde_json::json!({
            "nodes": [
                {"callsign": "~AAA1111", "grid": "IO91WM", "lat": 51.5, "lon": -0.1},
                {"callsign": "~BBB2222", "grid": "FN20XR", "lat": 40.7, "lon": -74.0}
            ]
        });
        let hit = nodes_show_pair(&body, "~AAA1111", "~BBB2222", "IO91WM").unwrap();
        assert_eq!(hit.peer_grid, "FN20XR");
        let same = serde_json::json!({
            "nodes": [
                {"callsign": "~AAA1111", "grid": "IO91WM", "lat": 51.5, "lon": -0.1},
                {"callsign": "~BBB2222", "grid": "IO91WM", "lat": 51.5, "lon": -0.1}
            ]
        });
        assert!(nodes_show_pair(&same, "~AAA1111", "~BBB2222", "IO91WM").is_none());
    }
}
