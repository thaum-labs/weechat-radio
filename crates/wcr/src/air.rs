//! SPDX-License-Identifier: Apache-2.0
//! RF air queue: serialise, prioritise, and defer TX when the channel is busy.

use crate::config::Config;
use crate::modem::control::{ControlClient, ModemStatus};
use crate::modem::KissClient;
use crate::presets::{airtime_secs, Preset, Rung};
use crate::proto::{Envelope, MsgId, MsgType, Priority};
use crate::status::SharedStatus;
use parking_lot::Mutex;
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const BASE_CW: u32 = 4;
const RELAY_DEFER_MIN_S: u64 = 2;
const RELAY_DEFER_MAX_S: u64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AirClass {
    Emergency = 0,
    Ack = 1,
    Priority = 2,
    Own = 3,
    Relay = 4,
    Background = 5,
}

impl AirClass {
    pub fn classify(env: &Envelope, our_call: &str) -> Self {
        if env.flags.priority() == Priority::Emergency {
            return Self::Emergency;
        }
        // ACKs share the emergency defer cap. A channel that stays busy
        // (occupancy near 100%) must not hold a short ACK for the full
        // `max_defer_ms`. `pace_and_send` still backs off slot by slot, then
        // hands the frame to the modem once `emergency_max_defer_ms` elapses.
        if env.kind == MsgType::Ack {
            return Self::Emergency;
        }
        if env.flags.priority() == Priority::Priority {
            return Self::Priority;
        }
        match env.kind {
            MsgType::Beacon | MsgType::Have | MsgType::Ping => Self::Background,
            _ if env.origin.as_str() == our_call => Self::Own,
            _ => Self::Relay,
        }
    }

    fn persist_p(self) -> f32 {
        match self {
            Self::Emergency => 1.0,
            Self::Ack => 0.9,
            Self::Priority => 0.7,
            Self::Own => 0.5,
            Self::Relay => 0.35,
            Self::Background => 0.2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    Idle,
    Rx,
    Tx,
}

impl ChannelState {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "tx" => Self::Tx,
            "rx" => Self::Rx,
            _ => Self::Idle,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Rx => "rx",
            Self::Tx => "tx",
        }
    }
}

pub trait ChannelSense: Send + Sync {
    fn state(&self) -> ChannelState;
    fn occupancy_pct(&self) -> u8;
    fn since_last_rx(&self) -> Duration;
    fn note_rx(&self) {}
}

struct SenseInner {
    state: ChannelState,
    occupancy: u8,
    last_rx: Instant,
    ptt_on: bool,
}

pub struct ModemSense {
    inner: Mutex<SenseInner>,
    /// No modem73 status poller: only RX frames tell us the channel is busy,
    /// so `Rx` decays back to `Idle` on its own.
    passive: bool,
}

/// How long after the last decoded frame a passive sense still reports `Rx`.
const PASSIVE_RX_HOLD: Duration = Duration::from_millis(800);

impl ModemSense {
    pub fn new() -> Self {
        Self::with_passive(false)
    }

    /// Sense for radios with their own TNC (no channel-state feed from a modem).
    pub fn passive() -> Self {
        Self::with_passive(true)
    }

    fn with_passive(passive: bool) -> Self {
        Self {
            inner: Mutex::new(SenseInner {
                state: ChannelState::Idle,
                occupancy: 0,
                last_rx: Instant::now() - Duration::from_secs(3600),
                ptt_on: false,
            }),
            passive,
        }
    }

    pub fn apply_status(&self, st: &ModemStatus) {
        let mut g = self.inner.lock();
        g.state = ChannelState::parse(&st.channel_state);
        g.occupancy = st.occupancy_pct.clamp(0, 100) as u8;
        g.ptt_on = st.ptt_on;
        if g.state == ChannelState::Rx {
            g.last_rx = Instant::now();
        }
    }

    pub fn ptt_on(&self) -> bool {
        self.inner.lock().ptt_on
    }

    pub fn spawn_poller(
        self: Arc<Self>,
        control: ControlClient,
        snap: Arc<SharedStatus>,
        queue: AirQueue,
        cancel: CancellationToken,
    ) {
        tokio::spawn(async move {
            let mut unhealthy = 0_u8;
            loop {
                if let Ok(st) = control.get_status().await {
                    self.apply_status(&st);
                    let mut st = st;
                    if st.audio_connected {
                        unhealthy = 0;
                    } else {
                        unhealthy = unhealthy.saturating_add(1);
                        // One reconnect blip must not wipe the meters.
                        if unhealthy < 4 {
                            st.audio_connected = true;
                        }
                    }
                    let mut s = snap.lock();
                    s.channel = self.state().as_str().into();
                    s.occupancy_pct = self.occupancy_pct();
                    s.ptt_on = st.ptt_on;
                    s.queue_air = queue.depth();
                    s.apply_modem_audio(&st);
                }
                let wait = Duration::from_millis(250);
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(wait) => {}
                }
            }
        });
    }
}

impl Default for ModemSense {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelSense for ModemSense {
    fn state(&self) -> ChannelState {
        let g = self.inner.lock();
        if self.passive
            && g.state == ChannelState::Rx
            && Instant::now().saturating_duration_since(g.last_rx) > PASSIVE_RX_HOLD
        {
            return ChannelState::Idle;
        }
        g.state
    }

    fn occupancy_pct(&self) -> u8 {
        self.inner.lock().occupancy
    }

    fn since_last_rx(&self) -> Duration {
        Instant::now().saturating_duration_since(self.inner.lock().last_rx)
    }

    fn note_rx(&self) {
        let mut g = self.inner.lock();
        g.state = ChannelState::Rx;
        g.last_rx = Instant::now();
    }
}

pub struct FakeSense {
    inner: Mutex<SenseInner>,
}

impl FakeSense {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(SenseInner {
                state: ChannelState::Idle,
                occupancy: 0,
                last_rx: Instant::now() - Duration::from_secs(3600),
                ptt_on: false,
            }),
        })
    }

    pub fn set_state(&self, state: ChannelState) {
        self.inner.lock().state = state;
        if state == ChannelState::Rx {
            self.inner.lock().last_rx = Instant::now();
        }
    }

    pub fn set_occupancy(&self, pct: u8) {
        self.inner.lock().occupancy = pct;
    }
}

