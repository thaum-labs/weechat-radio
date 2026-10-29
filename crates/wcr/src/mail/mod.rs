//! SPDX-License-Identifier: Apache-2.0
//! Email beside Live Chat. Radio access is a pause/restore lease only.

mod air;
mod api;
mod gateway;
mod hub;
mod lease;
mod progress;
mod proto;
mod status;
mod store;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::modes::Mode;
use crate::presets::Preset;
use crate::proto::IdentityKeys;
use crate::status::SharedStatus;
pub use air::{confirm_air_line, AirtimeMail};
pub use gateway::{local_send_path as gateway_path, state_mark, SendPath};
pub use lease::RadioPorts;
pub use progress::{delivery_word, get_step, send_step, SendStep, SendView};

pub fn airtime_parts(mail: &AirtimeMail<'_>, preset: Preset, turnaround_ms: u32) -> (f64, usize) {
    air::mail_airtime_secs(preset, mail, turnaround_ms)
}
use parking_lot::Mutex;
use serde_json::{json, Value};
pub use status::{mail_status, MailStatus, MailView};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
pub use store::{GetProgress, Hop, MailRow, WaitHeader};
use tokio::sync::{broadcast, mpsc};

pub use api::{router, MailSlot};
pub use proto::{
    callsign_from_wcr, copy_code_digits, looks_like_mail, pull_token, trim_body, try_decode,
    validate_copy_addr, validate_internet_addr, wcr_address, Incoming, MAIL_DOMAIN,
    WCR_COPY_HEADER,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeHold {
    pub to: String,
    pub subject: String,
    pub body: String,
    pub pending: bool,
}

impl ComposeHold {
    pub fn arm(&mut self) {
        self.pending = true;
    }

    /// Fields stay until the Sent toast. They clear together.
    pub fn on_sent(&mut self) {
        if self.pending {
            self.to.clear();
            self.subject.clear();
            self.body.clear();
            self.pending = false;
        }
    }
}

/// Why the Email tab stays grey. `None` means it can be used.
pub fn email_block_reason(
    callsign: &str,
    mode: Mode,
    gateway: &str,
    hub_ok: bool,
    station_running: bool,
) -> Option<&'static str> {
    let call = callsign.trim();
    if call.starts_with('~') || !crate::proto::is_plausible_callsign(call) {
        return Some("Email needs a licensed callsign");
    }
    if !station_running {
        return Some("Start the station to use Email");
    }
    match mode {
        Mode::Radio => Some("Use Internet, Internet+Radio, or Radio+ with a gateway"),
        Mode::RadioPlus if gateway.trim().is_empty() => {
            Some("Set a mail gateway callsign in setup")
        }
        Mode::Internet | Mode::InternetRadio if !hub_ok => Some("Email needs the hub"),
        _ => None,
    }
}

pub fn hint_line(mail: &AirtimeMail<'_>, mode: Mode, preset: Preset, turnaround_ms: u32) -> String {
    let hub_only = matches!(gateway::local_send_path(mode), SendPath::Hub);
    air::airtime_hint(mail, preset, hub_only, turnaround_ms)
}

#[derive(Clone)]
pub struct MailHandle {
    pub store: Arc<store::MailStore>,
    cmd: mpsc::Sender<MailCmd>,
    keying: Arc<AtomicBool>,
    pub cfg: Arc<Mutex<Config>>,
    pub snap: Arc<SharedStatus>,
    keys: IdentityKeys,
}

pub enum MailCmd {
    Send {
        id: String,
    },
    CheckList,
    CheckGet {
        ids: Vec<String>,
    },
    CheckQuiet {
        gen: u64,
    },
    /// A Radio+ Get has heard nothing from the gateway for a while.
    GetQuiet {
        gen: u64,
    },
    Inbound {
        incoming: Incoming,
        /// The burst was still keying when this frame arrived.
        during_burst: bool,
    },
    SyncHub,
    AckWait {
        id: String,
        gen: u64,
    },
    /// The outbox row is still `tx` and nothing has moved it on.
    TxStuck {
        id: String,
        gen: u64,
    },
    /// Gateway quiet gap elapsed. Key any ack that was held back.
    Quiet {
        gen: u64,
    },
}

pub fn db_path(cfg: &Config) -> PathBuf {
    let parent = cfg
        .store
        .path
        .parent()
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(crate::config::default_data_dir);
    parent.join("mail.sqlite")
}

pub fn start(
    cfg: Arc<Mutex<Config>>,
    snap: Arc<SharedStatus>,
    keys: IdentityKeys,
    ports: Arc<dyn Fn() -> RadioPorts + Send + Sync>,
    tel: broadcast::Sender<crate::telemetry::TelemetryEvent>,
) -> Result<MailHandle> {
    let path = db_path(&cfg.lock());
    let store = Arc::new(store::MailStore::open(&path)?);
    let (cmd, rx) = mpsc::channel(64);
    let keying = Arc::new(AtomicBool::new(false));
    let handle = MailHandle {
        store: store.clone(),
        cmd: cmd.clone(),
        keying: Arc::clone(&keying),
        cfg: cfg.clone(),
        snap: snap.clone(),
        keys: keys.clone(),
    };
    air::spawn(store, cfg, snap, keys, ports, cmd, rx, tel, keying);
    tokio::spawn(watch_delivery(handle.clone()));
    Ok(handle)
}

