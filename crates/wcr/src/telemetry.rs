//! SPDX-License-Identifier: Apache-2.0
//! Telemetry: signed node reports, public read API, SQLite storage.

use crate::error::{Error, Result};
use crate::grid;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS keys (
    callsign TEXT PRIMARY KEY,
    pubkey BLOB NOT NULL,
    first_seen INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS nodes (
    callsign TEXT PRIMARY KEY,
    grid TEXT,
    lat REAL,
    lon REAL,
    mode TEXT,
    ptt TEXT,
    preset TEXT,
    snr REAL,
    ber REAL,
    queue INTEGER,
    hub_ok INTEGER,
    last_seen INTEGER NOT NULL,
    settings TEXT,
    freq_khz INTEGER NOT NULL DEFAULT 0,
    band TEXT
);
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,
    kind TEXT NOT NULL,
    origin TEXT,
    dest TEXT,
    hops INTEGER,
    snr REAL,
    payload TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts);
CREATE TABLE IF NOT EXISTS blocks (
    pubkey BLOB PRIMARY KEY
);
"#;

pub struct TelemetryDb {
    conn: Mutex<Connection>,
    last_ts: Mutex<HashMap<String, u64>>,
}

impl TelemetryDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn);
        Ok(Self {
            conn: Mutex::new(conn),
            last_ts: Mutex::new(HashMap::new()),
        })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Self::migrate(&conn);
        Ok(Self {
            conn: Mutex::new(conn),
            last_ts: Mutex::new(HashMap::new()),
        })
    }

    fn migrate(conn: &Connection) {
        let _ = conn.execute(
            "ALTER TABLE nodes ADD COLUMN freq_khz INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = conn.execute("ALTER TABLE nodes ADD COLUMN band TEXT", []);
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS node_seen (
                ts INTEGER NOT NULL,
                callsign TEXT NOT NULL,
                mode TEXT,
                grid TEXT,
                lat REAL,
                lon REAL,
                band TEXT,
                freq_khz INTEGER,
                ptt TEXT,
                preset TEXT,
                snr REAL
            );
            CREATE INDEX IF NOT EXISTS idx_node_seen ON node_seen(callsign, ts);",
        );
    }

    pub fn bind_callsign(&self, call: &str, pk: &[u8; 32]) -> Result<()> {
        let now = now();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO keys(callsign, pubkey, first_seen) VALUES(?1,?2,?3)",
            params![call.to_ascii_uppercase(), pk.as_slice(), now as i64],
        )?;
        Ok(())
    }

    pub fn get_pubkey(&self, call: &str) -> Option<[u8; 32]> {
        let conn = self.conn.lock();
        let v: Option<Vec<u8>> = conn
            .query_row(
                "SELECT pubkey FROM keys WHERE callsign = ?1",
                params![call.to_ascii_uppercase()],
                |r| r.get(0),
            )
            .ok();
        v.and_then(|b| {
            if b.len() == 32 {
                let mut a = [0u8; 32];
                a.copy_from_slice(&b);
                Some(a)
            } else {
                None
            }
        })
    }

    pub fn is_blocked(&self, pk: &[u8; 32]) -> bool {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT 1 FROM blocks WHERE pubkey = ?1",
            params![pk.as_slice()],
            |_| Ok(()),
        )
        .is_ok()
    }

    pub fn block_pubkey(&self, pk: &[u8; 32]) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO blocks(pubkey) VALUES(?1)",
            params![pk.as_slice()],
        )?;
        Ok(())
    }

    pub fn release_callsign(&self, call: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM keys WHERE callsign = ?1",
            params![call.to_ascii_uppercase()],
        )?;
        Ok(())
    }

    pub fn upsert_node(&self, report: &NodeReport) -> Result<()> {
        let (lat, lon) = if report.grid.is_empty() {
            (None, None)
        } else {
            grid::to_lat_lon(&report.grid)
                .ok()
                .map(|(a, b)| (Some(a), Some(b)))
                .unwrap_or((None, None))
        };
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO nodes(callsign, grid, lat, lon, mode, ptt, preset, snr, ber, queue, hub_ok, last_seen, settings, freq_khz, band)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             ON CONFLICT(callsign) DO UPDATE SET
                grid=excluded.grid, lat=excluded.lat, lon=excluded.lon, mode=excluded.mode,
                ptt=excluded.ptt, preset=excluded.preset, snr=excluded.snr, ber=excluded.ber,
                queue=excluded.queue, hub_ok=excluded.hub_ok, last_seen=excluded.last_seen,
                settings=excluded.settings, freq_khz=excluded.freq_khz, band=excluded.band",
            params![
                report.callsign.to_ascii_uppercase(),
                report.grid,
                lat,
                lon,
                report.mode,
                report.ptt,
                report.preset,
                report.snr,
                report.ber,
                report.queue as i64,
                report.hub_ok as i64,
                report.ts as i64,
                serde_json::to_string(&report.settings).unwrap_or_else(|_| "{}".into()),
                report.freq_khz as i64,
                report.band,
            ],
        )?;
        conn.execute(
            "INSERT INTO node_seen(ts, callsign, mode, grid, lat, lon, band, freq_khz, ptt, preset, snr)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                report.ts as i64,
                report.callsign.to_ascii_uppercase(),
                report.mode,
                report.grid,
                lat,
                lon,
                report.band,
                report.freq_khz as i64,
                report.ptt,
                report.preset,
                report.snr,
            ],
        )?;
        conn.execute(
            "DELETE FROM node_seen WHERE ts < ?1",
            params![(now() - 7 * 86400) as i64],
        )?;
        Ok(())
    }

    /// Latest report for each station at or before `at`, if they were heard in the prior 30 minutes.
    pub fn nodes_at(&self, at: i64) -> Vec<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT n.callsign, n.grid, n.lat, n.lon, n.mode, n.ptt, n.preset, n.snr, n.band, n.freq_khz
                 FROM node_seen n
                 INNER JOIN (
                    SELECT callsign, MAX(ts) AS ts
                    FROM node_seen
                    WHERE ts <= ?1 AND ts >= ?2
                    GROUP BY callsign
                 ) latest ON n.callsign = latest.callsign AND n.ts = latest.ts",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![at, at - 1800], |r| {
                let freq_khz = r.get::<_, i64>(9).unwrap_or(0) as u32;
                let band: Option<String> = r.get(8)?;
                let frequency = if freq_khz == 0 {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(crate::band::fmt_mhz(freq_khz))
                };
                Ok(serde_json::json!({
                    "callsign": r.get::<_, String>(0)?,
                    "grid": r.get::<_, Option<String>>(1)?,
                    "lat": r.get::<_, Option<f64>>(2)?,
                    "lon": r.get::<_, Option<f64>>(3)?,
                    "mode": r.get::<_, Option<String>>(4)?,
                    "ptt": r.get::<_, Option<String>>(5)?,
                    "preset": r.get::<_, Option<String>>(6)?,
                    "snr": r.get::<_, Option<f64>>(7)?,
                    "band": band.unwrap_or_default(),
                    "freq_khz": freq_khz,
                    "frequency": frequency,
                }))
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Mode and position samples since `since`, oldest first. The map replays from this.
    pub fn nodes_since(&self, since: i64) -> Vec<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT ts, callsign, grid, lat, lon, mode, ptt, preset, snr, band, freq_khz
                 FROM node_seen WHERE ts >= ?1 ORDER BY ts ASC LIMIT 8000",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![since], |r| {
                let freq_khz = r.get::<_, i64>(10).unwrap_or(0) as u32;
                let band: Option<String> = r.get(9)?;
                Ok(serde_json::json!({
                    "ts": r.get::<_, i64>(0)?,
                    "callsign": r.get::<_, String>(1)?,
                    "grid": r.get::<_, Option<String>>(2)?,
                    "lat": r.get::<_, Option<f64>>(3)?,
                    "lon": r.get::<_, Option<f64>>(4)?,
                    "mode": r.get::<_, Option<String>>(5)?,
                    "ptt": r.get::<_, Option<String>>(6)?,
                    "preset": r.get::<_, Option<String>>(7)?,
                    "snr": r.get::<_, Option<f64>>(8)?,
                    "band": band.unwrap_or_default(),
                    "freq_khz": freq_khz,
                }))
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn add_event(&self, ev: &TelemetryEvent) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO events(ts, kind, origin, dest, hops, snr, payload) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                ev.ts as i64,
                ev.kind,
                ev.origin,
                ev.dest,
                ev.hops.map(|h| h as i64),
                ev.snr,
                serde_json::to_string(ev).unwrap_or_default()
            ],
        )?;
        conn.execute(
            "DELETE FROM events WHERE ts < ?1",
            params![(now() - 7 * 86400) as i64],
        )?;
        Ok(())
    }

    pub fn nodes(&self) -> Vec<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT callsign, grid, lat, lon, mode, ptt, preset, snr, ber, queue, hub_ok, last_seen, settings, freq_khz, band FROM nodes")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                let freq_khz = r.get::<_, i64>(13).unwrap_or(0) as u32;
                let band: Option<String> = r.get(14)?;
                let frequency = if freq_khz == 0 {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(crate::band::fmt_mhz(freq_khz))
                };
                Ok(serde_json::json!({
                    "callsign": r.get::<_, String>(0)?,
                    "grid": r.get::<_, Option<String>>(1)?,
                    "lat": r.get::<_, Option<f64>>(2)?,
                    "lon": r.get::<_, Option<f64>>(3)?,
                    "mode": r.get::<_, Option<String>>(4)?,
                    "ptt": r.get::<_, Option<String>>(5)?,
                    "preset": r.get::<_, Option<String>>(6)?,
                    "snr": r.get::<_, Option<f64>>(7)?,
                    "ber": r.get::<_, Option<f64>>(8)?,
                    "queue": r.get::<_, Option<i64>>(9)?,
                    "hub_ok": r.get::<_, Option<i64>>(10)? == Some(1),
                    "last_seen": r.get::<_, i64>(11)?,
                    "settings": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(12).unwrap_or_else(|_| "{}".into())).unwrap_or(serde_json::json!({})),
                    "freq_khz": freq_khz,
                    "band": band.unwrap_or_default(),
                    "frequency": frequency,
                }))
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn events(&self, since: i64, limit: i64) -> Vec<serde_json::Value> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT payload FROM events WHERE ts >= ?1 ORDER BY ts DESC LIMIT ?2")
            .unwrap();
        let rows = stmt
            .query_map(params![since, limit], |r| r.get::<_, String>(0))
            .unwrap();
        rows.filter_map(|r| r.ok())
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect()
    }

    pub fn check_replay(&self, call: &str, ts: u64) -> bool {
        let now = now();
        if ts + 300 < now || ts > now + 60 {
            return false;
        }
        let mut g = self.last_ts.lock();
        if let Some(prev) = g.get(call) {
            if ts <= *prev {
                return false;
            }
        }
        g.insert(call.to_string(), ts);
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeReport {
    pub callsign: String,
    pub ts: u64,
    #[serde(default)]
    pub grid: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub ptt: String,
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub snr: f32,
    #[serde(default)]
    pub ber: f32,
    #[serde(default)]
    pub queue: u64,
    #[serde(default)]
    pub hub_ok: bool,
    #[serde(default)]
    pub settings: serde_json::Value,
    #[serde(default)]
    pub events: Vec<TelemetryEvent>,
    #[serde(default)]
    pub freq_khz: u32,
    #[serde(default)]
    pub band: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub ts: u64,
    pub kind: String,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub dest: Option<String>,
    #[serde(default)]
    pub hops: Option<u8>,
    #[serde(default)]
    pub snr: Option<f32>,
    #[serde(default)]
    pub msgid: Option<String>,
    #[serde(default)]
    pub band: Option<String>,
}

pub async fn ingest_report(
    State(st): State<crate::net::hub_server::HubState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let call = header(&headers, "x-radio-callsign")
        .ok_or((StatusCode::BAD_REQUEST, "missing callsign".into()))?
        .to_ascii_uppercase();
    if !crate::rate_limit::allow(&st.report_by_call, &call) {
        return Err((StatusCode::TOO_MANY_REQUESTS, "rate limit".into()));
    }
    let ip_key = client_ip(&headers, peer);
    if !crate::rate_limit::allow(&st.report_by_ip, &ip_key) {
        return Err((StatusCode::TOO_MANY_REQUESTS, "rate limit".into()));
    }
    let pkhex = header(&headers, "x-radio-pubkey")
        .ok_or((StatusCode::BAD_REQUEST, "missing pubkey".into()))?;
    let sighex = header(&headers, "x-radio-signature")
        .ok_or((StatusCode::BAD_REQUEST, "missing signature".into()))?;
    let pk = hex::decode(pkhex).map_err(|_| (StatusCode::BAD_REQUEST, "bad pubkey".into()))?;
    if pk.len() != 32 {
        return Err((StatusCode::BAD_REQUEST, "pubkey length".into()));
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk);
    let vk = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|_| (StatusCode::BAD_REQUEST, "bad pubkey".into()))?;
    let sigb = hex::decode(sighex).map_err(|_| (StatusCode::BAD_REQUEST, "bad sig".into()))?;
    if sigb.len() != 64 {
        return Err((StatusCode::BAD_REQUEST, "sig length".into()));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sigb);
    vk.verify(&body, &Signature::from_bytes(&sig_arr))
        .map_err(|_| (StatusCode::UNAUTHORIZED, "bad signature".into()))?;

    if st.telemetry.is_blocked(&pk_arr) {
        return Err((StatusCode::FORBIDDEN, "blocked".into()));
    }

    if let Some(existing) = st.telemetry.get_pubkey(&call) {
        if existing != pk_arr {
            return Err((StatusCode::CONFLICT, "callsign already claimed".into()));
        }
    } else if call.starts_with('~') {
        let _ = st.telemetry.bind_callsign(&call, &pk_arr);
    } else if crate::proto::is_plausible_callsign(&call) {
        let _ = st.telemetry.bind_callsign(&call, &pk_arr);
    } else {
        return Err((StatusCode::BAD_REQUEST, "callsign not plausible".into()));
    }

    let mut report: NodeReport =
        serde_json::from_slice(&body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if report.grid.trim().is_empty() {
        let ip = ip_key.clone();
        if let Ok(Some(g)) = tokio::task::spawn_blocking(move || grid::grid_for_ip(&ip)).await {
            report.grid = g;
        }
    }
    if report.callsign.to_ascii_uppercase() != call {
        return Err((StatusCode::BAD_REQUEST, "callsign mismatch".into()));
    }
    if !st.telemetry.check_replay(&call, report.ts) {
        return Err((StatusCode::BAD_REQUEST, "replay or clock skew".into()));
    }
    let _ = st.telemetry.upsert_node(&report);
    for ev in &report.events {
        let _ = st.telemetry.add_event(ev);
        let _ = st.live.send(serde_json::to_value(ev).unwrap_or_default());
    }
    let _ = st.live.send(serde_json::json!({
        "type": "node",
        "callsign": report.callsign,
        "mode": report.mode,
        "grid": report.grid,
        "band": report.band,
        "freq_khz": report.freq_khz,
    }));
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct NodesQuery {
    pub at: Option<i64>,
    pub since: Option<i64>,
}

pub async fn get_nodes(
    State(st): State<crate::net::hub_server::HubState>,
    axum::extract::Query(q): axum::extract::Query<NodesQuery>,
) -> Json<serde_json::Value> {
    if let Some(since) = q.since {
        return Json(serde_json::json!({
            "trail": st.telemetry.nodes_since(since),
            "since": since,
        }));
    }
    match q.at {
        Some(at) => Json(serde_json::json!({ "nodes": st.telemetry.nodes_at(at), "at": at })),
        None => Json(serde_json::json!({ "nodes": st.telemetry.nodes() })),
    }
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> String {
    header(headers, "x-forwarded-for")
        .or_else(|| header(headers, "x-real-ip"))
        .map(|ip| ip.split(',').next().unwrap_or(ip).trim().to_string())
        .unwrap_or_else(|| peer.ip().to_string())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub async fn reporter_loop(
    url: String,
    keys: crate::proto::IdentityKeys,
    callsign: String,
    snap: Arc<crate::status::SharedStatus>,
    mut events: tokio::sync::broadcast::Receiver<TelemetryEvent>,
    interval_secs: u64,
) {
    let client = reqwest::Client::new();
    let secs = interval_secs.max(1);
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(secs));
    let mut pending = Vec::new();
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let report = {
                    let s = snap.lock();
                    NodeReport {
                        callsign: callsign.clone(),
                        ts: now(),
                        grid: s.grid.clone(),
                        mode: s.mode.as_str().into(),
                        ptt: s.ptt.clone(),
                        preset: s.preset.clone(),
                        snr: s.snr,
                        ber: s.ber,
                        queue: s.queue_out,
                        hub_ok: s.hub_ok,
                        settings: serde_json::json!({
                            "frequency": s.frequency,
                            "audio": s.audio_label,
                            "band": s.band,
                        }),
                        events: std::mem::take(&mut pending),
                        freq_khz: s.freq_khz,
                        band: s.band.clone(),
                    }
                };
                if let Err(e) = post_report(&client, &url, &keys, &callsign, &report).await {
                    tracing::debug!("telemetry: {e}");
                }
            }
            Ok(ev) = events.recv() => {
                pending.push(ev);
                if pending.len() > 50 {
                    pending.remove(0);
                }
            }
        }
    }
}