impl ChannelSense for FakeSense {
    fn state(&self) -> ChannelState {
        self.inner.lock().state
    }

    fn occupancy_pct(&self) -> u8 {
        self.inner.lock().occupancy
    }

    fn since_last_rx(&self) -> Duration {
        Instant::now().saturating_duration_since(self.inner.lock().last_rx)
    }

    fn note_rx(&self) {
        let mut g = self.inner.lock();
        g.state = ChannelState::Rx;
        g.last_rx = Instant::now();
    }
}

#[derive(Clone)]
pub struct AirItem {
    pub msg_id: MsgId,
    pub copy: u8,
    pub class: AirClass,
    pub frames: Vec<Vec<u8>>,
    pub enqueued_at: Instant,
    pub not_before: Instant,
    pub rung: Option<Rung>,
    pub preset: Preset,
    congest_deferred: bool,
}

impl AirItem {
    pub fn new(env: &Envelope, our_call: &str, frames: Vec<Vec<u8>>, preset: Preset) -> Self {
        let now = Instant::now();
        Self {
            msg_id: env.msg_id,
            copy: 0,
            class: AirClass::classify(env, our_call),
            frames,
            enqueued_at: now,
            not_before: now,
            rung: None,
            preset,
            congest_deferred: false,
        }
    }

    pub fn with_delay(mut self, d: Duration) -> Self {
        self.not_before += d;
        self
    }

    pub fn with_rung(mut self, r: Rung) -> Self {
        self.rung = Some(r);
        self
    }

    pub fn with_copy(mut self, c: u8) -> Self {
        self.copy = c;
        self
    }

    pub fn with_class(mut self, c: AirClass) -> Self {
        self.class = c;
        self
    }

    fn payload_len(&self) -> usize {
        self.frames.iter().map(|f| f.len()).sum()
    }
}

struct QueueInner {
    items: Vec<AirItem>,
    /// Email holds the radio lease. Chat may still enqueue; `pop_ready` waits.
    paused: bool,
    /// Set only by the mail lease. An ACK's `resume` clears `paused`, not this.
    mail_lease: bool,
    /// When a mail lease last started or ended (unix seconds, 0 for never).
    /// The beacon counts it as traffic.
    last_mail: u32,
}

#[derive(Clone)]
pub struct AirQueue {
    inner: Arc<Mutex<QueueInner>>,
    notify: Arc<Notify>,
}

impl AirQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(QueueInner {
                items: Vec::new(),
                paused: false,
                mail_lease: false,
                last_mail: 0,
            })),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Insert by class then FIFO. Returns false if `(msg_id, copy)` is already queued.
    pub fn enqueue(&self, item: AirItem) -> bool {
        let mut g = self.inner.lock();
        if g.items
            .iter()
            .any(|x| x.msg_id == item.msg_id && x.copy == item.copy)
        {
            return false;
        }
        let pos = g
            .items
            .iter()
            .position(|x| x.class > item.class)
            .unwrap_or(g.items.len());
        g.items.insert(pos, item);
        drop(g);
        self.notify.notify_waiters();
        true
    }

    pub fn cancel(&self, id: MsgId) -> usize {
        let mut g = self.inner.lock();
        let before = g.items.len();
        g.items.retain(|x| x.msg_id != id);
        before - g.items.len()
    }

    pub fn depth(&self) -> usize {
        self.inner.lock().items.len()
    }

    pub fn pop_ready(&self) -> Option<AirItem> {
        let now = Instant::now();
        let mut g = self.inner.lock();
        if g.paused || g.mail_lease {
            return None;
        }
        let idx = g.items.iter().position(|x| x.not_before <= now)?;
        Some(g.items.remove(idx))
    }

    /// Hold the queue while Email has the radio. Does not drop queued chat.
    pub fn pause(&self) {
        self.inner.lock().paused = true;
    }

    /// Let chat transmit again.
    pub fn resume(&self) {
        self.inner.lock().paused = false;
        self.notify.notify_waiters();
    }

    pub fn is_paused(&self) -> bool {
        self.inner.lock().paused
    }

    /// Hold chat for the whole mail lease. A chat ACK keyed meanwhile pauses
    /// and resumes the queue around itself; that must not end the lease.
    pub fn hold_for_mail(&self) {
        let mut g = self.inner.lock();
        g.mail_lease = true;
        g.last_mail = crate::proto::now_ts();
    }

    pub fn release_mail(&self) {
        {
            let mut g = self.inner.lock();
            g.mail_lease = false;
            g.last_mail = crate::proto::now_ts();
        }
        self.notify.notify_waiters();
    }

    /// When a mail lease last started or ended, 0 for never.
    pub fn last_mail(&self) -> u32 {
        self.inner.lock().last_mail
    }

    pub fn is_held_for_mail(&self) -> bool {
        self.inner.lock().mail_lease
    }

    fn next_deadline(&self) -> Option<Instant> {
        let g = self.inner.lock();
        if g.mail_lease {
            return None;
        }
        g.items.iter().map(|x| x.not_before).min()
    }

    fn apply_congestion(&self, occ: u8, congested_pct: u8) {
        if occ < congested_pct {
            return;
        }
        let now = Instant::now();
        let mut g = self.inner.lock();
        g.items.retain(|x| x.class != AirClass::Background);
        for item in g.items.iter_mut() {
            if item.class == AirClass::Relay && !item.congest_deferred {
                let extra = rand::thread_rng().gen_range(RELAY_DEFER_MIN_S..=RELAY_DEFER_MAX_S);
                let later = now + Duration::from_secs(extra);
                if later > item.not_before {
                    item.not_before = later;
                }
                item.congest_deferred = true;
            }
        }
    }
}

impl Default for AirQueue {
    fn default() -> Self {
        Self::new()
    }
}

fn contention_window(occ: u8) -> u32 {
    BASE_CW + u32::from(occ) / 10
}

