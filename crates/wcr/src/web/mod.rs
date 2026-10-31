//! SPDX-License-Identifier: Apache-2.0
//! `wcr web`: the station's own web page, for a phone or laptop on the same
//! LAN as a headless station (a Raspberry Pi gateway, for example).
//!
//! It runs the station as its child (see [`station`]) and reads it over the
//! same loopback HTTP the desktop app uses, so the station itself is
//! unchanged. HTTPS only, behind one password, and only for LAN addresses
//! unless `[web] allow_public` is set.

mod auth;
mod chat;
mod station;
mod tls;

use crate::config::Config;
use crate::error::{Error, Result};
use axum::body::Body;
use axum::extract::{ConnectInfo, Form, Query, State};
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub use auth::{hash_password, MIN_PASSWORD};
pub use station::Station;

const LOGIN_PAGE: &str = include_str!("assets/login.html");
const GATEWAY_PAGE: &str = include_str!("assets/gateway.html");
const STYLE: &str = include_str!("assets/station.css");
const GATEWAY_JS: &str = include_str!("assets/gateway.js");
const APP_PAGE: &str = include_str!("assets/app.html");
const APP_JS: &str = include_str!("assets/app.js");

#[derive(Clone)]
struct Web {
    cfg_path: PathBuf,
    password_hash: String,
    allow_public: bool,
    sessions: Arc<auth::Sessions>,
    station: Arc<Station>,
    /// The station's loopback HTTP (`/status`, `/mail/*`).
    local: String,
    http: reqwest::Client,
    /// modem73's control port, for the waterfall.
    modem_addr: String,
    modem: Arc<tokio::sync::Mutex<Option<crate::modem::control::ControlClient>>>,
    /// The `/api/whoami` answer: no login, nothing private; `wcr webui`
    /// uses it to tell a wcr web page from anything else on port 8443.
    whoami: Value,
    /// Live Chat, as one more IRC client of the station.
    chat: Arc<chat::Chat>,
}

/// The mDNS service `wcr web` advertises and `wcr webui` looks for.
pub const MDNS_SERVICE: &str = "_wcr-web._tcp.local.";
/// What `/api/whoami` says in `app`, so `wcr webui` knows it found us.
pub const WHOAMI_APP: &str = "wcr-web";

/// Run the page and the station until `stop` resolves, then stop the station
/// (which ends modem73) before returning.
pub async fn run(cfg_path: &Path, stop: impl std::future::Future<Output = ()>) -> Result<()> {
    keep_big_blocks_out_of_the_heap();
    let cfg = Config::load(cfg_path)?;
    // A gateway run by `wcr web` always shows as a tower on the map
    // (Tobias, #220). The station child gets WCR_ALWAYS_TOWER=1 (see
    // `station`); wcr.toml is left as it is (#225).
    if cfg.mode.is_gateway() {
        println!("map: this gateway shows as a tower (always, under wcr web)");
    }
    if cfg.web.password_hash.trim().is_empty() {
        return Err(Error::config(
            "Set a password for the web page first: wcr web password",
        ));
    }
    let (cert, key) = tls::cert_paths(&cfg.web);
    if tls::ensure_self_signed(&cert, &key)? {
        println!("made a self-signed certificate: {}", cert.display());
    }
    let tls_cfg = tls::server_config(&cert, &key)?;
    let tcp = tokio::net::TcpListener::bind(&cfg.web.bind)
        .await
        .map_err(|e| Error::config(format!("[web] bind {}: {e}", cfg.web.bind)))?;
    let listener = tls::TlsListener::new(tcp, tls_cfg)?;
    let station = Station::new(cfg_path.to_path_buf());
    let web = Web {
        cfg_path: cfg_path.to_path_buf(),
        password_hash: cfg.web.password_hash.clone(),
        allow_public: cfg.web.allow_public,
        sessions: Arc::new(auth::Sessions::default()),
        station: station.clone(),
        local: local_base(&cfg.status.bind),
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .no_proxy()
            .build()
            .map_err(|e| Error::Msg(e.to_string()))?,
        modem_addr: modem_control_addr(&cfg.modem.host, cfg.modem.control_port),
        modem: Arc::new(tokio::sync::Mutex::new(None)),
        whoami: json!({
            "app": WHOAMI_APP,
            "callsign": cfg.callsign,
            "host": this_host(),
            "version": crate::update::current_version(),
        }),
        chat: chat::Chat::start(
            loopback_addr(&cfg.irc.bind),
            cfg.callsign.clone(),
            cfg_path.with_file_name("web-chat.json"),
        ),
    };
    // Kept for as long as the page runs, so `wcr webui` finds it by name.
    let _mdns = advertise(&cfg.callsign, listener_port(&cfg.web.bind));
    println!(
        "web page: https://{}.local:{} (LAN only{})",
        this_host(),
        listener_port(&cfg.web.bind),
        if cfg.web.allow_public {
            "; allow_public is on"
        } else {
            ""
        }
    );
    station.start().await;
    tokio::spawn(station.clone().supervise());
    let app = router(web).into_make_service_with_connect_info::<Peer>();
    tokio::select! {
        r = axum::serve(listener, app) => r.map_err(|e| Error::Msg(e.to_string()))?,
        _ = stop => {}
    }
    station.stop().await;
    Ok(())
}

