//! SPDX-License-Identifier: Apache-2.0
//! Hub WebSocket server + telemetry HTTP API.

use crate::error::Result;
use crate::proto::frag::FragAssembler;
use crate::proto::{verify_envelope, Callsign, Envelope, IdentityKeys, MsgType};
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
}

pub struct Session {
    pub callsign: String,
    pub pubkey: [u8; 32],
    pub serves: HashSet<String>,
    pub tx: mpsc::Sender<Vec<u8>>,
    pub connected_at: std::time::Instant,
    pub freq_khz: u32,
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
    };
    let app = Router::new()
        .route("/", get(ws_upgrade))
        .route("/ws", get(ws_upgrade))
        .route("/ws/live", get(ws_live))
        .route("/api/v1/report", post(telemetry::ingest_report))
        .route("/api/v1/nodes", get(telemetry::get_nodes))
        .route("/api/v1/events", get(get_events))
        .route("/api/v1/hubs", get(get_hubs))
        .route("/api/v1/bands", get(get_bands))
        .route("/api/v1/stats", get(get_stats))
        .route("/healthz", get(|| async { "ok" }))
        .layer(CorsLayer::permissive())
        .with_state(state);
    let addr: SocketAddr = bind
        .parse()
        .map_err(|e| crate::error::Error::Net(format!("{e}")))?;
    tracing::info!("hub listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
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
    // first key wins
    if let Some(existing) = st.telemetry.get_pubkey(&hello.callsign) {
        if existing != pk {
            let _ = socket
                .send(Message::Text(
                    "{\"ok\":false,\"error\":\"callsign already claimed\"}".into(),
                ))
                .await;
            return;
        }
    } else {
        let _ = st.telemetry.bind_callsign(&hello.callsign, &pk);
    }
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(64);
    let mut serves: HashSet<String> = hello
        .heard
        .into_iter()
        .map(|s| s.to_ascii_uppercase())
        .collect();
    serves.insert(hello.callsign.to_ascii_uppercase());
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
            },
        );
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
        match msg {
            Message::Binary(bin) => {
                if let Ok(env) = Envelope::decode(&bin) {
                    if env.flags.signed() {
                        if let Ok(vk2) = VerifyingKey::from_bytes(&pk) {
                            let _ = verify_envelope(&env, &vk2);
                        }
                    }
                    let mut extras: Vec<Envelope> = Vec::new();
                    if env.kind == MsgType::Frag {
                        if let Ok(Some(full)) = st.assembler.lock().push(&env) {
                            if st.store.seen_before(&full.msg_id).ok() != Some(true) {
                                extras.push(full);
                            }
                        }
                    }
                    let _ = st.store.insert(&env, crate::store::Delivery::Queued);
                    *st.forwarded.lock() += 1;
                    let (from_khz, from_band, to_bands) = route(&st, &env, &bin, &call);
                    let _ = st.live.send(serde_json::json!({
                        "type": "hub_forward",
                        "origin": env.origin.to_string(),
                        "dest": env.dest.to_string(),
                        "kind": env.kind.as_str(),
                        "id": env.msg_id.hex(),
                        "from_band": from_band,
                        "from_freq_khz": from_khz,
                        "to_bands": to_bands,
                    }));
                    for full in extras {
                        if let Ok(raw) = full.encode() {
                            let _ = st.store.insert(&full, crate::store::Delivery::Queued);
                            *st.forwarded.lock() += 1;
                            let (from_khz, from_band, to_bands) = route(&st, &full, &raw, &call);
                            let _ = st.live.send(serde_json::json!({
                                "type": "hub_forward",
                                "origin": full.origin.to_string(),
                                "dest": full.dest.to_string(),
                                "kind": full.kind.as_str(),
                                "id": full.msg_id.hex(),
                                "from_band": from_band,
                                "from_freq_khz": from_khz,
                                "to_bands": to_bands,
                            }));
                        }
                    }
                }
            }
            Message::Text(t) => {
                if t.contains("heard") || t.contains("freq_khz") {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                        let mut g = st.sessions.lock();
                        if let Some(s) = g.get_mut(&call) {
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
    st.sessions.lock().remove(&call);
    drop(writer);
}

fn route(st: &HubState, env: &Envelope, raw: &[u8], from: &str) -> (u32, String, Vec<String>) {
    let dest = env.dest.to_string().to_ascii_uppercase();
    let (from_khz, sessions) = {
        let g = st.sessions.lock();
        let from_khz = g.get(from).map(|s| s.freq_khz).unwrap_or(0);
        let sessions: Vec<(u32, mpsc::Sender<Vec<u8>>)> = g
            .iter()
            .filter(|(call, s)| *call != from && (s.serves.contains(&dest) || *call == &dest))
            .map(|(_, s)| (s.freq_khz, s.tx.clone()))
            .collect();
        (from_khz, sessions)
    };
    let to_khz: Vec<u32> = sessions.iter().map(|(k, _)| *k).collect();
    for (_khz, tx) in sessions {
        let _ = tx.try_send(raw.to_vec());
    }
    (
        from_khz,
        crate::band::band_label(from_khz),
        crate::band::to_bands(to_khz),
    )
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

async fn get_hubs(State(st): State<HubState>) -> Json<serde_json::Value> {
    let n = st.sessions.lock().len();
    let fwd = *st.forwarded.lock();
    let bands = collect_bands(&st);
    Json(serde_json::json!({
        "hubs": [{
            "id": "hub.weechatradio.com",
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
