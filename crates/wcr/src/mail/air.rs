//! SPDX-License-Identifier: Apache-2.0
//! Email-only TX. Frames go to KISS directly. They never enter the chat air queue.

use super::gateway::{self, SendPath};
use super::hub;
use super::lease::{self, RadioPorts};
use super::proto::{self, Incoming, MailMeta, MailOp, MailWire};
use super::store::{MailRow, MailStore, WaitHeader};
use super::MailCmd;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::modes::Mode;
use crate::presets::{Preset, Rung};
use crate::proto::IdentityKeys;
use crate::status::SharedStatus;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Start at the preset ladder and only step more robust. Never a faster rung.
pub fn mail_rung(preset: Preset, retries: u32) -> Rung {
    let start = preset.ladder_start();
    let idx = (start + retries as usize).min(Rung::COUNT - 1);
    Rung::from_index(idx)
}

pub fn mail_airtime_secs(preset: Preset, body_len: usize, turnaround_ms: u32) -> (f64, usize) {
    let mtu = Rung::from_index(preset.ladder_start()).payload_bytes() as usize;
    let chunk = mtu.clamp(1, crate::proto::MAX_BODY);
    let n = body_len.max(1).div_ceil(chunk);
    let bytes = n.saturating_mul(chunk.min(body_len.max(1))) + n.saturating_mul(48);
    let tx = (bytes as f64) * 8.0 / preset.bitrate_bps() as f64;
    let secs = tx + n as f64 * (preset.overhead_ms() + turnaround_ms) as f64 / 1000.0;
    (secs, n.max(1))
}

pub fn humanize_airtime(secs: f64) -> String {
    if secs < 10.0 {
        format!("{secs:.1} s")
    } else if secs <= 90.0 {
        format!("{} s", secs.round() as u64)
    } else {
        let min = (secs / 60.0).round().max(1.0) as u64;
        format!("about {min} min")
    }
}

pub fn airtime_hint(bytes: usize, preset: Preset, hub_only: bool, turnaround_ms: u32) -> String {
    if hub_only {
        format!("{bytes} B · hub only")
    } else {
        let (secs, n) = mail_airtime_secs(preset, bytes, turnaround_ms);
        let warn = if secs >= 20.0 || bytes >= 1500 {
            " · not Sent until the gateway accepts it"
        } else {
            ""
        };
        format!(
            "{bytes} B · ~{} on {} ({n} bursts){warn}",
            humanize_airtime(secs),
            preset.as_str()
        )
    }
}

pub fn confirm_air_line(secs: f64, bursts: usize, preset: &str) -> String {
    format!(
        "About {} on the air ({bursts} bursts @ {preset}). Not Sent until the gateway accepts it. Retries can take longer.",
        humanize_airtime(secs)
    )
}

/// Chunk indexes still waiting for a gateway ack. Empty `got` means none yet.
pub fn missing_chunk_idxs(total: u32, got: &str) -> Vec<u16> {
    let have: std::collections::HashSet<u16> =
        got.split(',').filter_map(|s| s.parse().ok()).collect();
    (0..total as u16).filter(|i| !have.contains(i)).collect()
}

struct TxCtx {
    store: Arc<MailStore>,
    cfg: Arc<Mutex<Config>>,
    snap: Arc<SharedStatus>,
    keys: IdentityKeys,
    ports: Arc<dyn Fn() -> RadioPorts + Send + Sync>,
    cmd_tx: mpsc::Sender<MailCmd>,
    tel: broadcast::Sender<crate::telemetry::TelemetryEvent>,
    waits: Mutex<HashMap<String, u64>>,
    checks: Mutex<u64>,
}

struct Assembler {
    slices: HashMap<String, Vec<Option<String>>>,
    parts: HashMap<String, Vec<MailWire>>,
    from: HashMap<String, String>,
    to: HashMap<String, String>,
}

pub(super) fn spawn(
    store: Arc<MailStore>,
    cfg: Arc<Mutex<Config>>,
    snap: Arc<SharedStatus>,
    keys: IdentityKeys,
    ports: Arc<dyn Fn() -> RadioPorts + Send + Sync>,
    cmd_tx: mpsc::Sender<MailCmd>,
    rx: mpsc::Receiver<MailCmd>,
    tel: broadcast::Sender<crate::telemetry::TelemetryEvent>,
) {
    let ctx = TxCtx {
        store,
        cfg,
        snap,
        keys,
        ports,
        cmd_tx,
        tel,
        waits: Mutex::new(HashMap::new()),
        checks: Mutex::new(0),
    };
    tokio::spawn(run(rx, ctx));
}

