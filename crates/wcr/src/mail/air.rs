//! SPDX-License-Identifier: Apache-2.0
//! Email-only TX. Frames go to KISS directly. They never enter the chat air queue.

use super::gateway::{self, SendPath};
use super::hub;
use super::lease::{self, RadioPorts};
use super::proto::{self, Incoming, MailMeta, MailOp, MailWire};
use super::store::{GetProgress, MailRow, MailStore, WaitHeader};
use super::MailCmd;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::modes::Mode;
use crate::presets::{Preset, Rung};
use crate::proto::IdentityKeys;
use crate::status::SharedStatus;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Start at the preset ladder and only step more robust. Never a faster rung.
pub fn mail_rung(preset: Preset, retries: u32) -> Rung {
    let start = preset.ladder_start();
    let idx = (start + retries as usize).min(preset.ladder_end());
    Rung::from_index(idx)
}

/// The message the airtime hint and the sender both split. Chunk boundaries
/// follow this body and header, not a stand-in of the same length.
pub struct AirtimeMail<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub body: &'a str,
    pub gateway: &'a str,
    pub ack: bool,
}

fn plan_limit(preset: Preset) -> usize {
    (Rung::from_index(preset.ladder_start()).payload_bytes() as usize).min(crate::proto::MAX_BODY)
}

/// Chunk at the preset's ladder-start payload. A later retry uses a slower
/// rung for the modem, and may slice a frame, but it does not re-split the
/// body. Re-splitting on that smaller payload stamped the byte length as the
/// chunk total.
pub(super) fn plan_mail_wires(preset: Preset, id: &str, mail: &AirtimeMail<'_>) -> Vec<MailWire> {
    let meta = MailMeta {
        from: mail.from.to_string(),
        to: mail.to.to_string(),
        subject: mail.subject.to_string(),
        dest: mail.gateway.to_string(),
        via: mail.gateway.to_string(),
        ack: mail.ack,
        ..MailMeta::default()
    };
    proto::chunk_to_limit(id, &meta, mail.body, plan_limit(preset))
}

pub fn mail_airtime_secs(
    preset: Preset,
    mail: &AirtimeMail<'_>,
    turnaround_ms: u32,
) -> (f64, usize) {
    // A real mail id is 64 hex chars, and the first chunk carries the header.
    let id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let wires = plan_mail_wires(preset, id, mail);
    let mtu = plan_limit(preset);
    let mut lens = Vec::new();
    for w in &wires {
        if let Ok(parts) = proto::frames_for_mtu(&proto::encode_chunk(w), mtu) {
            lens.extend(parts.into_iter().map(|p| p.len()));
        }
    }
    // The hint's burst count is the chunk count the send log prints. Airtime
    // still uses every keyed frame, including a header that had to be sliced.
    let n = wires.len().max(1);
    (keyed_airtime_secs(preset, lens, turnaround_ms), n)
}

pub fn burst_label(n: usize) -> String {
    if n == 1 {
        "1 burst".to_string()
    } else {
        format!("{n} bursts")
    }
}

