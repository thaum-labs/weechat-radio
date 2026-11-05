//! SPDX-License-Identifier: Apache-2.0
//! Hub mail queue and Resend. The API key stays in the environment.

use crate::error::{Error, Result};
use crate::mail::{
    callsign_from_wcr, copy_code_digits, pull_token, trim_body, validate_copy_addr,
    validate_internet_addr, wcr_address, MAIL_DOMAIN, WCR_COPY_HEADER,
};
use crate::net::hub_server::HubState;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS hub_mail (
    id TEXT PRIMARY KEY,
    callsign TEXT NOT NULL,
    direction TEXT NOT NULL,
    from_addr TEXT NOT NULL,
    to_addr TEXT NOT NULL,
    subject TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    received INTEGER NOT NULL,
    fetched INTEGER NOT NULL DEFAULT 0,
    resend_id TEXT
);
CREATE TABLE IF NOT EXISTS hub_mail_copy (
    callsign TEXT PRIMARY KEY,
    copy_to TEXT NOT NULL DEFAULT '',
    confirmed INTEGER NOT NULL DEFAULT 0,
    pending_to TEXT NOT NULL DEFAULT '',
    pending_code TEXT NOT NULL DEFAULT ''
);
"#;

/// Columns added after the first hub release. `mail_id` is the station's own
/// id for an outbound mail, so the station can ask how it went.
fn migrate(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(hub_mail)")?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    for (col, ddl) in [
        (
            "mail_id",
            "ALTER TABLE hub_mail ADD COLUMN mail_id TEXT NOT NULL DEFAULT ''",
        ),
        (
            "last_event",
            "ALTER TABLE hub_mail ADD COLUMN last_event TEXT NOT NULL DEFAULT ''",
        ),
        (
            "checked",
            "ALTER TABLE hub_mail ADD COLUMN checked INTEGER NOT NULL DEFAULT 0",
        ),
    ] {
        if !names.iter().any(|n| n == col) {
            conn.execute(ddl, [])?;
        }
    }
    Ok(())
}

/// Resend's `last_event` values that will not change again.
pub fn delivery_final(event: &str) -> bool {
    matches!(
        event,
        "delivered" | "opened" | "clicked" | "bounced" | "complained" | "failed" | "canceled"
    )
}

/// An outbound mail as the hub knows it, for a status lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutStatus {
    pub poster: String,
    pub from_addr: String,
    pub resend_id: String,
    pub last_event: String,
    pub checked: i64,
}

pub struct MailHubDb {
    conn: Mutex<Connection>,
}

impl MailHubDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_in(
        &self,
        callsign: &str,
        from: &str,
        to: &str,
        subject: &str,
        body: &str,
        resend_id: Option<&str>,
    ) -> Result<String> {
        let id = blake3::hash(format!("in{callsign}{from}{to}{subject}{body}").as_bytes())
            .to_hex()
            .to_string();
        let now = chrono::Utc::now().timestamp();
        self.conn.lock().execute(
            "INSERT OR IGNORE INTO hub_mail(id, callsign, direction, from_addr, to_addr, subject, body, received, fetched, resend_id)
             VALUES(?1,?2,'in',?3,?4,?5,?6,?7,0,?8)",
            params![id, callsign.to_ascii_uppercase(), from, to, subject, body, now, resend_id],
        )?;
        Ok(id)
    }

    pub fn insert_out(
        &self,
        callsign: &str,
        from: &str,
        to: &str,
        subject: &str,
        body: &str,
        resend_id: &str,
        mail_id: &str,
    ) -> Result<String> {
        let id = blake3::hash(format!("out{callsign}{to}{subject}{body}{resend_id}").as_bytes())
            .to_hex()
            .to_string();
        let now = chrono::Utc::now().timestamp();
        self.conn.lock().execute(
            "INSERT INTO hub_mail(id, callsign, direction, from_addr, to_addr, subject, body, received, fetched, resend_id, mail_id)
             VALUES(?1,?2,'out',?3,?4,?5,?6,?7,1,?8,?9)",
            params![id, callsign.to_ascii_uppercase(), from, to, subject, body, now, resend_id, mail_id],
        )?;
        Ok(id)
    }

    /// The newest outbound row for a station's mail id.
    pub fn out_status(&self, mail_id: &str) -> Option<OutStatus> {
        if mail_id.trim().is_empty() {
            return None;
        }
        self.conn
            .lock()
            .query_row(
                "SELECT callsign, from_addr, COALESCE(resend_id, ''), last_event, checked FROM hub_mail
                 WHERE mail_id = ?1 AND direction = 'out' ORDER BY received DESC LIMIT 1",
                params![mail_id],
                |r| {
                    Ok(OutStatus {
                        poster: r.get(0)?,
                        from_addr: r.get(1)?,
                        resend_id: r.get(2)?,
                        last_event: r.get(3)?,
                        checked: r.get(4)?,
                    })
                },
            )
            .ok()
    }

    pub fn set_last_event(&self, mail_id: &str, event: &str, now: i64) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE hub_mail SET last_event = ?2, checked = ?3 WHERE mail_id = ?1 AND direction = 'out'",
            params![mail_id, event, now],
        )?;
        Ok(())
    }

    pub fn headers(&self, callsign: &str) -> Vec<Value> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, from_addr, subject, LENGTH(body) FROM hub_mail
                 WHERE callsign = ?1 AND direction = 'in' AND fetched = 0 ORDER BY received ASC",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![callsign.to_ascii_uppercase()], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "from_addr": r.get::<_, String>(1)?,
                    "subject": r.get::<_, String>(2)?,
                    "bytes": r.get::<_, i64>(3)?,
                }))
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn fetch(&self, callsign: &str, ids: &[String]) -> Vec<Value> {
        let conn = self.conn.lock();
        let mut out = Vec::new();
        for id in ids {
            let row = conn.query_row(
                "SELECT id, from_addr, to_addr, subject, body, received FROM hub_mail
                 WHERE id = ?1 AND callsign = ?2 AND direction = 'in'",
                params![id, callsign.to_ascii_uppercase()],
                |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "from": r.get::<_, String>(1)?,
                        "to": r.get::<_, String>(2)?,
                        "subject": r.get::<_, String>(3)?,
                        "body": r.get::<_, String>(4)?,
                        // When the mail reached the hub: the Inbox shows this,
                        // not when the station fetched it.
                        "ts": r.get::<_, i64>(5)?,
                    }))
                },
            );
            if let Ok(v) = row {
                let _ = conn.execute("UPDATE hub_mail SET fetched = 1 WHERE id = ?1", params![id]);
                out.push(v);
            }
        }
        out
    }

    pub fn set_pending(&self, callsign: &str, address: &str, code: &str) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO hub_mail_copy(callsign, copy_to, confirmed, pending_to, pending_code)
             VALUES(?1, '', 0, ?2, ?3)
             ON CONFLICT(callsign) DO UPDATE SET pending_to = excluded.pending_to, pending_code = excluded.pending_code, confirmed = 0",
            params![callsign.to_ascii_uppercase(), address, code],
        )?;
        Ok(())
    }

    /// The code is kept after it is used, so a station that missed the reply
    /// can confirm again with the same code.
    pub fn confirm(&self, callsign: &str, code: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let row: Option<(String, String, bool)> = conn
            .query_row(
                "SELECT pending_to, pending_code, confirmed FROM hub_mail_copy WHERE callsign = ?1",
                params![callsign.to_ascii_uppercase()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)),
            )
            .ok();
        let Some((to, expect, confirmed)) = row else {
            return Ok(false);
        };
        if expect.is_empty() || expect != code {
            return Ok(false);
        }
        if to.is_empty() {
            return Ok(confirmed);
        }
        conn.execute(
            "UPDATE hub_mail_copy SET copy_to = ?2, confirmed = 1, pending_to = '' WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase(), to],
        )?;
        Ok(true)
    }

    /// Copy-to off for this callsign: no address, no pending code.
    pub fn clear_copy(&self, callsign: &str) -> Result<()> {
        self.conn.lock().execute(
            "DELETE FROM hub_mail_copy WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
        )?;
        Ok(())
    }

    pub fn copy_to(&self, callsign: &str) -> Option<String> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT copy_to FROM hub_mail_copy WHERE callsign = ?1 AND confirmed = 1",
            params![callsign.to_ascii_uppercase()],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .filter(|s| !s.is_empty())
    }
}