/// Asks the hub how each recently sent mail is doing at Resend, on the
/// schedule in [`progress::delivery_check_due`].
async fn watch_delivery(h: MailHandle) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tick.tick().await;
        let now = chrono::Utc::now().timestamp();
        let Ok(ids) = h.store.delivery_due(now) else {
            continue;
        };
        for id in ids {
            let _ = h.check_delivery(&id).await;
        }
    }
}

/// Record a gateway have-report the moment it is decoded. The mail loop can
/// be inside a long lease; the next resend is planned from this, not from a
/// report still queued behind that lease.
fn note_have_report(store: &store::MailStore, us: &str, incoming: &Incoming) -> Option<(u32, u32)> {
    let Incoming::Wire(w) = incoming else {
        return None;
    };
    if w.op != proto::MailOp::ChunkAck || w.count > 1 {
        return None;
    }
    if !w.meta.dest.is_empty() && !w.meta.dest.eq_ignore_ascii_case(us) {
        return None;
    }
    let (count, bits) = proto::parse_have_bitmap(&w.payload)?;
    let row = store.get(&w.mail_id).ok()??;
    if row.folder != "outbox" || row.chunks != u32::from(count) {
        return None;
    }
    let mut last = None;
    for idx in bits {
        last = store.note_chunk(&w.mail_id, idx).ok().flatten().or(last);
    }
    last
}

/// Record the gateway's acceptance the moment it is decoded. A retry lease
/// already waiting for the channel then keys nothing.
fn note_complete_ack(store: &store::MailStore, us: &str, incoming: &Incoming) -> bool {
    let Incoming::Wire(w) = incoming else {
        return false;
    };
    if w.op != proto::MailOp::CompleteAck || w.count > 1 {
        return false;
    }
    if !w.meta.dest.is_empty() && !w.meta.dest.eq_ignore_ascii_case(us) {
        return false;
    }
    if gateway::failure_text(&w.payload).is_some() {
        return false;
    }
    let Ok(Some(row)) = store.get(&w.mail_id) else {
        return false;
    };
    if row.folder != "outbox" || row.state == "sent" || row.state == "failed" {
        return false;
    }
    store.set_outcome(&w.mail_id, "sent", "sent", "").is_ok()
}

/// Seconds a gateway holds its chat and relays after hearing `incoming`,
/// or `None` when it is not part of another station's mail burst. Our own
/// frames are never heard, so every chunk or slice is someone else's.
pub fn heard_mail_hold_secs(incoming: &Incoming, frame_secs: f64) -> Option<f64> {
    match incoming {
        Incoming::Wire(w) if matches!(w.op, proto::MailOp::Data | proto::MailOp::VoxData) => {
            Some(gateway::heard_mail_hold_secs(w.count, w.idx, frame_secs))
        }
        Incoming::Slice { .. } => Some(gateway::busy_hold_secs(frame_secs)),
        Incoming::Wire(_) => None,
    }
}

pub async fn on_rf(mail: &MailHandle, incoming: Incoming) {
    let us = mail.cfg.lock().callsign.to_ascii_uppercase();
    let _ = note_have_report(&mail.store, &us, &incoming);
    let _ = note_complete_ack(&mail.store, &us, &incoming);
    // Our own frames are never heard here, so a data chunk or a slice is
    // another station's burst, and a report not for us is the gateway
    // answering one. Noted now: the mail task may be busy keying.
    let other = match &incoming {
        Incoming::Wire(w) => match w.op {
            proto::MailOp::Data | proto::MailOp::VoxData => true,
            proto::MailOp::ChunkAck | proto::MailOp::CompleteAck => {
                !w.meta.dest.is_empty() && !w.meta.dest.eq_ignore_ascii_case(&us)
            }
            _ => false,
        },
        Incoming::Slice { .. } => true,
    };
    if other {
        mail.store.note_other_mail();
    }
    let during_burst = mail.keying.load(Ordering::Acquire);
    let _ = mail
        .cmd
        .send(MailCmd::Inbound {
            incoming,
            during_burst,
        })
        .await;
}

impl MailHandle {
    pub fn tab_json(&self) -> Value {
        let cfg = self.cfg.lock().clone();
        let snap = self.snap.lock().clone();
        let unread = self.store.unread().unwrap_or(0);
        let waiting = self.store.waiting().map(|w| w.len()).unwrap_or(0);
        json!({
            "address": proto::wcr_address(&cfg.callsign),
            "unread": unread,
            "waiting": waiting,
            "notice": self.store.notice(),
            "get": self.store.get_progress(),
            "gateway": cfg.mail.gateway,
            "block": email_block_reason(&cfg.callsign, cfg.mode, &cfg.mail.gateway, snap.hub_ok, true),
        })
    }

