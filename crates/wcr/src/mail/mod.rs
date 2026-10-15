//! SPDX-License-Identifier: Apache-2.0
//! Email beside Live Chat. Radio access is a pause/restore lease only.

mod air;
mod api;
mod gateway;
mod hub;
mod lease;
mod proto;
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

pub fn airtime_parts(mail: &AirtimeMail<'_>, preset: Preset, turnaround_ms: u32) -> (f64, usize) {
    air::mail_airtime_secs(preset, mail, turnaround_ms)
}
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
pub use store::{MailRow, WaitHeader};
use tokio::sync::{broadcast, mpsc};

pub use api::{router, MailSlot};
pub use proto::{
    callsign_from_wcr, pull_token, trim_body, try_decode, validate_internet_addr, wcr_address,
    Incoming, MAIL_DOMAIN, WCR_COPY_HEADER,
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
    Ok(handle)
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

pub async fn on_rf(mail: &MailHandle, incoming: Incoming) {
    let us = mail.cfg.lock().callsign.to_ascii_uppercase();
    let _ = note_have_report(&mail.store, &us, &incoming);
    let _ = note_complete_ack(&mail.store, &us, &incoming);
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
        })?;
        let _ = self.cmd.send(MailCmd::Send { id: id.clone() }).await;
        let path = match gateway::local_send_path(cfg.mode) {
            SendPath::Hub => "hub",
            SendPath::Rf => "rf",
            SendPath::Blocked => "blocked",
        };
        Ok(json!({ "id": id, "path": path, "airtime": airtime }))
    }

    pub async fn check_list(&self) -> Result<()> {
        let _ = self.cmd.send(MailCmd::CheckList).await;
        Ok(())
    }

    pub async fn check_get(&self, ids: Vec<String>) -> Result<()> {
        let _ = self.cmd.send(MailCmd::CheckGet { ids }).await;
        Ok(())
    }

    pub async fn set_copy(&self, address: &str) -> Result<()> {
        proto::validate_internet_addr(address)?;
        let cfg = self.cfg.lock().clone();
        if !self.snap.lock().hub_ok {
            return Err(Error::Net("Copy-to needs the hub.".into()));
        }
        let code = format!("{:06}", rand::random::<u32>() % 1_000_000);
        self.store
            .set_copy_pending(&cfg.callsign, address.trim(), &code)?;
        let base = hub::http_base_from_hub(&cfg.hub.url);
        hub::signed_post(
            &base,
            "/api/v1/mail/copy",
            &self.keys,
            &cfg.callsign,
            &json!({ "address": address.trim(), "code": code }),
        )
        .await?;
        Ok(())
    }

    /// `(code_matched, hub_agreed)`. The hub copies mail that arrives, so a
    /// local-only confirm leaves copy-to half on.
    pub async fn confirm_copy(&self, code: &str) -> Result<(bool, bool)> {
        let cfg = self.cfg.lock().clone();
        let code = code.trim();
        if !self.store.confirm_copy(&cfg.callsign, code)? {
            return Ok((false, false));
        }
        let base = hub::http_base_from_hub(&cfg.hub.url);
        let hub_ok = hub::signed_post(
            &base,
            "/api/v1/mail/copy/confirm",
            &self.keys,
            &cfg.callsign,
            &json!({ "code": code }),
        )
        .await
        .ok()
        .and_then(|v| v.get("ok").and_then(|x| x.as_bool()))
        .unwrap_or(false);
        Ok((true, hub_ok))
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
