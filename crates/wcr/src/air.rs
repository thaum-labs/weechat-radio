//! SPDX-License-Identifier: Apache-2.0
//! RF air queue: serialise, prioritise, and defer TX when the channel is busy.

use crate::config::Config;
use crate::modem::control::{ControlClient, ModemStatus};
use crate::presets::{airtime_secs, Preset, Rung};
use crate::proto::{Envelope, MsgId, MsgType, Priority};
use crate::status::SharedStatus;
use parking_lot::Mutex;
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Notify};
use tokio::time::Instant;

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
        if env.kind == MsgType::Ack {
            return Self::Ack;
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
    ) {
        tokio::spawn(async move {
            loop {
                if let Ok(st) = control.get_status().await {
                    self.apply_status(&st);
                    let mut s = snap.lock();
                    s.channel = self.state().as_str().into();
                    s.occupancy_pct = self.occupancy_pct();
                    s.ptt_on = st.ptt_on;
                    s.queue_air = queue.depth();
                }
                let wait = if queue.depth() > 0 {
                    Duration::from_millis(250)
                } else {
                    Duration::from_secs(1)
                };
                tokio::time::sleep(wait).await;
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
}

#[derive(Clone)]
pub struct AirQueue {
    inner: Arc<Mutex<QueueInner>>,
    notify: Arc<Notify>,
}

impl AirQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(QueueInner { items: Vec::new() })),
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
        let idx = g.items.iter().position(|x| x.not_before <= now)?;
        Some(g.items.remove(idx))
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.inner.lock().items.iter().map(|x| x.not_before).min()
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
) {
    loop {
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
            pace_and_send(&queue, &tx, sense.as_ref(), &control, &rf, &snap, item).await;
            continue;
        }

        let notified = queue.notify.notified();
        tokio::pin!(notified);
        if let Some(deadline) = queue.next_deadline() {
            tokio::select! {
                _ = notified => {}
                _ = tokio::time::sleep_until(deadline) => {}
            }
        } else {
            tokio::select! {
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

    for frame in &item.frames {
        if tx.send(frame.clone()).await.is_err() {
            break;
        }
    }

    let hold = airtime_secs(item.preset, item.payload_len(), 0) + rf.turnaround_ms as f64 / 1000.0;
    if hold > 0.0 {
        tokio::time::sleep(Duration::from_secs_f64(hold)).await;
    }

    snap.lock().queue_air = queue.depth();
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
        let h = tokio::spawn(run_air_queue(q.clone(), tx, sense.clone(), None, cfg, snap));
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
        let h = tokio::spawn(run_air_queue(q.clone(), tx, sense.clone(), None, cfg, snap));
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
    async fn background_dropped_when_congested() {
        let q = AirQueue::new();
        let sense = FakeSense::new();
        sense.set_occupancy(80);
        let (tx, mut rx) = mpsc::channel(8);
        let cfg = test_cfg();
        let snap = status::new_shared();
        let h = tokio::spawn(run_air_queue(q.clone(), tx, sense.clone(), None, cfg, snap));
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
        let h = tokio::spawn(run_air_queue(q.clone(), tx, sense.clone(), None, cfg, snap));
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
        assert_eq!(AirClass::classify(&ack, us), AirClass::Ack);
        let mut beacon = msg.clone();
        beacon.kind = MsgType::Beacon;
        assert_eq!(AirClass::classify(&beacon, us), AirClass::Background);
        let mut em = Envelope::new_msg(origin, dest, 2, b"!!".to_vec(), 3, Flags::new()).unwrap();
        em.flags.set_priority(Priority::Emergency);
        assert_eq!(AirClass::classify(&em, us), AirClass::Emergency);
    }
}
