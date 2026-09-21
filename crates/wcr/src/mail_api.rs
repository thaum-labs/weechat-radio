//! SPDX-License-Identifier: Apache-2.0
//! Local HTTP mail API for wcr-gui (mounted on the status listener).

use crate::config::Config;
use crate::mail::{
    chunk_payloads, chunks_fit_max_body, validate_internet_addr, wcr_address, MailMeta,
    CHECK_MAIL_MAX_MSGS, MAIL_MAX_BYTES,
};
use crate::modes::Mode;
use crate::net::hub_mail::{hub_api_base_from_telemetry, signed_post};
use crate::presets::{format_airtime_hint, mail_airtime_secs, Preset};
use crate::proto::callsign::Callsign;
use crate::proto::IdentityKeys;
use crate::status::SharedStatus;
use crate::store::MailRow;
use crate::store::{Delivery, Store};
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

type HttpResult<T> = std::result::Result<T, (StatusCode, String)>;

#[derive(Clone)]
pub struct MailApiState {
    pub store: Arc<Store>,
    pub cfg: Arc<Mutex<Config>>,
    pub keys: IdentityKeys,
    pub snap: Arc<SharedStatus>,
    pub mail_cmd: mpsc::Sender<MailNodeCmd>,
    pub mail_last_gateway: Arc<Mutex<String>>,
}

fn resolve_mail_via(st: &MailApiState, cfg: &Config) -> Option<String> {
    if !cfg.mail.gateway.trim().is_empty() {
        return Some(cfg.mail.gateway.trim().to_ascii_uppercase());
    }
    let g = st.mail_last_gateway.lock();
    if g.is_empty() {
        None
    } else {
        Some(g.clone())
    }
}

#[derive(Debug)]
pub enum MailNodeCmd {
    SendRf { mail_id: String },
    CheckList,
    CheckGet { ids: Vec<String> },
}

#[derive(Debug, Deserialize)]
struct SendBody {
    to: String,
    subject: String,
    body: String,
    #[serde(default)]
    rf: bool,
}

#[derive(Debug, Deserialize)]
struct GetBody {
    ids: Vec<String>,
    #[serde(default)]
    rf: bool,
}

#[derive(Debug, Deserialize)]
struct CopyBody {
    address: String,
}

#[derive(Debug, Deserialize)]
struct CopyConfirmBody {
    code: String,
}

#[derive(Debug, Serialize)]
struct Summary {
    unread: u64,
    hub_waiting: u64,
    wcr_address: String,
    mail_enabled: bool,
}

pub fn router(st: MailApiState) -> Router {
    Router::new()
        .route("/mail/summary", get(summary))
        .route("/mail/list/{folder}", get(list_folder))
        .route("/mail/send", post(send_mail))
        .route("/mail/read", post(mark_read))
        .route("/mail/sync", post(sync_hub))
        .route("/mail/check/list", post(check_list))
        .route("/mail/check/get", post(check_get))
        .route("/mail/settings", get(get_settings))
        .route("/mail/settings/copy", post(set_copy))
        .route("/mail/settings/copy/confirm", post(confirm_copy))
        .route("/mail/estimate", post(estimate))
        .with_state(st)
}

async fn summary(State(st): State<MailApiState>) -> HttpResult<Json<Summary>> {
    let cfg = st.cfg.lock().clone();
    let call = Callsign::parse(&cfg.callsign).map_err(map_err)?;
    let unread = st.store.mail_unread_count().map_err(map_err)?;
    let hub_waiting = if cfg.mode.uses_internet() && st.snap.lock().hub_ok {
        hub_waiting_count(&st, &cfg).await
    } else {
        st.store.mail_unfetched_count().map_err(map_err)?
    };
    Ok(Json(Summary {
        unread,
        hub_waiting,
        wcr_address: wcr_address(call.as_str()),
        mail_enabled: !call.is_guest(),
    }))
}

async fn list_folder(
    State(st): State<MailApiState>,
    axum::extract::Path(folder): axum::extract::Path<String>,
) -> HttpResult<Json<Vec<MailRow>>> {
    let rows = st.store.mail_list(&folder, 100).map_err(map_err)?;
    Ok(Json(rows))
}