async fn post_report(
    client: &reqwest::Client,
    url: &str,
    keys: &crate::proto::IdentityKeys,
    callsign: &str,
    report: &NodeReport,
) -> Result<()> {
    let body = serde_json::to_vec(report)?;
    let sig = keys.sign_bytes(&body);
    let res = client
        .post(url)
        .header("X-Radio-Callsign", callsign)
        .header("X-Radio-Pubkey", keys.public_hex())
        .header("X-Radio-Signature", hex::encode(sig))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if !res.status().is_success() {
        return Err(Error::Net(format!("telemetry HTTP {}", res.status())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::IdentityKeys;

    #[test]
    fn bind_and_replay() {
        let db = TelemetryDb::open_memory().unwrap();
        let keys = IdentityKeys::generate();
        db.bind_callsign("G4ABC", &keys.public_bytes()).unwrap();
        assert_eq!(db.get_pubkey("G4ABC").unwrap(), keys.public_bytes());
        let ts = now();
        assert!(db.check_replay("G4ABC", ts));
        assert!(!db.check_replay("G4ABC", ts));
        assert!(!db.check_replay("G4ABC", ts.saturating_sub(1000)));
    }

    #[test]
    fn events_roundtrip() {
        let db = TelemetryDb::open_memory().unwrap();
        let ev = TelemetryEvent {
            ts: now(),
            kind: "tx".into(),
            origin: Some("G4ABC".into()),
            dest: Some("BULLETIN".into()),
            hops: Some(1),
            snr: None,
            msgid: Some("abc".into()),
            band: Some("2m".into()),
        };
        db.add_event(&ev).unwrap();
        let rows = db.events(0, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["origin"], "G4ABC");
        assert_eq!(rows[0]["kind"], "tx");
    }

    #[test]
    fn nodes_at_keeps_the_mode_from_that_time() {
        let db = TelemetryDb::open_memory().unwrap();
        let t = now();
        let mut report = NodeReport {
            callsign: "M7TJF".into(),
            ts: t - 100,
            grid: "IO91".into(),
            mode: "internet".into(),
            ptt: String::new(),
            preset: "hf-poor".into(),
            snr: 0.0,
            ber: 0.0,
            queue: 0,
            hub_ok: true,
            settings: serde_json::json!({}),
            events: Vec::new(),
            freq_khz: 144_950,
            band: "2m".into(),
        };
        db.upsert_node(&report).unwrap();
        report.ts = t - 10;
        report.mode = "radio-plus".into();
        db.upsert_node(&report).unwrap();
        assert_eq!(db.nodes_at((t - 50) as i64)[0]["mode"], "internet");
        assert_eq!(db.nodes_at(t as i64)[0]["mode"], "radio-plus");
        assert!(db.nodes_at((t - 4000) as i64).is_empty());
    }

    #[test]
    fn blocked_pubkey() {
        let db = TelemetryDb::open_memory().unwrap();
        let keys = IdentityKeys::generate();
        let pk = keys.public_bytes();
        assert!(!db.is_blocked(&pk));
        db.block_pubkey(&pk).unwrap();
        assert!(db.is_blocked(&pk));
    }
}
