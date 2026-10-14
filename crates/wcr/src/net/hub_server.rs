//! SPDX-License-Identifier: Apache-2.0
//! Hub WebSocket server + telemetry HTTP API.

use crate::error::Result;
use crate::proto::frag::FragAssembler;
use crate::proto::{verify_envelope, Callsign, Envelope, IdentityKeys, MsgType};
use crate::rate_limit::{self, KeyedLimiter};
use crate::store::Store;
use crate::telemetry::{self, TelemetryDb};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::CorsLayer;

#[derive(Clone)]
pub struct HubState {
    pub store: Arc<Store>,
    pub telemetry: Arc<TelemetryDb>,
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub live: broadcast::Sender<serde_json::Value>,
    pub started: std::time::Instant,
    pub forwarded: Arc<Mutex<u64>>,
    pub identity: IdentityKeys,
    pub assembler: Arc<Mutex<FragAssembler>>,
    pub next_conn: Arc<AtomicU64>,
    pub report_by_call: Arc<KeyedLimiter>,
    pub report_by_ip: Arc<KeyedLimiter>,
    pub hello_by_call: Arc<KeyedLimiter>,
    pub mail: Arc<crate::net::hub_mail::MailHubDb>,
    /// Public map id. The production hub sets this from `WCR_DOMAIN`. A private
    /// hub must not claim to be `hub.weechatradio.com`.
    pub id: String,
    /// Direct messages for a callsign that is not connected. Groups and
    /// `#bulletin` are not held: those still need a live session.
    pub pending_dm: Arc<Mutex<Vec<HeldDm>>>,
}

/// A direct message waiting for its destination to say hello.
pub struct HeldDm {
    pub dest: String,
    pub raw: Vec<u8>,
    pub expires: u64,
}

/// How long an offline direct message stays on the hub.
pub const OFFLINE_DM_TTL_SECS: u64 = 600;
/// Per destination. The oldest copy is dropped when this fills.
pub const OFFLINE_DM_CAP: usize = 32;

pub struct Session {
    pub callsign: String,
    pub pubkey: [u8; 32],
    pub serves: HashSet<String>,
    pub tx: mpsc::Sender<Vec<u8>>,
    pub connected_at: std::time::Instant,
    pub freq_khz: u32,
    pub conn_id: u64,
}

#[derive(Deserialize)]
struct Hello {
    v: u8,
    callsign: String,
    pubkey: String,
    sig: String,
    ts: u64,
    #[serde(default)]
    heard: Vec<String>,
    #[serde(default)]
    freq_khz: u32,
}

pub async fn run_hub(
    bind: &str,
    store: Arc<Store>,
    telemetry: Arc<TelemetryDb>,
    keys: IdentityKeys,
) -> Result<()> {
    let (live, _) = broadcast::channel(256);
    let state = HubState {
        store,
        telemetry,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        live,
        started: std::time::Instant::now(),
        forwarded: Arc::new(Mutex::new(0)),
        identity: keys,
        assembler: Arc::new(Mutex::new(FragAssembler::new())),
        next_conn: Arc::new(AtomicU64::new(1)),
        report_by_call: rate_limit::per_minute(120),
        report_by_ip: rate_limit::per_minute(240),
        hello_by_call: rate_limit::per_minute(30),
        mail: Arc::new(
            crate::net::hub_mail::MailHubDb::open(
                &crate::config::default_data_dir().join("hub-mail.sqlite"),
            )
            .unwrap_or_else(|_| crate::net::hub_mail::MailHubDb::open_memory().expect("mail db")),
        ),
        id: hub_report_id(bind, &std::env::var("WCR_DOMAIN").unwrap_or_default()),
        pending_dm: Arc::new(Mutex::new(Vec::new())),
    };
    let app = Router::new()
        .route("/", get(ws_upgrade))
        .route("/ws", get(ws_upgrade))
        .route("/ws/live", get(ws_live))
        .route("/api/v1/report", post(telemetry::ingest_report))
        .route("/api/v1/nodes", get(telemetry::get_nodes))
        .route("/api/v1/events", get(get_events))
        .route("/api/v1/links", get(get_links))
        .route("/api/v1/hubs", get(get_hubs))
        .route("/api/v1/bands", get(get_bands))
        .route("/api/v1/stats", get(get_stats))
        .route("/api/v1/mail/send", post(crate::net::hub_mail::post_send))
        .route("/api/v1/mail/inbox", post(crate::net::hub_mail::post_inbox))
        .route("/api/v1/mail/fetch", post(crate::net::hub_mail::post_fetch))
        .route("/api/v1/mail/copy", post(crate::net::hub_mail::post_copy))
        .route(
            "/api/v1/mail/copy/confirm",
            post(crate::net::hub_mail::post_copy_confirm),
        )
        .route(
            "/api/v1/mail/webhook/resend",
            post(crate::net::hub_mail::resend_webhook),
        )
        .route("/healthz", get(|| async { "ok" }))
        .layer(CorsLayer::permissive())
        .with_state(state);
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| crate::error::Error::Net(format!("{e}")))?;
    tracing::info!("hub listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(st): State<HubState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_node(socket, st))
}