async fn run(mut rx: mpsc::Receiver<MailCmd>, ctx: TxCtx) {
    let mut asm = Assembler {
        slices: HashMap::new(),
        parts: HashMap::new(),
        from: HashMap::new(),
        to: HashMap::new(),
    };
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { return };
                dispatch(&ctx, &mut asm, cmd).await;
            }
            _ = tick.tick() => {
                dispatch(&ctx, &mut asm, MailCmd::SyncHub).await;
            }
        }
    }
}

async fn dispatch(ctx: &TxCtx, asm: &mut Assembler, cmd: MailCmd) {
    match cmd {
        MailCmd::Send { id } => {
            if let Err(e) = send_one(ctx, &id).await {
                tracing::warn!("mail send {id}: {e}");
            }
        }
        MailCmd::CheckList => {
            ctx.store.set_notice("");
            let gen = {
                let mut n = ctx.checks.lock();
                *n = n.wrapping_add(1);
                *n
            };
            let tx = ctx.cmd_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(45)).await;
                let _ = tx.send(MailCmd::CheckQuiet { gen }).await;
            });
            tracing::info!("mail check asked");
            let _ = send_control(ctx, MailOp::ListReq, MailMeta::default(), "").await;
        }
        MailCmd::CheckQuiet { gen } => {
            if *ctx.checks.lock() != gen {
                return;
            }
            if ctx.store.notice().is_empty() {
                ctx.store.set_notice("No reply from the gateway.");
                tracing::info!("mail check: no reply from the gateway");
            }
        }
        MailCmd::CheckGet { ids } => {
            let meta = MailMeta {
                ids,
                ..MailMeta::default()
            };
            let _ = send_control(ctx, MailOp::GetReq, meta, "").await;
        }
        MailCmd::Inbound(incoming) => on_incoming(ctx, asm, incoming).await,
        MailCmd::SyncHub => {
            let _ = sync_hub(ctx).await;
        }
        MailCmd::AckWait { id, gen } => {
            if let Err(e) = on_ack_wait(ctx, &id, gen).await {
                tracing::warn!("mail ack wait {id}: {e}");
            }
        }
        MailCmd::TxStuck { id, gen } => on_tx_stuck(ctx, &id, gen),
    }
}

async fn send_one(ctx: &TxCtx, id: &str) -> Result<()> {
    let Some(row) = ctx.store.get(id)? else {
        return Ok(());
    };
    let cfg = ctx.cfg.lock().clone();
    match gateway::local_send_path(cfg.mode) {
        SendPath::Blocked => Err(Error::config(
            "Email needs an internet hop. Radio-only cannot reach the mail gateway.",
        )),
        SendPath::Hub => hub_send(ctx, &row).await,
        SendPath::Rf => rf_send(ctx, &row).await,
    }
}

/// Map flag only. Callsigns, never the internet address.
fn note_mail(ctx: &TxCtx, origin: &str, dest: &str) {
    let origin = origin.trim().to_ascii_uppercase();
    if !crate::proto::is_plausible_callsign(&origin) {
        return;
    }
    let dest_cs = dest.trim().to_ascii_uppercase();
    let dest = if crate::proto::is_plausible_callsign(&dest_cs) && dest_cs != origin {
        Some(dest_cs)
    } else {
        None
    };
    let _ = ctx.tel.send(crate::telemetry::TelemetryEvent {
        ts: chrono::Utc::now().timestamp().max(0) as u64,
        kind: "mail".into(),
        origin: Some(origin),
        dest,
        hops: None,
        snr: None,
        msgid: None,
        band: None,
    });
}

async fn hub_send(ctx: &TxCtx, row: &MailRow) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    if !ctx.snap.lock().hub_ok {
        return Err(Error::Net("Hub is down.".into()));
    }
    let base = hub::http_base_from_hub(&cfg.hub.url);
    let copy = ctx.store.copy(&cfg.callsign)?;
    let bcc = if copy.enabled && copy.confirmed {
        Some(copy.address)
    } else {
        None
    };
    let body = hub::send_body(
        &row.from_addr,
        &row.to_addr,
        &row.subject,
        &row.body,
        bcc.as_deref(),
    );
    let v = hub::signed_post(&base, "/api/v1/mail/send", &ctx.keys, &cfg.callsign, &body).await?;
    if gateway::resend_accepted(v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false)) {
        ctx.store.set_outcome(&row.id, "sent", "sent", "")?;
        tracing::info!("mail accepted {}", row.id);
        note_mail(ctx, &cfg.callsign, "");
        Ok(())
    } else {
        let err = "Hub did not accept the message.";
        ctx.store.set_outcome(&row.id, "outbox", "failed", err)?;
        tracing::info!("mail failed {}: {err}", row.id);
        Err(Error::Net(err.into()))
    }
}