/// Each login checks the password with Argon2, which takes a 19 MB block.
/// glibc's default raises its mmap threshold after the first such block is
/// freed, so later ones come from a worker thread's heap and stay in RSS
/// (+19 MB per login on another thread, QA #225). A fixed threshold keeps
/// big blocks in mmap, so they go back to the system when freed.
fn keep_big_blocks_out_of_the_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: mallopt only sets an allocator tunable; called before any
    // login can allocate an Argon2 block.
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 1024 * 1024);
    }
}

fn this_host() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "this-computer".into())
}

/// Announce the page on the LAN by mDNS (`_wcr-web._tcp`), so `wcr webui`
/// on a laptop finds it even where `raspberrypi.local` does not resolve.
/// Best effort: a LAN without multicast still has the address scan.
fn advertise(callsign: &str, port: &str) -> Option<mdns_sd::ServiceDaemon> {
    let port: u16 = port.parse().ok()?;
    let host = this_host();
    let mdns = mdns_sd::ServiceDaemon::new().ok()?;
    let info = mdns_sd::ServiceInfo::new(
        MDNS_SERVICE,
        &host,
        &format!("{host}.local."),
        "",
        port,
        &[("call", callsign)][..],
    )
    .ok()?
    .enable_addr_auto();
    mdns.register(info).ok()?;
    Some(mdns)
}

/// The browser's address, for the LAN check.
#[derive(Clone, Copy, Debug)]
struct Peer(SocketAddr);

impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, tls::TlsListener>>
    for Peer
{
    fn connect_info(stream: axum::serve::IncomingStream<'_, tls::TlsListener>) -> Self {
        Peer(*stream.remote_addr())
    }
}

fn router(web: Web) -> Router {
    let private = Router::new()
        .route("/", get(|| async { Html(GATEWAY_PAGE) }))
        .route(
            "/gateway.js",
            get(|| async { asset("text/javascript", GATEWAY_JS) }),
        )
        .route("/api/gateway", get(gateway))
        .route("/api/spectrum", get(spectrum))
        .route("/api/station/start", post(start))
        .route("/api/station/stop", post(stop))
        .route("/app", get(|| async { Html(APP_PAGE) }))
        .route(
            "/app.js",
            get(|| async { asset("text/javascript", APP_JS) }),
        )
        .route("/api/events", get(events))
        .route("/api/chat/send", post(chat_send))
        .route("/api/chat/join", post(chat_join))
        .route("/api/stations", get(stations))
        .route("/api/mail", get(mail_list))
        .route("/api/mail/open", get(mail_open))
        .route("/api/mail/send", post(mail_send))
        .route("/api/mail/retry", post(mail_retry))
        .route("/api/mail/check", post(mail_check))
        .route("/api/mail/get", post(mail_get))
        .route("/logout", post(logout))
        .layer(middleware::from_fn_with_state(web.clone(), require_login));
    Router::new()
        .route("/login", get(|| async { login_page("") }).post(login))
        .route("/station.css", get(|| async { asset("text/css", STYLE) }))
        .route(
            "/api/whoami",
            get(|State(web): State<Web>| async move { Json(web.whoami) }),
        )
        .merge(private)
        .layer(middleware::from_fn_with_state(web.clone(), lan_only))
        .layer(middleware::from_fn(security_headers))
        .with_state(web)
}