/// Airtime of the frames actually keyed, including preset overhead.
pub fn keyed_airtime_secs(
    preset: Preset,
    frame_lens: impl IntoIterator<Item = usize>,
    turnaround_ms: u32,
) -> f64 {
    let mut bytes = 0usize;
    let mut n = 0usize;
    for len in frame_lens {
        bytes += len;
        n += 1;
    }
    let n = n.max(1);
    let bps = preset.bitrate_bps().max(1) as f64;
    let tx = (bytes as f64) * 8.0 / bps;
    tx + n as f64 * (preset.overhead_ms() + turnaround_ms) as f64 / 1000.0
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

pub fn airtime_hint(
    mail: &AirtimeMail<'_>,
    preset: Preset,
    hub_only: bool,
    turnaround_ms: u32,
) -> String {
    let bytes = mail.body.len();
    if hub_only {
        format!("{bytes} B · hub only")
    } else {
        let (secs, n) = mail_airtime_secs(preset, mail, turnaround_ms);
        let warn = if secs >= 20.0 || bytes >= 1500 {
            " · not Sent until the gateway accepts it"
        } else {
            ""
        };
        format!(
            "{bytes} B · ~{} on {} ({}){warn}",
            humanize_airtime(secs),
            preset.as_str(),
            burst_label(n)
        )
    }
}

pub fn confirm_air_line(secs: f64, bursts: usize, preset: &str) -> String {
    format!(
        "About {} on the air ({} @ {preset}). Not Sent until the gateway accepts it. Retries can take longer.",
        humanize_airtime(secs),
        burst_label(bursts)
    )
}

/// Chunk indexes still waiting for a gateway ack. Empty `got` means none yet.
pub fn missing_chunk_idxs(total: u32, got: &str) -> Vec<u16> {
    let have: std::collections::HashSet<u16> = got
        .split(',')
        .filter_map(|s| s.parse::<u16>().ok())
        .collect();
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
    /// True while `burst` holds the radio. Sampled when a frame arrives.
    keying: Arc<AtomicBool>,
    waits: Mutex<HashMap<String, u64>>,
    checks: Mutex<u64>,
    quiet: Mutex<u64>,
    gets: Mutex<u64>,
}

struct Assembler {
    slices: HashMap<String, Vec<Option<String>>>,
    /// op+mail id → index → wire. One mail is one bucket. The header total is not a key.
    buckets: HashMap<String, HashMap<u16, MailWire>>,
    from: HashMap<String, String>,
    to: HashMap<String, String>,
    /// Complete messages waiting out the quiet gap before the gateway keys.
    ready: Vec<(MailMeta, String, MailOp)>,
    /// Quiet gaps that ended while a frame was still being received.
    quiet_defers: u8,
    /// Mail id → sender callsign: a chunk arrived for mail the hub already
    /// accepted. The gateway repeats its acceptance once the channel is quiet.
    reack: HashMap<String, String>,
    /// When the last mail frame arrived, and the spacing before it.
    last_rx: Option<std::time::Instant>,
    rx_spacing: f64,
}

impl Assembler {
    fn new() -> Self {
        Self {
            slices: HashMap::new(),
            buckets: HashMap::new(),
            from: HashMap::new(),
            to: HashMap::new(),
            ready: Vec::new(),
            quiet_defers: 0,
            reack: HashMap::new(),
            last_rx: None,
            rx_spacing: 0.0,
        }
    }

    /// A gap longer than the quiet cap is a new burst, not frame spacing.
    fn note_frame(&mut self, now: std::time::Instant) {
        if let Some(prev) = self.last_rx {
            let gap = now.saturating_duration_since(prev).as_secs_f64();
            self.rx_spacing = if gap < gateway::MAX_QUIET_GAP_SECS {
                gap
            } else {
                0.0
            };
        }
        self.last_rx = Some(now);
    }

    /// The longest time any incomplete mail's sender may still be keying the
    /// rest of its burst. `frame_secs` stands in for spacing not yet heard.
    fn tail_wait_secs(&self, frame_secs: f64) -> f64 {
        let spacing = if self.rx_spacing > 0.0 {
            self.rx_spacing
        } else {
            frame_secs
        };
        self.buckets
            .iter()
            .filter(|(key, _)| key.starts_with("Data:") || key.starts_with("VoxData:"))
            .filter_map(|(_, slot)| {
                let count = slot.values().map(|w| w.count).min()?;
                let highest = *slot.keys().filter(|i| **i < count).max()?;
                Some(gateway::tail_wait_secs(count, highest, spacing))
            })
            .fold(0.0, f64::max)
    }
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
    keying: Arc<AtomicBool>,
) {
    let ctx = TxCtx {
        store,
        cfg,
        snap,
        keys,
        ports,
        cmd_tx,
        tel,
        keying,
        waits: Mutex::new(HashMap::new()),
        checks: Mutex::new(0),
        quiet: Mutex::new(0),
        gets: Mutex::new(0),
    };
    tokio::spawn(run(rx, ctx));
}

async fn run(mut rx: mpsc::Receiver<MailCmd>, ctx: TxCtx) {
    let mut asm = Assembler::new();
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
            let rf = ctx.cfg.lock().mode == Mode::RadioPlus;
            if rf {
                let subject = ids
                    .first()
                    .and_then(|id| ctx.store.waiting().ok()?.into_iter().find(|w| &w.id == id))
                    .map(|w| w.subject)
                    .unwrap_or_default();
                ctx.store.set_get_progress(GetProgress {
                    state: "asking".into(),
                    subject,
                    asked: ids.join(","),
                    ..GetProgress::default()
                });
                tracing::info!("mail get asked {} via the gateway", ids.join(","));
                arm_get_quiet(ctx);
            }
            let meta = MailMeta {
                ids,
                ..MailMeta::default()
            };
            if let Err(e) = send_control(ctx, MailOp::GetReq, meta, "").await {
                if rf {
                    fail_get(ctx, &e.to_string());
                }
            }
        }
        MailCmd::GetQuiet { gen } => {
            if *ctx.gets.lock() != gen || !ctx.store.get_progress().active() {
                return;
            }
            fail_get(ctx, "No reply from the gateway.");
        }
        MailCmd::Inbound {
            incoming,
            during_burst,
        } => on_incoming(ctx, asm, incoming, during_burst).await,
        MailCmd::SyncHub => {
            let _ = sync_hub(ctx).await;
        }
        MailCmd::AckWait { id, gen } => {
            if let Err(e) = on_ack_wait(ctx, &id, gen).await {
                tracing::warn!("mail ack wait {id}: {e}");
            }
        }
        MailCmd::TxStuck { id, gen } => on_tx_stuck(ctx, &id, gen),
        MailCmd::Quiet { gen } => on_quiet(ctx, asm, gen).await,
    }
}

/// A Get gives up after this long with nothing heard from the gateway. The
/// answer to a Get takes about a minute on RDM-600S, so the timer restarts
/// on every chunk.
const GET_SILENT: Duration = Duration::from_secs(90);

fn arm_get_quiet(ctx: &TxCtx) {
    let gen = {
        let mut n = ctx.gets.lock();
        *n = n.wrapping_add(1);
        *n
    };
    let tx = ctx.cmd_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(GET_SILENT).await;
        let _ = tx.send(MailCmd::GetQuiet { gen }).await;
    });
}