    pub async fn compose(&self, to: &str, subject: &str, body: &str) -> Result<Value> {
        proto::validate_internet_addr(to)?;
        let body = proto::trim_body(body)?;
        let cfg = self.cfg.lock().clone();
        if matches!(gateway::local_send_path(cfg.mode), SendPath::Blocked) {
            return Err(Error::config(
                "Use Internet, Internet+Radio, or Radio+ with a gateway.",
            ));
        }
        if matches!(gateway::local_send_path(cfg.mode), SendPath::Rf)
            && cfg.mail.gateway.trim().is_empty()
        {
            return Err(Error::config("Set a mail gateway callsign in setup."));
        }
        let from = proto::wcr_address(&cfg.callsign);
        let ack = matches!(gateway::local_send_path(cfg.mode), SendPath::Rf);
        let to_addr = to.trim();
        let subject_s = subject.trim();
        let gateway = cfg.mail.gateway.trim().to_ascii_uppercase();
        let preview = AirtimeMail {
            from: &from,
            to: to_addr,
            subject: subject_s,
            body: &body,
            gateway: &gateway,
            ack,
        };
        let id = proto::compute_mail_id(&from, to_addr, subject_s, &body, ack);
        let preset = crate::presets::Preset::parse(&cfg.modem.preset)
            .unwrap_or(crate::presets::Preset::HfPoor);
        let airtime = hint_line(&preview, cfg.mode, preset, cfg.rf.turnaround_ms);
        self.store.insert(&MailRow {
            id: id.clone(),
            folder: "outbox".into(),
            from_addr: from,
            to_addr: to_addr.to_string(),
            subject: subject_s.to_string(),
            body,
            ts: chrono::Utc::now().timestamp(),
            state: "queued".into(),
            unread: false,
            ack,
            retries: 0,
            detail: String::new(),
            chunks: 0,
            got: String::new(),
            delivery: String::new(),
        })?;
        let _ = self.cmd.send(MailCmd::Send { id: id.clone() }).await;
        let path = match gateway::local_send_path(cfg.mode) {
            SendPath::Hub => "hub",
            SendPath::Rf => "rf",
            SendPath::Blocked => "blocked",
        };
        Ok(json!({ "id": id, "path": path, "airtime": airtime }))
    }

    /// The row with its status and hop times, for the local mail API.
    pub fn view(&self, row: MailRow) -> MailView {
        let hops = self.store.hops(&row.id).unwrap_or_default();
        let online = self.cfg.lock().hub.enabled() && self.store.hub_reachable();
        let now = chrono::Utc::now().timestamp();
        MailView {
            status: mail_status(&row, &hops, now, online),
            row,
            hops,
        }
    }

    /// Retry a Failed mail: back to the outbox and sent again from the start,
    /// on the same path and RF pacing as a new send.
    pub async fn retry(&self, id: &str) -> Result<bool> {
        if !self.store.reset_for_retry(id)? {
            return Ok(false);
        }
        tracing::info!("mail retry by hand {id}");
        let _ = self.cmd.send(MailCmd::Send { id: id.to_string() }).await;
        Ok(true)
    }

    pub async fn check_list(&self) -> Result<()> {
        let _ = self.cmd.send(MailCmd::CheckList).await;
        Ok(())
    }

    pub async fn check_get(&self, ids: Vec<String>) -> Result<()> {
        // Set before the reply, so the Get window never reads the last Get's end.
        if self.cfg.lock().mode == crate::modes::Mode::RadioPlus {
            self.store.set_get_progress(GetProgress {
                state: "asking".into(),
                ..GetProgress::default()
            });
        }
        let _ = self.cmd.send(MailCmd::CheckGet { ids }).await;
        Ok(())
    }

    /// Saves copy-to on the hub first, then here, so the two never disagree.
    /// An empty address turns copy-to off. This is one signed HTTPS call and
    /// never RF, so Radio+ makes it over this computer's internet.
    pub async fn set_copy(&self, address: &str) -> Result<Value> {
        let cfg = self.cfg.lock().clone();
        let address = address.trim();
        if address.is_empty() {
            self.copy_hub(
                &cfg,
                "/api/v1/mail/copy",
                json!({ "address": "", "code": "" }),
            )
            .await?;
            self.store.clear_copy(&cfg.callsign)?;
            return Ok(json!({ "ok": true, "state": store::COPY_OFF }));
        }
        proto::validate_copy_addr(address).map_err(|e| Error::Msg(e.plain()))?;
        let code = format!("{:06}", rand::random::<u32>() % 1_000_000);
        self.copy_hub(
            &cfg,
            "/api/v1/mail/copy",
            json!({ "address": address, "code": code }),
        )
        .await?;
        self.store.set_copy_pending(&cfg.callsign, address, &code)?;
        Ok(json!({ "ok": true, "pending": true, "state": store::COPY_CODE_SENT }))
    }

    /// `(code_matched, hub_agreed)`. The hub copies mail that arrives, so the
    /// code is used up here only once the hub has taken it. Until then it is
    /// kept and the state is `hub not confirmed`, so confirm can be pressed again.
    pub async fn confirm_copy(&self, code: &str) -> Result<(bool, bool)> {
        let cfg = self.cfg.lock().clone();
        let code = proto::copy_code_digits(code);
        if code.is_empty() || !self.store.copy_code_matches(&cfg.callsign, &code)? {
            return Ok((false, false));
        }
        let hub_ok = self
            .copy_hub(&cfg, "/api/v1/mail/copy/confirm", json!({ "code": code }))
            .await
            .ok()
            .and_then(|v| v.get("ok").and_then(|x| x.as_bool()))
            .unwrap_or(false);
        if hub_ok {
            self.store.confirm_copy(&cfg.callsign, &code)?;
        } else {
            self.store.mark_copy_hub_wait(&cfg.callsign)?;
        }
        Ok((true, hub_ok))
    }