fn login_page(note: &str) -> Html<String> {
    Html(LOGIN_PAGE.replace("<!--NOTE-->", note))
}

fn asset(kind: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, kind)], body).into_response()
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    let set = |h: &mut HeaderMap, k: &'static str, v: &'static str| {
        h.insert(k, header::HeaderValue::from_static(v));
    };
    set(h, "strict-transport-security", "max-age=31536000");
    set(h, "x-content-type-options", "nosniff");
    set(h, "x-frame-options", "DENY");
    set(h, "referrer-policy", "no-referrer");
    set(h, "cache-control", "no-store");
    set(
        h,
        "content-security-policy",
        "default-src 'self'; frame-ancestors 'none'; form-action 'self'",
    );
    response
}

async fn lan_only(
    State(web): State<Web>,
    ConnectInfo(peer): ConnectInfo<Peer>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if web.allow_public || is_lan(peer.0.ip()) {
        return next.run(request).await;
    }
    (
        StatusCode::FORBIDDEN,
        "This station's web page answers only on its local network.",
    )
        .into_response()
}

/// Loopback, private, link-local and unique-local addresses.
pub fn is_lan(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_lan(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

fn session(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(auth::cookie_token)
}

async fn require_login(State(web): State<Web>, request: Request<Body>, next: Next) -> Response {
    let ok = session(request.headers()).is_some_and(|t| web.sessions.check(t));
    if ok {
        return next.run(request).await;
    }
    if request.uri().path().starts_with("/api/") {
        return (StatusCode::UNAUTHORIZED, "log in again").into_response();
    }
    Redirect::to("/login").into_response()
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

async fn login(State(web): State<Web>, Form(form): Form<LoginForm>) -> Response {
    let hash = web.password_hash.clone();
    let ok = tokio::task::spawn_blocking(move || auth::password_ok(&form.password, &hash))
        .await
        .unwrap_or(false);
    if !ok {
        // Slows guessing; the page says why it is back.
        tokio::time::sleep(Duration::from_secs(1)).await;
        web.station.note("a login failed");
        return (StatusCode::UNAUTHORIZED, login_page("Wrong password.")).into_response();
    }
    let token = web.sessions.open();
    let cookie = format!(
        "{}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={}",
        auth::COOKIE,
        auth::SESSION_IDLE.as_secs()
    );
    ([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response()
}

async fn logout(State(web): State<Web>, headers: HeaderMap) -> Response {
    if let Some(t) = session(&headers) {
        web.sessions.close(t);
    }
    let gone = format!(
        "{}=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
        auth::COOKIE
    );
    ([(header::SET_COOKIE, gone)], Redirect::to("/login")).into_response()
}

async fn start(State(web): State<Web>) -> Json<Value> {
    web.station.note("start pressed on the web page");
    web.station.start().await;
    Json(json!({ "ok": true }))
}

async fn stop(State(web): State<Web>) -> Json<Value> {
    web.station.note("stop pressed on the web page");
    web.station.stop().await;
    Json(json!({ "ok": true }))
}

/// Everything the gateway view shows, in one answer.
async fn gateway(State(web): State<Web>) -> Json<Value> {
    let state = web.station.state().await;
    let get = |path: &str| {
        let url = format!("{}{path}", web.local);
        let http = web.http.clone();
        async move {
            let r = http.get(url).send().await.ok()?;
            r.json::<Value>().await.ok()
        }
    };
    let (status, relay, outbox) = if state.running {
        tokio::join!(
            get("/status"),
            get("/mail/list?folder=relay"),
            get("/mail/list?folder=outbox")
        )
    } else {
        (None, None, None)
    };
    let status = status.unwrap_or(Value::Null);
    let pick = |rows: Option<Value>, n: usize| -> Vec<Value> {
        rows.and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .take(n)
            .map(|r| {
                json!({
                    "ts": r["ts"],
                    "from": r["from_addr"],
                    "to": r["to_addr"],
                    "subject": r["subject"],
                    "status": r["status"],
                })
            })
            .collect()
    };
    let internet = internet_up(&web.cfg_path).await;
    // The set frequency, for the Start question while the station is down.
    let frequency = Config::load(&web.cfg_path)
        .map(|c| c.rf.frequency_khz)
        .ok()
        .filter(|&k| k > 0)
        .map(crate::band::describe);
    Json(json!({
        "frequency": frequency,
        "station": state,
        "answering": !status.is_null(),
        "internet": internet,
        "status": status,
        "relay_mail": pick(relay, 20),
        "outbox": pick(outbox, 20),
        "errors": web.station.errors(20),
        "log": web.station.log_tail(60),
        "now": chrono::Utc::now().timestamp(),
    }))
}

/// The page's live feed: a snapshot of the chat, then each new or changed
/// line, as Server-Sent Events. It ends when the login does.
async fn events(State(web): State<Web>, headers: HeaderMap) -> Response {
    use futures_util::stream;
    let token = session(&headers).unwrap_or_default().to_string();
    let rx = web.chat.subscribe();
    let first = Some(web.chat.snapshot());
    let feed = stream::unfold(
        (rx, first, web, token),
        |(mut rx, first, web, token)| async move {
            if let Some(hello) = first {
                let ev = Event::default().data(hello.to_string());
                return Some((
                    Ok::<_, std::convert::Infallible>(ev),
                    (rx, None, web, token),
                ));
            }
            if !web.sessions.alive(&token) {
                return None;
            }
            let ev = match tokio::time::timeout(KEEP_ALIVE, rx.recv()).await {
                Ok(Ok(v)) => Event::default().data(v.to_string()),
                // Too far behind: start the page over.
                Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                    Event::default().data(web.chat.snapshot().to_string())
                }
                Ok(Err(_)) => return None,
                Err(_) => Event::default().comment("still here"),
            };
            Some((Ok(ev), (rx, None, web, token)))
        },
    );
    Sse::new(feed).into_response()
}

/// How often a quiet event stream says it is still there.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

#[derive(Deserialize)]
struct SendBody {
    to: String,
    text: String,
}

async fn chat_send(State(web): State<Web>, Json(b): Json<SendBody>) -> Response {
    match web.chat.send(&b.to, &b.text) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

#[derive(Deserialize)]
struct JoinBody {
    channel: String,
}

async fn chat_join(State(web): State<Web>, Json(b): Json<JoinBody>) -> Response {
    match web.chat.join(&b.channel) {
        Ok(ch) => Json(json!({ "ok": true, "channel": ch })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}

/// Stations heard, newest first, from the station's own status.
async fn stations(State(web): State<Web>) -> Json<Value> {
    let status = web
        .local_get("/status")
        .await
        .ok()
        .and_then(|(_, v)| serde_json::from_str::<Value>(&v).ok())
        .unwrap_or(Value::Null);
    let mut heard = status["heard"].as_array().cloned().unwrap_or_default();
    heard.sort_by_key(|h| std::cmp::Reverse(h["last_heard"].as_u64().unwrap_or(0)));
    Json(json!({
        "me": status["callsign"],
        "mode": status["mode"],
        "heard": heard,
        "now": chrono::Utc::now().timestamp(),
    }))
}

#[derive(Deserialize)]
struct FolderQ {
    folder: String,
}

/// The station's mail folders, for the Mail tab. Only the folders the page
/// shows are passed on.
async fn mail_list(State(web): State<Web>, Query(q): Query<FolderQ>) -> Response {
    if !matches!(q.folder.as_str(), "inbox" | "outbox" | "sent") {
        return (StatusCode::BAD_REQUEST, "unknown folder").into_response();
    }
    let path = format!("/mail/list?folder={}", q.folder);
    let (code, mut body) = match web.local_get(&path).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    // Radio+ headers waiting at the gateway sit at the top of the inbox.
    if q.folder == "inbox" && code.is_success() {
        if let Ok((_, waiting)) = web.local_get("/mail/waiting").await {
            body = json!({
                "rows": serde_json::from_str::<Value>(&body).unwrap_or(Value::Null),
                "waiting": serde_json::from_str::<Value>(&waiting).unwrap_or(Value::Null),
            })
            .to_string();
            return json_response(code, body);
        }
    }
    json_response(
        code,
        json!({ "rows": serde_json::from_str::<Value>(&body).unwrap_or(Value::Null) }).to_string(),
    )
}

#[derive(Deserialize)]
struct IdQ {
    id: String,
}

async fn mail_open(State(web): State<Web>, Query(q): Query<IdQ>) -> Response {
    let path = format!(
        "/mail/open?id={}",
        url::form_urlencoded::byte_serialize(q.id.as_bytes()).collect::<String>()
    );
    match web.local_get(&path).await {
        Ok((code, body)) => json_response(code, body),
        Err(r) => r,
    }
}

async fn mail_send(State(web): State<Web>, Json(b): Json<Value>) -> Response {
    let body = json!({ "to": b["to"], "subject": b["subject"], "body": b["body"] });
    web.local_post("/mail/compose", body).await
}

async fn mail_retry(State(web): State<Web>, Json(b): Json<Value>) -> Response {
    web.local_post("/mail/retry", json!({ "id": b["id"] }))
        .await
}

async fn mail_check(State(web): State<Web>) -> Response {
    web.local_post("/mail/check", json!({})).await
}

async fn mail_get(State(web): State<Web>, Json(b): Json<Value>) -> Response {
    web.local_post("/mail/get", b).await
}

fn json_response(code: reqwest::StatusCode, body: String) -> Response {
    let code = StatusCode::from_u16(code.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    (code, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

fn station_down() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({ "error": "the station is not running" })),
    )
        .into_response()
}

impl Web {
    /// GET on the station's loopback HTTP: status and body, or a reply
    /// saying the station is down.
    async fn local_get(
        &self,
        path: &str,
    ) -> std::result::Result<(reqwest::StatusCode, String), Response> {
        let r = self
            .http
            .get(format!("{}{path}", self.local))
            .send()
            .await
            .map_err(|_| station_down())?;
        let code = r.status();
        Ok((code, r.text().await.unwrap_or_default()))
    }

    /// POST to the station's loopback mail API, with its answer passed on
    /// (an error is plain text there; it goes to the page as `error`).
    async fn local_post(&self, path: &str, body: Value) -> Response {
        let Ok(r) = self
            .http
            .post(format!("{}{path}", self.local))
            .json(&body)
            .send()
            .await
        else {
            return station_down();
        };
        let code = r.status();
        let text = r.text().await.unwrap_or_default();
        if code.is_success() {
            json_response(code, text)
        } else {
            let why = if text.trim().is_empty() {
                code.canonical_reason().unwrap_or("failed").to_string()
            } else {
                text
            };
            json_response(code, json!({ "error": why }).to_string())
        }
    }
}

/// A loopback listener's address as seen from this computer.
fn loopback_addr(bind: &str) -> String {
    format!("127.0.0.1:{}", listener_port(bind))
}

/// modem73's control port as seen from this computer: `0.0.0.0` (opened
/// to the network on purpose) is reached on loopback.
fn modem_control_addr(host: &str, port: u16) -> String {
    let host = match host.trim() {
        "" | "0.0.0.0" => "127.0.0.1",
        h => h,
    };
    format!("{host}:{port}")
}

/// One row of the waterfall: modem73's audio spectrum, read over its
/// loopback control port and passed on behind the page's password. The
/// page asks only while it is open and visible, and modem73 stops keeping
/// audio for it a few seconds after the last ask.
///
/// `bins` is dB below full scale from 0 Hz up, `hz_per_bin` apart; empty
/// with a `why` when there is nothing to show (`stopped`, `no modem`,
/// `old modem`, or `no audio` when no new audio came in since the last ask,
/// so a stalled capture never repeats the last row).
async fn spectrum(State(web): State<Web>) -> Json<Value> {
    let empty = |why: &str| Json(json!({ "bins": [], "why": why }));
    if !web.station.state().await.running {
        return empty("stopped");
    }
    let mut slot = web.modem.lock().await;
    if slot.is_none() {
        match crate::modem::control::ControlClient::connect(&web.modem_addr).await {
            // Frame events are not needed here; dropping their receiver is fine.
            Ok((client, _events)) => *slot = Some(client),
            Err(_) => return empty("no modem"),
        }
    }
    let Some(client) = slot.as_ref() else {
        return empty("no modem");
    };
    let ask = client.request(json!({ "cmd": "get_spectrum" }));
    match tokio::time::timeout(Duration::from_secs(1), ask).await {
        Ok(Ok(v)) if v["no_audio"] == json!(true) => Json(json!({
            "bins": [],
            "why": "no audio",
            "tx": v["tx"],
        })),
        Ok(Ok(v)) if v["bins"].is_array() => Json(json!({
            "bins": v["bins"],
            "hz_per_bin": v["hz_per_bin"],
            "tx": v["tx"],
        })),
        // A modem73 without the wcr patch answers "unknown command".
        Ok(Ok(_)) => empty("old modem"),
        _ => {
            *slot = None;
            empty("no modem")
        }
    }
}

/// Can this computer resolve the hub's name? A cheap stand-in for "the
/// internet is up" that does not wait on the hub itself.
async fn internet_up(cfg_path: &Path) -> bool {
    let Ok(cfg) = Config::load(cfg_path) else {
        return false;
    };
    let Some(host) = url::Url::parse(&cfg.hub.url)
        .ok()
        .and_then(|u| u.host_str().map(|h| format!("{h}:443")))
    else {
        return false;
    };
    tokio::time::timeout(Duration::from_secs(2), tokio::net::lookup_host(host))
        .await
        .is_ok_and(|r| r.is_ok_and(|mut a| a.next().is_some()))
}

fn local_base(bind: &str) -> String {
    let port = listener_port(bind);
    format!("http://127.0.0.1:{port}")
}

fn listener_port(bind: &str) -> &str {
    bind.rsplit(':').next().unwrap_or("8074")
}

/// `wcr web password`: ask twice, save the hash in `[web]`.
pub fn set_password(cfg_path: &Path) -> Result<()> {
    let mut cfg = Config::load(cfg_path)?;
    let password = dialoguer::Password::new()
        .with_prompt(format!(
            "Web page password (at least {MIN_PASSWORD} characters)"
        ))
        .with_confirmation("Again", "The two did not match")
        .interact()
        .map_err(|e| Error::Msg(e.to_string()))?;
    cfg.web.password_hash = hash_password(&password).map_err(Error::Msg)?;
    cfg.save(cfg_path)?;
    println!("saved in {}", cfg_path.display());
    Ok(())
}

/// The systemd unit for a headless station: starts at boot, comes back
/// after a crash, runs as `user` with that user's config.
pub fn systemd_unit(exe: &Path, user: &str, cfg_path: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=WeeChat Radio station and web page\n\
         Wants=network-online.target\n\
         After=network-online.target sound.target\n\n\
         [Service]\n\
         User={user}\n\
         SupplementaryGroups=audio dialout\n\
         ExecStart={} -c {} web\n\
         Restart=always\n\
         RestartSec=5\n\n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        exe.display(),
        cfg_path.display()
    )
}

pub const UNIT_PATH: &str = "/etc/systemd/system/wcr-web.service";

/// `sudo wcr web install`: write and start the system service for the user
/// who ran sudo.
#[cfg(target_os = "linux")]
pub fn install(cfg_path: &Path) -> Result<()> {
    let user = std::env::var("SUDO_USER")
        .ok()
        .filter(|u| !u.is_empty() && u != "root")
        .ok_or_else(|| {
            Error::config("Run it with sudo from your own account: sudo wcr web install")
        })?;
    // sudo gives root's home, so the default config path points into /root.
    let cfg_path = match user_home(&user) {
        Some(home) => under_home(cfg_path, Path::new("/root"), &home),
        None => cfg_path.to_path_buf(),
    };
    if !cfg_path.exists() {
        return Err(Error::config(format!(
            "no config at {}. Run `wcr setup` first (without sudo).",
            cfg_path.display()
        )));
    }
    let exe = std::env::current_exe()?;
    std::fs::write(UNIT_PATH, systemd_unit(&exe, &user, &cfg_path))
        .map_err(|e| Error::Msg(format!("{UNIT_PATH}: {e} (run it with sudo)")))?;
    for args in [
        &["daemon-reload"][..],
        &["enable", "--now", "wcr-web.service"][..],
    ] {
        let ok = std::process::Command::new("systemctl")
            .args(args)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            return Err(Error::Msg(format!("systemctl {} failed", args.join(" "))));
        }
    }
    println!("installed {UNIT_PATH}; it starts at boot and restarts after a crash");
    Ok(())
}

/// `path` moved from `from` to `to` when it lies under `from`.
pub fn under_home(path: &Path, from: &Path, to: &Path) -> PathBuf {
    match path.strip_prefix(from) {
        Ok(rest) => to.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(target_os = "linux")]
fn user_home(user: &str) -> Option<PathBuf> {
    let name = std::ffi::CString::new(user).ok()?;
    // SAFETY: getpwnam returns a pointer into static storage or null; the
    // home directory is copied out before anything else calls it.
    unsafe {
        let pw = libc::getpwnam(name.as_ptr());
        if pw.is_null() || (*pw).pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr((*pw).pw_dir).to_str().ok()?;
        Some(PathBuf::from(dir))
    }
}

#[cfg(not(target_os = "linux"))]
pub fn install(_cfg_path: &Path) -> Result<()> {
    Err(Error::config(
        "wcr web install is for Linux (Raspberry Pi OS). Elsewhere, run `wcr web` yourself.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_waterfall_reaches_modem73_on_loopback() {
        assert_eq!(modem_control_addr("127.0.0.1", 8073), "127.0.0.1:8073");
        assert_eq!(modem_control_addr("0.0.0.0", 8073), "127.0.0.1:8073");
        assert_eq!(modem_control_addr("", 9000), "127.0.0.1:9000");
        assert_eq!(modem_control_addr("192.168.1.9", 8073), "192.168.1.9:8073");
    }

    #[test]
    fn only_lan_addresses_count_as_lan() {
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "172.16.4.1",
            "192.168.1.20",
            "169.254.3.3",
            "::1",
            "fd12::1",
            "fe80::1",
            "::ffff:192.168.1.2",
        ] {
            assert!(is_lan(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "172.32.0.1", "2001:db8::1", "::ffff:1.1.1.1"] {
            assert!(!is_lan(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn the_unit_restarts_and_starts_at_boot() {
        let unit = systemd_unit(
            Path::new("/usr/local/bin/wcr"),
            "pi",
            Path::new("/home/pi/.config/wcr/wcr.toml"),
        );
        assert!(unit.contains("User=pi\n"));
        assert!(
            unit.contains("ExecStart=/usr/local/bin/wcr -c /home/pi/.config/wcr/wcr.toml web\n")
        );
        assert!(unit.contains("Restart=always\n"));
        assert!(unit.contains("WantedBy=multi-user.target\n"));
    }

    #[test]
    fn sudo_config_paths_move_to_the_users_home() {
        let moved = under_home(
            Path::new("/root/.config/wcr/wcr.toml"),
            Path::new("/root"),
            Path::new("/home/pi"),
        );
        assert_eq!(moved, Path::new("/home/pi/.config/wcr/wcr.toml"));
        let kept = under_home(
            Path::new("/srv/wcr.toml"),
            Path::new("/root"),
            Path::new("/home/pi"),
        );
        assert_eq!(kept, Path::new("/srv/wcr.toml"));
    }

    #[test]
    fn the_station_is_read_on_loopback() {
        assert_eq!(local_base("127.0.0.1:8074"), "http://127.0.0.1:8074");
        assert_eq!(local_base("0.0.0.0:9000"), "http://127.0.0.1:9000");
    }
}