fn fail_get(ctx: &TxCtx, why: &str) {
    let mut p = ctx.store.get_progress();
    p.state = "failed".into();
    p.detail = why.to_string();
    ctx.store.set_get_progress(p);
    tracing::info!("mail get failed: {why}");
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
        &row.id,
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
    let planned = AirtimeMail {
        from: &row.from_addr,
        to: &row.to_addr,
        subject: &row.subject,
        body: &row.body,
        gateway: &gateway_cs,
        // Same flag `compose` hashed into the id. PTT does not change it.
        ack: row.ack,
    };
    let wires_all = plan_mail_wires(preset, &row.id, &planned);
    let total = wires_all.len().max(1) as u32;
    if row.chunks == 0 {
        ctx.store.set_chunks(&row.id, total)?;
    }
    let total = if row.chunks == 0 { total } else { row.chunks };
    let missing = missing_chunk_idxs(total, &row.got);
    let probe = gateway::probe_only(row.retries, !row.got.trim().is_empty(), missing.len());
    let mut chunks = Vec::new();
    for w in wires_all.iter().filter(|w| missing.contains(&w.idx)) {
        chunks.push((w.idx, proto::frames_for_mtu(&proto::encode_chunk(w), mtu)?));
    }
    if probe {
        // The first chunk is the only one that names this station. A gateway
        // that missed it holds the rest but cannot address a report, so the
        // probe is the first chunk. With it, the gateway answers with its
        // acceptance or what it still lacks.
        chunks.retain(|(idx, _)| *idx == 0);
        chunks.truncate(1);
    }
    if chunks.is_empty() {
        return Ok(());
    }
    let widest = chunks
        .iter()
        .flat_map(|(_, f)| f.iter().map(Vec::len))
        .max()
        .unwrap_or(0);
    if widest > mtu {
        return Err(Error::protocol("mail frame wider than the rung"));
    }
    ctx.store.set_state(&row.id, "outbox", "tx")?;
    let planned_secs = keyed_airtime_secs(
        preset,
        chunks.iter().flat_map(|(_, f)| f.iter().map(Vec::len)),
        cfg.rf.turnaround_ms,
    );
    arm_tx_stuck(ctx, &row.id, gateway::tx_stuck_after_secs(planned_secs));
    let cap = gateway::mail_attempt_cap(cfg.rf.max_retries);
    let frame_secs = keyed_airtime_secs(preset, std::iter::once(mtu), cfg.rf.turnaround_ms);
    let started = std::time::Instant::now();
    // A have-report can land while the lease waits for the channel. Key only
    // what the gateway still lacks at that moment, not what it lacked when
    // this try was scheduled.
    let id = row.id.clone();
    let plan = move || {
        let Some(now) = ctx.store.get(&id).ok().flatten() else {
            return KeyPlan::default();
        };
        if gateway::is_finished(&now.state) {
            return KeyPlan::default();
        }
        let still = missing_chunk_idxs(total, &now.got);
        let mut keyed = KeyPlan::default();
        for (idx, frames) in chunks.iter().filter(|(i, _)| still.contains(i)) {
            keyed.chunks.push(*idx);
            keyed.frames.extend(frames.iter().cloned());
        }
        keyed
    };
    let keyed = match burst(
        ctx,
        &plan,
        preset,
        rung,
        vox,
        cfg.modem.vox_lead_ms,
        cfg.modem.vox_tail_ms,
    )
    .await
    {
        Ok(keyed) => keyed,
        Err(e) => {
            tracing::warn!("mail key {} failed: {e}", row.id);
            if let Some(text) = gateway::radio_down_text(&e.to_string()) {
                fail_mail(ctx, &row.id, text);
                return Err(e);
            }
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
                tracing::info!("mail retry {} {}", row.id, gateway::mail_try_label(n, cap));
            }
            return Err(e);
        }
    };
    if keyed.chunks.is_empty() {
        let state = ctx.store.get(&row.id)?.map(|r| r.state).unwrap_or_default();
        if gateway::is_finished(&state) {
            return Ok(());
        }
        tracing::info!(
            "mail {} has nothing left to key; waiting on the gateway",
            row.id
        );
        let wait = gateway::ack_backoff_secs(row.retries) as f64 + frame_secs;
        arm_wait(ctx, &row.id, Duration::from_secs_f64(wait));
        return Ok(());
    }
    note_mail(ctx, &cfg.callsign, &gateway_cs);
    if probe {
        tracing::info!(
            "mail keyed {} to {gateway_cs} (probe chunk {}/{total}, {})",
            row.id,
            keyed.chunks.first().map(|i| i + 1).unwrap_or(1),
            gateway::mail_try_label(row.retries, cap)
        );
    } else {
        tracing::info!(
            "mail keyed {} to {gateway_cs} ({} chunks, {})",
            row.id,
            keyed.chunks.len(),
            gateway::mail_try_label(row.retries, cap)
        );
    }
    let airtime = keyed_airtime_secs(
        preset,
        keyed.frames.iter().map(Vec::len),
        cfg.rf.turnaround_ms,
    );
    let spent = started.elapsed().as_secs_f64();
    let wait = gateway::retry_after_secs(airtime, spent, row.retries, frame_secs);
    arm_wait(ctx, &row.id, Duration::from_secs_f64(wait.max(1.0)));
    Ok(())
}

