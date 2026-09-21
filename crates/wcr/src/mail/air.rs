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
        let (secs, _) = mail_airtime_secs(preset, bytes, turnaround_ms);
        format!(
            "{bytes} B · ~{} on {}",
            humanize_airtime(secs),
            preset.as_str()
        )
    }
}

pub fn confirm_air_line(secs: f64, bursts: usize, preset: &str) -> String {
    format!(
        "About {} on the air ({bursts} bursts @ {preset}). Retries can take longer.",
        humanize_airtime(secs)
    )
}

struct TxCtx {
    store: Arc<MailStore>,
    cfg: Arc<Mutex<Config>>,
    snap: Arc<SharedStatus>,
    keys: IdentityKeys,
    ports: Arc<dyn Fn() -> RadioPorts + Send + Sync>,
    cmd_tx: mpsc::Sender<MailCmd>,
    tel: broadcast::Sender<crate::telemetry::TelemetryEvent>,
}

struct Assembler {
    slices: HashMap<String, Vec<Option<String>>>,
    parts: HashMap<String, Vec<MailWire>>,
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
    };
    tokio::spawn(run(rx, ctx));
}

async fn run(mut rx: mpsc::Receiver<MailCmd>, ctx: TxCtx) {
    let mut asm = Assembler {
        slices: HashMap::new(),
        parts: HashMap::new(),
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
            let _ = send_control(ctx, MailOp::ListReq, MailMeta::default(), "").await;
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
        ctx.store.set_state(&row.id, "sent", "sent")?;
        note_mail(ctx, &cfg.callsign, "");
        Ok(())
    } else {
        Err(Error::Net("Hub did not accept the message.".into()))
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
        ack: vox,
        ..MailMeta::default()
    };
    let wires = proto::chunk_to_limit(&row.id, &meta, &row.body, mtu.min(crate::proto::MAX_BODY));
    let mut frames = Vec::new();
    for w in wires {
        frames.extend(proto::frames_for_mtu(&proto::encode_chunk(&w), mtu)?);
    }
    let widest = frames.iter().map(Vec::len).max().unwrap_or(0);
    if widest > mtu {
        return Err(Error::protocol("mail frame wider than the rung"));
    }
    ctx.store.set_state(&row.id, "outbox", "tx")?;
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
        let n = ctx
            .store
            .bump_retry(&row.id)
            .unwrap_or(row.retries.saturating_add(1));
        if n <= cfg.rf.max_retries {
            let tx = ctx.cmd_tx.clone();
            let id = row.id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(8)).await;
                let _ = tx.send(MailCmd::Send { id }).await;
            });
        }
        return Err(e);
    }
    note_mail(ctx, &cfg.callsign, &gateway_cs);
    if !vox {
        ctx.store.set_state(&row.id, "sent", "sent")?;
    }
    Ok(())
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
    let id = proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, payload, false);
    let wire = MailWire {
        op,
        mail_id: id,
        idx: 0,
        count: 1,
        meta,
        payload: payload.to_string(),
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
                ctx.store.set_state(id, "sent", "sent")?;
            }
            Ok(())
        }
        MailOp::ChunkAck | MailOp::HubSync => Ok(()),
        MailOp::ListHdr => {
            let rows: Vec<WaitHeader> = serde_json::from_str(&body).unwrap_or_default();
            ctx.store.replace_waiting(&rows)
        }
        MailOp::ListReq if cfg.mode == Mode::InternetRadio => answer_list(ctx, &meta).await,
        MailOp::GetReq if cfg.mode == Mode::InternetRadio => answer_get(ctx, &meta).await,
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
        let base = hub::http_base_from_hub(&cfg.hub.url);
        let copy = ctx.store.copy(&cfg.callsign)?;
        let bcc = if copy.enabled && copy.confirmed {
            Some(copy.address)
        } else {
            None
        };
        let payload = hub::send_body(&meta.from, &meta.to, &meta.subject, &body, bcc.as_deref());
        let v = hub::signed_post(
            &base,
            "/api/v1/mail/send",
            &ctx.keys,
            &cfg.callsign,
            &payload,
        )
        .await?;
        if gateway::resend_accepted(v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false)) {
            if let Some(origin) = from_call.as_deref() {
                note_mail(ctx, origin, &cfg.callsign);
            }
        }
        if meta.ack
            && gateway::resend_accepted(v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false))
        {
            if let Some(origin) = from_call {
                let ack = MailMeta {
                    dest: origin,
                    ids: vec![proto::compute_mail_id(
                        &meta.from,
                        &meta.to,
                        &meta.subject,
                        &body,
                        true,
                    )],
                    ack: true,
                    ..MailMeta::default()
                };
                let _ =
                    reply_wire(ctx, MailOp::CompleteAck, ack, &meta_id_body(&meta, &body)).await;
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

async fn answer_list(ctx: &TxCtx, req: &MailMeta) -> Result<()> {
    let headers = fetch_headers(ctx).await.unwrap_or_default();
    let payload = serde_json::to_string(&headers).unwrap_or_else(|_| "[]".into());
    let dest = if req.from.contains('@') {
        proto::callsign_from_wcr(&req.from).unwrap_or_else(|| req.from.clone())
    } else {
        req.from.clone()
    };
    let meta = MailMeta {
        dest,
        ..MailMeta::default()
    };
    reply_wire(ctx, MailOp::ListHdr, meta, &payload).await
}

async fn answer_get(ctx: &TxCtx, req: &MailMeta) -> Result<()> {
    let cfg = ctx.cfg.lock().clone();
    let base = hub::http_base_from_hub(&cfg.hub.url);
    let v = hub::signed_post(
        &base,
        "/api/v1/mail/fetch",
        &ctx.keys,
        &cfg.callsign,
        &serde_json::json!({ "ids": req.ids }),
    )
    .await?;
    let messages = v
        .get("messages")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let dest = if req.from.contains('@') {
        proto::callsign_from_wcr(&req.from).unwrap_or_else(|| req.from.clone())
    } else {
        req.from.clone()
    };
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
            dest: dest.clone(),
            ack: false,
            ..MailMeta::default()
        };
        let id = proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, &body, false);
        let _ = reply_chunks(ctx, MailOp::Data, &id, &meta, &body).await;
    }
    Ok(())
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
        })?;
    }
    Ok(())
}

fn meta_id_body(meta: &MailMeta, body: &str) -> String {
    proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, body, meta.ack)
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
        false,
        cfg.modem.vox_lead_ms,
        cfg.modem.vox_tail_ms,
    )
    .await
}