async fn rf_send(ctx: &TxCtx, row: &MailRow) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    let gateway_cs = cfg.mail.gateway.trim().to_ascii_uppercase();
    if gateway_cs.is_empty() {
        return Err(Error::config("Set a mail gateway callsign in setup."));
    }
    let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::HfPoor);
    let vox = cfg.modem.is_vox();
    let rung = mail_rung(preset, row.retries);
    let mtu = rung.payload_bytes() as usize;
    let meta = MailMeta {
        from: row.from_addr.clone(),
        to: row.to_addr.clone(),
        subject: row.subject.clone(),
        dest: gateway_cs.clone(),
        via: gateway_cs.clone(),
        // Same flag `compose` hashed into the id. PTT does not change it.
        ack: row.ack,
        ..MailMeta::default()
    };
    let wires_all =
        proto::chunk_to_limit(&row.id, &meta, &row.body, mtu.min(crate::proto::MAX_BODY));
    let total = wires_all.len().max(1) as u32;
    if row.chunks == 0 {
        ctx.store.set_chunks(&row.id, total)?;
    }
    let total = if row.chunks == 0 { total } else { row.chunks };
    let missing = missing_chunk_idxs(total, &row.got);
    let wires: Vec<_> = wires_all
        .into_iter()
        .filter(|w| missing.contains(&w.idx))
        .collect();
    if wires.is_empty() {
        return Ok(());
    }
    let mut frames = Vec::new();
    for w in &wires {
        frames.extend(proto::frames_for_mtu(&proto::encode_chunk(w), mtu)?);
    }
    let widest = frames.iter().map(Vec::len).max().unwrap_or(0);
    if widest > mtu {
        return Err(Error::protocol("mail frame wider than the rung"));
    }
    ctx.store.set_state(&row.id, "outbox", "tx")?;
    arm_tx_stuck(ctx, &row.id);
    let cap = gateway::mail_attempt_cap(cfg.rf.max_retries);
    if let Err(e) = burst(
        ctx,
        &frames,
        preset,
        rung,
        vox,
        cfg.modem.vox_lead_ms,
        cfg.modem.vox_tail_ms,
    )
    .await
    {
        tracing::warn!("mail key {} failed: {e}", row.id);
        let n = ctx
            .store
            .bump_retry(&row.id)
            .unwrap_or(row.retries.saturating_add(1));
        if n >= cap {
            fail_mail(ctx, &row.id, gateway::NO_GATEWAY_ACK);
        } else {
            silence_wait(ctx, &row.id);
            let tx = ctx.cmd_tx.clone();
            let id = row.id.clone();
            let wait = Duration::from_secs(gateway::ack_backoff_secs(n));
            tokio::spawn(async move {
                tokio::time::sleep(wait).await;
                let _ = tx.send(MailCmd::Send { id }).await;
            });
            tracing::info!("mail retry {} try {n}/{cap}", row.id);
        }
        return Err(e);
    }
    note_mail(ctx, &cfg.callsign, &gateway_cs);
    tracing::info!(
        "mail keyed {} to {gateway_cs} ({} chunks, try {}/{cap})",
        row.id,
        wires.len(),
        row.retries + 1
    );
    arm_wait(
        ctx,
        &row.id,
        Duration::from_secs(gateway::ack_backoff_secs(row.retries)),
    );
    Ok(())
}

fn silence_wait(ctx: &TxCtx, id: &str) {
    let mut waits = ctx.waits.lock();
    let n = waits.get(id).copied().unwrap_or(0).wrapping_add(1);
    waits.insert(id.to_string(), n);
}

fn arm_tx_stuck(ctx: &TxCtx, id: &str) {
    let gen = {
        let mut waits = ctx.waits.lock();
        let n = waits.get(id).copied().unwrap_or(0).wrapping_add(1);
        waits.insert(id.to_string(), n);
        n
    };
    let tx = ctx.cmd_tx.clone();
    let id = id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(gateway::TX_STUCK_AFTER_SECS)).await;
        let _ = tx.send(MailCmd::TxStuck { id, gen }).await;
    });
}

fn on_tx_stuck(ctx: &TxCtx, id: &str, gen: u64) {
    if ctx.waits.lock().get(id).copied() != Some(gen) {
        return;
    }
    let Ok(Some(row)) = ctx.store.get(id) else {
        return;
    };
    if let Some(error) = gateway::tx_stuck_error(&row.state) {
        fail_mail(ctx, id, error);
    }
}

