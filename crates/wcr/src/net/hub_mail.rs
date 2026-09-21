//! SPDX-License-Identifier: Apache-2.0
//! Hub mail queue and Resend. The API key stays in the environment.

use crate::error::{Error, Result};
use crate::mail::{
    callsign_from_wcr, pull_token, trim_body, validate_internet_addr, wcr_address, MAIL_DOMAIN,
    WCR_COPY_HEADER,
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
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
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
    ) -> Result<String> {
        let id = blake3::hash(format!("out{callsign}{to}{subject}{body}{resend_id}").as_bytes())
            .to_hex()
            .to_string();
        let now = chrono::Utc::now().timestamp();
        self.conn.lock().execute(
            "INSERT INTO hub_mail(id, callsign, direction, from_addr, to_addr, subject, body, received, fetched, resend_id)
             VALUES(?1,?2,'out',?3,?4,?5,?6,?7,1,?8)",
            params![id, callsign.to_ascii_uppercase(), from, to, subject, body, now, resend_id],
        )?;
        Ok(id)
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
                "SELECT id, from_addr, to_addr, subject, body FROM hub_mail
                 WHERE id = ?1 AND callsign = ?2 AND direction = 'in'",
                params![id, callsign.to_ascii_uppercase()],
                |r| {
                    Ok(json!({
                        "id": r.get::<_, String>(0)?,
                        "from": r.get::<_, String>(1)?,
                        "to": r.get::<_, String>(2)?,
                        "subject": r.get::<_, String>(3)?,
                        "body": r.get::<_, String>(4)?,
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

    pub fn confirm(&self, callsign: &str, code: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT pending_to, pending_code FROM hub_mail_copy WHERE callsign = ?1",
                params![callsign.to_ascii_uppercase()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        if let Some((to, expect)) = row {
            if !to.is_empty() && expect == code {
                conn.execute(
                    "UPDATE hub_mail_copy SET copy_to = ?2, confirmed = 1, pending_to = '', pending_code = '' WHERE callsign = ?1",
                    params![callsign.to_ascii_uppercase(), to],
                )?;
                return Ok(true);
            }
        }
        Ok(false)
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
/// when that station signed the request, and only with the key already on file.
fn authorize_pull(
    st: &HubState,
    signer: &str,
    auth: &PullAuth,
    ids: &[String],
) -> std::result::Result<String, (StatusCode, String)> {
    let wanted = auth.for_call.trim();
    if wanted.is_empty() || wanted.eq_ignore_ascii_case(signer) {
        return Ok(signer.to_ascii_uppercase());
    }
    let bound = st.telemetry.get_pubkey(wanted);
    let now = chrono::Utc::now().timestamp();
    if !owner_pull_ok(
        &auth.pk,
        &auth.sig,
        wanted,
        auth.ts,
        ids,
        now,
        bound.as_ref(),
    ) {
        return Err((StatusCode::UNAUTHORIZED, "mail pull".into()));
    }
    if bound.is_none() {
        if let Ok(bytes) = hex::decode(auth.pk.trim()) {
            if bytes.len() == 32 {
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&bytes);
                let _ = st.telemetry.bind_callsign(wanted, &pk);
            }
        }
    }
    Ok(wanted.to_ascii_uppercase())
}

/// `bound` is the pubkey this callsign already registered. A different key is refused.
pub fn owner_pull_ok(
    pk_hex: &str,
    sig_hex: &str,
    callsign: &str,
    ts: i64,
    ids: &[String],
    now: i64,
    bound: Option<&[u8; 32]>,
) -> bool {
    if (now - ts).unsigned_abs() > 10 * 60 {
        return false;
    }
    let Ok(pk) = hex::decode(pk_hex.trim()) else {
        return false;
    };
    let Ok(sig) = hex::decode(sig_hex.trim()) else {
        return false;
    };
    if pk.len() != 32 || sig.len() != 64 {
        return false;
    }
    if let Some(bound) = bound {
        if pk.as_slice() != bound.as_slice() {
            return false;
        }
    }
    let mut pk_arr = [0u8; 32];
    let mut sig_arr = [0u8; 64];
    pk_arr.copy_from_slice(&pk);
    sig_arr.copy_from_slice(&sig);
    let Ok(key) = VerifyingKey::from_bytes(&pk_arr) else {
        return false;
    };
    key.verify(
        &pull_token(callsign, ts, ids),
        &Signature::from_bytes(&sig_arr),
    )
    .is_ok()
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
            let _ = st
                .mail
                .insert_out(&call, &from, &req.to, &req.subject, &req.body, &resend_id);
            Json(json!({ "ok": true, "id": resend_id })).into_response()
        }
        Err(e) => bad(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    }
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
    if validate_internet_addr(&req.address).is_err() {
        return bad(StatusCode::BAD_REQUEST, "bad address").into_response();
    }
    if st.mail.set_pending(&call, &req.address, &req.code).is_err() {
        return bad(StatusCode::INTERNAL_SERVER_ERROR, "store").into_response();
    }
    let text = format!(
        "Confirm copy-to for {call} with this code: {}.\nIf you did not ask for this, ignore it.",
        req.code
    );
    match resend_send(
        &wcr_address(&call),
        &req.address,
        "Confirm your WeeChat Radio copy-to address",
        &text,
        None,
        false,
    )
    .await
    {
        Ok(_) => Json(json!({ "ok": true })).into_response(),
        Err(e) => bad(StatusCode::BAD_GATEWAY, &e.to_string()).into_response(),
    }
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
    let ok = st.mail.confirm(&call, req.code.trim()).unwrap_or(false);
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
    fn a_gateway_pull_needs_the_owners_signature() {
        use crate::proto::IdentityKeys;
        let keys = IdentityKeys::generate();
        let ts = 1_700_000_000i64;
        let token = pull_token("M7TJF", ts, &[]);
        let sig = hex::encode(keys.sign_bytes(&token));
        let pk = keys.public_hex();
        let bound = keys.verifying_key().to_bytes();
        assert!(owner_pull_ok(&pk, &sig, "M7TJF", ts, &[], ts, Some(&bound)));
        assert!(!owner_pull_ok(
            &pk,
            &sig,
            "G0ABC",
            ts,
            &[],
            ts,
            Some(&bound)
        ));
        assert!(!owner_pull_ok(
            &pk,
            &sig,
            "M7TJF",
            ts,
            &[],
            ts + 3600,
            Some(&bound)
        ));
        let other = [9u8; 32];
        assert!(!owner_pull_ok(
            &pk,
            &sig,
            "M7TJF",
            ts,
            &[],
            ts,
            Some(&other)
        ));
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