async fn handle_node(mut socket: WebSocket, st: HubState) {
    let hello = match socket.recv().await {
        Some(Ok(Message::Text(t))) => t.to_string(),
        Some(Ok(Message::Binary(b))) => String::from_utf8_lossy(&b).into_owned(),
        _ => return,
    };
    let hello: Hello = match serde_json::from_str(&hello) {
        Ok(h) => h,
        Err(_) => {
            let _ = socket
                .send(Message::Text(
                    "{\"ok\":false,\"error\":\"bad hello\"}".into(),
                ))
                .await;
            return;
        }
    };
    if hello.v != 1 {
        let _ = socket
            .send(Message::Text(
                "{\"ok\":false,\"error\":\"bad version\"}".into(),
            ))
            .await;
        return;
    }
    if Callsign::parse(&hello.callsign).is_err() && !hello.callsign.starts_with('~') {
        let _ = socket
            .send(Message::Text(
                "{\"ok\":false,\"error\":\"bad callsign\"}".into(),
            ))
            .await;
        return;
    }
    if !rate_limit::allow(&st.hello_by_call, &hello.callsign) {
        let _ = socket
            .send(Message::Text(
                "{\"ok\":false,\"error\":\"rate limit\"}".into(),
            ))
            .await;
        return;
    }
    let pk_bytes = match hex::decode(&hello.pubkey) {
        Ok(b) if b.len() == 32 => b,
        _ => return,
    };
    let mut pk = [0u8; 32];
    pk.copy_from_slice(&pk_bytes);
    let vk = match VerifyingKey::from_bytes(&pk) {
        Ok(v) => v,
        Err(_) => return,
    };
    let sig_bytes = match hex::decode(&hello.sig) {
        Ok(b) if b.len() == 64 => b,
        _ => return,
    };
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let payload = format!("{}|{}", hello.callsign, hello.ts);
    if vk
        .verify(payload.as_bytes(), &Signature::from_bytes(&sig_arr))
        .is_err()
    {
        let _ = socket
            .send(Message::Text(
                "{\"ok\":false,\"error\":\"bad signature\"}".into(),
            ))
            .await;
        return;
    }
    if !st.telemetry.check_replay(&hello.callsign, hello.ts) {
        let _ = socket
            .send(Message::Text(
                "{\"ok\":false,\"error\":\"replay or clock skew\"}".into(),
            ))
            .await;
        return;
    }
    let call_key = hello.callsign.to_ascii_uppercase();
    let live = st.sessions.lock().contains_key(&call_key);
    match crate::telemetry::callsign_claim(st.telemetry.get_pubkey(&call_key).as_ref(), &pk, live) {
        crate::telemetry::CallsignClaim::Refuse => {
            let _ = socket
                .send(Message::Text(
                    "{\"ok\":false,\"error\":\"callsign already claimed\"}".into(),
                ))
                .await;
            return;
        }
        crate::telemetry::CallsignClaim::Replace => {
            let _ = st.telemetry.replace_callsign(&call_key, &pk);
        }
        crate::telemetry::CallsignClaim::Bind => {
            let _ = st.telemetry.bind_callsign(&call_key, &pk);
        }
        crate::telemetry::CallsignClaim::Keep => {}
    }
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    let mut serves: HashSet<String> = hello
        .heard
        .into_iter()
        .map(|s| s.to_ascii_uppercase())
        .collect();
    serves.insert(hello.callsign.to_ascii_uppercase());
    let conn_id = st.next_conn.fetch_add(1, Ordering::Relaxed);
    {
        let mut g = st.sessions.lock();
        g.insert(
            hello.callsign.to_ascii_uppercase(),
            Session {
                callsign: hello.callsign.to_ascii_uppercase(),
                pubkey: pk,
                serves,
                tx,
                connected_at: std::time::Instant::now(),
                freq_khz: hello.freq_khz,
                conn_id,
            },
        );
    }
    let call_up = hello.callsign.to_ascii_uppercase();
    for raw in take_offline_dm(&st, &call_up) {
        if let Some(tx) = st.sessions.lock().get(&call_up).map(|s| s.tx.clone()) {
            let _ = tx.try_send(raw);
        }
    }
    let _ = socket.send(Message::Text("{\"ok\":true}".into())).await;
    let call = hello.callsign.to_ascii_uppercase();
    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(bin) = rx.recv().await {
            if sink.send(Message::Binary(bin.into())).await.is_err() {
                break;
            }
        }
    });
    while let Some(Ok(msg)) = stream.next().await {
        if !session_is_current(&st, &call, conn_id) {
            break;
        }
        match msg {
            Message::Binary(bin) => {
                if let Ok(env) = Envelope::decode(&bin) {
                    if !verify_inbound(&st, &env, &call, &pk) {
                        tracing::debug!(
                            "hub drop unverified {} from {call} kind={}",
                            env.origin,
                            env.kind.as_str()
                        );
                        continue;
                    }
                    let mut extras: Vec<Envelope> = Vec::new();
                    if env.kind == MsgType::Frag {
                        if let Ok(Some(full)) = st.assembler.lock().push(&env) {
                            if st.store.seen_before(&full.msg_id).ok() != Some(true)
                                && verify_inbound(&st, &full, &call, &pk)
                            {
                                extras.push(full);
                            }
                        }
                    }
                    let _ = st.store.insert(&env, crate::store::Delivery::Queued);
                    if env.kind != MsgType::Frag {
                        *st.forwarded.lock() += 1;
                        let (from_khz, from_band, to_bands, n) = route(&st, &env, &bin, &call);
                        if n == 0 {
                            hold_offline_dm(&st, &env, &bin);
                        }
                        publish_forward(
                            &st,
                            env.origin.to_string(),
                            env.dest.to_string(),
                            env.kind.as_str(),
                            env.msg_id.hex(),
                            from_khz,
                            from_band,
                            to_bands,
                        );
                    }
                    for full in extras {
                        if let Ok(raw) = full.encode() {
                            let _ = st.store.insert(&full, crate::store::Delivery::Queued);
                            *st.forwarded.lock() += 1;
                            let (from_khz, from_band, to_bands, n) = route(&st, &full, &raw, &call);
                            if n == 0 {
                                hold_offline_dm(&st, &full, &raw);
                            }
                            publish_forward(
                                &st,
                                full.origin.to_string(),
                                full.dest.to_string(),
                                full.kind.as_str(),
                                full.msg_id.hex(),
                                from_khz,
                                from_band,
                                to_bands,
                            );
                        }
                    }
                }
            }
            Message::Text(t) => {
                if t.contains("heard") || t.contains("freq_khz") {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                        let mut g = st.sessions.lock();
                        if let Some(s) = g.get_mut(&call) {
                            if s.conn_id != conn_id {
                                break;
                            }
                            if let Some(list) = v.get("heard").and_then(|x| x.as_array()) {
                                for h in list {
                                    if let Some(c) = h.as_str() {
                                        s.serves.insert(c.to_ascii_uppercase());
                                    }
                                }
                            }
                            if let Some(khz) = v.get("freq_khz").and_then(|x| x.as_u64()) {
                                s.freq_khz = khz as u32;
                            }
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    {
        let mut g = st.sessions.lock();
        if g.get(&call).map(|s| s.conn_id) == Some(conn_id) {
            g.remove(&call);
        }
    }
    drop(writer);
}

fn session_is_current(st: &HubState, call: &str, conn_id: u64) -> bool {
    st.sessions.lock().get(call).map(|s| s.conn_id) == Some(conn_id)
}

/// Unsigned frames (typical RF) pass. Signed frames must match the origin's bound key.
pub(crate) fn verify_inbound(
    st: &HubState,
    env: &Envelope,
    session_call: &str,
    session_pk: &[u8; 32],
) -> bool {
    if !env.flags.signed() {
        return true;
    }
    let origin = env.origin.as_str();
    let pk = if let Some(existing) = st.telemetry.get_pubkey(origin) {
        existing
    } else if origin.eq_ignore_ascii_case(session_call) {
        *session_pk
    } else {
        return false;
    };
    let Ok(vk) = VerifyingKey::from_bytes(&pk) else {
        return false;
    };
    verify_envelope(env, &vk).is_ok()
}

fn unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn publish_forward(
    st: &HubState,
    origin: String,
    dest: String,
    kind: &str,
    msgid: String,
    from_khz: u32,
    from_band: String,
    to_bands: Vec<String>,
) {
    let ts = unix_ts();
    let ev = telemetry::TelemetryEvent {
        ts,
        kind: kind.to_string(),
        origin: Some(origin.clone()),
        dest: Some(dest.clone()),
        hops: None,
        snr: None,
        msgid: Some(msgid.clone()),
        band: if from_band.is_empty() {
            None
        } else {
            Some(from_band.clone())
        },
    };
    let _ = st.telemetry.add_event(&ev);
    let _ = st.live.send(serde_json::json!({
        "type": "hub_forward",
        "ts": ts,
        "origin": origin,
        "dest": dest,
        "kind": kind,
        "id": msgid,
        "from_band": from_band,
        "from_freq_khz": from_khz,
        "to_bands": to_bands,
    }));
}

fn route(
    st: &HubState,
    env: &Envelope,
    raw: &[u8],
    from: &str,
) -> (u32, String, Vec<String>, usize) {
    if env.kind == MsgType::Frag {
        return (0, String::new(), Vec::new(), 0);
    }
    let dest = env.dest.to_string().to_ascii_uppercase();
    let broadcast = crate::slash::is_hub_broadcast(&dest);
    // Closed groups are not hub-wide. Members register the group name from
    // their local group list (hello / heard `serves`), and any membership
    // row on this hub is included too.
    let members: HashSet<String> = if env.flags.group() && !broadcast {
        st.store
            .group_members(&dest)
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.to_ascii_uppercase())
            .collect()
    } else {
        HashSet::new()
    };
    let (from_khz, sessions) = {
        let g = st.sessions.lock();
        let from_khz = g.get(from).map(|s| s.freq_khz).unwrap_or(0);
        let sessions: Vec<(u32, mpsc::Sender<Vec<u8>>)> = g
            .iter()
            .filter(|(call, _)| *call != from)
            .filter(|(call, s)| {
                broadcast || *call == &dest || s.serves.contains(&dest) || members.contains(*call)
            })
            .map(|(_, s)| (s.freq_khz, s.tx.clone()))
            .collect();
        (from_khz, sessions)
    };
    let n = sessions.len();
    let to_khz: Vec<u32> = sessions.iter().map(|(k, _)| *k).collect();
    for (_khz, tx) in sessions {
        let _ = tx.try_send(raw.to_vec());
    }
    (
        from_khz,
        crate::band::band_label(from_khz),
        crate::band::to_bands(to_khz),
        n,
    )
}

fn offline_dm_holdable(env: &Envelope) -> bool {
    matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    ) && !env.flags.group()
        && !crate::slash::is_hub_broadcast(env.dest.as_str())
}

/// Keep a direct message when nobody was connected to receive it.
pub fn hold_offline_dm(st: &HubState, env: &Envelope, raw: &[u8]) -> bool {
    if !offline_dm_holdable(env) {
        return false;
    }
    let dest = env.dest.as_str().to_ascii_uppercase();
    let now = unix_ts();
    let mut q = st.pending_dm.lock();
    q.retain(|h| h.expires > now);
    let mut count = q.iter().filter(|h| h.dest == dest).count();
    while count >= OFFLINE_DM_CAP {
        if let Some(i) = q.iter().position(|h| h.dest == dest) {
            q.remove(i);
            count -= 1;
        } else {
            break;
        }
    }
    q.push(HeldDm {
        dest,
        raw: raw.to_vec(),
        expires: now.saturating_add(OFFLINE_DM_TTL_SECS),
    });
    true
}

pub fn take_offline_dm(st: &HubState, dest: &str) -> Vec<Vec<u8>> {
    let dest = dest.to_ascii_uppercase();
    let now = unix_ts();
    let mut q = st.pending_dm.lock();
    let mut out = Vec::new();
    q.retain(|h| {
        if h.expires <= now {
            return false;
        }
        if h.dest == dest {
            out.push(h.raw.clone());
            false
        } else {
            true
        }
    });
    out
}

fn collect_bands(st: &HubState) -> Vec<serde_json::Value> {
    #[derive(Default)]
    struct Acc {
        freq_khz: u32,
        gateways: u32,
        stations: std::collections::BTreeSet<String>,
    }
    let mut map: std::collections::BTreeMap<String, Acc> = std::collections::BTreeMap::new();
    {
        let g = st.sessions.lock();
        for (call, s) in g.iter() {
            let band = crate::band::band_label(s.freq_khz);
            let e = map.entry(band).or_default();
            e.stations.insert(call.clone());
            if s.freq_khz > 0 {
                e.gateways += 1;
                if e.freq_khz == 0 {
                    e.freq_khz = s.freq_khz;
                }
            }
        }
    }
    for n in st.telemetry.nodes() {
        let call = n
            .get("callsign")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        if call.is_empty() {
            continue;
        }
        let khz = n.get("freq_khz").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let band = n
            .get("band")
            .and_then(|b| b.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| crate::band::band_label(khz));
        let e = map.entry(band).or_default();
        e.stations.insert(call);
        if khz > 0 && e.freq_khz == 0 {
            e.freq_khz = khz;
        }
    }
    map.into_iter()
        .map(|(band, acc)| {
            serde_json::json!({
                "band": band,
                "freq_khz": acc.freq_khz,
                "frequency": if acc.freq_khz == 0 {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(crate::band::fmt_mhz(acc.freq_khz))
                },
                "gateways": acc.gateways,
                "stations": acc.stations.len(),
            })
        })
        .collect()
}

async fn ws_live(ws: WebSocketUpgrade, State(st): State<HubState>) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        let mut rx = st.live.subscribe();
        while let Ok(v) = rx.recv().await {
            if socket
                .send(Message::Text(v.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}

#[derive(Deserialize)]
struct EventsQ {
    since: Option<i64>,
    limit: Option<i64>,
}

async fn get_events(
    State(st): State<HubState>,
    Query(q): Query<EventsQ>,
) -> Json<serde_json::Value> {
    let events = st
        .telemetry
        .events(q.since.unwrap_or(0), q.limit.unwrap_or(200));
    Json(serde_json::json!({ "events": events }))
}

#[derive(Deserialize)]
struct LinksQ {
    since: Option<i64>,
    limit: Option<i64>,
}

/// Latest contact per station pair. The map uses this for the 48-hour line fade
/// so it does not download every tx/rx in that window.
async fn get_links(State(st): State<HubState>, Query(q): Query<LinksQ>) -> Json<serde_json::Value> {
    let since = q.since.unwrap_or(0);
    let links = st.telemetry.links(since, q.limit.unwrap_or(2000));
    Json(serde_json::json!({ "links": links, "since": since }))
}

async fn get_hubs(State(st): State<HubState>) -> Json<serde_json::Value> {
    let n = st.sessions.lock().len();
    let fwd = *st.forwarded.lock();
    let bands = collect_bands(&st);
    Json(serde_json::json!({
        "hubs": [{
            "id": st.id,
            "connected_nodes": n,
            "forwarded": fwd,
            "uptime_secs": st.started.elapsed().as_secs(),
            "bands": bands
        }]
    }))
}

async fn get_bands(State(st): State<HubState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "bands": collect_bands(&st) }))
}

async fn get_stats(State(st): State<HubState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "nodes_online": st.sessions.lock().len(),
        "forwarded": *st.forwarded.lock(),
        "uptime_secs": st.started.elapsed().as_secs()
    }))
}