    /// One signed `POST /api/v1/mail/status` for a sent mail. The answer is
    /// Resend's last event, saved on the row. No internet keeps the old answer.
    pub async fn check_delivery(&self, id: &str) -> Result<()> {
        let cfg = self.cfg.lock().clone();
        let now = chrono::Utc::now().timestamp();
        let prev = self.store.get(id)?.map(|r| r.delivery).unwrap_or_default();
        let mut delivery = prev.clone();
        if cfg.hub.enabled() {
            let base = hub::http_base_from_hub(&cfg.hub.url);
            let body = json!({ "mail_id": id });
            let answer = hub::signed_post_raw(
                &base,
                "/api/v1/mail/status",
                &self.keys,
                &cfg.callsign,
                &body,
            )
            .await;
            self.store.note_hub_reach(answer.is_ok());
            if let Ok((status, text)) = answer {
                let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                if (200..300).contains(&status) {
                    let state = v.get("state").and_then(|s| s.as_str()).unwrap_or("");
                    if !state.is_empty() {
                        delivery = state.to_string();
                    }
                } else if status == 404 && v.get("error").is_some() {
                    delivery = "unknown".into();
                }
            }
        }
        if delivery != prev {
            tracing::info!("mail delivery {id}: {delivery}");
        }
        self.store.set_delivery(id, &delivery, now)
    }

    async fn copy_hub(&self, cfg: &Config, path: &str, body: Value) -> Result<Value> {
        if !cfg.hub.enabled() {
            return Err(Error::Msg(COPY_NO_HUB_URL.into()));
        }
        let base = hub::http_base_from_hub(&cfg.hub.url);
        let Ok((status, text)) =
            hub::signed_post_raw(&base, path, &self.keys, &cfg.callsign, &body).await
        else {
            return Err(Error::Msg(copy_hub_unreachable(cfg.mode).into()));
        };
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        if (200..300).contains(&status) {
            return Ok(v);
        }
        let why = v
            .get("error")
            .and_then(|e| e.as_str())
            .unwrap_or("no reason given");
        Err(Error::Msg(format!("The hub refused copy-to: {why}")))
    }
}

/// An empty `[hub] url` means "no hub" everywhere else, so copy-to says so
/// rather than guessing a hub or blaming the internet.
pub const COPY_NO_HUB_URL: &str = "Copy-to needs the hub, and no hub address is set. Set [hub] url in wcr.toml (the public hub is wss://hub.weechatradio.com/ws) and try again.";