fn verify(headers: &HeaderMap, body: &[u8]) -> std::result::Result<String, (StatusCode, String)> {
    let call = header(headers, "x-wcr-callsign")
        .ok_or((StatusCode::UNAUTHORIZED, "missing callsign".into()))?;
    let pk = header(headers, "x-wcr-pubkey")
        .ok_or((StatusCode::UNAUTHORIZED, "missing pubkey".into()))?;
    let sig = header(headers, "x-wcr-signature")
        .ok_or((StatusCode::UNAUTHORIZED, "missing signature".into()))?;
    let pk_bytes = hex::decode(pk).map_err(|_| (StatusCode::UNAUTHORIZED, "bad pubkey".into()))?;
    let sig_bytes =
        hex::decode(sig).map_err(|_| (StatusCode::UNAUTHORIZED, "bad signature".into()))?;
    if pk_bytes.len() != 32 || sig_bytes.len() != 64 {
        return Err((StatusCode::UNAUTHORIZED, "bad signature".into()));
    }
    let mut pk_arr = [0u8; 32];
    let mut sig_arr = [0u8; 64];
    pk_arr.copy_from_slice(&pk_bytes);
    sig_arr.copy_from_slice(&sig_bytes);
    let key = VerifyingKey::from_bytes(&pk_arr)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "bad pubkey".into()))?;
    key.verify(body, &Signature::from_bytes(&sig_arr))
        .map_err(|_| (StatusCode::UNAUTHORIZED, "bad signature".into()))?;
    Ok(call.to_ascii_uppercase())
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

fn bad(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({ "ok": false, "error": msg })))
}

#[derive(Deserialize, Default)]
struct PullAuth {
    #[serde(default, rename = "for")]
    for_call: String,
    #[serde(default)]
    pk: String,
    #[serde(default)]
    sig: String,
    #[serde(default)]
    ts: i64,
}

/// A station reads its own inbox. A gateway reads another station's inbox only
/// when that station signed the request with its bound key (see [`owner_check`]).
fn authorize_pull(
    st: &HubState,
    signer: &str,
    auth: &PullAuth,
    ids: &[String],
) -> std::result::Result<String, (StatusCode, String)> {
    let wanted = auth.for_call.trim().to_ascii_uppercase();
    if wanted.is_empty() || wanted.eq_ignore_ascii_case(signer) {
        return Ok(signer.to_ascii_uppercase());
    }
    let now = chrono::Utc::now().timestamp();
    let token = pull_token(&wanted, auth.ts, ids);
    if let Err(why) = owner_check(st, &wanted, &auth.pk, &auth.sig, &token, auth.ts, now) {
        let why = why.text();
        tracing::warn!("mail pull for {wanted} via {signer} refused: {why}");
        return Err((StatusCode::UNAUTHORIZED, format!("mail pull: {why}")));
    }
    Ok(wanted)
}

