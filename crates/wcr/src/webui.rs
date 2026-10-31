//! SPDX-License-Identifier: Apache-2.0
//! `wcr webui`: find a gateway's web page (`wcr web`, usually a Raspberry
//! Pi) on the LAN and open it. On the gateway itself it prints the page's
//! addresses, whether `wcr-web` runs, and a QR code for a phone.
//!
//! Read only: it opens no port, changes no service or firewall, and never
//! sees the page's password. A gateway is anything on port 8443 whose
//! `/api/whoami` says `wcr-web`.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::web::{MDNS_SERVICE, WHOAMI_APP};
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::{Duration, Instant};

/// The port `wcr web` listens on unless `[web] bind` says otherwise.
const DEFAULT_PORT: u16 = 8443;
/// How long the whole search may take, so a LAN with no gateway answers
/// within about 5 s.
const SEARCH: Duration = Duration::from_millis(3500);
/// Per-address TCP connect wait during the scan.
const CONNECT: Duration = Duration::from_millis(500);
/// Connects in flight at once: macOS allows a process only 256 open files
/// by default, so not all 254 at once.
const SCAN_AT_ONCE: usize = 96;
/// Per-gateway wait for `/api/whoami`.
const ASK: Duration = Duration::from_millis(1500);

const CERT_NOTE: &str = "The browser warns about the certificate because the gateway made its own (self-signed): choose Advanced, then continue to the site.";

#[derive(Debug, Clone, Default)]
pub struct Opts {
    pub host: Option<String>,
    pub no_open: bool,
    pub qr: bool,
}

/// A `wcr web` page that answered.
#[derive(Debug, Clone, PartialEq)]
struct Gateway {
    addr: SocketAddr,
    callsign: String,
    host: String,
}

impl Gateway {
    fn url(&self) -> String {
        format!("https://{}/", self.addr)
    }
}

pub async fn run(cfg_path: &Path, opts: Opts) -> Result<()> {
    let cfg = Config::load(cfg_path).ok();
    if opts.host.is_none() {
        if let Some(cfg) = cfg.as_ref().filter(|c| !c.web.password_hash.is_empty()) {
            return on_the_gateway(cfg, &opts).await;
        }
    }
    let http = client()?;
    let found = match &opts.host {
        Some(h) => {
            let addr = with_port(h).await?;
            match whoami(&http, addr).await {
                Some(g) => vec![g],
                None => {
                    return Err(Error::Msg(format!(
                        "no wcr web page answers at {addr}. Is `wcr web` running on the gateway?"
                    )))
                }
            }
        }
        None => find(&http, cfg.as_ref()).await,
    };
    let gw = match found.len() {
        0 => {
            return Err(Error::Msg(
                "no gateway web page found on this network. On the gateway, run `wcr webui` to see its address, then here: wcr webui --host <address>".into(),
            ))
        }
        1 => found[0].clone(),
        _ => pick(&found)?,
    };
    remember(cfg_path, cfg, &gw);
    let url = gw.url();
    println!("{} ({}, {})", url, show(&gw.callsign), show(&gw.host));
    println!("{CERT_NOTE}");
    if opts.qr {
        print_qr(&url);
    }
    if !opts.no_open {
        open_browser(&url);
    }
    Ok(())
}

fn show(s: &str) -> &str {
    if s.is_empty() {
        "?"
    } else {
        s
    }
}

/// On the gateway: where phones and laptops reach the page, and whether it
/// runs.
async fn on_the_gateway(cfg: &Config, opts: &Opts) -> Result<()> {
    let port = port_of(&cfg.web.bind);
    let host = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ips = lan_ipv4s();
    println!("This is the gateway. Its web page, from a phone or laptop on the same network:");
    for ip in &ips {
        println!("  https://{ip}:{port}/");
    }
    if !host.is_empty() {
        println!("  https://{host}.local:{port}/  (where .local names work)");
    }
    let http = client()?;
    let up = whoami(&http, SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .is_some();
    println!(
        "wcr-web service: {}; the page {}.",
        service_state(),
        if up { "answers" } else { "does not answer" }
    );
    if !up {
        println!("Start it: sudo systemctl start wcr-web (or run `wcr web`; first time: sudo wcr web install)");
    }
    println!("{CERT_NOTE}");
    // A phone often cannot resolve .local names, so the QR holds the IP.
    if let Some(ip) = ips.first() {
        print_qr(&format!("https://{ip}:{port}/"));
    } else if opts.qr {
        println!("(no LAN address yet, so no QR code)");
    }
    Ok(())
}

fn service_state() -> String {
    if !cfg!(target_os = "linux") {
        return "not a Linux service here".into();
    }
    match std::process::Command::new("systemctl")
        .args(["is-active", "wcr-web"])
        .output()
    {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            match s.as_str() {
                "active" => "running".into(),
                "" => "unknown".into(),
                other => format!("{other} (not running)"),
            }
        }
        Err(_) => "unknown (no systemctl)".into(),
    }
}