/// What to say when the hub cannot be reached for copy-to.
pub fn copy_hub_unreachable(mode: Mode) -> &'static str {
    if mode.uses_internet() {
        "Copy-to needs the hub. The hub did not answer; try again when it is up."
    } else {
        "Copy-to needs the hub. Radio+ reaches it over this computer's internet for this one step, and it did not answer. Connect to the internet and try again."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::Rung;
    use proto::{
        assemble_chunks, chunk_to_limit, compute_mail_id, encode_chunk, frames_for_mtu,
        join_slices, MailMeta, MailOp,
    };
    use sha2::{Digest, Sha256};

    fn hash_fn(src: &str, needle: &str) -> String {
        // Line endings follow the checkout, so hash the content alone.
        let body = extract_fn(src, needle).replace("\r\n", "\n");
        hex::encode(Sha256::digest(body.as_bytes()))
    }

    fn extract_fn(src: &str, needle: &str) -> String {
        let at = src
            .find(needle)
            .unwrap_or_else(|| panic!("missing {needle}"));
        let start = src[..at]
            .rfind("fn ")
            .unwrap_or_else(|| panic!("fn before {needle}"));
        let line = src[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let bytes = src[line..].as_bytes();
        let mut depth = 0i32;
        let mut seen = false;
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'{' {
                depth += 1;
                seen = true;
            } else if b == b'}' {
                depth -= 1;
                if seen && depth == 0 {
                    return src[line..line + i + 1].to_string();
                }
            }
        }
        panic!("unclosed {needle}");
    }

    #[test]
    fn chat_source_matches_v0_1_73() {
        // Frozen functions at a4e75da. pause/resume on AirQueue is outside these spans.
        let checks = [
            (
                "pace_and_send",
                include_str!("../air.rs"),
                "async fn pace_and_send(",
            ),
            (
                "run_air_queue",
                include_str!("../air.rs"),
                "pub async fn run_air_queue(",
            ),
            (
                "rung_control_config",
                include_str!("../presets.rs"),
                "Self::OfdmQpskHalf => serde_json::json!",
            ),
            (
                "preset_control_config",
                include_str!("../presets.rs"),
                "\"modulation\": \"QPSK\"",
            ),
            (
                "send_chat",
                include_str!("../node.rs"),
                "async fn send_chat(",
            ),
            ("rf_frames", include_str!("../node.rs"), "fn rf_frames("),
            (
                "split_to_fit",
                include_str!("../proto/frag.rs"),
                "pub fn split_to_fit(",
            ),
            ("split", include_str!("../proto/frag.rs"), "pub fn split("),
        ];
        let mut lines = Vec::new();
        for (name, src, needle) in checks {
            lines.push(format!("{name} {}", hash_fn(src, needle)));
        }
        let got = lines.join("\n");
        let pinned = include_str!("drift.sha256").trim().replace("\r\n", "\n");
        assert_eq!(got, pinned, "Live Chat source drifted from v0.1.73");
    }

    #[test]
    fn chunks_fit_512_170_and_55() {
        let body = "a".repeat(1800);
        let meta = MailMeta {
            from: "M7TJF@mail.weechatradio.com".into(),
            to: "alex@example.com".into(),
            subject: "field note".into(),
            dest: "TF101".into(),
            via: "TF101".into(),
            ack: true,
            ..MailMeta::default()
        };
        let id = compute_mail_id(&meta.from, &meta.to, &meta.subject, &body, true);
        for limit in [512usize, 170, 55] {
            let chunks = chunk_to_limit(&id, &meta, &body, limit.min(crate::proto::MAX_BODY));
            let mut keyed = Vec::new();
            for chunk in &chunks {
                let raw = encode_chunk(chunk);
                let frames = frames_for_mtu(&raw, limit).unwrap();
                for frame in &frames {
                    assert!(frame.len() <= limit, "{} > {limit}", frame.len());
                }
                keyed.extend(frames);
            }
            let mut groups: std::collections::HashMap<String, Vec<(u16, String)>> =
                std::collections::HashMap::new();
            let mut wires = Vec::new();
            for frame in keyed {
                match try_decode(&frame).unwrap() {
                    Incoming::Wire(w) => wires.push(w),
                    Incoming::Slice {
                        group,
                        part,
                        parts: _,
                        data,
                    } => {
                        groups.entry(group).or_default().push((part, data));
                    }
                }
            }
            for (_, parts) in groups {
                let raw = join_slices(&parts).unwrap();
                wires.push(proto::decode_chunk(&raw).unwrap());
            }
            let (back, text, op) = assemble_chunks(wires).unwrap();
            assert_eq!(op, MailOp::VoxData);
            assert!(back.ack);
            assert_eq!(text, body);
            assert_eq!(
                id,
                compute_mail_id(&back.from, &back.to, &back.subject, &text, back.ack)
            );
        }
    }

    #[test]
    fn rung_never_faster_than_ladder_start_on_vox() {
        for preset in [Preset::VoxSafe, Preset::HfPoor, Preset::HfDeep] {
            for retries in 0..6 {
                let rung = air::mail_rung(preset, retries);
                assert!(
                    rung.index() >= preset.ladder_start(),
                    "{preset:?} retry {retries} -> {}",
                    rung.as_str()
                );
                assert_ne!(rung, Rung::OfdmQpskHalf);
            }
        }
    }

    #[test]
    fn lease_drains_before_and_after_and_restores_csma() {
        let steps = lease::lease_sequence();
        assert!(steps.contains(&"drain_before"));
        assert!(steps.contains(&"drain_after"));
        assert!(
            steps.iter().position(|s| *s == "drain_before").unwrap()
                < steps.iter().position(|s| *s == "set_mail_config").unwrap()
        );
        assert!(
            steps.iter().position(|s| *s == "drain_after").unwrap()
                < steps.iter().position(|s| *s == "restore_snapshot").unwrap()
        );
        let mail = lease::mail_tx_config(Preset::HfPoor, air::mail_rung(Preset::HfPoor, 0));
        assert_eq!(mail["csma_enabled"], false);
        let restore = lease::restore_config(Preset::HfPoor);
        assert_eq!(restore["csma_enabled"], true);
        assert!(lease::tx_drained(3, 3));
        assert!(!lease::tx_drained(2, 3));
        assert_eq!(lease::drain_target_after(10, 4), 14);
    }

    #[test]
    fn compose_holds_fields_until_sent() {
        let mut hold = ComposeHold {
            to: "alex@example.com".into(),
            subject: "field".into(),
            body: "on the air".into(),
            pending: false,
        };
        hold.arm();
        assert_eq!(hold.to, "alex@example.com");
        assert_eq!(hold.body, "on the air");
        hold.on_sent();
        assert!(hold.to.is_empty() && hold.subject.is_empty() && hold.body.is_empty());
    }

    #[test]
    fn grey_tab_states_the_missing_piece() {
        assert_eq!(
            email_block_reason("~NICK", Mode::InternetRadio, "TF101", true, true),
            Some("Email needs a licensed callsign")
        );
        assert_eq!(
            email_block_reason("M7TJF", Mode::Radio, "TF101", true, true),
            Some("Use Internet, Internet+Radio, or Radio+ with a gateway")
        );
        assert_eq!(
            email_block_reason("M7TJF", Mode::RadioPlus, "", true, true),
            Some("Set a mail gateway callsign in setup")
        );
        assert_eq!(
            email_block_reason("M7TJF", Mode::RadioPlus, "TF101", true, false),
            Some("Start the station to use Email")
        );
        assert_eq!(
            email_block_reason("M7TJF", Mode::InternetRadio, "", false, true),
            Some("Email needs the hub")
        );
        assert_eq!(
            email_block_reason("M7TJF", Mode::RadioPlus, "TF101", false, true),
            None
        );
    }

    #[test]
    fn only_the_emailed_code_turns_copy_to_on() {
        let store = store::MailStore::open_memory().expect("store");
        store
            .set_copy_pending("M7TJF", "tj@example.com", "123456")
            .expect("pending");
        assert!(!store.confirm_copy("M7TJF", "000000").expect("wrong code"));
        let pending = store.copy("M7TJF").expect("copy");
        assert!(!pending.confirmed && pending.address.is_empty());

        assert!(store.confirm_copy("M7TJF", "123456").expect("right code"));
        let live = store.copy("M7TJF").expect("copy");
        assert!(live.confirmed && live.enabled);
        assert_eq!(live.address, "tj@example.com");
    }

    /// The real hub's copy-to routes and store. Only the code email is stood
    /// in for: its code lands in `inbox` instead of going through Resend.
    async fn copy_hub() -> (
        String,
        crate::net::hub_server::HubState,
        Arc<Mutex<Vec<String>>>,
    ) {
        use crate::net::hub_mail::{post_copy, post_copy_confirm};
        use axum::body::Bytes;
        use axum::extract::State;
        use axum::http::HeaderMap;
        use axum::response::IntoResponse;
        let st = crate::net::hub_server::test_hub_state();
        let inbox = Arc::new(Mutex::new(Vec::<String>::new()));
        let mailbox = inbox.clone();
        let send_code = move |State(st): State<crate::net::hub_server::HubState>,
                              headers: HeaderMap,
                              body: Bytes| {
            let mailbox = mailbox.clone();
            async move {
                let v: Value = serde_json::from_slice(&body).unwrap();
                let address = v["address"].as_str().unwrap_or("").to_string();
                if address.is_empty() {
                    return post_copy(State(st), headers, body).await.into_response();
                }
                let call = headers["x-wcr-callsign"].to_str().unwrap().to_string();
                let code = v["code"].as_str().unwrap().to_string();
                st.mail.set_pending(&call, &address, &code).unwrap();
                mailbox.lock().push(code);
                axum::Json(json!({ "ok": true })).into_response()
            }
        };
        let app = axum::Router::new()
            .route("/api/v1/mail/copy", axum::routing::post(send_code))
            .route(
                "/api/v1/mail/copy/confirm",
                axum::routing::post(post_copy_confirm),
            )
            .with_state(st.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, st, inbox)
    }

    /// Nothing listens here, so the hub is unreachable.
    const HUB_DOWN: &str = "http://127.0.0.1:1";

    fn copy_station(mode: Mode, hub_url: &str) -> MailHandle {
        let mut cfg = Config {
            callsign: "M7TJF".into(),
            mode,
            ..Config::default()
        };
        cfg.hub.url = hub_url.into();
        let (cmd, _rx) = mpsc::channel(4);
        MailHandle {
            store: Arc::new(store::MailStore::open_memory().unwrap()),
            cmd,
            keying: Arc::new(AtomicBool::new(false)),
            cfg: Arc::new(Mutex::new(cfg)),
            snap: crate::status::new_shared(),
            keys: IdentityKeys::generate(),
        }
    }

    fn copy_state(h: &MailHandle) -> String {
        h.store.copy("M7TJF").unwrap().state
    }

    #[tokio::test]
    async fn a_confirm_the_hub_missed_keeps_the_code_for_a_retry() {
        let (hub_up, hub, inbox) = copy_hub().await;
        let h = copy_station(Mode::InternetRadio, &hub_up);
        h.set_copy("tj@example.com").await.expect("save");
        assert_eq!(copy_state(&h), store::COPY_CODE_SENT);
        let code = inbox.lock()[0].clone();

        // Hub down: the code matches here, but nothing is used up.
        h.cfg.lock().hub.url = HUB_DOWN.into();
        assert_eq!(h.confirm_copy(&code).await.unwrap(), (true, false));
        let local = h.store.copy("M7TJF").unwrap();
        assert_eq!(local.state, store::COPY_HUB_WAIT);
        assert!(!local.confirmed && !local.enabled);
        assert_eq!(hub.mail.copy_to("M7TJF"), None);
        assert_eq!(h.confirm_copy("000000").await.unwrap(), (false, false));

        // Hub back: the same code works, even copied with the full stop.
        h.cfg.lock().hub.url = hub_up;
        assert_eq!(
            h.confirm_copy(&format!("{code}.")).await.unwrap(),
            (true, true)
        );
        let local = h.store.copy("M7TJF").unwrap();
        assert_eq!(local.state, store::COPY_ON);
        assert_eq!(local.address, "tj@example.com");
        assert_eq!(hub.mail.copy_to("M7TJF").as_deref(), Some("tj@example.com"));
    }

    #[tokio::test]
    async fn radio_plus_sets_copy_to_with_the_hub_directly() {
        let (hub_up, hub, inbox) = copy_hub().await;
        let h = copy_station(Mode::RadioPlus, &hub_up);
        assert!(!h.snap.lock().hub_ok, "Radio+ has no hub link");
        h.set_copy("tj@example.com").await.expect("save on Radio+");
        let code = inbox.lock()[0].clone();
        assert_eq!(h.confirm_copy(&code).await.unwrap(), (true, true));
        assert_eq!(copy_state(&h), store::COPY_ON);
        // The hub copies by the author's callsign, so mail a gateway posts for us is copied.
        assert_eq!(hub.mail.copy_to("M7TJF").as_deref(), Some("tj@example.com"));

        // No internet on this computer: say so plainly and change nothing.
        h.cfg.lock().hub.url = HUB_DOWN.into();
        let err = h.set_copy("other@example.com").await.unwrap_err().plain();
        assert!(err.starts_with("Copy-to needs the hub."), "{err}");
        assert!(err.contains("internet"), "{err}");
        assert_eq!(copy_state(&h), store::COPY_ON);
        assert_eq!(inbox.lock().len(), 1);

        // No hub address at all: say that, not "connect to the internet".
        h.cfg.lock().hub.url = String::new();
        let err = h.set_copy("other@example.com").await.unwrap_err().plain();
        assert_eq!(err, COPY_NO_HUB_URL);
        assert!(err.contains("wcr.toml"), "{err}");
        assert_eq!(copy_state(&h), store::COPY_ON);
        assert_eq!(inbox.lock().len(), 1);
    }

    #[tokio::test]
    async fn an_empty_copy_to_address_turns_it_off() {
        let (hub_up, hub, inbox) = copy_hub().await;
        let h = copy_station(Mode::Internet, &hub_up);
        h.set_copy("tj@example.com").await.unwrap();
        let code = inbox.lock()[0].clone();
        assert_eq!(h.confirm_copy(&code).await.unwrap(), (true, true));

        // Off needs the hub too, or mail that arrives would still be copied.
        h.cfg.lock().hub.url = HUB_DOWN.into();
        assert!(h.set_copy("").await.is_err());
        assert_eq!(copy_state(&h), store::COPY_ON);

        h.cfg.lock().hub.url = hub_up;
        let off = h.set_copy("  ").await.expect("off");
        assert_eq!(off["state"], store::COPY_OFF);
        assert_eq!(copy_state(&h), store::COPY_OFF);
        assert_eq!(hub.mail.copy_to("M7TJF"), None);

        for bad in [
            "not-an-address",
            "a b@example.com",
            "M7TJF@mail.weechatradio.com",
        ] {
            let err = h.set_copy(bad).await.unwrap_err().plain();
            assert!(err.starts_with("Copy-to address"), "{bad}: {err}");
        }
        assert_eq!(inbox.lock().len(), 1, "a bad address never reaches the hub");
    }

    fn preview<'a>(body: &'a str) -> AirtimeMail<'a> {
        AirtimeMail {
            from: "M7TJF@mail.weechatradio.com",
            to: "operator@example.com",
            subject: "subject",
            body,
            gateway: "GATEWAY",
            ack: true,
        }
    }

    #[test]
    fn a_long_mail_warns_before_it_is_sent() {
        let long_body = "hello mail ".repeat(3900 / 11) + &"x".repeat(3900 % 11);
        assert_eq!(long_body.len(), 3900);
        let long = preview(&long_body);
        let hint = hint_line(&long, Mode::RadioPlus, Preset::HfPoor, 250);
        assert!(
            hint.contains("not Sent until the gateway accepts it"),
            "{hint}"
        );
        assert!(hint.contains("bursts"), "{hint}");
        let (secs, long_n) = air::mail_airtime_secs(Preset::HfPoor, &long, 250);
        let planned = air::plan_mail_wires(
            Preset::HfPoor,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            &long,
        );
        assert_eq!(long_n, planned.len(), "hint must match the send split");
        assert!(long_n >= 46, "3900 B estimated {long_n} bursts");
        assert!(secs > 150.0, "3900 B estimated {secs:.0}s");
        let short_body = "x".repeat(115);
        let (_, short_n) = air::mail_airtime_secs(Preset::HfPoor, &preview(&short_body), 250);
        assert!(short_n >= 3, "115 B estimated {short_n} bursts");
        let one_body = "x";
        let one = hint_line(&preview(&one_body), Mode::RadioPlus, Preset::VhfFm, 250);
        assert!(one.contains("(1 burst)"), "{one}");
        assert!(!one.contains("bursts"), "{one}");
        let line = confirm_air_line(120.0, 23, "hf-poor");
        assert!(line.contains("23 bursts"));
        assert!(confirm_air_line(4.0, 1, "hf-poor").contains("(1 burst @ hf-poor)"));
        assert!(line.contains("Not Sent until the gateway accepts it"));
        assert!(air::missing_chunk_idxs(4, "0,2").is_empty() == false);
        assert_eq!(air::missing_chunk_idxs(4, "0,2"), vec![1, 3]);
    }

    #[test]
    fn a_have_report_is_noted_before_the_mail_loop_runs() {
        let store = store::MailStore::open_memory().expect("store");
        store
            .insert(&store::MailRow {
                id: "m48".into(),
                folder: "outbox".into(),
                from_addr: "M7TJF@mail.weechatradio.com".into(),
                to_addr: "a@b.co".into(),
                subject: "s".into(),
                body: "long".into(),
                ts: 1,
                state: "tx".into(),
                unread: false,
                ack: true,
                retries: 2,
                detail: String::new(),
                chunks: 48,
                got: "0,1,2".into(),
                delivery: String::new(),
            })
            .unwrap();
        let have: Vec<u16> = (0..48)
            .filter(|i| *i != 7 && *i != 30 && *i != 47)
            .collect();
        let report = |dest: &str, count: u16| {
            Incoming::Wire(proto::MailWire {
                op: MailOp::ChunkAck,
                mail_id: "m48".into(),
                idx: 0,
                count: 1,
                meta: MailMeta {
                    dest: dest.into(),
                    ..MailMeta::default()
                },
                payload: proto::have_bitmap(count, &have),
            })
        };
        assert_eq!(
            note_have_report(&store, "M7TJF", &report("G4XYZ", 48)),
            None
        );
        assert_eq!(
            note_have_report(&store, "M7TJF", &report("M7TJF", 40)),
            None
        );
        assert_eq!(store.get("m48").unwrap().unwrap().got, "0,1,2");
        assert_eq!(
            note_have_report(&store, "M7TJF", &report("M7TJF", 48)),
            Some((45, 48))
        );
        let row = store.get("m48").unwrap().unwrap();
        assert_eq!(
            air::missing_chunk_idxs(row.chunks, &row.got),
            vec![7, 30, 47]
        );
        let data = Incoming::Wire(proto::MailWire {
            op: MailOp::VoxData,
            ..match report("M7TJF", 48) {
                Incoming::Wire(w) => w,
                Incoming::Slice { .. } => unreachable!(),
            }
        });
        assert_eq!(note_have_report(&store, "M7TJF", &data), None);
    }

    #[test]
    fn a_gateway_acceptance_is_noted_before_the_mail_loop_runs() {
        let store = store::MailStore::open_memory().expect("store");
        let row = |id: &str, state: &str| store::MailRow {
            id: id.into(),
            folder: "outbox".into(),
            from_addr: "M7TJF@mail.weechatradio.com".into(),
            to_addr: "a@b.co".into(),
            subject: "s".into(),
            body: "long".into(),
            ts: 1,
            state: state.into(),
            unread: false,
            ack: true,
            retries: 1,
            detail: String::new(),
            chunks: 56,
            got: String::new(),
            delivery: String::new(),
        };
        store.insert(&row("m56", "tx")).unwrap();
        store.insert(&row("bad", "tx")).unwrap();
        let ack = |id: &str, dest: &str, payload: &str| {
            Incoming::Wire(proto::MailWire {
                op: MailOp::CompleteAck,
                mail_id: id.into(),
                idx: 0,
                count: 1,
                meta: MailMeta {
                    dest: dest.into(),
                    ..MailMeta::default()
                },
                payload: payload.into(),
            })
        };
        assert!(!note_complete_ack(
            &store,
            "M7TJF",
            &ack("m56", "G4XYZ", "ok")
        ));
        assert_eq!(store.get("m56").unwrap().unwrap().state, "tx");
        assert!(!note_complete_ack(
            &store,
            "M7TJF",
            &ack("bad", "M7TJF", "fail Hub did not accept the message.")
        ));
        assert_eq!(store.get("bad").unwrap().unwrap().state, "tx");
        assert!(note_complete_ack(
            &store,
            "M7TJF",
            &ack("m56", "M7TJF", "ok")
        ));
        let done = store.get("m56").unwrap().unwrap();
        assert_eq!(
            (done.folder.as_str(), done.state.as_str()),
            ("sent", "sent")
        );
        assert!(!note_complete_ack(
            &store,
            "M7TJF",
            &ack("m56", "M7TJF", "ok")
        ));
    }

    #[test]
    fn the_lease_lets_an_inbound_frame_finish_before_keying() {
        let steps = lease::lease_sequence();
        let at = |s: &str| steps.iter().position(|x| *x == s).unwrap();
        assert!(at("pause") < at("wait_clear"));
        assert!(at("wait_clear") < at("snapshot"));
        assert!(at("wait_clear") < at("send_frames"));
    }

    #[test]
    fn gateway_chunk_acks_are_stored() {
        let store = store::MailStore::open_memory().expect("store");
        store
            .insert(&store::MailRow {
                id: "m1".into(),
                folder: "outbox".into(),
                from_addr: "M7TJF@mail.weechatradio.com".into(),
                to_addr: "a@b.co".into(),
                subject: "s".into(),
                body: "hello".into(),
                ts: 1,
                state: "tx".into(),
                unread: false,
                ack: true,
                retries: 0,
                detail: String::new(),
                chunks: 2,
                got: String::new(),
                delivery: String::new(),
            })
            .unwrap();
        assert_eq!(store.note_chunk("m1", 0).unwrap(), Some((1, 2)));
        assert_eq!(store.note_chunk("m1", 1).unwrap(), Some((2, 2)));
        store
            .set_outcome("m1", "outbox", "failed", gateway::NO_GATEWAY_ACK)
            .unwrap();
        let row = store.get("m1").unwrap().unwrap();
        assert_eq!(row.state, "failed");
        assert_eq!(row.detail, gateway::NO_GATEWAY_ACK);
        store.set_notice("no new mail");
        assert_eq!(store.notice(), "no new mail");
    }

    #[test]
    fn third_party_deny_does_not_block_gateway_mail_post() {
        assert!(gateway::gateway_may_post(Mode::InternetRadio, false));
        assert!(!gateway::gateway_may_post(Mode::RadioPlus, true));
        assert!(!gateway::gateway_may_post(Mode::Radio, true));
        assert!(gateway::resend_accepted(true));
        assert!(!gateway::resend_accepted(false));
    }
}