/// A Radio+ station's gateway ask, as a gateway that heard it reports it.
#[derive(Deserialize, Default)]
struct GatewayAskReq {
    #[serde(default, rename = "for")]
    for_call: String,
    #[serde(default)]
    n: String,
    #[serde(default)]
    ts: i64,
    #[serde(default)]
    pk: String,
    #[serde(default)]
    sig: String,
    /// The asking station's grid.
    #[serde(default)]
    g: String,
    #[serde(default)]
    snr: Option<f32>,
    /// The reporting gateway's grid.
    #[serde(default)]
    grid: String,
    #[serde(default)]
    preset: String,
}

/// Why the hub refused a request a Radio+ station signed and a gateway forwarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerFail {
    /// The station's clock and the hub's differ by more than ten minutes.
    StaleTime,
    /// The key or signature does not decode, or does not sign this request.
    BadSignature,
    /// The callsign is registered to another key. Holds the callsign and the
    /// first 8 hex digits of the key that signed (public, never the seed).
    WrongKey { call: String, key: String },
}

impl OwnerFail {
    pub fn text(&self) -> String {
        match self {
            Self::StaleTime => "clock more than 10 minutes out".into(),
            Self::BadSignature => "bad signature".into(),
            Self::WrongKey { call, key } => format!(
                "{call} is registered to a different key on the hub (this station's key {key}…). \
                 Use the original identity.key or re-register."
            ),
        }
    }
}

/// True when `sig` is `pk` signing `msg` and `ts` is within ten minutes of
/// `now`. Returns the key.
pub fn owner_sig_ok(
    pk_hex: &str,
    sig_hex: &str,
    msg: &[u8],
    ts: i64,
    now: i64,
) -> std::result::Result<[u8; 32], OwnerFail> {
    if (now - ts).unsigned_abs() > 10 * 60 {
        return Err(OwnerFail::StaleTime);
    }
    let pk = hex::decode(pk_hex.trim()).map_err(|_| OwnerFail::BadSignature)?;
    let sig = hex::decode(sig_hex.trim()).map_err(|_| OwnerFail::BadSignature)?;
    if pk.len() != 32 || sig.len() != 64 {
        return Err(OwnerFail::BadSignature);
    }
    let mut pk_arr = [0u8; 32];
    let mut sig_arr = [0u8; 64];
    pk_arr.copy_from_slice(&pk);
    sig_arr.copy_from_slice(&sig);
    let key = VerifyingKey::from_bytes(&pk_arr).map_err(|_| OwnerFail::BadSignature)?;
    key.verify(msg, &Signature::from_bytes(&sig_arr))
        .map_err(|_| OwnerFail::BadSignature)?;
    Ok(pk_arr)
}

/// A request a Radio+ station signed and a gateway forwarded. The signature
/// must be good and the key must be the one already bound to the callsign;
/// a callsign the hub has never seen is bound to this key.
fn owner_check(
    st: &HubState,
    call: &str,
    pk_hex: &str,
    sig_hex: &str,
    msg: &[u8],
    ts: i64,
    now: i64,
) -> std::result::Result<(), OwnerFail> {
    let pk = owner_sig_ok(pk_hex, sig_hex, msg, ts, now)?;
    let call = call.trim().to_ascii_uppercase();
    match st.telemetry.get_pubkey(&call) {
        Some(bound) if bound != pk => Err(OwnerFail::WrongKey {
            call,
            key: hex::encode_upper(&pk[..4]),
        }),
        Some(_) => Ok(()),
        None => {
            let _ = st.telemetry.bind_callsign(&call, &pk);
            Ok(())
        }
    }
}

/// Mail gateway Auto (#275). A gateway reports a Radio+ station's ask that it
/// heard. The hub checks the station signed it, waits a short window for the
/// other gateways' reports, then answers each with its reply slot: best SNR
/// first, at most three, one after the other so no replies overlap.
pub async fn post_gateway_ask(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let gateway = match verify(&headers, &body) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    let req: GatewayAskReq = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return bad(StatusCode::BAD_REQUEST, "bad json").into_response(),
    };
    let asker = req.for_call.trim().to_ascii_uppercase();
    let nonce = req.n.trim();
    if asker.is_empty() || nonce.is_empty() || nonce.len() > 32 {
        return bad(StatusCode::BAD_REQUEST, "ask needs for and n").into_response();
    }
    let now = chrono::Utc::now().timestamp();
    let token = crate::mail::pick::ask_token(&asker, req.ts, nonce);
    if let Err(why) = owner_check(&st, &asker, &req.pk, &req.sig, &token, req.ts, now) {
        let why = why.text();
        tracing::warn!("gateway ask from {asker} via {gateway} refused: {why}");
        let msg = format!("gateway ask: {why}");
        return bad(StatusCode::UNAUTHORIZED, &msg).into_response();
    }
    let preset =
        crate::presets::Preset::parse(&req.preset).unwrap_or(crate::presets::Preset::HfPoor);
    let report = crate::mail::pick::Report {
        gateway,
        snr: req.snr.filter(|s| s.is_finite()),
        grid: crate::grid::position(&req.grid).unwrap_or_default(),
    };
    let slot = st
        .gateway_asks
        .report(&asker, nonce, &req.g, preset, report)
        .await;
    Json(json!({ "ok": true, "slot": slot })).into_response()
}

/// Copy-to belongs to whoever wrote the mail. A relaying station cannot attach
/// its own address to someone else's message, or ask for one.
fn author_bcc(
    poster: &str,
    from: &str,
    requested: Option<String>,
    confirmed: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let author = callsign_from_wcr(from);
    let own = author
        .as_deref()
        .is_some_and(|a| a.eq_ignore_ascii_case(poster));
    let asked = if own { requested } else { None };
    asked.or_else(|| author.as_deref().and_then(confirmed))
}