fn arm_wait(ctx: &TxCtx, id: &str, after: Duration) {
    let gen = {
        let mut waits = ctx.waits.lock();
        let n = waits.get(id).copied().unwrap_or(0).wrapping_add(1);
        waits.insert(id.to_string(), n);
        n
    };
    let tx = ctx.cmd_tx.clone();
    let id = id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(after).await;
        let _ = tx.send(MailCmd::AckWait { id, gen }).await;
    });
}

fn fail_mail(ctx: &TxCtx, id: &str, error: &str) {
    silence_wait(ctx, id);
    let _ = ctx.store.set_outcome(id, "outbox", "failed", error);
    tracing::info!("mail failed {id}: {error}");
}

fn accept_mail(ctx: &TxCtx, id: &str) {
    silence_wait(ctx, id);
    let _ = ctx.store.set_outcome(id, "sent", "sent", "");
    tracing::info!("mail accepted {id}");
}

async fn on_ack_wait(ctx: &TxCtx, id: &str, gen: u64) -> Result<()> {
    if ctx.waits.lock().get(id).copied() != Some(gen) {
        return Ok(());
    }
    let Some(row) = ctx.store.get(id)? else {
        return Ok(());
    };
    let cap = gateway::mail_attempt_cap(ctx.cfg.lock().rf.max_retries);
    match gateway::on_ack_timeout(&row.state, row.retries, cap) {
        None => Ok(()),
        Some(gateway::AckTimeout::GiveUp { error }) => {
            fail_mail(ctx, id, error);
            Ok(())
        }
        Some(gateway::AckTimeout::Retry) => {
            let n = ctx
                .store
                .bump_retry(id)
                .unwrap_or(row.retries.saturating_add(1));
            tracing::info!("mail retry {id} try {n}/{cap}");
            let Some(row) = ctx.store.get(id)? else {
                return Ok(());
            };
            rf_send(ctx, &row).await
        }
    }
}

async fn send_control(ctx: &TxCtx, op: MailOp, mut meta: MailMeta, payload: &str) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    if cfg.mode != Mode::RadioPlus {
        return sync_hub(ctx).await;
    }
    let gateway_cs = cfg.mail.gateway.trim().to_ascii_uppercase();
    if gateway_cs.is_empty() {
        return Err(Error::config("Set a mail gateway callsign in setup."));
    }
    meta.dest = gateway_cs.clone();
    meta.via = gateway_cs;
    meta.from = cfg.callsign.clone();
    let payload = if matches!(op, MailOp::ListReq | MailOp::GetReq) {
        pull_auth(ctx, &meta.ids)
    } else {
        payload.to_string()
    };
    let id = proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, &payload, false);
    let wire = MailWire {
        op,
        mail_id: id,
        idx: 0,
        count: 1,
        meta,
        payload,
    };
    let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::HfPoor);
    let rung = mail_rung(preset, 0);
    let mtu = rung.payload_bytes() as usize;
    let frames = proto::frames_for_mtu(&proto::encode_chunk(&wire), mtu)?;
    burst(
        ctx,
        &frames,
        preset,
        rung,
        cfg.modem.is_vox(),
        cfg.modem.vox_lead_ms,
        cfg.modem.vox_tail_ms,
    )
    .await
}