fn silence_wait(ctx: &TxCtx, id: &str) {
    let mut waits = ctx.waits.lock();
    let n = waits.get(id).copied().unwrap_or(0).wrapping_add(1);
    waits.insert(id.to_string(), n);
}

fn arm_tx_stuck(ctx: &TxCtx, id: &str, after_secs: u64) {
    let gen = {
        let mut waits = ctx.waits.lock();
        let n = waits.get(id).copied().unwrap_or(0).wrapping_add(1);
        waits.insert(id.to_string(), n);
        n
    };
    let tx = ctx.cmd_tx.clone();
    let id = id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(after_secs)).await;
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
            tracing::info!("mail retry {id} {}", gateway::mail_try_label(n, cap));
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
    let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::HfPoor);
    let rung = mail_rung(preset, 0);
    let mtu = rung.payload_bytes() as usize;
    let mut wires = proto::chunk_to_limit(&id, &meta, &payload, mtu.min(crate::proto::MAX_BODY));
    for w in &mut wires {
        w.op = op;
    }
    let mut frames = Vec::new();
    for w in &wires {
        frames.extend(proto::frames_for_mtu(&proto::encode_chunk(w), mtu)?);
    }
    burst(
        ctx,
        &KeyPlan::fixed(frames),
        preset,
        rung,
        cfg.modem.is_vox(),
        cfg.modem.vox_lead_ms,
        cfg.modem.vox_tail_ms,
    )
    .await
    .map(|_| ())
}

struct ClearKeying(Arc<AtomicBool>);