#[derive(Deserialize)]
struct SendReq {
    from: String,
    to: String,
    subject: String,
    body: String,
    #[serde(default)]
    bcc: Option<String>,
    /// The station's id for this mail. Older stations leave it out.
    #[serde(default)]
    mail_id: String,
}

pub async fn post_send(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let call = match verify(&headers, &body) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    let req: SendReq = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return bad(StatusCode::BAD_REQUEST, "bad json").into_response(),
    };
    if validate_internet_addr(&req.to).is_err() {
        return bad(StatusCode::BAD_REQUEST, "To must be an internet address").into_response();
    }
    let from = if req.from.contains('@') {
        req.from
    } else {
        wcr_address(&call)
    };
    if !from
        .to_ascii_lowercase()
        .ends_with(&format!("@{MAIL_DOMAIN}"))
    {
        return bad(
            StatusCode::BAD_REQUEST,
            "From must be a WeeChat Radio address",
        )
        .into_response();
    }
    // Safety net (#275): Auto sends Radio+ mail to one gateway, but should two
    // gateways ever post the same mail, Resend gets it once.
    if let Some(prev) = st.mail.out_status(req.mail_id.trim()) {
        if !prev.resend_id.is_empty() && prev.from_addr.eq_ignore_ascii_case(&from) {
            return Json(json!({ "ok": true, "id": prev.resend_id, "duplicate": true }))
                .into_response();
        }
    }
    let bcc = author_bcc(&call, &from, req.bcc, |a| st.mail.copy_to(a));
    match resend_send(
        &from,
        &req.to,
        &req.subject,
        &req.body,
        bcc.as_deref(),
        bcc.is_some(),
    )
    .await
    {
        Ok(resend_id) => {
            let _ = st.mail.insert_out(
                &call,
                &from,
                &req.to,
                &req.subject,
                &req.body,
                &resend_id,
                req.mail_id.trim(),
            );
            Json(json!({ "ok": true, "id": resend_id })).into_response()
        }
        Err(e) => bad(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    }
}

/// The station that posted the mail, or the station that wrote it (a Radio+
/// station whose gateway posted for it), may ask how it went.
pub fn may_read_status(signer: &str, row: &OutStatus) -> bool {
    row.poster.eq_ignore_ascii_case(signer)
        || callsign_from_wcr(&row.from_addr).is_some_and(|a| a.eq_ignore_ascii_case(signer))
}

#[derive(Deserialize)]
struct StatusReq {
    mail_id: String,
}

/// `POST /api/v1/mail/status`: Resend's `last_event` for one sent mail. The
/// hub asks Resend at most every 10 s per mail and stops once it is final.
pub async fn post_status(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let call = match verify(&headers, &body) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    let req: StatusReq = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return bad(StatusCode::BAD_REQUEST, "bad json").into_response(),
    };
    let Some(row) = st.mail.out_status(req.mail_id.trim()) else {
        return bad(StatusCode::NOT_FOUND, "the hub has no record of this mail").into_response();
    };
    if !may_read_status(&call, &row) {
        return bad(StatusCode::NOT_FOUND, "the hub has no record of this mail").into_response();
    }
    let now = chrono::Utc::now().timestamp();
    let mut event = row.last_event.clone();
    if !delivery_final(&event) && now - row.checked >= 10 && !row.resend_id.is_empty() {
        match resend_last_event(&row.resend_id).await {
            Ok(e) => {
                event = e;
                let _ = st.mail.set_last_event(req.mail_id.trim(), &event, now);
            }
            Err(e) => {
                return bad(StatusCode::BAD_GATEWAY, &e.to_string()).into_response();
            }
        }
    }
    Json(json!({ "ok": true, "state": event, "resend_id": row.resend_id })).into_response()
}

pub async fn post_inbox(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let call = match verify(&headers, &body) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    let req: PullAuth = serde_json::from_slice(&body).unwrap_or_default();
    let target = match authorize_pull(&st, &call, &req, &[]) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    Json(json!({ "ok": true, "headers": st.mail.headers(&target) })).into_response()
}

#[derive(Deserialize, Default)]
struct FetchReq {
    #[serde(default)]
    ids: Vec<String>,
    #[serde(flatten)]
    auth: PullAuth,
}

pub async fn post_fetch(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let call = match verify(&headers, &body) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    let req: FetchReq = serde_json::from_slice(&body).unwrap_or_default();
    let target = match authorize_pull(&st, &call, &req.auth, &req.ids) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    Json(json!({ "ok": true, "messages": st.mail.fetch(&target, &req.ids) })).into_response()
}

#[derive(Deserialize)]
struct CopyReq {
    address: String,
    #[serde(default)]
    code: String,
}

pub async fn post_copy(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let call = match verify(&headers, &body) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    let req: CopyReq = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return bad(StatusCode::BAD_REQUEST, "bad json").into_response(),
    };
    if req.address.trim().is_empty() {
        if st.mail.clear_copy(&call).is_err() {
            return bad(StatusCode::INTERNAL_SERVER_ERROR, "store").into_response();
        }
        return Json(json!({ "ok": true, "off": true })).into_response();
    }
    if let Err(e) = validate_copy_addr(&req.address) {
        return bad(StatusCode::BAD_REQUEST, &e.plain()).into_response();
    }
    let code = copy_code_digits(&req.code);
    if code.is_empty() {
        return bad(StatusCode::BAD_REQUEST, "missing code").into_response();
    }
    if st
        .mail
        .set_pending(&call, req.address.trim(), &code)
        .is_err()
    {
        return bad(StatusCode::INTERNAL_SERVER_ERROR, "store").into_response();
    }
    match resend_send(
        &wcr_address(&call),
        req.address.trim(),
        "Confirm your WeeChat Radio copy-to address",
        &copy_code_text(&call, &code),
        None,
        false,
    )
    .await
    {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(e) => bad(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    }
}