async fn burst(
    ctx: &TxCtx,
    frames: &[Vec<u8>],
    preset: Preset,
    rung: Rung,
    vox: bool,
    lead_ms: u32,
    tail_ms: u32,
) -> Result<()> {
    let ports = (ctx.ports)();
    let Some(kiss) = ports.kiss.clone() else {
        return Err(Error::Modem("Radio is not running.".into()));
    };
    let mut paused = false;
    let mut snapshot = None;
    let mut before = 0u64;
    let result = async {
        for step in lease::lease_sequence() {
            match *step {
                "wait_idle" => {
                    if !lease::wait_idle(&ports, Duration::from_secs(45)).await {
                        return Err(Error::Modem("Chat is still using the radio.".into()));
                    }
                }
                "pause" => {
                    if let Some(air) = &ports.air {
                        air.pause();
                        paused = true;
                    }
                }
                "drain_before" => {
                    if let Some(c) = &ports.control {
                        before = lease::wait_tx_stable(c, Duration::from_secs(20)).await;
                    }
                }
                "snapshot" => {
                    snapshot = Some(lease::restore_config(preset));
                }
                "set_mail_config" => {
                    if let Some(c) = &ports.control {
                        c.set_config(lease::mail_tx_config(preset, rung)).await?;
                    }
                }
                "vox_lead" => {
                    if vox && lead_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(u64::from(lead_ms))).await;
                    }
                }
                "send_frames" => {
                    for frame in frames {
                        kiss.send(frame).await?;
                    }
                }
                "drain_after" => {
                    if let Some(c) = &ports.control {
                        let target = lease::drain_target_after(before, frames.len() as u64);
                        lease::wait_tx_count(c, target, Duration::from_secs(90)).await;
                    }
                    if vox && tail_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(u64::from(tail_ms))).await;
                    }
                }
                "restore_snapshot" => {
                    if let (Some(c), Some(snap)) = (&ports.control, snapshot.clone()) {
                        let _ = c.set_config(snap).await;
                    }
                }
                "resume" => {
                    if let Some(air) = &ports.air {
                        air.resume();
                        paused = false;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    .await;
    if paused {
        if let Some(air) = &ports.air {
            air.resume();
        }
        if let (Some(c), Some(snap)) = (&ports.control, snapshot) {
            let _ = c.set_config(snap).await;
        }
    }
    result
}

async fn on_incoming(ctx: &TxCtx, asm: &mut Assembler, incoming: Incoming) {
    let wire = match incoming {
        Incoming::Wire(w) => Some(w),
        Incoming::Slice {
            group,
            part,
            parts,
            data,
        } => finish_slice(asm, &group, part, parts, data),
    };
    let Some(wire) = wire else { return };
    remember_party(asm, &wire);
    maybe_chunk_ack(ctx, asm, &wire).await;
    if let Some((meta, body, op)) = finish_wire(asm, wire) {
        if let Err(e) = apply_mail(ctx, meta, body, op).await {
            tracing::warn!("mail inbound: {e}");
        }
    }
}

fn finish_slice(
    asm: &mut Assembler,
    group: &str,
    part: u16,
    parts: u16,
    data: String,
) -> Option<MailWire> {
    if parts == 0 || part >= parts {
        return None;
    }
    let buf = asm
        .slices
        .entry(group.to_string())
        .or_insert_with(|| vec![None; parts as usize]);
    if buf.len() != parts as usize {
        *buf = vec![None; parts as usize];
    }
    buf[part as usize] = Some(data);
    if buf.iter().any(|s| s.is_none()) {
        return None;
    }
    let pairs: Vec<(u16, String)> = buf
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.clone().map(|d| (i as u16, d)))
        .collect();
    asm.slices.remove(group);
    let bytes = proto::join_slices(&pairs).ok()?;
    proto::decode_chunk(&bytes).ok()
}

fn remember_party(asm: &mut Assembler, wire: &MailWire) {
    if !wire.meta.from.is_empty() {
        asm.from
            .insert(wire.mail_id.clone(), wire.meta.from.clone());
    }
    if !wire.meta.to.is_empty() {
        asm.to.insert(wire.mail_id.clone(), wire.meta.to.clone());
    }
}

async fn maybe_chunk_ack(ctx: &TxCtx, asm: &Assembler, wire: &MailWire) {
    if !matches!(wire.op, MailOp::Data | MailOp::VoxData) {
        return;
    }
    let cfg = ctx.cfg.lock().clone();
    if cfg.mode != Mode::InternetRadio {
        return;
    }
    let Some(from) = asm.from.get(&wire.mail_id) else {
        return;
    };
    let to = asm.to.get(&wire.mail_id).map(String::as_str).unwrap_or("");
    if proto::validate_internet_addr(to).is_err() {
        return;
    }
    let Some(origin) = proto::callsign_from_wcr(from) else {
        return;
    };
    if origin.eq_ignore_ascii_case(&cfg.callsign) {
        return;
    }
    tracing::info!(
        "mail received chunk {} {}/{} from {origin}",
        wire.mail_id,
        wire.idx + 1,
        wire.count
    );
    let ack = MailMeta {
        dest: origin,
        ids: vec![wire.mail_id.clone()],
        ..MailMeta::default()
    };
    if let Err(e) = reply_wire(ctx, MailOp::ChunkAck, ack, &wire.idx.to_string()).await {
        tracing::warn!("mail chunk ack {}: {e}", wire.mail_id);
    }
}

fn finish_wire(asm: &mut Assembler, wire: MailWire) -> Option<(MailMeta, String, MailOp)> {
    let id = format!("{:?}:{}", wire.op, wire.mail_id);
    let count = wire.count as usize;
    if count == 0 {
        return None;
    }
    let entry = asm.parts.entry(id.clone()).or_default();
    if !entry.iter().any(|e| e.idx == wire.idx) {
        entry.push(wire);
    }
    if entry.len() < count {
        return None;
    }
    let parts = asm.parts.remove(&id).unwrap_or_default();
    proto::assemble_chunks(parts).ok()
}