impl Drop for ClearKeying {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// What one lease keys: the chunk indexes and their frames, in air order.
#[derive(Default)]
struct KeyPlan {
    chunks: Vec<u16>,
    frames: Vec<Vec<u8>>,
}

impl KeyPlan {
    fn fixed(frames: Vec<Vec<u8>>) -> impl Fn() -> KeyPlan + Send + Sync {
        move || KeyPlan {
            chunks: Vec::new(),
            frames: frames.clone(),
        }
    }
}

/// Borrow the modem and key `plan()`. The plan is taken after the channel
/// is idle and chat has drained, so a resend keys only what is still missing
/// at that moment. An empty plan keys nothing and leaves the mode alone.
async fn burst(
    ctx: &TxCtx,
    plan: &(dyn Fn() -> KeyPlan + Send + Sync),
    preset: Preset,
    rung: Rung,
    vox: bool,
    lead_ms: u32,
    tail_ms: u32,
) -> Result<KeyPlan> {
    ctx.keying.store(true, Ordering::Release);
    let _clear = ClearKeying(Arc::clone(&ctx.keying));
    let ports = (ctx.ports)();
    let Some(kiss) = ports.kiss.clone() else {
        return Err(Error::Modem("Radio is not running.".into()));
    };
    let (turnaround_ms, chat_defer) = {
        let rf = &ctx.cfg.lock().rf;
        (
            rf.turnaround_ms,
            Duration::from_millis(u64::from(rf.max_defer_ms)) + Duration::from_secs(5),
        )
    };
    let mut held = false;
    let mut snapshot = None;
    let mut mode_set = false;
    let mut before = 0u64;
    let mut keyed = KeyPlan::default();
    let result = async {
        for step in lease::lease_sequence() {
            match *step {
                "wait_idle" => {
                    if !lease::wait_idle(&ports, Duration::from_secs(45)).await {
                        return Err(Error::Modem("Chat is still using the radio.".into()));
                    }
                }
                "wait_clear" => {
                    lease::wait_clear(&ports, lease::WAIT_CLEAR_CAP).await;
                }
                "pause" => {
                    if let Some(air) = &ports.air {
                        air.hold_for_mail();
                        held = true;
                    }
                    lease::wait_ack_done(&ports, Duration::from_secs(20)).await;
                    lease::wait_chat_handed(&ports, chat_defer).await;
                }
                "drain_before" => {
                    if let Some(c) = &ports.control {
                        before = lease::wait_tx_stable(c, Duration::from_secs(20)).await;
                    }
                }
                "snapshot" => {
                    snapshot = Some(lease::restore_config(preset));
                    keyed = plan();
                    if keyed.frames.is_empty() {
                        return Ok(());
                    }
                }
                "set_mail_config" => {
                    if let Some(c) = &ports.control {
                        mode_set = true;
                        c.set_config(lease::mail_tx_config(preset, rung)).await?;
                    }
                }
                "vox_lead" => {
                    if vox && lead_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(u64::from(lead_ms))).await;
                    }
                }
                "send_frames" => {
                    for frame in &keyed.frames {
                        kiss.send(frame).await?;
                    }
                }
                "drain_after" => {
                    if let Some(c) = &ports.control {
                        let target = lease::drain_target_after(before, keyed.frames.len() as u64);
                        let airtime = keyed_airtime_secs(
                            preset,
                            keyed.frames.iter().map(Vec::len),
                            turnaround_ms,
                        );
                        lease::wait_drained(c, target, Duration::from_secs_f64(airtime)).await;
                    }
                    if vox && tail_ms > 0 {
                        tokio::time::sleep(Duration::from_millis(u64::from(tail_ms))).await;
                    }
                }
                "restore_snapshot" => {
                    if let (Some(c), Some(snap)) = (&ports.control, snapshot.clone()) {
                        let _ = c.set_config(snap).await;
                        mode_set = false;
                    }
                }
                "resume" => {
                    if let Some(air) = &ports.air {
                        air.release_mail();
                        held = false;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    .await;
    if mode_set {
        if let (Some(c), Some(snap)) = (&ports.control, snapshot) {
            let _ = c.set_config(snap).await;
        }
    }
    if held {
        if let Some(air) = &ports.air {
            air.release_mail();
        }
    }
    result.map(|()| keyed)
}

async fn on_incoming(ctx: &TxCtx, asm: &mut Assembler, incoming: Incoming, during_burst: bool) {
    let gateway = ctx.cfg.lock().mode == Mode::InternetRadio;
    if gateway {
        asm.note_frame(std::time::Instant::now());
    }
    let wire = match incoming {
        Incoming::Wire(w) => Some(w),
        Incoming::Slice {
            group,
            part,
            parts,
            data,
        } => finish_slice(asm, &group, part, parts, data),
    };
    let Some(wire) = wire else {
        // A slice of a chunk still on the air. Keying now would deafen the
        // gateway to the rest of the burst.
        if gateway {
            arm_quiet(ctx, asm);
        }
        return;
    };
    remember_party(asm, &wire);
    log_chunk(ctx, asm, &wire);
    let gateway_reply = gateway
        && matches!(
            wire.op,
            MailOp::Data | MailOp::VoxData | MailOp::ListReq | MailOp::GetReq
        );
    if gateway_reply && matches!(wire.op, MailOp::Data | MailOp::VoxData) {
        if let Some(origin) = accepted_origin(ctx, &wire.mail_id) {
            tracing::info!(
                "mail chunk {} {}/{} ignored: already accepted",
                wire.mail_id,
                wire.idx + 1,
                wire.count
            );
            asm.buckets
                .remove(&format!("{:?}:{}", wire.op, wire.mail_id));
            asm.reack.insert(wire.mail_id, origin);
            arm_quiet(ctx, asm);
            return;
        }
    }
    let get_chunk = !gateway_reply
        && matches!(wire.op, MailOp::Data | MailOp::VoxData)
        && ctx.store.get_progress().active();
    let chunk_key = format!("{:?}:{}", wire.op, wire.mail_id);
    let chunk_total = wire.count;
    let absorbed = absorb(asm, wire);
    if get_chunk && proto::chunk_acceptable(0, chunk_total) {
        let got = match &absorbed {
            Absorb::Ready(..) => chunk_total,
            _ => asm
                .buckets
                .get(&chunk_key)
                .map(|b| b.len() as u16)
                .unwrap_or(0),
        };
        if ctx.store.note_get_chunks(got, chunk_total) {
            tracing::info!("mail get chunk {got}/{chunk_total}");
        }
        arm_get_quiet(ctx);
    }
    match absorbed {
        Absorb::Rejected {
            mail_id,
            idx,
            count,
        } => {
            tracing::warn!("mail drop chunk {mail_id} {idx}/{count} (bad total)");
            if gateway_reply {
                arm_quiet(ctx, asm);
            }
        }
        Absorb::Pending => {
            if gateway_reply {
                arm_quiet(ctx, asm);
            }
        }
        Absorb::Ready(meta, body, op) => {
            if gateway_reply {
                push_ready(asm, (meta, body, op));
                arm_quiet(ctx, asm);
            } else if let Err(e) = apply_mail(ctx, meta, body, op, during_burst).await {
                tracing::warn!("mail inbound: {e}");
            }
        }
    }
}

/// Sender of a mail this gateway already handed to the hub, which Resend
/// accepted. Any chunk of it, from any try, is answered with that acceptance.
fn accepted_origin(ctx: &TxCtx, mail_id: &str) -> Option<String> {
    let row = ctx.store.get(mail_id).ok().flatten()?;
    if row.folder != "relay" || row.state != "sent" {
        return None;
    }
    proto::callsign_from_wcr(&row.from_addr)
}

enum Absorb {
    Rejected {
        mail_id: String,
        idx: u16,
        count: u16,
    },
    Pending,
    Ready(MailMeta, String, MailOp),
}

fn absorb(asm: &mut Assembler, wire: MailWire) -> Absorb {
    if !proto::chunk_acceptable(wire.idx, wire.count) {
        return Absorb::Rejected {
            mail_id: wire.mail_id,
            idx: wire.idx,
            count: wire.count,
        };
    }
    let mail_id = wire.mail_id.clone();
    let key = format!("{:?}:{}", wire.op, wire.mail_id);
    let slot = asm.buckets.entry(key).or_default();
    match proto::put_chunk(slot, wire) {
        proto::ChunkPut::Rejected => Absorb::Rejected {
            mail_id,
            idx: 0,
            count: 0,
        },
        proto::ChunkPut::Pending => Absorb::Pending,
        proto::ChunkPut::Ready(parts) => match proto::assemble_chunks(parts) {
            Ok((mut meta, body, op)) => {
                if meta.ids.is_empty() {
                    meta.ids.push(mail_id);
                }
                Absorb::Ready(meta, body, op)
            }
            Err(e) => {
                tracing::warn!("mail assemble {mail_id}: {e}");
                Absorb::Pending
            }
        },
    }
}

fn push_ready(asm: &mut Assembler, item: (MailMeta, String, MailOp)) {
    let id = item.0.ids.first().cloned();
    if let Some(id) = id {
        if asm
            .ready
            .iter()
            .any(|(meta, _, op)| *op == item.2 && meta.ids.first() == Some(&id))
        {
            return;
        }
    }
    asm.ready.push(item);
}

fn arm_quiet(ctx: &TxCtx, asm: &Assembler) {
    let gen = {
        let mut n = ctx.quiet.lock();
        *n = n.wrapping_add(1);
        *n
    };
    let cfg = ctx.cfg.lock().clone();
    let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::HfPoor);
    let mtu = Rung::from_index(preset.ladder_start()).payload_bytes() as usize;
    let frame = keyed_airtime_secs(preset, std::iter::once(mtu), cfg.rf.turnaround_ms);
    // A sender held up mid-burst must not have its next chunks keyed over.
    let gap = gateway::quiet_gap_heard_secs(frame, asm.rx_spacing) + asm.tail_wait_secs(frame);
    let gap = Duration::from_secs_f64(gap);
    let tx = ctx.cmd_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(gap).await;
        let _ = tx.send(MailCmd::Quiet { gen }).await;
    });
}

async fn on_quiet(ctx: &TxCtx, asm: &mut Assembler, gen: u64) {
    if *ctx.quiet.lock() != gen {
        return;
    }
    if gateway::quiet_defers(lease::channel_receiving(&(ctx.ports)()), asm.quiet_defers) {
        asm.quiet_defers += 1;
        arm_quiet(ctx, asm);
        return;
    }
    asm.quiet_defers = 0;
    let ready = std::mem::take(&mut asm.ready);
    let mut completed = std::collections::HashSet::new();
    for (meta, body, op) in ready {
        if let Some(id) = meta.ids.first() {
            completed.insert(id.clone());
        }
        if let Err(e) = apply_mail(ctx, meta, body, op, false).await {
            tracing::warn!("mail inbound: {e}");
        }
    }
    if ctx.cfg.lock().mode != Mode::InternetRadio {
        return;
    }
    for (mail_id, origin) in std::mem::take(&mut asm.reack) {
        if completed.contains(&mail_id) {
            continue;
        }
        tracing::info!("mail {mail_id} already accepted; repeating the ack");
        if let Err(e) = send_complete(ctx, &origin, &mail_id, "ok").await {
            tracing::warn!("mail ack {mail_id}: {e}");
        }
    }
    for report in pending_have_reports(asm, &completed) {
        let payload = proto::have_bitmap(report.count, &report.indexes);
        let meta = MailMeta {
            dest: report.origin,
            ids: vec![report.mail_id.clone()],
            ..MailMeta::default()
        };
        if let Err(e) = reply_wire(ctx, MailOp::ChunkAck, meta, &payload).await {
            tracing::warn!("mail have {}: {e}", report.mail_id);
        } else {
            tracing::info!(
                "mail have {} {}/{}",
                report.mail_id,
                report.indexes.len(),
                report.count
            );
        }
    }
}

struct HaveReport {
    mail_id: String,
    origin: String,
    count: u16,
    indexes: Vec<u16>,
}

/// Incomplete Data/VoxData mails. The count is the smallest total stored,
/// which is the chunk count when a byte-length label also arrived. A finished
/// set has already left the bucket.
fn pending_have_reports(
    asm: &Assembler,
    completed: &std::collections::HashSet<String>,
) -> Vec<HaveReport> {
    let mut out = Vec::new();
    for (key, slot) in &asm.buckets {
        let Some((op_name, mail_id)) = key.split_once(':') else {
            continue;
        };
        if op_name != "Data" && op_name != "VoxData" {
            continue;
        }
        if completed.contains(mail_id) {
            continue;
        }
        let Some(count) = slot.values().map(|w| w.count).min() else {
            continue;
        };
        let mut indexes: Vec<u16> = slot.keys().copied().filter(|i| *i < count).collect();
        indexes.sort_unstable();
        if indexes.is_empty() || indexes.len() >= usize::from(count) {
            continue;
        }
        let Some(from) = asm.from.get(mail_id) else {
            continue;
        };
        let Some(origin) = proto::callsign_from_wcr(from) else {
            continue;
        };
        out.push(HaveReport {
            mail_id: mail_id.to_string(),
            origin,
            count,
            indexes,
        });
    }
    out
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

fn log_chunk(ctx: &TxCtx, asm: &Assembler, wire: &MailWire) {
    if !matches!(wire.op, MailOp::Data | MailOp::VoxData) {
        return;
    }
    let cfg = ctx.cfg.lock().clone();
    if cfg.mode != Mode::InternetRadio {
        return;
    }
    let Some(from) = asm.from.get(&wire.mail_id) else {
        tracing::info!(
            "mail received chunk {} {}/{}, sender not known until chunk 1 arrives",
            wire.mail_id,
            wire.idx + 1,
            wire.count
        );
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
}

async fn apply_mail(
    ctx: &TxCtx,
    meta: MailMeta,
    body: String,
    op: MailOp,
    during_burst: bool,
) -> Result<()> {
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
            let Some(id) = meta.ids.first().cloned() else {
                return Ok(());
            };
            let indexes = if let Some((count, bits)) = proto::parse_have_bitmap(&body) {
                let Some(row) = ctx.store.get(&id)? else {
                    return Ok(());
                };
                if row.chunks != 0 && row.chunks != u32::from(count) {
                    tracing::info!(
                        "mail gateway ack {id} ignored bitmap for {count} (row {})",
                        row.chunks
                    );
                    return Ok(());
                }
                tracing::info!("mail gateway ack {id} has {}/{} chunks", bits.len(), count);
                bits
            } else if let Ok(idx) = body.trim().parse::<u16>() {
                tracing::info!("mail gateway ack {id} chunk {idx}");
                vec![idx]
            } else {
                return Ok(());
            };
            let mut have = 0u32;
            let mut total = 0u32;
            for idx in &indexes {
                if let Some((h, t)) = ctx.store.note_chunk(&id, *idx)? {
                    have = h;
                    total = t;
                }
            }
            let Some(row) = ctx.store.get(&id)? else {
                return Ok(());
            };
            if row.state == "sent" || row.state == "failed" {
                return Ok(());
            }
            if total > 0 && have >= total {
                ctx.store.set_outcome(&id, "outbox", "gateway", "")?;
                tracing::info!("mail at gateway {id}");
                arm_wait(ctx, &id, Duration::from_secs(45));
            } else if !indexes.is_empty()
                && gateway::progress_ack_retries(during_burst, have, total)
            {
                arm_wait(
                    ctx,
                    &id,
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
        let payload = hub::send_body(&meta.from, &meta.to, &meta.subject, &body, None, &mail_id);
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
                delivery: String::new(),
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
        ts: arrived_at(meta.ts),
        state: INBOX_STATE.into(),
        unread: true,
        ack: meta.ack,
        retries: 0,
        detail: String::new(),
        chunks: 0,
        got: String::new(),
        delivery: String::new(),
    };
    ctx.store.insert(&row)?;
    // The mail fetched by Get is in, so its waiting header goes.
    let _ = ctx.store.clear_waiting_for(&row.from_addr, &row.subject);
    let mut get = ctx.store.get_progress();
    if get.active() {
        get.state = "fetched".into();
        get.got = get.total;
        tracing::info!(
            "mail fetched {} from {} (inbox row {})",
            get.asked,
            row.from_addr,
            row.id
        );
        ctx.store.set_get_progress(get);
    }
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
    let headers = match fetch_for(ctx, &who, auth, &[]).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("mail list {who}: {e}");
            Vec::new()
        }
    };
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
            ts: msg.get("ts").and_then(|x| x.as_i64()).unwrap_or(0),
            ..MailMeta::default()
        };
        let id = proto::compute_mail_id(&meta.from, &meta.to, &meta.subject, &body, false);
        reply_chunks(ctx, MailOp::Data, &id, &meta, &body).await?;
    }
    Ok(())
}