/// Saved address, then mDNS (our own `_wcr-web._tcp` and
/// `raspberrypi.local`), then a scan of this computer's /24 for port 8443.
/// All run at once within [`SEARCH`]; the order only decides which is
/// listed first.
async fn find(http: &reqwest::Client, cfg: Option<&Config>) -> Vec<Gateway> {
    let saved = cfg
        .map(|c| c.web.gateway.trim().to_string())
        .unwrap_or_default();
    let saved = async {
        if saved.is_empty() {
            return vec![];
        }
        match with_port(&saved).await {
            Ok(a) => vec![a],
            Err(_) => vec![],
        }
    };
    let deadline = Instant::now() + SEARCH;
    let (saved, by_name, scanned) = tokio::join!(saved, mdns(deadline), scan(deadline));
    let mut order: Vec<SocketAddr> = vec![];
    for a in saved.into_iter().chain(by_name).chain(scanned) {
        if !order.contains(&a) {
            order.push(a);
        }
    }
    let asks = order.iter().map(|a| whoami(http, *a));
    let mut seen = BTreeMap::new();
    for g in futures_util::future::join_all(asks)
        .await
        .into_iter()
        .flatten()
    {
        // One gateway can answer on several addresses: keep the first.
        let key = (g.callsign.clone(), g.host.clone());
        seen.entry(key).or_insert(g);
    }
    let mut out: Vec<Gateway> = seen.into_values().collect();
    out.sort_by_key(|g| order.iter().position(|a| *a == g.addr));
    out
}

async fn mdns(deadline: Instant) -> Vec<SocketAddr> {
    let named = async {
        let ask = tokio::net::lookup_host(("raspberrypi.local", DEFAULT_PORT));
        match tokio::time::timeout(Duration::from_millis(1500), ask).await {
            Ok(Ok(addrs)) => addrs.filter(|a| a.is_ipv4()).collect(),
            _ => vec![],
        }
    };
    let browse = async {
        let mut out = vec![];
        let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
            return out;
        };
        let Ok(rx) = daemon.browse(MDNS_SERVICE) else {
            return out;
        };
        let wait = deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(2000));
        let _ = tokio::time::timeout(wait, async {
            while let Ok(ev) = rx.recv_async().await {
                if let mdns_sd::ServiceEvent::ServiceResolved(info) = ev {
                    for ip in info.get_addresses() {
                        if ip.is_ipv4() {
                            out.push(SocketAddr::new(*ip, info.get_port()));
                        }
                    }
                }
            }
        })
        .await;
        let _ = daemon.shutdown();
        out
    };
    let (mut a, b) = tokio::join!(browse, named);
    a.extend(b);
    a
}

/// Every address of this computer's private IPv4 networks (as /24s) that
/// takes a TCP connection on port 8443.
async fn scan(deadline: Instant) -> Vec<SocketAddr> {
    let mut targets = vec![];
    for ip in lan_ipv4s() {
        let [a, b, c, own] = ip.octets();
        for d in 1..=254u8 {
            if d != own {
                targets.push(SocketAddr::from(([a, b, c, d], DEFAULT_PORT)));
            }
        }
    }
    use futures_util::StreamExt;
    futures_util::stream::iter(targets)
        .map(|a| async move {
            let wait = CONNECT.min(deadline.saturating_duration_since(Instant::now()));
            match tokio::time::timeout(wait, tokio::net::TcpStream::connect(a)).await {
                Ok(Ok(_)) => Some(a),
                _ => None,
            }
        })
        .buffer_unordered(SCAN_AT_ONCE)
        .filter_map(|a| async move { a })
        .collect()
        .await
}