async fn apply_mail(ctx: &TxCtx, meta: MailMeta, body: String, op: MailOp) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    let us = cfg.callsign.to_ascii_uppercase();
    if !meta.dest.is_empty() && !meta.dest.eq_ignore_ascii_case(&us) {
        return Ok(());
    }
    match op {
        MailOp::CompleteAck => {
            if let Some(id) = meta.ids.first() {
                if let Some(err) = gateway::failure_text(&body) {
                    fail_mail(ctx, id, &err);
                } else {
                    accept_mail(ctx, id);
                    note_mail(ctx, &cfg.callsign, "");
                }
            }
            Ok(())
        }
        MailOp::ChunkAck => {
            let Some(id) = meta.ids.first() else {
                return Ok(());
            };
            let Ok(idx) = body.trim().parse::<u16>() else {
                return Ok(());
            };
            tracing::info!("mail gateway ack {id} chunk {idx}");
            let Some((have, total)) = ctx.store.note_chunk(id, idx)? else {
                return Ok(());
            };
            let Some(row) = ctx.store.get(id)? else {
                return Ok(());
            };
            if row.state == "sent" || row.state == "failed" {
                return Ok(());
            }
            if total > 0 && have >= total {
                ctx.store.set_outcome(id, "outbox", "gateway", "")?;
                tracing::info!("mail at gateway {id}");
                arm_wait(ctx, id, Duration::from_secs(45));
            } else {
                arm_wait(
                    ctx,
                    id,
                    Duration::from_secs(gateway::ack_backoff_secs(row.retries)),
                );
            }
            Ok(())
        }
        MailOp::HubSync => Ok(()),
        MailOp::ListHdr => {
            let rows: Vec<WaitHeader> = serde_json::from_str(&body).unwrap_or_default();
            {
                let mut n = ctx.checks.lock();
                *n = n.wrapping_add(1);
            }
            let notice = gateway::check_notice(rows.len());
            ctx.store.set_notice(notice);
            if notice.is_empty() {
                tracing::info!("mail check: {} waiting", rows.len());
            } else {
                tracing::info!("mail check: {notice}");
            }
            ctx.store.replace_waiting(&rows)
        }
        MailOp::ListReq if cfg.mode == Mode::InternetRadio => answer_list(ctx, &meta, &body).await,
        MailOp::GetReq if cfg.mode == Mode::InternetRadio => answer_get(ctx, &meta, &body).await,
        MailOp::Data | MailOp::VoxData => deliver(ctx, meta, body, op).await,
        _ => Ok(()),
    }
}