pub fn release_callsign(db: &TelemetryDb, call: &str) -> Result<()> {
    db.release_callsign(call)
}

/// In-memory hub state for integration tests.
pub fn test_hub_state() -> HubState {
    let (live, _) = broadcast::channel(8);
    HubState {
        store: Arc::new(Store::open_memory().unwrap()),
        telemetry: Arc::new(TelemetryDb::open_memory().unwrap()),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        live,
        started: std::time::Instant::now(),
        forwarded: Arc::new(Mutex::new(0)),
        identity: IdentityKeys::generate(),
        assembler: Arc::new(Mutex::new(FragAssembler::new())),
        next_conn: Arc::new(AtomicU64::new(1)),
        report_by_call: rate_limit::per_minute(120),
        report_by_ip: rate_limit::per_minute(240),
        hello_by_call: rate_limit::per_minute(30),
        mail: Arc::new(crate::net::hub_mail::MailHubDb::open_memory().unwrap()),
        id: hub_report_id("127.0.0.1:0", ""),
        pending_dm: Arc::new(Mutex::new(Vec::new())),
    }
}

/// Production sets `WCR_DOMAIN=weechatradio.com`. Any other hub reports a local id.
pub fn hub_report_id(bind: &str, domain: &str) -> String {
    if domain.eq_ignore_ascii_case("weechatradio.com") || bind.contains("weechatradio.com") {
        "hub.weechatradio.com".into()
    } else {
        format!("local:{bind}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Callsign, Envelope, Flags, IdentityKeys, MsgType};

    fn sample_msg(origin: &str) -> Envelope {
        Envelope::new_msg(
            Callsign::parse(origin).unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap()
    }

    #[test]
    fn unsigned_frames_forward() {
        let st = test_hub_state();
        let env = sample_msg("G4ABC");
        assert!(verify_inbound(&st, &env, "G4ABC", &[0u8; 32]));
    }

    #[test]
    fn signed_wrong_key_rejected() {
        let st = test_hub_state();
        let origin = IdentityKeys::generate();
        let other = IdentityKeys::generate();
        st.telemetry
            .bind_callsign("G4ABC", &origin.public_bytes())
            .unwrap();
        let mut env = sample_msg("G4ABC");
        other.sign_envelope(&mut env).unwrap();
        assert!(!verify_inbound(&st, &env, "G4ABC", &origin.public_bytes()));
        let mut good = sample_msg("G4ABC");
        origin.sign_envelope(&mut good).unwrap();
        assert!(verify_inbound(&st, &good, "G4ABC", &origin.public_bytes()));
    }

    #[test]
    fn signed_uses_session_key_when_unbound() {
        let st = test_hub_state();
        let origin = IdentityKeys::generate();
        let other = IdentityKeys::generate();
        let mut env = sample_msg("G4ABC");
        origin.sign_envelope(&mut env).unwrap();
        assert!(verify_inbound(&st, &env, "G4ABC", &origin.public_bytes()));
        assert!(!verify_inbound(&st, &env, "M0XYZ", &other.public_bytes()));
    }

    #[test]
    fn frag_frames_are_not_forwarded_only_reassembled() {
        let st = test_hub_state();
        let (tx_b, mut rx_b) = mpsc::channel(4);
        {
            let mut g = st.sessions.lock();
            g.insert(
                "G4ABC".into(),
                Session {
                    callsign: "G4ABC".into(),
                    pubkey: [0u8; 32],
                    serves: HashSet::from(["G4ABC".into()]),
                    tx: mpsc::channel(1).0,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 1,
                },
            );
            g.insert(
                "M0XYZ".into(),
                Session {
                    callsign: "M0XYZ".into(),
                    pubkey: [1u8; 32],
                    serves: HashSet::from(["M0XYZ".into()]),
                    tx: tx_b,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 2,
                },
            );
        }
        let mut env = sample_msg("G4ABC");
        env.dest = Callsign::from_raw("BULLETIN");
        env.kind = MsgType::Frag;
        let raw = env.encode().unwrap();
        let (_khz, _band, _bands, n) = route(&st, &env, &raw, "G4ABC");
        assert_eq!(n, 0);
        assert!(rx_b.try_recv().is_err(), "raw frag must not be routed");
    }

    #[test]
    fn private_hub_id_is_not_the_public_name() {
        assert_eq!(hub_report_id("127.0.0.1:7376", ""), "local:127.0.0.1:7376");
        assert_eq!(
            hub_report_id("0.0.0.0:7373", "weechatradio.com"),
            "hub.weechatradio.com"
        );
    }

    #[test]
    fn bulletin_reaches_all_sessions() {
        let st = test_hub_state();
        let (tx_a, mut rx_a) = mpsc::channel(4);
        let (tx_b, mut rx_b) = mpsc::channel(4);
        {
            let mut g = st.sessions.lock();
            g.insert(
                "G4ABC".into(),
                Session {
                    callsign: "G4ABC".into(),
                    pubkey: [0u8; 32],
                    serves: HashSet::from(["G4ABC".into()]),
                    tx: tx_a,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 1,
                },
            );
            g.insert(
                "M0XYZ".into(),
                Session {
                    callsign: "M0XYZ".into(),
                    pubkey: [1u8; 32],
                    serves: HashSet::from(["M0XYZ".into()]),
                    tx: tx_b,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 2,
                },
            );
        }
        let mut env = sample_msg("G4ABC");
        env.dest = Callsign::from_raw("BULLETIN");
        let raw = env.encode().unwrap();
        let (_khz, _band, _bands, n) = route(&st, &env, &raw, "G4ABC");
        assert_eq!(n, 1);
        assert!(!hold_offline_dm(&st, &env, &raw));
        assert!(rx_b.try_recv().is_ok());
        assert!(rx_a.try_recv().is_err());
    }

    #[test]
    fn closed_group_reaches_members_only() {
        let st = test_hub_state();
        st.store
            .group_create("net", &["G4AAA".into(), "M0BBB".into(), "2E0CCC".into()])
            .unwrap();
        let (tx_b, mut rx_b) = mpsc::channel(4);
        let (tx_c, mut rx_c) = mpsc::channel(4);
        let (tx_z, mut rx_z) = mpsc::channel(4);
        {
            let mut g = st.sessions.lock();
            for (call, tx) in [("G4AAA", tx_b), ("M0BBB", tx_c), ("G0ZZZ", tx_z)] {
                g.insert(
                    call.into(),
                    Session {
                        callsign: call.into(),
                        pubkey: [0u8; 32],
                        serves: HashSet::from([call.into()]),
                        tx,
                        connected_at: std::time::Instant::now(),
                        freq_khz: 0,
                        conn_id: 1,
                    },
                );
            }
        }
        let mut env = sample_msg("G4AAA");
        env.dest = Callsign::from_raw("NET");
        env.flags.set(crate::proto::FLAG_GROUP, true);
        let raw = env.encode().unwrap();
        let (_khz, _band, _bands, _n) = route(&st, &env, &raw, "G4AAA");
        assert!(
            !hold_offline_dm(&st, &env, &raw),
            "closed groups are not stored"
        );
        assert!(rx_b.try_recv().is_err(), "sender is not echoed");
        assert!(rx_c.try_recv().is_ok(), "member should receive the group");
        assert!(rx_z.try_recv().is_err(), "non-member must not receive it");
    }

    #[test]
    fn closed_group_follows_a_registered_group_name() {
        let st = test_hub_state();
        let (tx_m, mut rx_m) = mpsc::channel(4);
        let (tx_z, mut rx_z) = mpsc::channel(4);
        {
            let mut g = st.sessions.lock();
            g.insert(
                "M0BBB".into(),
                Session {
                    callsign: "M0BBB".into(),
                    pubkey: [0u8; 32],
                    serves: HashSet::from(["M0BBB".into(), "NET".into()]),
                    tx: tx_m,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 1,
                },
            );
            g.insert(
                "G0ZZZ".into(),
                Session {
                    callsign: "G0ZZZ".into(),
                    pubkey: [1u8; 32],
                    serves: HashSet::from(["G0ZZZ".into()]),
                    tx: tx_z,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 2,
                },
            );
        }
        let mut env = sample_msg("G4AAA");
        env.dest = Callsign::from_raw("NET");
        env.flags.set(crate::proto::FLAG_GROUP, true);
        let raw = env.encode().unwrap();
        let (_khz, _band, _bands, _n) = route(&st, &env, &raw, "G4AAA");
        assert!(rx_m.try_recv().is_ok());
        assert!(rx_z.try_recv().is_err());
    }

    #[test]
    fn session_replace_invalidates_old_conn() {
        let st = test_hub_state();
        let (tx1, _rx1) = mpsc::channel(1);
        let (tx2, _rx2) = mpsc::channel(1);
        {
            let mut g = st.sessions.lock();
            g.insert(
                "G4ABC".into(),
                Session {
                    callsign: "G4ABC".into(),
                    pubkey: [0u8; 32],
                    serves: HashSet::new(),
                    tx: tx1,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 1,
                },
            );
            g.insert(
                "G4ABC".into(),
                Session {
                    callsign: "G4ABC".into(),
                    pubkey: [1u8; 32],
                    serves: HashSet::new(),
                    tx: tx2,
                    connected_at: std::time::Instant::now(),
                    freq_khz: 0,
                    conn_id: 2,
                },
            );
        }
        assert!(!session_is_current(&st, "G4ABC", 1));
        assert!(session_is_current(&st, "G4ABC", 2));
        {
            let mut g = st.sessions.lock();
            if g.get("G4ABC").map(|s| s.conn_id) == Some(1) {
                g.remove("G4ABC");
            }
        }
        assert!(session_is_current(&st, "G4ABC", 2));
    }

    #[test]
    fn offline_dm_is_held_until_take() {
        let st = test_hub_state();
        let env = sample_msg("G4ABC");
        let raw = env.encode().unwrap();
        let (_khz, _band, _bands, n) = route(&st, &env, &raw, "G4ABC");
        assert_eq!(n, 0, "nobody is connected as M0XYZ");
        assert!(hold_offline_dm(&st, &env, &raw));
        let got = take_offline_dm(&st, "m0xyz");
        assert_eq!(got.len(), 1);
        assert!(take_offline_dm(&st, "M0XYZ").is_empty());
    }

    #[test]
    fn lonely_bulletin_is_not_stored() {
        let st = test_hub_state();
        let mut env = sample_msg("G4ABC");
        env.dest = Callsign::from_raw("BULLETIN");
        env.flags.set(crate::proto::FLAG_GROUP, true);
        let raw = env.encode().unwrap();
        let (_khz, _band, _bands, n) = route(&st, &env, &raw, "G4ABC");
        assert_eq!(n, 0);
        assert!(!hold_offline_dm(&st, &env, &raw));
        assert!(st.pending_dm.lock().is_empty());
    }

    #[test]
    fn offline_dm_drops_expired_and_caps_per_dest() {
        let st = test_hub_state();
        st.pending_dm.lock().push(HeldDm {
            dest: "M0XYZ".into(),
            raw: b"stale".to_vec(),
            expires: 1,
        });
        assert!(take_offline_dm(&st, "M0XYZ").is_empty());
        for i in 0..(OFFLINE_DM_CAP as u32 + 1) {
            let mut env = sample_msg("G4ABC");
            env.seq = i;
            env.body = format!("n{i}").into_bytes();
            let raw = env.encode().unwrap();
            assert!(hold_offline_dm(&st, &env, &raw));
        }
        assert_eq!(st.pending_dm.lock().len(), OFFLINE_DM_CAP);
        let got = take_offline_dm(&st, "M0XYZ");
        assert_eq!(got.len(), OFFLINE_DM_CAP);
        let first = Envelope::decode(&got[0]).unwrap();
        let last = Envelope::decode(got.last().unwrap()).unwrap();
        assert_ne!(first.body, b"n0");
        assert_eq!(last.body, format!("n{}", OFFLINE_DM_CAP).into_bytes());
    }
}