/// The code sits on its own line, with nothing after it to copy by mistake.
pub fn copy_code_text(call: &str, code: &str) -> String {
    format!(
        "Your copy-to code for {call}:\n\n{code}\n\nType it in the code box next to Copy-to on the Email tab of WeeChat Radio, then press confirm.\n\nIf you did not ask for this, ignore this email."
    )
}

#[derive(Deserialize)]
struct CodeReq {
    code: String,
}

pub async fn post_copy_confirm(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let call = match verify(&headers, &body) {
        Ok(c) => c,
        Err((s, m)) => return bad(s, &m).into_response(),
    };
    let req: CodeReq = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return bad(StatusCode::BAD_REQUEST, "bad json").into_response(),
    };
    let ok = st
        .mail
        .confirm(&call, &copy_code_digits(&req.code))
        .unwrap_or(false);
    Json(json!({ "ok": ok })).into_response()
}

#[derive(Deserialize)]
struct ResendEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: ResendEventData,
}

#[derive(Deserialize, Default)]
struct ResendEventData {
    #[serde(default)]
    email_id: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: Vec<String>,
    #[serde(default)]
    subject: String,
}

#[derive(Deserialize)]
struct ReceivedMail {
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: Vec<String>,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    headers: std::collections::BTreeMap<String, String>,
}

/// Resend signs with Svix. `secret` is the webhook signing secret (`whsec_…`).
pub fn webhook_signature_ok(
    secret: &str,
    msg_id: &str,
    timestamp: &str,
    body: &[u8],
    signature: &str,
) -> bool {
    let Ok(ts) = timestamp.parse::<i64>() else {
        return false;
    };
    let skew = (chrono::Utc::now().timestamp() - ts).unsigned_abs();
    if skew > 5 * 60 {
        return false;
    }
    let Some(expected) = sign_v1(secret, msg_id, timestamp, body) else {
        return false;
    };
    signature.split(' ').any(|part| {
        part.strip_prefix("v1,")
            .is_some_and(|sig| sig_eq(sig, &expected))
    })
}

fn sign_v1(secret: &str, msg_id: &str, timestamp: &str, body: &[u8]) -> Option<String> {
    let raw = secret.strip_prefix("whsec_").unwrap_or(secret);
    let key = base64::engine::general_purpose::STANDARD.decode(raw).ok()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).ok()?;
    mac.update(msg_id.as_bytes());
    mac.update(b".");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body);
    Some(base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()))
}

fn sig_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

pub async fn resend_webhook(
    State(st): State<HubState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let secret = std::env::var("RESEND_WEBHOOK_SECRET").unwrap_or_default();
    let ok = header(&headers, "svix-id").is_some_and(|id| {
        header(&headers, "svix-timestamp").is_some_and(|ts| {
            header(&headers, "svix-signature")
                .is_some_and(|sig| webhook_signature_ok(&secret, id, ts, &body, sig))
        })
    });
    if !ok {
        return bad(StatusCode::UNAUTHORIZED, "webhook").into_response();
    }
    let event: ResendEvent = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return bad(StatusCode::BAD_REQUEST, "bad json").into_response(),
    };
    if event.kind != "email.received" {
        return Json(json!({ "ok": true })).into_response();
    }
    let mail = match fetch_received(&event.data.email_id).await {
        Ok(v) => v,
        Err(_) => return bad(StatusCode::BAD_GATEWAY, "receiving").into_response(),
    };
    let from = if mail.from.is_empty() {
        event.data.from
    } else {
        mail.from
    };
    let to_list = if mail.to.is_empty() {
        event.data.to
    } else {
        mail.to
    };
    let subject = if mail.subject.is_empty() {
        event.data.subject
    } else {
        mail.subject
    };
    let text = trim_body(mail.text.as_deref().unwrap_or("")).unwrap_or_default();
    let to_joined = to_list.join(", ");
    let copy_header = mail
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case(WCR_COPY_HEADER) && v == "1");
    let mut stored = false;
    for addr in &to_list {
        let Some(call) = callsign_from_wcr(addr) else {
            continue;
        };
        if st
            .mail
            .insert_in(
                &call,
                &from,
                &to_joined,
                &subject,
                &text,
                Some(&event.data.email_id),
            )
            .is_ok()
        {
            stored = true;
            if !copy_header {
                if let Some(copy) = st.mail.copy_to(&call) {
                    let _ = resend_send(
                        &wcr_address(&call),
                        &copy,
                        &format!("Copy: {subject}"),
                        &text,
                        None,
                        true,
                    )
                    .await;
                }
            }
        }
    }
    Json(json!({ "ok": true, "stored": stored })).into_response()
}

async fn fetch_received(email_id: &str) -> Result<ReceivedMail> {
    if email_id.is_empty() || email_id.contains('/') {
        return Err(Error::protocol("missing email id"));
    }
    let key = std::env::var("RESEND_API_KEY")
        .map_err(|_| Error::config("RESEND_API_KEY is not set on the hub"))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let resp = client
        .get(format!(
            "https://api.resend.com/emails/receiving/{email_id}"
        ))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| Error::Net(e.to_string()))?;
    if !status.is_success() {
        return Err(Error::Net(
            "Resend did not return the inbound message".into(),
        ));
    }
    serde_json::from_value(v).map_err(|e| Error::Net(e.to_string()))
}