async fn send_mail(
    State(st): State<MailApiState>,
    Json(body): Json<SendBody>,
) -> HttpResult<Json<serde_json::Value>> {
    let cfg = st.cfg.lock().clone();
    let call = Callsign::parse(&cfg.callsign).map_err(map_err)?;
    if call.is_guest() {
        return Err((StatusCode::FORBIDDEN, "licensed callsign required".into()));
    }
    validate_internet_addr(&body.to).map_err(map_err)?;
    if body.body.len() > MAIL_MAX_BYTES {
        return Err((StatusCode::BAD_REQUEST, "body over 4 KB".into()));
    }
    let mode = cfg.mode;
    if mode == Mode::Radio {
        return Err((
            StatusCode::FORBIDDEN,
            "pure radio cannot send internet mail".into(),
        ));
    }
    let id = {
        let nonce = rand::random::<u64>();
        let nanos = chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_else(|| chrono::Utc::now().timestamp() * 1_000_000_000);
        blake3::hash(
            format!(
                "{}{}{}{}{}{}",
                call.as_str(),
                body.to,
                body.subject,
                body.body,
                nanos,
                nonce
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string()
    };

    let from = wcr_address(call.as_str());
    let meta = MailMeta {
        from: from.clone(),
        to: body.to.clone(),
        subject: body.subject.clone(),
        ids: vec![],
    };
    let probe = chunk_payloads(&id, &meta, &body.body);
    if !chunks_fit_max_body(&probe) {
        return Err((
            StatusCode::BAD_REQUEST,
            "mail too large for radio frames — shorten subject or body".into(),
        ));
    }

    let hub_ok = st.snap.lock().hub_ok;
    let use_rf = mail_send_will_rf(&cfg, hub_ok, body.rf);
    if use_rf && cfg.mail.gateway.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "set Email gateway in Setup (internet-radio callsign on your dial)".into(),
        ));
    }
    if !use_rf && !(mode.uses_internet() && hub_ok) {
        return Err((
            StatusCode::BAD_GATEWAY,
            "hub offline — set an Email gateway or wait for the hub".into(),
        ));
    }

    st.store
        .mail_insert(
            &id,
            "outbox",
            &from,
            &body.to,
            &body.subject,
            &body.body,
            Delivery::Queued,
            None,
        )
        .map_err(map_err)?;
    if use_rf {
        let _ = st
            .mail_cmd
            .send(MailNodeCmd::SendRf {
                mail_id: id.clone(),
            })
            .await;
    } else if mode.uses_internet() && hub_ok {
        hub_send(&st, &cfg, &id, &body.to, &body.subject, &body.body).await?;
        st.store
            .mail_set_delivery(&id, Delivery::Sent)
            .map_err(map_err)?;
        st.store.mail_move_folder(&id, "sent").map_err(map_err)?;
    }
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

#[derive(Debug, Deserialize)]
struct ReadBody {
    id: String,
}