/// This computer's private (LAN) IPv4 addresses.
fn lan_ipv4s() -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v4) if v4.is_private() => Some(v4),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn client() -> Result<reqwest::Client> {
    // The page's certificate is self-signed; whoami carries nothing private.
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(ASK)
        .no_proxy()
        .build()
        .map_err(|e| Error::Msg(e.to_string()))
}

async fn whoami(http: &reqwest::Client, addr: SocketAddr) -> Option<Gateway> {
    let v: Value = http
        .get(format!("https://{addr}/api/whoami"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    if v["app"] != WHOAMI_APP {
        return None;
    }
    Some(Gateway {
        addr,
        callsign: v["callsign"].as_str().unwrap_or_default().to_string(),
        host: v["host"].as_str().unwrap_or_default().to_string(),
    })
}

/// `host`, `host:port` or an IP, resolved to one IPv4 address.
async fn with_port(h: &str) -> Result<SocketAddr> {
    let h = h
        .trim()
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let (name, port) = match h.rsplit_once(':') {
        Some((n, p)) if p.parse::<u16>().is_ok() => (n, p.parse().unwrap_or(DEFAULT_PORT)),
        _ => (h, DEFAULT_PORT),
    };
    let ask = tokio::net::lookup_host((name, port));
    let addrs = tokio::time::timeout(Duration::from_secs(3), ask)
        .await
        .map_err(|_| Error::Msg(format!("{name}: no answer from the name lookup")))?
        .map_err(|e| Error::Msg(format!("{name}: {e}")))?;
    addrs
        .into_iter()
        .find(|a| a.is_ipv4())
        .ok_or_else(|| Error::Msg(format!("{name}: no IPv4 address")))
}

fn port_of(bind: &str) -> u16 {
    bind.rsplit(':')
        .next()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn pick(found: &[Gateway]) -> Result<Gateway> {
    let rows: Vec<String> = found
        .iter()
        .map(|g| format!("{}  {}  {}", show(&g.callsign), show(&g.host), g.addr.ip()))
        .collect();
    if !console::user_attended() {
        println!("Several gateways answered:");
        for r in &rows {
            println!("  {r}");
        }
        return Err(Error::Msg(
            "pick one with: wcr webui --host <address>".into(),
        ));
    }
    let i = dialoguer::Select::new()
        .with_prompt("Several gateways answered. Open which one?")
        .items(&rows)
        .default(0)
        .interact()
        .map_err(|e| Error::Msg(e.to_string()))?;
    Ok(found[i].clone())
}

/// Save the gateway in `[web] gateway`, so the next run checks it first.
/// Only into an existing wcr.toml: a laptop without one keeps none, so
/// the station's first-run setup is not skipped.
fn remember(cfg_path: &Path, cfg: Option<Config>, gw: &Gateway) {
    let Some(mut cfg) = cfg else { return };
    let addr = gw.addr.to_string();
    if cfg.web.gateway == addr {
        return;
    }
    cfg.web.gateway = addr;
    if cfg.save(cfg_path).is_ok() {
        println!("saved in {} ([web] gateway)", cfg_path.display());
    }
}

fn print_qr(text: &str) {
    match qrcode::QrCode::new(text.as_bytes()) {
        Ok(code) => {
            let art = code
                .render::<qrcode::render::unicode::Dense1x2>()
                .dark_color(qrcode::render::unicode::Dense1x2::Light)
                .light_color(qrcode::render::unicode::Dense1x2::Dark)
                .quiet_zone(true)
                .build();
            println!("{art}");
        }
        Err(e) => println!("(no QR code: {e})"),
    }
}

fn open_browser(url: &str) {
    let ok = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .is_ok()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn().is_ok()
    } else {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .is_ok()
    };
    if !ok {
        println!("Open it in a browser: {url}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_forms_get_the_default_port() {
        assert_eq!(
            with_port("192.168.1.158").await.unwrap(),
            "192.168.1.158:8443".parse().unwrap()
        );
        assert_eq!(
            with_port("https://192.168.1.158:9443/").await.unwrap(),
            "192.168.1.158:9443".parse().unwrap()
        );
    }

    #[test]
    fn port_comes_from_the_bind() {
        assert_eq!(port_of("0.0.0.0:8443"), 8443);
        assert_eq!(port_of("0.0.0.0:9000"), 9000);
        assert_eq!(port_of("junk"), 8443);
    }
}