/// Serialise RF TX: busy-gate, p-persist, then one item at a time onto KISS.
pub async fn run_air_queue(
    queue: AirQueue,
    tx: mpsc::Sender<Vec<u8>>,
    sense: Arc<dyn ChannelSense>,
    control: Option<ControlClient>,
    cfg: Arc<Mutex<Config>>,
    snap: Arc<SharedStatus>,
    cancel: CancellationToken,
) {
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let rf = cfg.lock().rf.clone();
        let occ = sense.occupancy_pct();
        if rf.csma {
            queue.apply_congestion(occ, rf.congested_pct);
        }
        {
            let mut s = snap.lock();
            s.queue_air = queue.depth();
            s.occupancy_pct = occ;
            s.channel = sense.state().as_str().into();
        }

        if let Some(item) = queue.pop_ready() {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = pace_and_send(&queue, &tx, sense.as_ref(), &control, &rf, &snap, item) => {}
            }
            continue;
        }

        let notified = queue.notify.notified();
        tokio::pin!(notified);
        if let Some(deadline) = queue.next_deadline() {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = notified => {}
                _ = tokio::time::sleep_until(deadline) => {}
            }
        } else {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = notified => {}
                _ = tokio::time::sleep(Duration::from_millis(rf.slot_ms.max(50) as u64)) => {}
            }
        }
    }
}

async fn pace_and_send(
    queue: &AirQueue,
    tx: &mpsc::Sender<Vec<u8>>,
    sense: &dyn ChannelSense,
    control: &Option<ControlClient>,
    rf: &crate::config::RfConfig,
    snap: &Arc<SharedStatus>,
    item: AirItem,
) {
    let cap = if item.class == AirClass::Emergency {
        rf.emergency_max_defer_ms
    } else {
        rf.max_defer_ms
    };
    let started = Instant::now();

    if rf.csma {
        loop {
            let idle = sense.state() == ChannelState::Idle
                && sense.since_last_rx() >= Duration::from_millis(rf.quiet_ms as u64);
            if idle {
                let p = item.class.persist_p();
                if rand::random::<f32>() < p {
                    break;
                }
                let cw = contention_window(sense.occupancy_pct()).max(1);
                let slots = rand::thread_rng().gen_range(1..=cw);
                snap.lock().deferred = true;
                tokio::time::sleep(Duration::from_millis(rf.slot_ms as u64 * slots as u64)).await;
            } else {
                snap.lock().deferred = true;
                tokio::time::sleep(Duration::from_millis(rf.slot_ms.max(1) as u64)).await;
            }
            if started.elapsed() >= Duration::from_millis(cap as u64) {
                break;
            }
        }
    }

    snap.lock().deferred = false;

    if let Some(rung) = item.rung {
        if let Some(c) = control {
            let _ = c.set_config(rung.control_config()).await;
        }
        snap.lock().tx_rung = rung.as_str().into();
    }

    let pushed_at = match control {
        Some(c) => c.get_status().await.ok().map(|s| s.tx_frame_count),
        None => None,
    };

    for frame in &item.frames {
        if tx.send(frame.clone()).await.is_err() {
            break;
        }
    }

    let hold = airtime_secs(item.preset, item.payload_len(), 0) + rf.turnaround_ms as f64 / 1000.0;
    if hold > 0.0 {
        tokio::time::sleep(Duration::from_secs_f64(hold)).await;
    }

    // modem73 has one global TX mode and its own CSMA, so its queue can outlive
    // the estimate above. Returning early lets the next item switch the mode and
    // the modem then drops any queued frame wider than the new capacity.
    if let (Some(c), Some(before)) = (control, pushed_at) {
        wait_for_tx_drain(
            c,
            before + item.frames.len() as u64,
            Duration::from_secs_f64(hold * 3.0 + DRAIN_GRACE_S),
        )
        .await;
    }

    snap.lock().queue_air = queue.depth();
}

/// Extra patience on top of the airtime estimate while modem73 clears its queue.
const DRAIN_GRACE_S: f64 = 15.0;

/// Preset modem config with carrier sense off, for one ACK that has already
/// waited out `emergency_max_defer_ms`. The rung stays the preset's own
/// `control_config` (never a faster mode). Chat's config is not modified.
pub fn ack_force_config(preset: Preset) -> serde_json::Value {
    let mut v = preset.control_config();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("csma_enabled".into(), serde_json::json!(false));
    }
    v
}

/// Key one ACK without waiting behind the air queue.
///
/// The queue is paused so a backlog cannot take the modem while this frame
/// is borrowed. Modem carrier sense is turned off for this ACK once
/// `cap` elapses, and also as soon as the modem reports undecodable busy
/// (noise, not a frame it is demodulating). A decoded `rx` keeps carrier
/// sense on until the cap, so we do not key over a station we can hear.
/// The preset config, with carrier sense on, is restored afterwards.
pub struct AckTx<'a> {
    pub frames: &'a [Vec<u8>],
    pub preset: Preset,
    pub air: Option<&'a AirQueue>,
    pub sense: Option<&'a dyn ChannelSense>,
    pub control: Option<&'a ControlClient>,
    pub kiss: Option<&'a KissClient>,
    pub cap: Duration,
    pub slot: Duration,
    pub quiet: Duration,
    pub csma: bool,
    pub drain_cap: Duration,
}

pub async fn transmit_ack(tx: AckTx<'_>) -> bool {
    if let Some(air) = tx.air {
        air.pause();
    }
    let sent = transmit_ack_while_paused(&tx).await;
    if let Some(air) = tx.air {
        air.resume();
    }
    sent
}

/// modem73 is holding the channel even if we have not decoded a frame.
pub fn modem_reports_busy(st: &ModemStatus) -> bool {
    match st.channel_state.trim().to_ascii_lowercase().as_str() {
        "rx" | "tx" | "busy" => true,
        _ => st.occupancy_pct > 0,
    }
}

/// A frame is on the air and this station can hear it. `rx` is the modem
/// demodulating; our own sense is `Rx` once a frame has started to decode.
/// Neither is the undecodable noise floor that leaves sense idle.
pub fn ack_rx_in_progress(sense_rx: bool, channel_state: &str) -> bool {
    sense_rx || channel_state.trim().eq_ignore_ascii_case("rx")
}

/// Carrier sense goes off for undecodable busy/noise, or once `cap` elapses.
/// It stays on while a decodable frame is still being received, so an ACK
/// does not key over a station we can hear.
pub fn ack_forces_csma_off(cap_elapsed: bool, modem_busy: bool, rx_in_progress: bool) -> bool {
    cap_elapsed || (modem_busy && !rx_in_progress)
}