async fn mark_read(
    State(st): State<MailApiState>,
    Json(body): Json<ReadBody>,
) -> HttpResult<Json<serde_json::Value>> {
    st.store.mail_set_read(&body.id, true).map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn sync_hub(State(st): State<MailApiState>) -> HttpResult<Json<serde_json::Value>> {
    let cfg = st.cfg.lock().clone();
    if !cfg.mode.uses_internet() {
        return Err((StatusCode::FORBIDDEN, "no internet on this mode".into()));
    }
    let call = cfg.callsign.clone();
    let base = hub_api_base_from_telemetry(&cfg.telemetry.url);
    let url = format!("{}/api/v1/mail/inbox", base);
    let body = b"{}";
    let v = signed_post(&url, &st.keys, &call, body)
        .await
        .map_err(map_err)?;
    let headers = v
        .get("headers")
        .and_then(|h| h.as_array())
        .cloned()
        .unwrap_or_default();
    if headers.is_empty() {
        return Ok(Json(serde_json::json!({ "ok": true, "new": 0 })));
    }
    let ids: Vec<String> = headers
        .iter()
        .filter_map(|h| h.get("id").and_then(|x| x.as_str()).map(String::from))
        .collect();
    let fetch_url = format!("{}/api/v1/mail/fetch", base);
    let req = serde_json::json!({ "ids": ids });
    let raw = serde_json::to_vec(&req).map_err(map_err)?;
    let fetched = signed_post(&fetch_url, &st.keys, &call, &raw)
        .await
        .map_err(map_err)?;
    let mut new = 0u64;
    if let Some(arr) = fetched.get("messages").and_then(|m| m.as_array()) {
        for m in arr {
            let id = m.get("id").and_then(|x| x.as_str()).unwrap_or("");
            let from = m.get("from").and_then(|x| x.as_str()).unwrap_or("");
            let to = m.get("to").and_then(|x| x.as_str()).unwrap_or("");
            let subject = m.get("subject").and_then(|x| x.as_str()).unwrap_or("");
            let body = m.get("body").and_then(|x| x.as_str()).unwrap_or("");
            if id.is_empty() {
                continue;
            }
            let rid = id.to_string();
            st.store
                .mail_insert(
                    &rid,
                    "inbox",
                    from,
                    to,
                    subject,
                    body,
                    Delivery::Delivered,
                    Some(&rid),
                )
                .map_err(map_err)?;
            new += 1;
        }
    }
    Ok(Json(serde_json::json!({ "ok": true, "new": new })))
}

async fn check_list(
    State(st): State<MailApiState>,
    Json(body): Json<serde_json::Value>,
) -> HttpResult<Json<serde_json::Value>> {
    let rf = body.get("rf").and_then(|v| v.as_bool()).unwrap_or(true);
    if rf {
        let _ = st.mail_cmd.send(MailNodeCmd::CheckList).await;
    }
    let cfg = st.cfg.lock().clone();
    let headers = if cfg.mode.uses_internet() && st.snap.lock().hub_ok {
        let call = cfg.callsign.clone();
        let base = hub_api_base_from_telemetry(&cfg.telemetry.url);
        let url = format!("{}/api/v1/mail/inbox", base);
        let v = signed_post(&url, &st.keys, &call, b"{}")
            .await
            .map_err(map_err)?;
        v.get("headers").cloned().unwrap_or(serde_json::json!([]))
    } else {
        serde_json::json!([])
    };
    Ok(Json(serde_json::json!({ "headers": headers })))
}

async fn check_get(
    State(st): State<MailApiState>,
    Json(body): Json<GetBody>,
) -> HttpResult<Json<serde_json::Value>> {
    if body.ids.len() > CHECK_MAIL_MAX_MSGS {
        return Err((StatusCode::BAD_REQUEST, "too many messages".into()));
    }
    if body.rf {
        let _ = st
            .mail_cmd
            .send(MailNodeCmd::CheckGet {
                ids: body.ids.clone(),
            })
            .await;
    }
    let cfg = st.cfg.lock().clone();
    if cfg.mode.uses_internet() && st.snap.lock().hub_ok && !body.rf {
        let call = cfg.callsign.clone();
        let base = hub_api_base_from_telemetry(&cfg.telemetry.url);
        let url = format!("{}/api/v1/mail/fetch", base);
        let req = serde_json::json!({ "ids": body.ids });
        let raw = serde_json::to_vec(&req).map_err(map_err)?;
        let fetched = signed_post(&url, &st.keys, &call, &raw)
            .await
            .map_err(map_err)?;
        return Ok(Json(fetched));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_settings(State(st): State<MailApiState>) -> HttpResult<Json<serde_json::Value>> {
    let s = st.store.mail_settings().map_err(map_err)?;
    Ok(Json(serde_json::to_value(s).unwrap_or_default()))
}

async fn set_copy(
    State(st): State<MailApiState>,
    Json(body): Json<CopyBody>,
) -> HttpResult<Json<serde_json::Value>> {
    validate_internet_addr(&body.address).map_err(map_err)?;
    let cfg = st.cfg.lock().clone();
    if !cfg.mode.uses_internet() {
        return Err((StatusCode::FORBIDDEN, "hub required for copy-to".into()));
    }
    let call = cfg.callsign.clone();
    let base = hub_api_base_from_telemetry(&cfg.telemetry.url);
    let url = format!("{}/api/v1/mail/copy", base);
    let req = serde_json::json!({ "address": body.address });
    let raw = serde_json::to_vec(&req).map_err(map_err)?;
    signed_post(&url, &st.keys, &call, &raw)
        .await
        .map_err(map_err)?;
    st.store
        .mail_set_kv("copy_pending_to", &body.address)
        .map_err(map_err)?;
    st.store
        .mail_set_kv("copy_confirmed", "0")
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "pending": true })))
}