/// State of fetched inbound mail. Not "sent": that painted [ok] in the Inbox.
const INBOX_STATE: &str = "received";

/// The Inbox time: when the mail reached the hub, or now from an older hub.
fn arrived_at(hub_ts: i64) -> i64 {
    if hub_ts > 0 {
        hub_ts
    } else {
        chrono::Utc::now().timestamp()
    }
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
            ts: arrived_at(msg.get("ts").and_then(|x| x.as_i64()).unwrap_or(0)),
            state: INBOX_STATE.into(),
            unread: true,
            ack: false,
            retries: 0,
            detail: String::new(),
            chunks: 0,
            got: String::new(),
            delivery: String::new(),
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
    reply_wire(ctx, MailOp::CompleteAck, ack, payload).await?;
    tracing::info!("mail ack keyed {mail_id} to {origin}");
    Ok(())
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
    // The wire id is the mail id. Leaving it out of the JSON keeps a
    // CompleteAck inside one hf-poor frame. The receiver copies it back.
    let mut on_air = meta.clone();
    on_air.ids.clear();
    on_air.ack = false;
    let mut wires = proto::chunk_to_limit(id, &on_air, payload, mtu.min(crate::proto::MAX_BODY));
    for w in &mut wires {
        w.op = op;
    }
    let mut frames = Vec::new();
    for w in &wires {
        frames.extend(proto::frames_for_mtu(&proto::encode_chunk(w), mtu)?);
    }
    burst(
        ctx,
        &KeyPlan::fixed(frames),
        preset,
        rung,
        cfg.modem.is_vox(),
        cfg.modem.vox_lead_ms,
        cfg.modem.vox_tail_ms,
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(idx: u16, count: u16) -> MailWire {
        MailWire {
            op: MailOp::VoxData,
            mail_id: "abc".into(),
            idx,
            count,
            meta: MailMeta::default(),
            payload: String::new(),
        }
    }

    #[test]
    fn have_report_uses_the_smaller_total_for_one_mail() {
        let mut asm = Assembler::new();
        let mut slot = HashMap::new();
        slot.insert(0u16, wire(0, 48));
        slot.insert(2u16, wire(2, 3900));
        asm.buckets.insert("VoxData:abc".into(), slot);
        asm.from
            .insert("abc".into(), "M7TJF@mail.weechatradio.com".into());
        let reports = pending_have_reports(&asm, &std::collections::HashSet::new());
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].origin, "M7TJF");
        assert_eq!(reports[0].count, 48);
        assert_eq!(reports[0].indexes, vec![0, 2]);
        let done = std::collections::HashSet::from(["abc".to_string()]);
        assert!(pending_have_reports(&asm, &done).is_empty());
    }

    #[test]
    fn the_quiet_gap_follows_the_frame_spacing_heard() {
        let mut asm = Assembler::new();
        let t0 = std::time::Instant::now();
        asm.note_frame(t0);
        assert_eq!(asm.rx_spacing, 0.0);
        asm.note_frame(t0 + Duration::from_millis(7_500));
        assert!((asm.rx_spacing - 7.5).abs() < 0.01);
        let gap = gateway::quiet_gap_heard_secs(4.0, asm.rx_spacing);
        assert!(gap > 7.5, "gap {gap} must outlast one RDM-300S chunk");
        asm.note_frame(t0 + Duration::from_secs(120));
        assert_eq!(asm.rx_spacing, 0.0, "a new burst is not frame spacing");
    }

    fn spaced_body(n: usize) -> String {
        let word = "hello mail ";
        let mut body = word.repeat(n / word.len());
        body.push_str(&"x".repeat(n - body.len()));
        body
    }

    fn sample_mail(body: &str) -> AirtimeMail<'_> {
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
    fn a_retry_plans_the_same_chunk_count_as_the_first_send() {
        let body = spaced_body(3900);
        let mail = sample_mail(&body);
        let id = "ab".repeat(32);
        let first = plan_mail_wires(Preset::HfPoor, &id, &mail);
        let total = first.len() as u32;
        assert!(total > 1 && total < 200, "ladder-start plan was {total}");
        assert!(first.iter().all(|w| u32::from(w.count) == total));
        let again = plan_mail_wires(Preset::HfPoor, &id, &mail);
        assert_eq!(again.len() as u32, total);
        let (_, hinted) = mail_airtime_secs(Preset::HfPoor, &mail, 250);
        assert_eq!(hinted, first.len());
        let trap = mail_rung(Preset::HfPoor, 2).payload_bytes() as usize;
        assert!(trap < plan_limit(Preset::HfPoor));
        let meta = MailMeta {
            from: mail.from.into(),
            to: mail.to.into(),
            subject: mail.subject.into(),
            dest: mail.gateway.into(),
            via: mail.gateway.into(),
            ack: mail.ack,
            ..MailMeta::default()
        };
        let bad = proto::chunk_to_limit(&id, &meta, &body, trap);
        assert!(
            bad.len() as u32 > total,
            "slower rung would re-split {total} into {}",
            bad.len()
        );
        let missing = missing_chunk_idxs(total, "0,7,14");
        let resend: Vec<_> = first.iter().filter(|w| missing.contains(&w.idx)).collect();
        assert!(resend.iter().all(|w| u32::from(w.count) == total));
        assert!(!resend
            .iter()
            .any(|w| w.idx == 0 || w.idx == 7 || w.idx == 14));
    }

    #[test]
    fn absorb_completes_when_a_resend_uses_the_byte_length() {
        let body = spaced_body(3900);
        let mail = sample_mail(&body);
        let id = proto::compute_mail_id(mail.from, mail.to, mail.subject, &body, mail.ack);
        let wires = plan_mail_wires(Preset::HfPoor, &id, &mail);
        let mut asm = Assembler::new();
        let missing: Vec<u16> = wires.iter().map(|w| w.idx).filter(|i| i % 7 == 0).collect();
        assert!(!missing.is_empty());
        for w in wires.iter().filter(|w| !missing.contains(&w.idx)) {
            assert!(
                matches!(absorb(&mut asm, w.clone()), Absorb::Pending),
                "partial set must not complete"
            );
        }
        let mut last = Absorb::Pending;
        for idx in &missing {
            let mut w = wires[*idx as usize].clone();
            w.count = body.len() as u16;
            last = absorb(&mut asm, w);
        }
        match last {
            Absorb::Ready(_, text, op) => {
                assert_eq!(op, MailOp::VoxData);
                assert_eq!(text, body);
            }
            Absorb::Pending => panic!("resend did not complete the mail"),
            Absorb::Rejected { idx, count, .. } => {
                panic!("resend rejected {idx}/{count}")
            }
        }
    }
}