async fn transmit_ack_while_paused(tx: &AckTx<'_>) -> bool {
    let started = Instant::now();
    let mut force = false;
    if tx.csma {
        loop {
            let status = match tx.control {
                Some(c) => c.get_status().await.ok(),
                None => None,
            };
            let modem_busy = status.as_ref().is_some_and(|st| modem_reports_busy(st));
            let sense_rx = tx.sense.is_some_and(|s| s.state() == ChannelState::Rx);
            let channel = status
                .as_ref()
                .map(|st| st.channel_state.as_str())
                .unwrap_or("");
            // Do not send early just because we have not decoded a frame.
            // That left CSMA on, and modem73 then held the ACK over noise.
            // Do not force CSMA off while a frame is actually being received.
            if ack_forces_csma_off(
                started.elapsed() >= tx.cap,
                modem_busy,
                ack_rx_in_progress(sense_rx, channel),
            ) {
                force = true;
                break;
            }
            let slot = if tx.slot.is_zero() {
                Duration::from_millis(1)
            } else {
                tx.slot
            };
            tokio::time::sleep(slot).await;
        }
    }
    if force {
        if let Some(c) = tx.control {
            let _ = c.set_config(ack_force_config(tx.preset)).await;
        }
    }
    let before = match tx.control {
        Some(c) => c.get_status().await.ok().map(|s| s.tx_frame_count),
        None => None,
    };
    let mut sent = false;
    if let Some(k) = tx.kiss {
        for frame in tx.frames {
            if k.send(frame).await.is_err() {
                break;
            }
            sent = true;
        }
    }
    if let (Some(c), Some(before)) = (tx.control, before) {
        let target = before.saturating_add(tx.frames.len() as u64);
        let start = Instant::now();
        loop {
            match c.get_status().await {
                Ok(st) if st.tx_frame_count >= target => break,
                Err(_) => break,
                _ => {}
            }
            if start.elapsed() >= tx.drain_cap {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    if force {
        if let Some(c) = tx.control {
            let _ = c.set_config(tx.preset.control_config()).await;
        }
    }
    sent
}

/// Wait until the modem reports `target` transmitted frames, or `cap` elapses.
async fn wait_for_tx_drain(control: &ControlClient, target: u64, cap: Duration) {
    let start = Instant::now();
    loop {
        match control.get_status().await {
            Ok(st) if st.tx_frame_count >= target => return,
            Err(_) => return,
            _ => {}
        }
        if start.elapsed() >= cap {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::ids::MsgId;
    use crate::status;

    fn mid(n: u8) -> MsgId {
        MsgId::from_bytes([n, 0, 0, 0, 0, 0, 0, 0])
    }

    fn item(class: AirClass, n: u8) -> AirItem {
        let now = Instant::now();
        AirItem {
            msg_id: mid(n),
            copy: 0,
            class,
            frames: vec![vec![n]],
            enqueued_at: now,
            not_before: now,
            rung: None,
            preset: Preset::VhfFm,
            congest_deferred: false,
        }
    }

    fn test_cfg() -> Arc<Mutex<Config>> {
        let mut c = Config::default();
        c.rf.csma = true;
        c.rf.slot_ms = 50;
        c.rf.quiet_ms = 50;
        c.rf.max_defer_ms = 15_000;
        c.rf.emergency_max_defer_ms = 200;
        c.rf.turnaround_ms = 0;
        c.rf.congested_pct = 60;
        Arc::new(Mutex::new(c))
    }

    async fn drain(rx: &mut mpsc::Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Ok(f) = rx.try_recv() {
            out.push(f);
        }
        out
    }

    #[tokio::test(start_paused = true)]
    async fn passive_sense_decays_to_idle() {
        let s = ModemSense::passive();
        assert_eq!(s.state(), ChannelState::Idle);
        s.note_rx();
        assert_eq!(s.state(), ChannelState::Rx);
        tokio::time::advance(Duration::from_millis(1000)).await;
        assert_eq!(s.state(), ChannelState::Idle);
        // A poller-driven sense keeps whatever the modem last reported.
        let m = ModemSense::new();
        m.note_rx();
        tokio::time::advance(Duration::from_millis(1000)).await;
        assert_eq!(m.state(), ChannelState::Rx);
    }

    #[test]
    fn class_order_emergency_first() {
        let q = AirQueue::new();
        assert!(q.enqueue(item(AirClass::Background, 1)));
        assert!(q.enqueue(item(AirClass::Own, 2)));
        assert!(q.enqueue(item(AirClass::Emergency, 3)));
        assert_eq!(q.pop_ready().unwrap().class, AirClass::Emergency);
        assert_eq!(q.pop_ready().unwrap().class, AirClass::Own);
        assert_eq!(q.pop_ready().unwrap().class, AirClass::Background);
    }

    #[test]
    fn dedupe_same_msgid_copy() {
        let q = AirQueue::new();
        assert!(q.enqueue(item(AirClass::Own, 1)));
        assert!(!q.enqueue(item(AirClass::Own, 1)));
        assert_eq!(q.depth(), 1);
        let mut dup = item(AirClass::Own, 1);
        dup.copy = 1;
        assert!(q.enqueue(dup));
        assert_eq!(q.depth(), 2);
    }

    #[test]
    fn cancel_removes_all_copies() {
        let q = AirQueue::new();
        q.enqueue(item(AirClass::Own, 7));
        let mut dup = item(AirClass::Own, 7);
        dup.copy = 1;
        q.enqueue(dup);
        q.enqueue(item(AirClass::Ack, 8));
        assert_eq!(q.cancel(mid(7)), 2);
        assert_eq!(q.depth(), 1);
        assert_eq!(q.pop_ready().unwrap().msg_id, mid(8));
    }

    #[tokio::test(start_paused = true)]
    async fn busy_gate_defers_until_idle() {
        let q = AirQueue::new();
        let sense = FakeSense::new();
        sense.set_state(ChannelState::Rx);
        let (tx, mut rx) = mpsc::channel(8);
        let mut c = Config::default();
        c.rf.csma = true;
        c.rf.slot_ms = 50;
        c.rf.quiet_ms = 50;
        c.rf.max_defer_ms = 15_000;
        c.rf.emergency_max_defer_ms = 15_000;
        c.rf.turnaround_ms = 0;
        let cfg = Arc::new(Mutex::new(c));
        let snap = status::new_shared();
        let h = tokio::spawn(run_air_queue(
            q.clone(),
            tx,
            sense.clone(),
            None,
            cfg,
            snap,
            CancellationToken::new(),
        ));
        q.enqueue(item(AirClass::Emergency, 1));
        tokio::time::advance(Duration::from_millis(400)).await;
        tokio::task::yield_now().await;
        assert!(drain(&mut rx).await.is_empty(), "must not TX while Rx");
        sense.set_state(ChannelState::Idle);
        tokio::time::advance(Duration::from_millis(200)).await;
        tokio::task::yield_now().await;
        let got = drain(&mut rx).await;
        assert_eq!(got, vec![vec![1]]);
        h.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn emergency_sends_after_short_defer_cap() {
        let q = AirQueue::new();
        let sense = FakeSense::new();
        sense.set_state(ChannelState::Rx);
        let (tx, mut rx) = mpsc::channel(8);
        let cfg = test_cfg();
        let snap = status::new_shared();
        let h = tokio::spawn(run_air_queue(
            q.clone(),
            tx,
            sense.clone(),
            None,
            cfg,
            snap,
            CancellationToken::new(),
        ));
        q.enqueue(item(AirClass::Emergency, 9));
        tokio::time::advance(Duration::from_millis(80)).await;
        tokio::task::yield_now().await;
        assert!(drain(&mut rx).await.is_empty());
        tokio::time::advance(Duration::from_millis(250)).await;
        tokio::task::yield_now().await;
        assert_eq!(drain(&mut rx).await, vec![vec![9]]);
        h.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn own_frame_keys_after_defer_cap_while_rx() {
        let q = AirQueue::new();
        let sense = FakeSense::new();
        sense.set_state(ChannelState::Rx);
        sense.set_occupancy(50);
        let (tx, mut rx) = mpsc::channel(8);
        let mut c = Config::default();
        c.rf.csma = true;
        c.rf.slot_ms = 50;
        c.rf.quiet_ms = 50;
        c.rf.max_defer_ms = 400;
        c.rf.emergency_max_defer_ms = 400;
        c.rf.turnaround_ms = 0;
        c.rf.congested_pct = 60;
        let cfg = Arc::new(Mutex::new(c));
        let snap = status::new_shared();
        let h = tokio::spawn(run_air_queue(
            q.clone(),
            tx,
            sense.clone(),
            None,
            cfg,
            snap,
            CancellationToken::new(),
        ));
        q.enqueue(item(AirClass::Own, 2));
        tokio::time::advance(Duration::from_millis(80)).await;
        tokio::task::yield_now().await;
        assert!(
            drain(&mut rx).await.is_empty(),
            "routine frame waits inside the defer cap"
        );
        assert_eq!(sense.state(), ChannelState::Rx);
        tokio::time::advance(Duration::from_millis(400)).await;
        tokio::task::yield_now().await;
        assert_eq!(drain(&mut rx).await, vec![vec![2]]);
        h.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn ack_goes_out_while_channel_stays_busy() {
        use crate::proto::{Callsign, Flags};
        let q = AirQueue::new();
        let sense = FakeSense::new();
        sense.set_state(ChannelState::Rx);
        sense.set_occupancy(99);
        let (tx, mut rx) = mpsc::channel(8);
        let cfg = test_cfg();
        let snap = status::new_shared();
        let h = tokio::spawn(run_air_queue(
            q.clone(),
            tx,
            sense.clone(),
            None,
            cfg,
            snap,
            CancellationToken::new(),
        ));
        let ack = Envelope::ack_for(
            &Envelope::new_msg(
                Callsign::parse("M0XYZ").unwrap(),
                Callsign::parse("G4ABC").unwrap(),
                1,
                b"hi".to_vec(),
                3,
                Flags::new(),
            )
            .unwrap(),
            Callsign::parse("G4ABC").unwrap(),
            2,
            None,
        );
        q.enqueue(AirItem::new(&ack, "G4ABC", vec![vec![9]], Preset::VhfFm));
        let own = item(AirClass::Own, 3);
        q.enqueue(own);
        tokio::time::advance(Duration::from_millis(80)).await;
        tokio::task::yield_now().await;
        assert!(
            drain(&mut rx).await.is_empty(),
            "ACK waits through the busy backoff"
        );
        // emergency_max_defer_ms is 200 in test_cfg. The ACK uses that cap.
        // The ordinary frame keeps the 15s cap and must still be held.
        tokio::time::advance(Duration::from_millis(250)).await;
        tokio::task::yield_now().await;
        assert_eq!(drain(&mut rx).await, vec![vec![9]]);
        h.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn background_dropped_when_congested() {
        let q = AirQueue::new();
        let sense = FakeSense::new();
        sense.set_occupancy(80);
        let (tx, mut rx) = mpsc::channel(8);
        let cfg = test_cfg();
        let snap = status::new_shared();
        let h = tokio::spawn(run_air_queue(
            q.clone(),
            tx,
            sense.clone(),
            None,
            cfg,
            snap,
            CancellationToken::new(),
        ));
        q.enqueue(item(AirClass::Background, 4));
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert!(drain(&mut rx).await.is_empty());
        assert_eq!(q.depth(), 0);
        h.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn airtime_holdoff_between_items() {
        let q = AirQueue::new();
        let sense = FakeSense::new();
        let (tx, mut rx) = mpsc::channel(8);
        let mut c = Config::default();
        c.rf.csma = false;
        c.rf.turnaround_ms = 100;
        let cfg = Arc::new(Mutex::new(c));
        let snap = status::new_shared();
        let h = tokio::spawn(run_air_queue(
            q.clone(),
            tx,
            sense.clone(),
            None,
            cfg,
            snap,
            CancellationToken::new(),
        ));
        let mut a = item(AirClass::Emergency, 1);
        a.frames = vec![vec![0u8; 100]];
        let mut b = item(AirClass::Emergency, 2);
        b.frames = vec![vec![0u8; 100]];
        q.enqueue(a);
        q.enqueue(b);
        tokio::time::advance(Duration::from_millis(10)).await;
        tokio::task::yield_now().await;
        let first = drain(&mut rx).await;
        assert_eq!(first.len(), 1);
        tokio::time::advance(Duration::from_millis(50)).await;
        tokio::task::yield_now().await;
        assert!(
            drain(&mut rx).await.is_empty(),
            "second frame must wait for airtime + turnaround"
        );
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        assert_eq!(drain(&mut rx).await.len(), 1);
        h.abort();
    }

    #[test]
    fn classify_matches_plan() {
        use crate::proto::{Callsign, Envelope, Flags};
        let us = "G4ABC";
        let origin = Callsign::parse(us).unwrap();
        let dest = Callsign::parse("M0XYZ").unwrap();
        let msg = Envelope::new_msg(
            origin.clone(),
            dest.clone(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert_eq!(AirClass::classify(&msg, us), AirClass::Own);
        let mut relay = msg.clone();
        relay.origin = dest.clone();
        assert_eq!(AirClass::classify(&relay, us), AirClass::Relay);
        let mut ack = msg.clone();
        ack.kind = MsgType::Ack;
        assert_eq!(AirClass::classify(&ack, us), AirClass::Emergency);
        let mut beacon = msg.clone();
        beacon.kind = MsgType::Beacon;
        assert_eq!(AirClass::classify(&beacon, us), AirClass::Background);
        let mut em = Envelope::new_msg(origin, dest, 2, b"!!".to_vec(), 3, Flags::new()).unwrap();
        em.flags.set_priority(Priority::Emergency);
        assert_eq!(AirClass::classify(&em, us), AirClass::Emergency);
    }

    #[test]
    fn pause_blocks_pop_ready_until_resume() {
        use crate::proto::{Callsign, Envelope, Flags};
        let q = AirQueue::new();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        let item = AirItem::new(&env, "G4ABC", vec![b"frame".to_vec()], Preset::HfPoor);
        assert!(q.enqueue(item));
        q.pause();
        assert!(q.is_paused());
        assert!(q.pop_ready().is_none());
        assert_eq!(q.depth(), 1);
        q.resume();
        assert!(!q.is_paused());
        assert!(q.pop_ready().is_some());
        assert_eq!(q.depth(), 0);
    }

    #[test]
    fn an_ack_resume_does_not_end_the_mail_lease() {
        use crate::proto::{Callsign, Envelope, Flags};
        let q = AirQueue::new();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert!(q.enqueue(AirItem::new(
            &env,
            "G4ABC",
            vec![b"frame".to_vec()],
            Preset::HfPoor
        )));
        q.hold_for_mail();
        q.pause();
        q.resume();
        assert!(q.is_held_for_mail());
        assert!(q.pop_ready().is_none(), "chat keyed inside the mail lease");
        q.release_mail();
        assert!(q.pop_ready().is_some());
    }

    #[test]
    fn a_mail_lease_is_traffic_for_the_beacon() {
        let q = AirQueue::new();
        assert_eq!(q.last_mail(), 0, "no mail yet");
        let before = crate::proto::now_ts();
        q.hold_for_mail();
        assert!(q.last_mail() >= before, "the lease start counts");
        q.release_mail();
        assert!(q.last_mail() >= before, "and so does its end");
    }

    #[test]
    fn ack_force_config_keeps_the_preset_and_drops_csma() {
        let forced = ack_force_config(Preset::HfPoor);
        assert_eq!(
            forced.get("csma_enabled").and_then(|v| v.as_bool()),
            Some(false)
        );
        assert_eq!(forced.get("robust_mode").and_then(|v| v.as_i64()), Some(6));
        let restored = Preset::HfPoor.control_config();
        assert_eq!(
            restored.get("csma_enabled").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            restored.get("robust_mode").and_then(|v| v.as_i64()),
            Some(6)
        );
    }

    #[test]
    fn ack_csma_off_after_cap_or_modem_busy() {
        assert!(ack_forces_csma_off(true, false, true));
        assert!(ack_forces_csma_off(false, true, false));
        assert!(!ack_forces_csma_off(false, true, true));
        assert!(!ack_forces_csma_off(false, false, false));
        assert!(ack_rx_in_progress(false, "rx"));
        assert!(ack_rx_in_progress(true, "idle"));
        assert!(!ack_rx_in_progress(false, "busy"));
        assert!(modem_reports_busy(&ModemStatus {
            channel_state: "rx".into(),
            ..ModemStatus::default()
        }));
        assert!(modem_reports_busy(&ModemStatus {
            channel_state: "busy".into(),
            ..ModemStatus::default()
        }));
        assert!(modem_reports_busy(&ModemStatus {
            channel_state: "idle".into(),
            occupancy_pct: 99,
            ..ModemStatus::default()
        }));
        assert!(!modem_reports_busy(&ModemStatus {
            channel_state: "idle".into(),
            occupancy_pct: 0,
            ..ModemStatus::default()
        }));
    }

    /// Steady noise, occupancy stuck high, and a full air queue. The ACK must
    /// be keyed (CSMA off) within the short cap, and the backlog must stay queued.
    #[tokio::test]
    async fn ack_keys_past_busy_cap_ahead_of_backlog() {
        use crate::modem::control::{decode_control_frames, encode_control_frame};
        use crate::modem::kiss::KissDecoder;
        use crate::modem::{ControlClient, KissClient};
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let csma_on = Arc::new(AtomicBool::new(true));
        let tx_count = Arc::new(AtomicU64::new(0));
        let csma_seen = Arc::new(Mutex::new(Vec::<bool>::new()));
        let got = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));

        let ctrl_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ctrl_addr = ctrl_listener.local_addr().unwrap();
        let kiss_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let kiss_addr = kiss_listener.local_addr().unwrap();

        let csma_c = csma_on.clone();
        let tx_c = tx_count.clone();
        let seen_c = csma_seen.clone();
        tokio::spawn(async move {
            let (mut stream, _) = ctrl_listener.accept().await.unwrap();
            let mut stash = Vec::new();
            let mut buf = [0u8; 2048];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        stash.extend_from_slice(&buf[..n]);
                        let Ok(frames) = decode_control_frames(&mut stash) else {
                            continue;
                        };
                        for v in frames {
                            let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
                            if cmd == "set_config" {
                                if let Some(on) = v.get("csma_enabled").and_then(|x| x.as_bool()) {
                                    csma_c.store(on, AtomicOrdering::SeqCst);
                                    seen_c.lock().push(on);
                                }
                            }
                            let reply = if cmd == "get_status" {
                                serde_json::json!({
                                    "channel_state": "rx",
                                    "occupancy_pct": 99,
                                    "tx_frame_count": tx_c.load(AtomicOrdering::SeqCst),
                                    "audio_connected": true
                                })
                            } else {
                                serde_json::json!({"ok": true})
                            };
                            if let Ok(f) = encode_control_frame(&reply) {
                                if stream.write_all(&f).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
        let csma_k = csma_on.clone();
        let tx_k = tx_count.clone();
        let got_k = got.clone();
        tokio::spawn(async move {
            let (mut stream, _) = kiss_listener.accept().await.unwrap();
            let mut dec = KissDecoder::new();
            let mut buf = [0u8; 2048];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        for payload in dec.push(&buf[..n]) {
                            // Carrier sense on: the modem accepts the frame and
                            // does not key it. tx_frame_count stays put.
                            if !csma_k.load(AtomicOrdering::SeqCst) {
                                tx_k.fetch_add(1, AtomicOrdering::SeqCst);
                            }
                            got_k.lock().push(payload);
                        }
                    }
                }
            }
        });

        let (control, _ev) = ControlClient::connect(&ctrl_addr.to_string())
            .await
            .unwrap();
        let (kiss, _rx) = KissClient::connect(&kiss_addr.to_string()).await.unwrap();

        let q = AirQueue::new();
        let sense = FakeSense::new();
        sense.set_state(ChannelState::Rx);
        sense.set_occupancy(99);
        for n in 0..22u8 {
            q.enqueue(item(AirClass::Own, n));
        }
        let (air_tx, mut air_rx) = mpsc::channel(32);
        let cfg = test_cfg();
        let snap = status::new_shared();
        let worker = tokio::spawn(run_air_queue(
            q.clone(),
            air_tx,
            sense.clone(),
            Some(control.clone()),
            cfg,
            snap,
            CancellationToken::new(),
        ));

        let ack = vec![vec![0xA1, 0xA1]];
        let keyed = transmit_ack(AckTx {
            frames: &ack,
            preset: Preset::HfPoor,
            air: Some(&q),
            sense: Some(sense.as_ref()),
            control: Some(&control),
            kiss: Some(&kiss),
            cap: Duration::from_millis(200),
            slot: Duration::from_millis(20),
            quiet: Duration::from_millis(50),
            csma: true,
            drain_cap: Duration::from_secs(2),
        })
        .await;
        assert!(keyed, "ACK was not handed to the modem");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && got.lock().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            got.lock().clone(),
            vec![vec![0xA1, 0xA1]],
            "ACK did not key ahead of the backlog"
        );
        assert!(
            !csma_on.load(AtomicOrdering::SeqCst) || csma_seen.lock().contains(&false),
            "ACK must disable carrier sense after the cap: {:?}",
            csma_seen.lock()
        );
        assert_eq!(
            csma_seen.lock().last().copied(),
            Some(true),
            "carrier sense must be restored: {:?}",
            csma_seen.lock()
        );
        // One backlog frame may be in flight inside pace_and_send. The rest stay.
        assert!(
            q.depth() >= 21,
            "backlog was dumped with the ACK: {}",
            q.depth()
        );
        assert!(
            drain(&mut air_rx).await.is_empty(),
            "the busy backlog must not reach KISS before its own cap"
        );
        worker.abort();
    }

    struct AckModem {
        control: crate::modem::ControlClient,
        kiss: crate::modem::KissClient,
        csma_seen: Arc<Mutex<Vec<bool>>>,
        tx_count: Arc<std::sync::atomic::AtomicU64>,
        got: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    /// Control port reports `channel_state` / occupancy. KISS accepts a frame
    /// always, but `tx_frame_count` moves only while carrier sense is off —
    /// the same hold modem73 applies when its detector is busy and CSMA is on.
    async fn spawn_ack_modem(channel_state: &'static str, occupancy: i32) -> AckModem {
        use crate::modem::control::{decode_control_frames, encode_control_frame};
        use crate::modem::kiss::KissDecoder;
        use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let csma_on = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let tx_count = Arc::new(AtomicU64::new(0));
        let csma_seen = Arc::new(Mutex::new(Vec::<bool>::new()));
        let got = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));

        let ctrl_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ctrl_addr = ctrl_listener.local_addr().unwrap();
        let kiss_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let kiss_addr = kiss_listener.local_addr().unwrap();

        let csma_c = csma_on.clone();
        let tx_c = tx_count.clone();
        let seen_c = csma_seen.clone();
        tokio::spawn(async move {
            let (mut stream, _) = ctrl_listener.accept().await.unwrap();
            let mut stash = Vec::new();
            let mut buf = [0u8; 2048];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        stash.extend_from_slice(&buf[..n]);
                        let Ok(frames) = decode_control_frames(&mut stash) else {
                            continue;
                        };
                        for v in frames {
                            let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("");
                            if cmd == "set_config" {
                                if let Some(on) = v.get("csma_enabled").and_then(|x| x.as_bool()) {
                                    csma_c.store(on, AtomicOrdering::SeqCst);
                                    seen_c.lock().push(on);
                                }
                            }
                            let reply = if cmd == "get_status" {
                                serde_json::json!({
                                    "channel_state": channel_state,
                                    "occupancy_pct": occupancy,
                                    "tx_frame_count": tx_c.load(AtomicOrdering::SeqCst),
                                    "audio_connected": true
                                })
                            } else {
                                serde_json::json!({"ok": true})
                            };
                            if let Ok(f) = encode_control_frame(&reply) {
                                if stream.write_all(&f).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
        let csma_k = csma_on.clone();
        let tx_k = tx_count.clone();
        let got_k = got.clone();
        tokio::spawn(async move {
            let (mut stream, _) = kiss_listener.accept().await.unwrap();
            let mut dec = KissDecoder::new();
            let mut buf = [0u8; 2048];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        for payload in dec.push(&buf[..n]) {
                            if !csma_k.load(AtomicOrdering::SeqCst) {
                                tx_k.fetch_add(1, AtomicOrdering::SeqCst);
                            }
                            got_k.lock().push(payload);
                        }
                    }
                }
            }
        });

        let (control, _ev) = crate::modem::ControlClient::connect(&ctrl_addr.to_string())
            .await
            .unwrap();
        let (kiss, _rx) = crate::modem::KissClient::connect(&kiss_addr.to_string())
            .await
            .unwrap();
        AckModem {
            control,
            kiss,
            csma_seen,
            tx_count,
            got,
        }
    }

    /// Steady noise: modem73's detector is busy, this station has decoded
    /// nothing so its own sense stays idle. The ACK must still leave with
    /// carrier sense off, well before a long cap, and sense must be restored.
    #[tokio::test]
    async fn ack_keys_csma_off_when_modem_busy_and_wcr_idle() {
        use std::sync::atomic::Ordering as AtomicOrdering;

        let modem = spawn_ack_modem("busy", 99).await;
        let sense = FakeSense::new();
        assert_eq!(sense.state(), ChannelState::Idle);
        sense.set_occupancy(0);

        let ack = vec![vec![0xA1, 0xA1]];
        let started = Instant::now();
        let keyed = transmit_ack(AckTx {
            frames: &ack,
            preset: Preset::HfPoor,
            air: None,
            sense: Some(sense.as_ref()),
            control: Some(&modem.control),
            kiss: Some(&modem.kiss),
            cap: Duration::from_secs(5),
            slot: Duration::from_millis(20),
            quiet: Duration::from_millis(50),
            csma: true,
            drain_cap: Duration::from_millis(800),
        })
        .await;
        let elapsed = started.elapsed();
        assert!(keyed, "ACK was not handed to the modem");
        assert!(
            elapsed < Duration::from_millis(1500),
            "modem-busy ACK waited {elapsed:?}; carrier sense should drop immediately"
        );
        assert_eq!(sense.state(), ChannelState::Idle);
        assert_eq!(modem.got.lock().clone(), vec![vec![0xA1, 0xA1]]);
        assert!(
            modem.tx_count.load(AtomicOrdering::SeqCst) >= 1,
            "frame was accepted while carrier sense was still on"
        );
        assert!(
            modem.csma_seen.lock().contains(&false),
            "ACK must disable carrier sense while the modem is busy and wcr is idle: {:?}",
            modem.csma_seen.lock()
        );
        assert_eq!(
            modem.csma_seen.lock().last().copied(),
            Some(true),
            "carrier sense must be restored: {:?}",
            modem.csma_seen.lock()
        );
    }

    /// Quiet channel, modem channel_state `rx`: another station's frame is on
    /// the air. Carrier sense stays on until the cap, then the ACK is forced.
    #[tokio::test]
    async fn ack_keeps_csma_while_a_frame_is_being_received() {
        use std::sync::atomic::Ordering as AtomicOrdering;

        let modem = spawn_ack_modem("rx", 0).await;
        let sense = FakeSense::new();
        assert_eq!(sense.state(), ChannelState::Idle);

        let ack = vec![vec![0xC3]];
        let started = Instant::now();
        let keyed = transmit_ack(AckTx {
            frames: &ack,
            preset: Preset::HfPoor,
            air: None,
            sense: Some(sense.as_ref()),
            control: Some(&modem.control),
            kiss: Some(&modem.kiss),
            cap: Duration::from_millis(300),
            slot: Duration::from_millis(20),
            quiet: Duration::from_millis(1),
            csma: true,
            drain_cap: Duration::from_millis(800),
        })
        .await;
        let elapsed = started.elapsed();
        assert!(keyed, "ACK was not handed to the modem");
        assert!(
            elapsed >= Duration::from_millis(220),
            "ACK forced carrier sense off over an in-progress frame ({elapsed:?})"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "ACK did not key once the cap elapsed ({elapsed:?})"
        );
        assert!(
            modem.tx_count.load(AtomicOrdering::SeqCst) >= 1,
            "frame was accepted while carrier sense was still on"
        );
        assert_eq!(
            modem.csma_seen.lock().last().copied(),
            Some(true),
            "carrier sense must be restored: {:?}",
            modem.csma_seen.lock()
        );
    }

    /// Neither side reports busy. After the cap the ACK is still keyed with
    /// carrier sense off, so a detector that does not show up in status cannot
    /// hold the frame the way modem73 did for 53 s.
    #[tokio::test]
    async fn ack_keys_csma_off_after_cap_when_both_idle() {
        use std::sync::atomic::Ordering as AtomicOrdering;

        let modem = spawn_ack_modem("idle", 0).await;
        let sense = FakeSense::new();
        assert_eq!(sense.state(), ChannelState::Idle);

        let ack = vec![vec![0xB2]];
        let started = Instant::now();
        let keyed = transmit_ack(AckTx {
            frames: &ack,
            preset: Preset::HfPoor,
            air: None,
            sense: Some(sense.as_ref()),
            control: Some(&modem.control),
            kiss: Some(&modem.kiss),
            cap: Duration::from_millis(200),
            slot: Duration::from_millis(20),
            quiet: Duration::from_millis(1),
            csma: true,
            drain_cap: Duration::from_millis(800),
        })
        .await;
        let elapsed = started.elapsed();
        assert!(keyed, "ACK was not handed to the modem");
        assert!(
            elapsed >= Duration::from_millis(150),
            "ACK left before the cap ({elapsed:?}) with carrier sense still on"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "ACK did not key once the cap elapsed ({elapsed:?})"
        );
        assert_eq!(sense.state(), ChannelState::Idle);
        assert!(
            modem.tx_count.load(AtomicOrdering::SeqCst) >= 1,
            "frame was accepted while carrier sense was still on"
        );
        assert_eq!(
            modem.csma_seen.lock().last().copied(),
            Some(true),
            "carrier sense must be restored: {:?}",
            modem.csma_seen.lock()
        );
    }
}