async fn deliver(ctx: &TxCtx, meta: MailMeta, body: String, op: MailOp) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    let from_call = proto::callsign_from_wcr(&meta.from);
    let internet_to = proto::validate_internet_addr(&meta.to).is_ok();
    let gateway_hop = cfg.mode == Mode::InternetRadio
        && internet_to
        && from_call.as_deref() != Some(cfg.callsign.as_str());
    if gateway_hop && gateway::gateway_may_post(cfg.mode, cfg.gateway.third_party_allow()) {
        let mail_id = proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, &body, meta.ack);
        tracing::info!(
            "mail received {mail_id} from {} to {} ({} B)",
            meta.from,
            meta.to,
            body.len()
        );
        if let Some(prev) = ctx.store.get(&mail_id)? {
            if prev.folder == "relay" && prev.state == "sent" {
                tracing::info!("mail {mail_id} already accepted; repeating the ack");
                if let Some(origin) = from_call {
                    send_complete(ctx, &origin, &mail_id, "ok").await?;
                }
                return Ok(());
            }
        }
        let base = hub::http_base_from_hub(&cfg.hub.url);
        // Relayed mail is not ours. The hub adds the sender's copy-to, if any.
        let payload = hub::send_body(&meta.from, &meta.to, &meta.subject, &body, None);
        let posted = hub::signed_post(
            &base,
            "/api/v1/mail/send",
            &ctx.keys,
            &cfg.callsign,
            &payload,
        )
        .await;
        let accepted = gateway::resend_accepted(
            posted
                .as_ref()
                .ok()
                .and_then(|v| v.get("ok").and_then(|x| x.as_bool()))
                .unwrap_or(false),
        );
        if accepted {
            let resend_id = posted
                .as_ref()
                .ok()
                .and_then(|v| v.get("id").and_then(|x| x.as_str()))
                .unwrap_or("");
            tracing::info!("mail posted {mail_id} to hub resend {resend_id}");
            let _ = ctx.store.insert(&MailRow {
                id: mail_id.clone(),
                folder: "relay".into(),
                from_addr: meta.from.clone(),
                to_addr: meta.to.clone(),
                subject: meta.subject.clone(),
                body: body.clone(),
                ts: chrono::Utc::now().timestamp(),
                state: "sent".into(),
                unread: false,
                ack: meta.ack,
                retries: 0,
                detail: String::new(),
                chunks: 0,
                got: String::new(),
            });
            if let Some(origin) = from_call.as_deref() {
                note_mail(ctx, origin, &cfg.callsign);
            }
            if let Some(origin) = from_call {
                send_complete(ctx, &origin, &mail_id, "ok").await?;
            }
        } else {
            let err = posted
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Hub did not accept the message.".into());
            tracing::info!("mail rejected {mail_id}: {err}");
            if let Some(origin) = from_call {
                let payload = format!("fail {err}");
                send_complete(ctx, &origin, &mail_id, &payload).await?;
            }
        }
        return Ok(());
    }
    let id = proto::compute_mail_id(
        &meta.from,
        &meta.to,
        &meta.subject,
        &body,
        meta.ack || op.wants_ack(),
    );
    let row = MailRow {
        id,
        folder: "inbox".into(),
        from_addr: meta.from,
        to_addr: meta.to,
        subject: meta.subject,
        body,
        ts: chrono::Utc::now().timestamp(),
        state: "sent".into(),
        unread: true,
        ack: meta.ack,
        retries: 0,
        detail: String::new(),
        chunks: 0,
        got: String::new(),
    };
    ctx.store.insert(&row)?;
    let us = cfg.callsign.to_ascii_uppercase();
    if let Some(from_cs) = proto::callsign_from_wcr(&row.from_addr) {
        note_mail(ctx, &from_cs, &us);
    } else {
        note_mail(ctx, &us, "");
    }
    Ok(())
}

async fn answer_list(ctx: &TxCtx, req: &MailMeta, auth: &str) -> Result<()> {
    let who = requester(req);
    let headers = fetch_for(ctx, &who, auth, &[]).await?;
    let payload = serde_json::to_string(&headers).unwrap_or_else(|_| "[]".into());
    let meta = MailMeta {
        dest: who,
        ..MailMeta::default()
    };
    reply_wire(ctx, MailOp::ListHdr, meta, &payload).await
}