async fn confirm_copy(
    State(st): State<MailApiState>,
    Json(body): Json<CopyConfirmBody>,
) -> HttpResult<Json<serde_json::Value>> {
    let cfg = st.cfg.lock().clone();
    let call = cfg.callsign.clone();
    let base = hub_api_base_from_telemetry(&cfg.telemetry.url);
    let url = format!("{}/api/v1/mail/copy/confirm", base);
    let req = serde_json::json!({ "code": body.code });
    let raw = serde_json::to_vec(&req).map_err(map_err)?;
    let v = signed_post(&url, &st.keys, &call, &raw)
        .await
        .map_err(map_err)?;
    if v.get("ok").and_then(|x| x.as_bool()) == Some(true) {
        let s = st.store.mail_settings().map_err(map_err)?;
        st.store
            .mail_set_kv("copy_to", &s.copy_pending_to)
            .map_err(map_err)?;
        st.store
            .mail_set_kv("copy_confirmed", "1")
            .map_err(map_err)?;
    }
    Ok(Json(v))
}

#[derive(Debug, Deserialize)]
struct EstimateBody {
    body_len: usize,
    #[serde(default)]
    rf: bool,
}

async fn estimate(
    State(st): State<MailApiState>,
    Json(body): Json<EstimateBody>,
) -> HttpResult<Json<serde_json::Value>> {
    let cfg = st.cfg.lock().clone();
    let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm);
    let len = body.body_len.min(MAIL_MAX_BYTES);
    if !body.rf && cfg.mode.uses_internet() {
        return Ok(Json(serde_json::json!({
            "bytes": len,
            "hint": format!("{} B · hub only", len),
            "bursts": 0,
            "secs": 0.0
        })));
    }
    let (bursts, secs) = mail_airtime_secs(
        preset,
        len,
        cfg.rf.frag_k,
        cfg.rf.frag_m,
        cfg.mode.uses_internet(),
        cfg.rf.turnaround_ms,
    );
    let hint = format!(
        "{} B · {} on {}",
        len,
        format_airtime_hint(secs),
        preset.as_str()
    );
    Ok(Json(serde_json::json!({
        "bytes": len,
        "hint": hint,
        "bursts": bursts,
        "secs": secs
    })))
}

async fn hub_send(
    st: &MailApiState,
    cfg: &Config,
    mail_id: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> HttpResult<()> {
    let base = hub_api_base_from_telemetry(&cfg.telemetry.url);
    let url = format!("{}/api/v1/mail/send", base);
    let via = resolve_mail_via(st, cfg).unwrap_or_default();
    let req = serde_json::json!({
        "to": to,
        "subject": subject,
        "body": body,
        "mail_id": mail_id,
        "via": via,
    });
    let raw = serde_json::to_vec(&req).map_err(map_err)?;
    signed_post(&url, &st.keys, &cfg.callsign, &raw)
        .await
        .map_err(map_err)?;
    Ok(())
}

async fn hub_waiting_count(st: &MailApiState, cfg: &Config) -> u64 {
    if !cfg.mode.uses_internet() || !st.snap.lock().hub_ok {
        return 0;
    }
    let base = hub_api_base_from_telemetry(&cfg.telemetry.url);
    let url = format!("{}/api/v1/mail/inbox", base);
    if let Ok(v) = signed_post(&url, &st.keys, &cfg.callsign, b"{}").await {
        return v.get("waiting").and_then(|w| w.as_u64()).unwrap_or(0);
    }
    0
}

fn map_err(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, e.to_string())
}

pub fn mail_send_will_rf(cfg: &Config, hub_ok: bool, explicit_rf: bool) -> bool {
    match cfg.mode {
        Mode::Radio => false,
        Mode::Internet => false,
        // Hub when online; RF only if asked or the hub is down.
        Mode::InternetRadio => explicit_rf || !hub_ok,
        // Field path: always RF → gateway → hub (same as chat on the hill / VOX).
        Mode::RadioPlus => true,
    }
}
