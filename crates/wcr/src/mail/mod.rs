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
pub use air::confirm_air_line;
pub use gateway::{local_send_path as gateway_path, SendPath};
pub use lease::RadioPorts;

pub fn airtime_parts(bytes: usize, preset: Preset, turnaround_ms: u32) -> (f64, usize) {
    air::mail_airtime_secs(preset, bytes, turnaround_ms)
}
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
pub use store::{MailRow, WaitHeader};
use tokio::sync::mpsc;

pub use api::{router, MailSlot};
pub use proto::{
    callsign_from_wcr, trim_body, try_decode, validate_internet_addr, wcr_address, Incoming,
    MAIL_DOMAIN, WCR_COPY_HEADER,
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
        Mode::Radio => Some("Email needs an internet hop"),
        Mode::RadioPlus if gateway.trim().is_empty() => {
            Some("Set a mail gateway callsign in setup")
        }
        Mode::Internet | Mode::InternetRadio if !hub_ok => Some("Email needs the hub"),
        _ => None,
    }
}

pub fn hint_line(body_len: usize, mode: Mode, preset: Preset, turnaround_ms: u32) -> String {
    let hub_only = matches!(gateway::local_send_path(mode), SendPath::Hub);
    air::airtime_hint(body_len, preset, hub_only, turnaround_ms)
}

#[derive(Clone)]
pub struct MailHandle {
    pub store: Arc<store::MailStore>,
    cmd: mpsc::Sender<MailCmd>,
    pub cfg: Arc<Mutex<Config>>,
    pub snap: Arc<SharedStatus>,
    keys: IdentityKeys,
}

pub enum MailCmd {
    Send { id: String },
    CheckList,
    CheckGet { ids: Vec<String> },
    Inbound(Incoming),
    SyncHub,
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
) -> Result<MailHandle> {
    let path = db_path(&cfg.lock());
    let store = Arc::new(store::MailStore::open(&path)?);
    let (cmd, rx) = mpsc::channel(64);
    let handle = MailHandle {
        store: store.clone(),
        cmd: cmd.clone(),
        cfg: cfg.clone(),
        snap: snap.clone(),
        keys: keys.clone(),
    };
    air::spawn(store, cfg, snap, keys, ports, cmd, rx);
    Ok(handle)
}

pub async fn on_rf(mail: &MailHandle, incoming: Incoming) {
    let _ = mail.cmd.send(MailCmd::Inbound(incoming)).await;
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
                "Email needs an internet hop. Radio-only cannot reach Resend.",
            ));
        }
        if matches!(gateway::local_send_path(cfg.mode), SendPath::Rf)
            && cfg.mail.gateway.trim().is_empty()
        {
            return Err(Error::config("Set a mail gateway callsign in setup."));
        }
        let from = proto::wcr_address(&cfg.callsign);
        let ack = cfg.modem.is_vox() && matches!(gateway::local_send_path(cfg.mode), SendPath::Rf);
        let id = proto::compute_mail_id(&from, to.trim(), subject.trim(), &body, ack);
        self.store.insert(&MailRow {
            id: id.clone(),
            folder: "outbox".into(),
            from_addr: from,
            to_addr: to.trim().to_string(),
            subject: subject.trim().to_string(),
            body,
            ts: chrono::Utc::now().timestamp(),
            state: "queued".into(),
            unread: false,
            ack,
            retries: 0,
        })?;
        let _ = self.cmd.send(MailCmd::Send { id: id.clone() }).await;
        let path = match gateway::local_send_path(cfg.mode) {
            SendPath::Hub => "hub",
            SendPath::Rf => "rf",
            SendPath::Blocked => "blocked",
        };
        Ok(json!({ "id": id, "path": path }))
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
        let body = extract_fn(src, needle);
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
        // Frozen functions at 950acca. pause/resume on AirQueue is outside these spans.
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
            Some("Email needs an internet hop")
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
    fn third_party_deny_does_not_block_gateway_mail_post() {
        assert!(gateway::gateway_may_post(Mode::InternetRadio, false));
        assert!(!gateway::gateway_may_post(Mode::RadioPlus, true));
        assert!(!gateway::gateway_may_post(Mode::Radio, true));
        assert!(gateway::resend_accepted(true));
        assert!(!gateway::resend_accepted(false));
    }
}