/// `GET /emails/{id}` on Resend. Returns its `last_event`, for example
/// `sent`, `delivered`, `delivery_delayed` or `bounced`.
async fn resend_last_event(resend_id: &str) -> Result<String> {
    if resend_id.is_empty() || resend_id.contains('/') {
        return Err(Error::protocol("missing email id"));
    }
    let key = std::env::var("RESEND_API_KEY")
        .map_err(|_| Error::config("RESEND_API_KEY is not set on the hub"))?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let resp = client
        .get(format!("https://api.resend.com/emails/{resend_id}"))
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| Error::Net(e.to_string()))?;
    if !status.is_success() {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Resend did not return the message status");
        return Err(Error::Net(msg.to_string()));
    }
    Ok(v.get("last_event")
        .and_then(|e| e.as_str())
        .unwrap_or("")
        .to_string())
}

async fn resend_send(
    from: &str,
    to: &str,
    subject: &str,
    text: &str,
    bcc: Option<&str>,
    mark_copy: bool,
) -> Result<String> {
    let key = std::env::var("RESEND_API_KEY")
        .map_err(|_| Error::config("RESEND_API_KEY is not set on the hub"))?;
    let mut payload = json!({
        "from": from,
        "to": [to],
        "subject": subject,
        "text": text,
    });
    if let Some(b) = bcc {
        payload["bcc"] = json!([b]);
    }
    if mark_copy || bcc.is_some() {
        payload["headers"] = json!({ WCR_COPY_HEADER: "1" });
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let resp = client
        .post("https://api.resend.com/emails")
        .bearer_auth(key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| Error::Net(e.to_string()))?;
    if !status.is_success() {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Resend rejected the message");
        return Err(Error::Net(msg.to_string()));
    }
    Ok(v.get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("accepted")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret() -> String {
        format!(
            "whsec_{}",
            base64::engine::general_purpose::STANDARD.encode(b"test-secret-that-is-long-enough")
        )
    }

    #[test]
    fn fetch_body_keeps_the_message_id_and_the_signature() {
        let body = r#"{"ids":["abc"],"for":"M7TJF","pk":"aa","sig":"bb","ts":1700000000}"#;
        let req: FetchReq = serde_json::from_str(body).unwrap_or_default();
        assert_eq!(req.ids, vec!["abc".to_string()]);
        assert_eq!(req.auth.for_call, "M7TJF");
        assert_eq!(req.auth.pk, "aa");
        assert_eq!(req.auth.sig, "bb");
        assert_eq!(req.auth.ts, 1_700_000_000);
    }

    fn signed(keys: &crate::proto::IdentityKeys, call: &str, body: &Value) -> (HeaderMap, Bytes) {
        let raw = serde_json::to_vec(body).unwrap();
        let mut h = HeaderMap::new();
        h.insert("x-wcr-callsign", call.parse().unwrap());
        h.insert("x-wcr-pubkey", keys.public_hex().parse().unwrap());
        h.insert(
            "x-wcr-signature",
            hex::encode(keys.sign_bytes(&raw)).parse().unwrap(),
        );
        (h, Bytes::from(raw))
    }

    async fn json_of(r: axum::response::Response) -> (StatusCode, Value) {
        let code = r.status();
        let b = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (code, serde_json::from_slice(&b).unwrap_or(Value::Null))
    }

    /// #275 safety net: a second gateway posting the same mail id gets the
    /// first post's Resend id back, and Resend is not called again.
    #[tokio::test]
    async fn the_hub_posts_a_mail_id_once() {
        let st = crate::net::hub_server::test_hub_state();
        let from = wcr_address("M7SND");
        st.mail
            .insert_out(
                "G4GW",
                &from,
                "op@example.com",
                "hi",
                "body",
                "re_first",
                "mailid1",
            )
            .unwrap();
        let gw2 = crate::proto::IdentityKeys::generate();
        let body = json!({
            "from": from, "to": "op@example.com", "subject": "hi", "body": "body",
            "mail_id": "mailid1",
        });
        let (h, b) = signed(&gw2, "G4TWO", &body);
        let (code, v) = json_of(post_send(State(st.clone()), h, b).await.into_response()).await;
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["id"], "re_first");
        assert_eq!(v["duplicate"], true);
    }

    /// A gateway ask is coordinated only when the asking station signed it.
    #[tokio::test]
    async fn the_hub_gives_a_slot_only_for_a_signed_ask() {
        let st = crate::net::hub_server::test_hub_state();
        let asker = crate::proto::IdentityKeys::generate();
        let gw = crate::proto::IdentityKeys::generate();
        let ts = chrono::Utc::now().timestamp();
        let sig = hex::encode(asker.sign_bytes(&crate::mail::pick::ask_token("M7ASK", ts, "abcd")));
        let mut ask = json!({
            "for": "M7ASK", "n": "abcd", "ts": ts, "pk": asker.public_hex(), "sig": sig,
            "g": "IO91WM", "snr": 4.5, "grid": "IO91VL", "preset": "vhf-fm",
        });
        let (h, b) = signed(&gw, "G4GW", &ask);
        let (code, v) = json_of(
            post_gateway_ask(State(st.clone()), h, b)
                .await
                .into_response(),
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["slot"]["idx"], 0);
        assert_eq!(v["slot"]["gateway"], "G4GW");
        // Someone else's nonce under the same signature is refused.
        ask["n"] = json!("ffff");
        let (h, b) = signed(&gw, "G4GW", &ask);
        let (code, _) = json_of(
            post_gateway_ask(State(st.clone()), h, b)
                .await
                .into_response(),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
    }

    // #278: a Radio+ station with a portable /M callsign, heard by the
    // internet-radio gateway M7TJF. These go through the real hub handlers and
    // the real key table, and build the station's and the gateway's requests
    // with the same functions the station and the gateway use.
    const RP: &str = "M7TJF/M";
    const GW: &str = "M7TJF";

    async fn ask_as(
        st: &HubState,
        station: &crate::proto::IdentityKeys,
        signed_as: &str,
        reported_as: &str,
        nonce: &str,
    ) -> (StatusCode, Value) {
        let ts = chrono::Utc::now().timestamp();
        let ask = crate::mail::pick::signed_ask(station, signed_as, ts, nonce, "IO91WM");
        let report =
            crate::mail::pick::ask_report(reported_as, nonce, &ask, Some(7.9), "IO91VL", "vhf-fm");
        let gw = crate::proto::IdentityKeys::generate();
        let (h, b) = signed(&gw, GW, &report);
        json_of(
            post_gateway_ask(State(st.clone()), h, b)
                .await
                .into_response(),
        )
        .await
    }

    /// Check mail (`ids` empty) or Get, as the gateway posts it for `RP`.
    async fn pull_as(st: &HubState, auth: &Value, ids: &[String]) -> (StatusCode, Value) {
        let body = crate::mail::gateway_pull_body(RP, ids, &auth.to_string());
        let gw = crate::proto::IdentityKeys::generate();
        let (h, b) = signed(&gw, GW, &body);
        let r = if ids.is_empty() {
            post_inbox(State(st.clone()), h, b).await.into_response()
        } else {
            post_fetch(State(st.clone()), h, b).await.into_response()
        };
        json_of(r).await
    }

    fn now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    #[tokio::test]
    async fn a_portable_radio_plus_ask_gets_a_slot() {
        let st = crate::net::hub_server::test_hub_state();
        let station = crate::proto::IdentityKeys::generate();
        let (code, v) = ask_as(&st, &station, RP, RP, "44ede7de00000001").await;
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["slot"]["idx"], 0, "{v}");
        assert_eq!(v["slot"]["gateway"], GW, "{v}");
        assert_eq!(st.telemetry.get_pubkey(RP), Some(station.public_bytes()));
    }

    #[tokio::test]
    async fn a_portable_radio_plus_check_mail_and_get_are_answered() {
        let st = crate::net::hub_server::test_hub_state();
        let station = crate::proto::IdentityKeys::generate();
        let id = st
            .mail
            .insert_in(RP, "op@example.com", &wcr_address(RP), "hi", "hello", None)
            .unwrap();
        let (code, v) = pull_as(&st, &crate::mail::pull_auth(&station, RP, now(), &[]), &[]).await;
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["headers"][0]["id"], id.as_str(), "{v}");
        let ids = vec![id.clone()];
        let (code, v) = pull_as(
            &st,
            &crate::mail::pull_auth(&station, RP, now(), &ids),
            &ids,
        )
        .await;
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["messages"][0]["body"], "hello", "{v}");
    }

    /// The #278 on-air case: M7TJF/M is bound to the Mac's key, and a
    /// scratch home signed with a new key. The bound key works for the ask
    /// and the pull; the other key is refused on both and binds nothing.
    #[tokio::test]
    async fn only_the_bound_key_may_ask_or_pull_for_a_portable_callsign() {
        let st = crate::net::hub_server::test_hub_state();
        let bound = crate::proto::IdentityKeys::generate();
        st.telemetry
            .bind_callsign(RP, &bound.public_bytes())
            .unwrap();
        let id = st
            .mail
            .insert_in(RP, "op@example.com", &wcr_address(RP), "hi", "hello", None)
            .unwrap();

        let (code, v) = ask_as(&st, &bound, RP, RP, "44ede7de00000002").await;
        assert_eq!(code, StatusCode::OK, "{v}");
        let (code, v) = pull_as(&st, &crate::mail::pull_auth(&bound, RP, now(), &[]), &[]).await;
        assert_eq!(code, StatusCode::OK, "{v}");
        assert_eq!(v["headers"][0]["id"], id.as_str(), "{v}");

        let scratch = crate::proto::IdentityKeys::generate();
        let fp = hex::encode_upper(&scratch.public_bytes()[..4]);
        let (code, v) = ask_as(&st, &scratch, RP, RP, "44ede7de00000005").await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{v}");
        let msg = v["error"].as_str().unwrap();
        assert!(
            msg.starts_with("gateway ask: M7TJF/M is registered to a different key on the hub"),
            "{msg}"
        );
        assert!(msg.contains(&fp), "{msg}");
        assert!(
            msg.contains("Use the original identity.key or re-register."),
            "{msg}"
        );

        let (code, v) = pull_as(&st, &crate::mail::pull_auth(&scratch, RP, now(), &[]), &[]).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{v}");
        let msg = v["error"].as_str().unwrap();
        assert!(
            msg.starts_with("mail pull: M7TJF/M is registered to a different key"),
            "{msg}"
        );
        let ids = vec![id.clone()];
        let (code, _) = pull_as(
            &st,
            &crate::mail::pull_auth(&scratch, RP, now(), &ids),
            &ids,
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);

        assert_eq!(st.telemetry.get_pubkey(RP), Some(bound.public_bytes()));
    }

    #[tokio::test]
    async fn a_wrong_signature_is_refused_and_keeps_the_key() {
        let st = crate::net::hub_server::test_hub_state();
        let station = crate::proto::IdentityKeys::generate();
        st.telemetry
            .bind_callsign(RP, &station.public_bytes())
            .unwrap();

        // Signed for another callsign, reported as RP.
        let (code, v) = ask_as(&st, &station, "M7TJF/P", RP, "44ede7de00000003").await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{v}");
        assert_eq!(v["error"], "gateway ask: bad signature");

        // Someone else's signature under the station's key.
        let ts = now();
        let mut ask = crate::mail::pick::signed_ask(&station, RP, ts, "44ede7de00000004", "");
        let forger = crate::proto::IdentityKeys::generate();
        ask.sig = crate::mail::pick::signed_ask(&forger, RP, ts, "44ede7de00000004", "").sig;
        let report =
            crate::mail::pick::ask_report(RP, "44ede7de00000004", &ask, None, "", "vhf-fm");
        let (h, b) = signed(&forger, GW, &report);
        let (code, _) = json_of(
            post_gateway_ask(State(st.clone()), h, b)
                .await
                .into_response(),
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);

        // A pull signed for the plain callsign, sent for RP.
        let (code, v) = pull_as(&st, &crate::mail::pull_auth(&station, GW, now(), &[]), &[]).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{v}");
        assert_eq!(v["error"], "mail pull: bad signature");

        // A pull for one message signed for another.
        let ids = vec!["abc".to_string()];
        let other = vec!["def".to_string()];
        let (code, _) = pull_as(
            &st,
            &crate::mail::pull_auth(&station, RP, now(), &other),
            &ids,
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);

        // A stale timestamp.
        let (code, v) = pull_as(
            &st,
            &crate::mail::pull_auth(&station, RP, now() - 3600, &[]),
            &[],
        )
        .await;
        assert_eq!(code, StatusCode::UNAUTHORIZED, "{v}");
        assert_eq!(v["error"], "mail pull: clock more than 10 minutes out");

        assert_eq!(st.telemetry.get_pubkey(RP), Some(station.public_bytes()));
    }

    #[test]
    fn a_relay_never_attaches_its_own_copy_address() {
        let confirmed = |call: &str| match call {
            "M7TJF" => Some("tj@example.com".to_string()),
            "G0ABC" => Some("club@example.com".to_string()),
            _ => None,
        };
        let tj = "M7TJF@mail.weechatradio.com";

        // A gateway posting M7TJF's mail uses M7TJF's address.
        assert_eq!(
            author_bcc("G0ABC", tj, None, confirmed),
            Some("tj@example.com".into())
        );
        // Asking for its own copy on someone else's mail is ignored.
        assert_eq!(
            author_bcc("G0ABC", tj, Some("club@example.com".into()), confirmed),
            Some("tj@example.com".into())
        );
        // Our own mail keeps the address this station sent.
        assert_eq!(
            author_bcc("M7TJF", tj, Some("phone@example.com".into()), confirmed),
            Some("phone@example.com".into())
        );
        // Nobody confirmed a copy address, so nothing is attached.
        assert_eq!(
            author_bcc("G0ABC", "2E0XYZ@mail.weechatradio.com", None, confirmed),
            None
        );
    }

    #[test]
    fn the_hub_takes_a_copy_code_twice_and_forgets_it_when_off() {
        let db = MailHubDb::open_memory().unwrap();
        db.set_pending("M7TJF", "tj@example.com", "123456").unwrap();
        assert!(!db.confirm("M7TJF", "654321").unwrap());
        assert_eq!(db.copy_to("M7TJF"), None);
        assert!(db.confirm("M7TJF", "123456").unwrap());
        // A station that missed the reply asks again with the same code.
        assert!(db.confirm("M7TJF", "123456").unwrap());
        assert_eq!(db.copy_to("M7TJF").as_deref(), Some("tj@example.com"));
        db.clear_copy("M7TJF").unwrap();
        assert_eq!(db.copy_to("M7TJF"), None);
        assert!(!db.confirm("M7TJF", "123456").unwrap());
    }

    #[test]
    fn the_code_email_has_nothing_stuck_to_the_code() {
        let text = copy_code_text("M7TJF", "123456");
        assert!(text.contains("\n123456\n"), "{text}");
        assert!(!text.contains("123456."), "{text}");
        assert!(text.contains("code box next to Copy-to on the Email tab"));
        assert_eq!(copy_code_digits(" 123 456. "), "123456");
    }

    #[test]
    fn only_the_poster_or_the_author_reads_a_mails_delivery() {
        let db = MailHubDb::open_memory().unwrap();
        db.insert_out(
            "GW1",
            "m7tjf@mail.weechatradio.com",
            "a@b.c",
            "s",
            "b",
            "re_1",
            "mid1",
        )
        .unwrap();
        let row = db.out_status("mid1").unwrap();
        assert_eq!(row.resend_id, "re_1");
        assert!(may_read_status("GW1", &row));
        assert!(may_read_status("m7tjf", &row));
        assert!(!may_read_status("QA1A", &row));
        assert!(db.out_status("").is_none());
        assert!(db.out_status("other").is_none());
        db.set_last_event("mid1", "delivered", 5).unwrap();
        let row = db.out_status("mid1").unwrap();
        assert_eq!((row.last_event.as_str(), row.checked), ("delivered", 5));
        assert!(delivery_final(&row.last_event));
        assert!(!delivery_final("delivery_delayed"));
    }

    #[test]
    fn fresh_svix_signature_matches() {
        let secret = secret();
        let id = "msg_test";
        let ts = chrono::Utc::now().timestamp().to_string();
        let body = br#"{"type":"email.received"}"#;
        let sig = sign_v1(&secret, id, &ts, body).expect("sign");
        assert!(webhook_signature_ok(
            &secret,
            id,
            &ts,
            body,
            &format!("v1,{sig}")
        ));
        assert!(!webhook_signature_ok(
            &secret,
            id,
            &ts,
            body,
            "v1,bm90LXRoZS1zaWc="
        ));
    }

    #[test]
    fn stale_svix_signature_is_rejected() {
        let secret = secret();
        let id = "msg_old";
        let ts = (chrono::Utc::now().timestamp() - 3600).to_string();
        let body = b"{}";
        let sig = sign_v1(&secret, id, &ts, body).expect("sign");
        assert!(!webhook_signature_ok(
            &secret,
            id,
            &ts,
            body,
            &format!("v1,{sig}")
        ));
    }
}
