//! SPDX-License-Identifier: Apache-2.0
//! Hub mail wait-queue, Resend outbound, and signed node API.

use crate::error::{Error, Result};
use crate::mail::{
    strip_html, validate_internet_addr, wcr_address, MAIL_MAX_BYTES, WCR_COPY_HEADER,
};
use crate::net::hub_server::HubState;
use crate::proto::IdentityKeys;
use crate::telemetry::TelemetryEvent;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use parking_lot::Mutex;
use rusqlite::OptionalExtension;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

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
CREATE INDEX IF NOT EXISTS idx_hub_mail_call ON hub_mail(callsign, fetched, received DESC);

CREATE TABLE IF NOT EXISTS hub_mail_copy (
    callsign TEXT PRIMARY KEY,
    copy_to TEXT,
    confirmed INTEGER NOT NULL DEFAULT 0,
    pending_to TEXT,
    pending_code TEXT
);
"#;

pub struct MailHubDb {
    conn: Mutex<Connection>,
}

impl MailHubDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
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

    pub fn insert_inbound(
        &self,
        callsign: &str,
        from: &str,
        to: &str,
        subject: &str,
        body: &str,
        resend_id: Option<&str>,
    ) -> Result<String> {
        let id = blake3::hash(format!("{callsign}{from}{to}{subject}{body}").as_bytes())
            .to_hex()
            .to_string();
        let body = if body.len() > MAIL_MAX_BYTES {
            body[..MAIL_MAX_BYTES].to_string()
        } else {
            body.to_string()
        };
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR IGNORE INTO hub_mail(id, callsign, direction, from_addr, to_addr, subject, body, received, fetched, resend_id)
             VALUES(?1,?2,'in',?3,?4,?5,?6,?7,0,?8)",
            params![id, callsign.to_ascii_uppercase(), from, to, subject, body, now, resend_id],
        )?;
        Ok(id)
    }

    pub fn insert_outbound_record(
        &self,
        callsign: &str,
        from: &str,
        to: &str,
        subject: &str,
        body: &str,
        resend_id: Option<&str>,
    ) -> Result<String> {
        let id = blake3::hash(format!("out{callsign}{to}{subject}{body}").as_bytes())
            .to_hex()
            .to_string();
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO hub_mail(id, callsign, direction, from_addr, to_addr, subject, body, received, fetched, resend_id)
             VALUES(?1,?2,'out',?3,?4,?5,?6,?7,1,?8)",
            params![id, callsign.to_ascii_uppercase(), from, to, subject, body, now, resend_id],
        )?;
        Ok(id)
    }

    pub fn waiting_headers(&self, callsign: &str, limit: i64) -> Vec<HubMailHeader> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, from_addr, subject, LENGTH(body) FROM hub_mail
                 WHERE callsign = ?1 AND direction = 'in' AND fetched = 0
                 ORDER BY received ASC LIMIT ?2",
            )
            .unwrap();
        let rows = stmt
            .query_map(params![callsign.to_ascii_uppercase(), limit], |r| {
                Ok(HubMailHeader {
                    id: r.get(0)?,
                    from: r.get(1)?,
                    subject: r.get(2)?,
                    bytes: r.get::<_, i64>(3)? as usize,
                })
            })
            .unwrap();
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn waiting_count(&self, callsign: &str) -> u64 {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT COUNT(*) FROM hub_mail WHERE callsign = ?1 AND direction = 'in' AND fetched = 0",
            params![callsign.to_ascii_uppercase()],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as u64
    }

    pub fn fetch_bodies(&self, callsign: &str, ids: &[String]) -> Vec<HubMailBody> {
        let conn = self.conn.lock();
        let mut out = Vec::new();
        for id in ids {
            let row = conn.query_row(
                "SELECT id, from_addr, to_addr, subject, body FROM hub_mail
                 WHERE id = ?1 AND callsign = ?2 AND direction = 'in'",
                params![id, callsign.to_ascii_uppercase()],
                |r| {
                    Ok(HubMailBody {
                        id: r.get(0)?,
                        from: r.get(1)?,
                        to: r.get(2)?,
                        subject: r.get(3)?,
                        body: r.get(4)?,
                    })
                },
            );
            if let Ok(b) = row {
                let _ = conn.execute("UPDATE hub_mail SET fetched = 1 WHERE id = ?1", params![id]);
                out.push(b);
            }
        }
        out
    }

    pub fn copy_settings(&self, callsign: &str) -> HubCopySettings {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT copy_to, confirmed, pending_to, pending_code FROM hub_mail_copy WHERE callsign = ?1",
            params![callsign.to_ascii_uppercase()],
            |r| {
                Ok(HubCopySettings {
                    copy_to: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                    confirmed: r.get::<_, i64>(1)? != 0,
                    pending_to: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    pending_code: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                })
            },
        )
        .unwrap_or_default()
    }

    pub fn set_copy_pending(&self, callsign: &str, to: &str, code: &str) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO hub_mail_copy(callsign, copy_to, confirmed, pending_to, pending_code)
             VALUES(?1, COALESCE((SELECT copy_to FROM hub_mail_copy WHERE callsign=?1),''), 0, ?2, ?3)
             ON CONFLICT(callsign) DO UPDATE SET pending_to=excluded.pending_to, pending_code=excluded.pending_code",
            params![callsign.to_ascii_uppercase(), to, code],
        )?;
        Ok(())
    }

    pub fn confirm_copy(&self, callsign: &str, code: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let row: Option<(String, String)> = conn
            .query_row(
                "SELECT pending_to, pending_code FROM hub_mail_copy WHERE callsign = ?1",
                params![callsign.to_ascii_uppercase()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((to, expect)) = row {
            if expect == code && !to.is_empty() {
                conn.execute(
                    "UPDATE hub_mail_copy SET copy_to = ?2, confirmed = 1, pending_to = '', pending_code = '' WHERE callsign = ?1",
                    params![callsign.to_ascii_uppercase(), to],
                )?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn confirmed_copy_to(&self, callsign: &str) -> Option<String> {
        let s = self.copy_settings(callsign);
        if s.confirmed && !s.copy_to.is_empty() {
            Some(s.copy_to)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HubCopySettings {
    pub copy_to: String,
    pub confirmed: bool,
    pub pending_to: String,
    pub pending_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HubMailHeader {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HubMailBody {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct MailSendRequest {
    pub to: String,
    pub subject: String,
    pub body: String,
    pub mail_id: String,
    /// RF gateway callsign when hub send follows a radio path (map arc only).
    #[serde(default)]
    pub via: String,
}

#[derive(Debug, Deserialize)]
pub struct MailFetchRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct CopyToRequest {
    pub address: String,
}

#[derive(Debug, Deserialize)]
pub struct CopyConfirmRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct ResendWebhook {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: serde_json::Value,
}

fn verify_signed(
    st: &HubState,
    headers: &HeaderMap,
    body: &[u8],
) -> std::result::Result<String, (StatusCode, String)> {
    let call = header(headers, "x-radio-callsign")
        .ok_or((StatusCode::BAD_REQUEST, "missing callsign".into()))?
        .to_ascii_uppercase();
    if !crate::rate_limit::allow(&st.report_by_call, &call) {
        return Err((StatusCode::TOO_MANY_REQUESTS, "rate limit".into()));
    }
    let pkhex = header(headers, "x-radio-pubkey")
        .ok_or((StatusCode::BAD_REQUEST, "missing pubkey".into()))?;
    let sighex = header(headers, "x-radio-signature")
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
    vk.verify(body, &Signature::from_bytes(&sig_arr))
        .map_err(|_| (StatusCode::UNAUTHORIZED, "bad signature".into()))?;
    if let Some(existing) = st.telemetry.get_pubkey(&call) {
        if existing != pk_arr {
            return Err((StatusCode::CONFLICT, "callsign already claimed".into()));
        }
    } else if call.starts_with('~') {
        return Err((StatusCode::FORBIDDEN, "guests cannot use hub mail".into()));
    } else if crate::proto::is_plausible_callsign(&call) {
        let _ = st.telemetry.bind_callsign(&call, &pk_arr);
    } else {
        return Err((StatusCode::BAD_REQUEST, "callsign not plausible".into()));
    }
    Ok(call)
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

fn plausible_gateway(origin: &str, via: &str) -> Option<String> {
    let v = via.trim().to_ascii_uppercase();
    if v.is_empty() || v == origin.to_ascii_uppercase() {
        return None;
    }
    if crate::proto::is_plausible_callsign(&v) {
        Some(v)
    } else {
        None
    }
}

/// Map-visible mail hop (callsigns only; no internet addresses).
pub fn publish_mail_map(st: &HubState, origin: &str, via: Option<&str>, msgid: Option<&str>) {
    let origin = origin.trim().to_ascii_uppercase();
    if origin.is_empty() {
        return;
    }
    let dest = via.and_then(|v| plausible_gateway(&origin, v));
    let ts = chrono::Utc::now().timestamp() as u64;
    let ev = TelemetryEvent {
        ts,
        kind: "mail".into(),
        origin: Some(origin.clone()),
        dest: dest.clone(),
        hops: None,
        snr: None,
        msgid: msgid.map(String::from),
        band: None,
    };
    let _ = st.telemetry.add_event(&ev);
    let _ = st.live.send(serde_json::json!({
        "ts": ts,
        "kind": "mail",
        "origin": origin,
        "dest": dest,
        "msgid": msgid,
    }));
}

pub async fn post_send(
    State(st): State<HubState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let call = verify_signed(&st, &headers, &body)?;
    let req: MailSendRequest =
        serde_json::from_slice(&body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    validate_internet_addr(&req.to).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if req.body.len() > MAIL_MAX_BYTES {
        return Err((StatusCode::BAD_REQUEST, "body too large".into()));
    }
    let from = wcr_address(&call);
    let resend_id = st
        .mail_resend
        .send_email(
            &from,
            &req.to,
            &req.subject,
            &req.body,
            st.mail.as_ref(),
            &call,
        )
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    let _ = st.mail.insert_outbound_record(
        &call,
        &from,
        &req.to,
        &req.subject,
        &req.body,
        resend_id.as_deref(),
    );
    let via = if req.via.is_empty() {
        None
    } else {
        Some(req.via.as_str())
    };
    publish_mail_map(
        &st,
        &call,
        via,
        Some(req.mail_id.as_str()).or(resend_id.as_deref()),
    );
    Ok(Json(
        serde_json::json!({ "ok": true, "resend_id": resend_id }),
    ))
}

pub async fn post_inbox(
    State(st): State<HubState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let call = verify_signed(&st, &headers, &body)?;
    let headers = st.mail.waiting_headers(&call, 50);
    let waiting = st.mail.waiting_count(&call);
    Ok(Json(
        serde_json::json!({ "waiting": waiting, "headers": headers }),
    ))
}

pub async fn post_fetch(
    State(st): State<HubState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let call = verify_signed(&st, &headers, &body)?;
    let req: MailFetchRequest =
        serde_json::from_slice(&body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let msgs = st.mail.fetch_bodies(&call, &req.ids);
    Ok(Json(serde_json::json!({ "messages": msgs })))
}

pub async fn post_copy(
    State(st): State<HubState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let call = verify_signed(&st, &headers, &body)?;
    let req: CopyToRequest =
        serde_json::from_slice(&body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    validate_internet_addr(&req.address).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let code: String =
        rand::Rng::sample_iter(rand::thread_rng(), &rand::distributions::Alphanumeric)
            .take(8)
            .map(char::from)
            .collect();
    st.mail
        .set_copy_pending(&call, &req.address, &code)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = st.mail_resend.send_copy_confirm(&req.address, &code, &call);
    Ok(Json(serde_json::json!({ "ok": true, "pending": true })))
}

pub async fn post_copy_confirm(
    State(st): State<HubState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let call = verify_signed(&st, &headers, &body)?;
    let req: CopyConfirmRequest =
        serde_json::from_slice(&body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let ok = st
        .mail
        .confirm_copy(&call, &req.code)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "ok": ok })))
}

pub async fn resend_webhook(
    State(st): State<HubState>,
    body: bytes::Bytes,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, String)> {
    let payload: ResendWebhook =
        serde_json::from_slice(&body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if payload.kind != "email.received" {
        return Ok(Json(serde_json::json!({ "ok": true, "ignored": true })));
    }
    let email_id = payload
        .data
        .get("email_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if email_id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "missing email_id".into()));
    }
    let detail = st
        .mail_resend
        .fetch_received(email_id)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    if detail.headers.get(WCR_COPY_HEADER).is_some() {
        return Ok(Json(serde_json::json!({ "ok": true, "loop": true })));
    }
    let to_local = detail
        .to
        .split('@')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    if !crate::proto::is_plausible_callsign(&to_local) {
        return Ok(Json(serde_json::json!({ "ok": true, "ignored": true })));
    }
    let plain = strip_html(&detail.text);
    let _ = st.mail.insert_inbound(
        &to_local,
        &detail.from,
        &detail.to,
        &detail.subject,
        &plain,
        Some(email_id),
    );
    if let Some(copy) = st.mail.confirmed_copy_to(&to_local) {
        let _ = st.mail_resend.send_copy_inbound(&copy, &detail);
    }
    publish_mail_map(&st, &to_local, None, Some(email_id));
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Resend HTTP client (API key from env on the hub only).
pub struct ResendHub {
    key: String,
    client: reqwest::Client,
}

impl ResendHub {
    pub fn from_env() -> Self {
        Self {
            key: std::env::var("RESEND_API_KEY").unwrap_or_default(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn send_email(
        &self,
        from: &str,
        to: &str,
        subject: &str,
        body: &str,
        mail_db: &MailHubDb,
        callsign: &str,
    ) -> Result<Option<String>> {
        if self.key.is_empty() {
            tracing::warn!("RESEND_API_KEY unset; mail queued locally only");
            return Ok(None);
        }
        let mut req = serde_json::json!({
            "from": from,
            "to": [to],
            "subject": subject,
            "text": body,
            "headers": { "X-Entity-Ref-ID": format!("wcr-mail/{}", blake3::hash(body.as_bytes()).to_hex()) }
        });
        if let Some(bcc) = mail_db.confirmed_copy_to(callsign) {
            req["bcc"] = serde_json::json!([bcc]);
            if let Some(hdrs) = req.get_mut("headers").and_then(|h| h.as_object_mut()) {
                hdrs.insert(WCR_COPY_HEADER.into(), serde_json::json!("1"));
            }
        }
        let resp = self
            .client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.key))
            .json(&req)
            .send()
            .await
            .map_err(|e| Error::Net(e.to_string()))?;
        if !resp.status().is_success() {
            let t = resp.text().await.unwrap_or_default();
            return Err(Error::Net(format!("resend send: {}", t)));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| Error::Net(e.to_string()))?;
        Ok(v.get("id").and_then(|x| x.as_str()).map(String::from))
    }

    pub fn send_copy_confirm(&self, to: &str, code: &str, callsign: &str) -> Result<()> {
        if self.key.is_empty() {
            return Ok(());
        }
        let from = wcr_address(callsign);
        let subject = "Confirm WeeChat Radio mail copy";
        let body = format!(
            "Enter this code in the Email settings to copy mail to {}:\n\n{}\n",
            to, code
        );
        let rt = tokio::runtime::Handle::current();
        let to = to.to_string();
        rt.spawn({
            let client = self.client.clone();
            let key = self.key.clone();
            let from = from.clone();
            let to = to.clone();
            async move {
                let _ = client
                    .post("https://api.resend.com/emails")
                    .header("Authorization", format!("Bearer {}", key))
                    .json(&serde_json::json!({
                        "from": from,
                        "to": [to],
                        "subject": subject,
                        "text": body,
                    }))
                    .send()
                    .await;
            }
        });
        Ok(())
    }

    pub fn send_copy_inbound(&self, copy_to: &str, detail: &ReceivedDetail) -> Result<()> {
        if self.key.is_empty() {
            return Ok(());
        }
        let body = format!(
            "Copy of inbound mail to {}\nFrom: {}\nSubject: {}\n\n{}",
            detail.to, detail.from, detail.subject, detail.text
        );
        let rt = tokio::runtime::Handle::current();
        let from = format!("noreply@{}", crate::mail::MAIL_DOMAIN);
        let copy_to = copy_to.to_string();
        let subject = format!("WCR copy: {}", detail.subject);
        let detail_to = detail.to.clone();
        let detail_from = detail.from.clone();
        rt.spawn({
            let client = self.client.clone();
            let key = self.key.clone();
            let copy_to = copy_to.clone();
            let subject = subject.clone();
            let body = body.clone();
            async move {
                let _ = client
                    .post("https://api.resend.com/emails")
                    .header("Authorization", format!("Bearer {}", key))
                    .json(&serde_json::json!({
                        "from": from,
                        "to": [copy_to],
                        "subject": subject,
                        "text": body,
                        "headers": { WCR_COPY_HEADER: "1" }
                    }))
                    .send()
                    .await;
            }
        });
        Ok(())
    }

    pub async fn fetch_received(&self, email_id: &str) -> Result<ReceivedDetail> {
        if self.key.is_empty() {
            return Err(Error::Net("RESEND_API_KEY unset".into()));
        }
        let url = format!("https://api.resend.com/emails/receiving/{}", email_id);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.key))
            .send()
            .await
            .map_err(|e| Error::Net(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(Error::Net(format!("resend receive {}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| Error::Net(e.to_string()))?;
        let from = v
            .pointer("/from")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let to = v
            .pointer("/to/0")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let subject = v
            .get("subject")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let text = v
            .get("text")
            .and_then(|x| x.as_str())
            .or_else(|| v.get("html").and_then(|x| x.as_str()))
            .unwrap_or("")
            .to_string();
        let mut headers = std::collections::HashMap::new();
        if let Some(arr) = v.get("headers").and_then(|h| h.as_array()) {
            for h in arr {
                if let (Some(k), Some(v)) = (
                    h.get("name").and_then(|x| x.as_str()),
                    h.get("value").and_then(|x| x.as_str()),
                ) {
                    headers.insert(k.to_string(), v.to_string());
                }
            }
        }
        Ok(ReceivedDetail {
            from,
            to,
            subject,
            text,
            headers,
        })
    }
}

pub struct ReceivedDetail {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub text: String,
    pub headers: std::collections::HashMap<String, String>,
}

pub fn hub_api_base_from_telemetry(url: &str) -> String {
    let u = url.trim();
    if let Some((scheme, rest)) = u.split_once("://") {
        if let Some(host) = rest.split('/').next() {
            return format!("{}://{}", scheme, host);
        }
    }
    "https://hub.weechatradio.com".into()
}

pub async fn signed_post(
    url: &str,
    keys: &IdentityKeys,
    callsign: &str,
    body: &[u8],
) -> Result<serde_json::Value> {
    let sig = keys.sign_bytes(body);
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("x-radio-callsign", callsign)
        .header("x-radio-pubkey", hex::encode(keys.public_bytes()))
        .header("x-radio-signature", hex::encode(sig))
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(Error::Net(format!("hub mail {}: {}", status, text)));
    }
    serde_json::from_str(&text).map_err(|e| Error::Net(e.to_string()))
}