async fn answer_get(ctx: &TxCtx, req: &MailMeta, auth: &str) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    let base = hub::http_base_from_hub(&cfg.hub.url);
    let who = requester(req);
    let mut body = serde_json::json!({ "ids": req.ids, "for": who });
    if let Ok(extra) = serde_json::from_str::<serde_json::Value>(auth) {
        if let Some(obj) = body.as_object_mut() {
            if let Some(more) = extra.as_object() {
                for (k, v) in more {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
    }
    let v = hub::signed_post(&base, "/api/v1/mail/fetch", &ctx.keys, &cfg.callsign, &body).await?;
    let messages = v
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    for msg in messages {
        let from = msg
            .get("from")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let to = msg
            .get("to")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let subject = msg
            .get("subject")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let body = msg
            .get("body")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let meta = MailMeta {
            from,
            to,
            subject,
            dest: who.clone(),
            ack: false,
            ..MailMeta::default()
        };
        let id = proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, &body, false);
        reply_chunks(ctx, MailOp::Data, &id, &meta, &body).await?;
    }
    Ok(())
}

fn requester(req: &MailMeta) -> String {
    let from = req.from.trim();
    if from.contains('@') {
        proto::callsign_from_wcr(from).unwrap_or_else(|| from.to_ascii_uppercase())
    } else {
        from.to_ascii_uppercase()
    }
}

/// Signed by the station whose inbox is being pulled. The gateway only forwards it.
fn pull_auth(ctx: &TxCtx, ids: &[String]) -> String {
    let call = ctx.cfg.lock().callsign.clone();
    let ts = chrono::Utc::now().timestamp();
    let sig = hex::encode(ctx.keys.sign_bytes(&proto::pull_token(&call, ts, ids)));
    serde_json::json!({
        "pk": ctx.keys.public_hex(),
        "sig": sig,
        "ts": ts,
    })
    .to_string()
}

async fn fetch_for(ctx: &TxCtx, who: &str, auth: &str, ids: &[String]) -> Result<Vec<WaitHeader>> {
    let cfg = ctx.cfg.lock().clone();
    let base = hub::http_base_from_hub(&cfg.hub.url);
    let mut body = serde_json::json!({ "for": who, "ids": ids });
    if let Ok(extra) = serde_json::from_str::<serde_json::Value>(auth) {
        if let (Some(obj), Some(more)) = (body.as_object_mut(), extra.as_object()) {
            for (k, v) in more {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    let v = hub::signed_post(&base, "/api/v1/mail/inbox", &ctx.keys, &cfg.callsign, &body).await?;
    let rows = v
        .get("headers")
        .and_then(|h| serde_json::from_value::<Vec<WaitHeader>>(h.clone()).ok())
        .unwrap_or_default();
    Ok(rows)
}

async fn fetch_headers(ctx: &TxCtx) -> Result<Vec<WaitHeader>> {
    let cfg = ctx.cfg.lock().clone();
    let base = hub::http_base_from_hub(&cfg.hub.url);
    let v = hub::signed_post(
        &base,
        "/api/v1/mail/inbox",
        &ctx.keys,
        &cfg.callsign,
        &serde_json::json!({}),
    )
    .await?;
    let rows = v
        .get("headers")
        .and_then(|h| serde_json::from_value::<Vec<WaitHeader>>(h.clone()).ok())
        .unwrap_or_default();
    Ok(rows)
}

async fn sync_hub(ctx: &TxCtx) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    if !matches!(cfg.mode, Mode::Internet | Mode::InternetRadio) || !ctx.snap.lock().hub_ok {
        return Ok(());
    }
    let headers = fetch_headers(ctx).await?;
    if headers.is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = headers.iter().map(|h| h.id.clone()).collect();
    let base = hub::http_base_from_hub(&cfg.hub.url);
    let v = hub::signed_post(
        &base,
        "/api/v1/mail/fetch",
        &ctx.keys,
        &cfg.callsign,
        &serde_json::json!({ "ids": ids }),
    )
    .await?;
    let messages = v
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    for msg in messages {
        let from = msg.get("from").and_then(|x| x.as_str()).unwrap_or("");
        let to = msg.get("to").and_then(|x| x.as_str()).unwrap_or("");
        let subject = msg.get("subject").and_then(|x| x.as_str()).unwrap_or("");
        let body = msg.get("body").and_then(|x| x.as_str()).unwrap_or("");
        let id = msg
            .get("id")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let id = if id.is_empty() {
            proto::compute_mail_id(from, to, subject, body, false)
        } else {
            id
        };
        if ctx.store.get(&id)?.is_some() {
            continue;
        }
        ctx.store.insert(&MailRow {
            id,
            folder: "inbox".into(),
            from_addr: from.into(),
            to_addr: to.into(),
            subject: subject.into(),
            body: body.into(),
            ts: chrono::Utc::now().timestamp(),
            state: "sent".into(),
            unread: true,
            ack: false,
            retries: 0,
            detail: String::new(),
            chunks: 0,
            got: String::new(),
        })?;
    }
    Ok(())
}

async fn send_complete(ctx: &TxCtx, origin: &str, mail_id: &str, payload: &str) -> Result<()> {
    let ack = MailMeta {
        dest: origin.to_string(),
        ids: vec![mail_id.to_string()],
        ack: true,
        ..MailMeta::default()
    };
    reply_wire(ctx, MailOp::CompleteAck, ack, payload).await
}

async fn reply_wire(ctx: &TxCtx, op: MailOp, meta: MailMeta, payload: &str) -> Result<()> {
    let id = if let Some(id) = meta.ids.first() {
        id.clone()
    } else {
        proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, payload, meta.ack)
    };
    reply_chunks(ctx, op, &id, &meta, payload).await
}

async fn reply_chunks(
    ctx: &TxCtx,
    op: MailOp,
    id: &str,
    meta: &MailMeta,
    payload: &str,
) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::HfPoor);
    let rung = mail_rung(preset, 0);
    let mtu = rung.payload_bytes() as usize;
    let mut wires = proto::chunk_to_limit(id, meta, payload, mtu.min(crate::proto::MAX_BODY));
    for w in &mut wires {
        w.op = op;
    }
    let mut frames = Vec::new();
    for w in &wires {
        frames.extend(proto::frames_for_mtu(&proto::encode_chunk(w), mtu)?);
    }
    burst(
        ctx,
        &frames,
        preset,
        rung,
        cfg.modem.is_vox(),
        cfg.modem.vox_lead_ms,
        cfg.modem.vox_tail_ms,
    )
    .await
}
