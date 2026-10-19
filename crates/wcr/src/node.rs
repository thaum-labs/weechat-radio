//! SPDX-License-Identifier: Apache-2.0
//! Station runtime: IRC + modem + hub + LAN + relay.

use crate::air::{self, AirItem, AirQueue, ChannelSense, ChannelState, ModemSense};
use crate::config::Config;
use crate::emcomm::{Form, FormKind, Welfare, BULLETIN_CHANNEL, BULLETIN_TTL};
use crate::error::Result;
use crate::ircd::{IrcEvent, IrcEventKind, IrcServer};
use crate::modem::{ControlClient, KissClient, ModemProcess};
use crate::modes::Mode;
use crate::net::hub_client::{ArcFlag, HubClient};
use crate::net::lan::LanMesh;
use crate::net::peers::DirectPeers;
use crate::presets::{self, Preset, Rung};
use crate::proto::frag::{self, FragAssembler};
use crate::proto::{
    load_or_create, split_body_chunks, Callsign, Envelope, Flags, IdentityKeys, MsgId, MsgType,
    Priority, FLAG_GROUP, FLAG_INET_OK, FLAG_NO_INET, FLAG_REQ_ACK, FLAG_THIRD_PARTY, MAX_BODY,
};
use crate::relay::{self, Action, Engine};
use crate::status::{self, SharedStatus};
use crate::store::{Delivery, Store};
use crate::telemetry::{self, TelemetryEvent};
use parking_lot::Mutex;
use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Transports {
    hub: Option<HubClient>,
    kiss: Option<KissClient>,
    control: Option<ControlClient>,
    modem: Option<ModemProcess>,
    air: Option<AirQueue>,
    sense: Option<Arc<ModemSense>>,
    peers: Option<mpsc::Sender<Envelope>>,
    _peers_mesh: Option<DirectPeers>,
    radio_cancel: Option<CancellationToken>,
}

#[derive(Default)]
struct IoSwap {
    kiss_rx: Option<mpsc::Receiver<Vec<u8>>>,
    drop_kiss: bool,
    peer_in: Option<mpsc::Receiver<Envelope>>,
    drop_peer: bool,
}

#[derive(Clone)]
struct Runtime {
    cfg: Arc<Mutex<Config>>,
    store: Arc<Store>,
    keys: IdentityKeys,
    engine: Engine,
    irc: IrcServer,
    txp: Arc<Mutex<Transports>>,
    io_swap: Arc<Mutex<IoSwap>>,
    hub_in_tx: mpsc::Sender<Envelope>,
    hub_flag: ArcFlag,
    pending_inet: Arc<Mutex<Vec<Envelope>>>,
    lan: Option<mpsc::Sender<Envelope>>,
    snap: Arc<SharedStatus>,
    tel: broadcast::Sender<TelemetryEvent>,
    dest_rungs: Arc<Mutex<HashMap<String, usize>>>,
    assembler: Arc<Mutex<FragAssembler>>,
    last_rx_snr: Arc<Mutex<Option<f32>>>,
    radio_lock: Arc<tokio::sync::Mutex<()>>,
    /// One ACK key at a time. The air queue is paused while it runs.
    ack_lock: Arc<tokio::sync::Mutex<()>>,
    mail: crate::mail::MailHandle,
    /// When we last ACKed `msg_id`. A later copy of an unacked frame is
    /// answered again once [`relay::RETRY_ANSWER_GAP_S`] has passed.
    ack_again: Arc<Mutex<HashMap<MsgId, u32>>>,
    /// When we last relayed `msg_id`. Same gap, so an echo does not loop and
    /// a sender retry still gets forwarded if the receipt never came back.
    relay_again: Arc<Mutex<HashMap<MsgId, u32>>>,
    /// Ids we have heard from a station further along, or an ACK from
    /// someone else. A sender retry is not relayed again after that.
    downstream_heard: Arc<Mutex<HashSet<MsgId>>>,
    /// Our own lines a relay was heard carrying on (fewer hops than we keyed).
    carried_on: Arc<Mutex<HashSet<MsgId>>>,
    /// Message ids already counted in `relayed_unique` this process.
    relayed_ids: Arc<Mutex<HashSet<MsgId>>>,
    /// Frames already handed to the hub socket or the pending queue.
    hub_offered: Arc<Mutex<HashSet<MsgId>>>,
    /// Origins already told that this frame reached the hub.
    hub_marked: Arc<Mutex<HashSet<MsgId>>>,
    /// Msgids whose sender already has [gw], so a later RF echo does not paint [rl].
    gw_marked: Arc<Mutex<HashSet<MsgId>>>,
    /// Msgids that already painted [rl]. A neighbour relay does not paint it again.
    rl_painted: Arc<Mutex<HashSet<MsgId>>>,
    /// Lines and mail keyed or heard here, awaiting or just given their
    /// answer. A due beacon waits for them.
    exchanges: Arc<Mutex<relay::Exchanges>>,
    /// When our last beacon reached the modem (unix seconds, 0 for never).
    beacon_keyed: Arc<std::sync::atomic::AtomicU32>,
    /// A frame from another station has decoded here since start.
    heard_on_air: Arc<std::sync::atomic::AtomicBool>,
    /// Stations heard on RF with no relay between, and when.
    neighbours: Arc<Mutex<HashMap<String, u32>>>,
    /// When this node started listening.
    started: u32,
    /// Copies of someone else's line this station has keyed.
    relay_copies: Arc<Mutex<HashMap<MsgId, RelayCopies>>>,
    /// The roster this station last sent for each closed channel.
    rosters: Arc<Mutex<HashMap<String, MsgId>>>,
    /// Retries of someone else's line heard here on the air from its
    /// originator's side, by line, and copies of our own receipt keyed for
    /// it. Our receipt goes once, plus once per retry heard (QA v0.1.97: B
    /// keyed its receipt for one group line three times).
    retries_heard: Arc<Mutex<HashMap<MsgId, u32>>>,
    own_receipts: Arc<Mutex<HashMap<MsgId, u32>>>,
}

/// A beacon handed to the air queue that has not reached the modem yet.
#[derive(Clone, Copy)]
struct PendingBeacon {
    id: MsgId,
    /// When the beacon loop handed it over.
    decided: u32,
    /// When it was last seen waiting in the air queue.
    queued_at: u32,
}

/// A beacon out of the air queue reaches the modem within the queue's carrier
/// sense cap (15 s). One gone this long without reaching it never will: the
/// radio restarted under it.
const BEACON_LOST_S: u32 = 60;

/// How many copies of someone else's line we have handed to the modem, and
/// when the last one went (after its slot delay).
#[derive(Clone, Copy, Default)]
struct RelayCopies {
    count: u32,
    last_at: u32,
}

impl Runtime {
    fn hub(&self) -> Option<HubClient> {
        self.txp.lock().hub.clone()
    }
    fn kiss(&self) -> Option<KissClient> {
        self.txp.lock().kiss.clone()
    }
    fn control(&self) -> Option<ControlClient> {
        self.txp.lock().control.clone()
    }
    fn air(&self) -> Option<AirQueue> {
        self.txp.lock().air.clone()
    }
    fn sense(&self) -> Option<Arc<ModemSense>> {
        self.txp.lock().sense.clone()
    }
    fn peers_tx(&self) -> Option<mpsc::Sender<Envelope>> {
        self.txp.lock().peers.clone()
    }

    fn stamp_ack(&self, id: &MsgId) {
        self.ack_again.lock().insert(*id, crate::proto::now_ts());
    }

    fn stamp_relay(&self, id: &MsgId) {
        self.relay_again.lock().insert(*id, crate::proto::now_ts());
    }

    fn ack_due(&self, id: &MsgId, now: u32) -> bool {
        let mut g = self.ack_again.lock();
        let last = g.get(id).copied();
        if relay::answer_again(now, last, relay::RETRY_ANSWER_GAP_S) {
            g.insert(*id, now);
            true
        } else {
            false
        }
    }

    fn relay_due(&self, id: &MsgId, now: u32) -> bool {
        let mut g = self.relay_again.lock();
        let last = g.get(id).copied();
        if relay::answer_again(now, last, relay::RETRY_ANSWER_GAP_S) {
            g.insert(*id, now);
            true
        } else {
            false
        }
    }
}

async fn connect_kiss_retry(
    addr: &str,
) -> crate::error::Result<(KissClient, mpsc::Receiver<Vec<u8>>)> {
    let mut last = None;
    // Audio device open on macOS often takes several seconds; 5s was not enough.
    for _ in 0..80 {
        match KissClient::connect(addr).await {
            Ok(pair) => return Ok(pair),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
    Err(last.unwrap_or_else(|| {
        crate::error::Error::Modem(format!("cannot connect to modem73 KISS at {addr}"))
    }))
}

async fn connect_control_retry(
    addr: &str,
) -> crate::error::Result<(ControlClient, mpsc::Receiver<crate::modem::RxFrameEvent>)> {
    let mut last = None;
    for _ in 0..80 {
        match ControlClient::connect(addr).await {
            Ok(pair) => return Ok(pair),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
    Err(last.unwrap_or_else(|| {
        crate::error::Error::Modem(format!("cannot connect to modem73 control at {addr}"))
    }))
}

enum HubTick {
    Flush(Vec<Envelope>),
    Refused {
        banner: String,
        queued: Vec<Envelope>,
    },
}

fn irc_bind_error(bind: &str, err: std::io::Error) -> crate::error::Error {
    if err.kind() == std::io::ErrorKind::AddrInUse {
        crate::error::Error::Msg(format!(
            "IRC port {bind} is already in use. Stop the other wcr, or set irc.bind in wcr.toml."
        ))
    } else {
        crate::error::Error::Msg(format!("IRC port {bind} failed to bind: {err}"))
    }
}

fn is_hub_refusal(err: &str) -> bool {
    err.contains("already claimed") || err.contains("hub refused")
}

fn hub_refusal(rt: &Runtime) -> Option<String> {
    if rt.hub_flag.get() {
        return None;
    }
    let err = rt.hub_flag.error();
    if is_hub_refusal(&err) {
        Some(err)
    } else {
        None
    }
}

/// Own retries are queued even when the modem is not idle.
///
/// Carrier sense lives in `pace_and_send`, which keys a routine frame once
/// `max_defer_ms` elapses. Skipping the retry here used to `continue` without
/// `bump_retry` whenever modem73 stayed in Rx, so `hold_until` remained in
/// the past, the sidebar sat on "retry now", and AIR stayed 0 (VOX, no rig,
/// noise floor on an open sound card).
fn channel_blocks_own_retry(state: Option<ChannelState>) -> bool {
    match state {
        None | Some(ChannelState::Idle | ChannelState::Rx | ChannelState::Tx) => false,
    }
}

/// Queued / sent / relayed still need an ACK. A closed group that has [ok]
/// from only some members still needs one too. Failed, [all], and a direct
/// or bulletin [ok] do not.
fn still_awaiting_ack(store: &Store, id: &crate::proto::MsgId) -> bool {
    let Ok(Some(m)) = store.get(id) else {
        return false;
    };
    match m.delivery {
        Delivery::Queued | Delivery::Sent | Delivery::Relayed | Delivery::Gateway => true,
        Delivery::Delivered => group_still_waiting(store, &m.env),
        Delivery::Failed | Delivery::All => false,
    }
}

/// The last copy waits two intervals before [no], not one. On a long or busy
/// chain an ACK takes as long to walk back as the line took to walk out, and
/// a late ACK would turn [no] into [ok].
fn ack_wait_runs_on(delivery: Delivery, retries: u32, max: u32) -> bool {
    matches!(delivery, Delivery::Sent | Delivery::Relayed) && retries == max
}

/// `retry-n/max` is progress on a line nobody has carried or answered yet.
/// After [rl], [gw] or [ok] the retries go on quietly: marks never move back.
fn retry_mark_shows(delivery: Option<Delivery>) -> bool {
    matches!(delivery, None | Some(Delivery::Queued | Delivery::Sent))
}

fn finish_ack_walk(rt: &Runtime, id: &crate::proto::MsgId) {
    let _ = rt.store.set_hold(id, 0, 0);
    let _ = rt.store.advance_delivery(id, Delivery::Delivered);
}

fn group_still_waiting(store: &Store, env: &Envelope) -> bool {
    let Some(name) = closed_group_name(env) else {
        return false;
    };
    !store.group_all_received(&name, &env.msg_id).unwrap_or(true)
}

fn closed_group_name(env: &Envelope) -> Option<String> {
    if !env.flags.group() || dest_is_bulletin(env.dest.as_str()) {
        return None;
    }
    Some(env.dest.as_str().to_ascii_lowercase())
}

pub async fn run_node(mut cfg: Config, with_tui: bool) -> Result<()> {
    cfg.normalize();
    crate::config::ensure_dirs()?;
    if cfg.callsign.is_empty() {
        return Err(crate::error::Error::config(
            "no callsign set. Run `wcr setup` first.",
        ));
    }
    let snap = status::new_shared();
    {
        let mut s = snap.lock();
        s.callsign = cfg.callsign.clone();
        s.grid = cfg.grid.clone();
        s.mode = cfg.mode;
        s.ptt = cfg.modem.ptt.clone();
        s.preset = cfg.modem.preset.clone();
        s.set_freq(
            cfg.rf.frequency_khz,
            if cfg.rf.frequency_khz > 0 {
                "manual"
            } else {
                "none"
            },
        );
        s.activity_panel = cfg.ui.activity_panel;
        s.version = crate::update::current_version().into();
        s.tower = cfg.gateway.show_tower(cfg.mode);
    }

    // Bind chat and status before opening the store. A locked SQLite (old
    // LaunchAgent, leftover node) is synchronous and would otherwise leave
    // the GUI on "station starting…" forever.
    let (irc_tx, mut irc_rx) = mpsc::channel(64);
    let irc = IrcServer::new(irc_tx);
    let irc_bind = cfg.irc.bind.clone();
    let listener = tokio::net::TcpListener::bind(&irc_bind)
        .await
        .map_err(|e| irc_bind_error(&irc_bind, e))?;
    {
        let irc_s = irc.clone();
        tokio::spawn(async move {
            if let Err(e) = irc_s.accept_loop(listener).await {
                tracing::error!("irc: {e}");
            }
        });
    }

    let mail_slot: crate::mail::MailSlot = Arc::new(Mutex::new(None));
    let status_bind = cfg.status.bind.clone();
    match tokio::net::TcpListener::bind(&status_bind).await {
        Ok(listener) => {
            tracing::info!("status HTTP on {status_bind}");
            let snap_s = snap.clone();
            let slot = mail_slot.clone();
            tokio::spawn(async move {
                status::serve_listener_with(listener, snap_s, crate::mail::router(slot)).await
            });
        }
        Err(e) => tracing::warn!("status HTTP {status_bind}: {e}"),
    }

    let keys = load_or_create(&cfg.identity_key_path())?;
    let store = Arc::new(Store::open(
        &cfg.store.path,
        cfg.store.max_age_hours,
        cfg.store.max_msgs,
    )?);

    let (tel_tx, tel_rx) = broadcast::channel::<TelemetryEvent>(64);
    if cfg.reports_telemetry() {
        let url = cfg.telemetry.url.clone();
        let keys_t = keys.clone();
        let call = cfg.callsign.clone();
        let snap_t = snap.clone();
        let interval = cfg.telemetry.interval_secs;
        tokio::spawn(telemetry::reporter_loop(
            url, keys_t, call, snap_t, tel_rx, interval,
        ));
    }

    let last_rx_snr = Arc::new(Mutex::new(None::<f32>));
    let hub_flag = ArcFlag::new();
    let (hub_in_tx, mut hub_in_rx) = mpsc::channel::<Envelope>(64);
    let txp = Arc::new(Mutex::new(Transports::default()));
    let io_swap = Arc::new(Mutex::new(IoSwap::default()));
    let mut kiss_rx: Option<mpsc::Receiver<Vec<u8>>> = None;
    let mut peer_in: Option<mpsc::Receiver<Envelope>> = None;

    let mut lan_out: Option<mpsc::Sender<Envelope>> = None;
    let mut lan_in: Option<mpsc::Receiver<Envelope>> = None;
    if cfg.lan.discovery {
        match LanMesh::start(
            &cfg.callsign,
            cfg.lan.port,
            &cfg.lan.service,
            &cfg.lan.hub_advertise,
        )
        .await
        {
            Ok((mesh, tx)) => {
                let _ = mesh.bind_port;
                let peers = mesh.peer_set();
                let snap_l = snap.clone();
                tokio::spawn(async move {
                    let mut tick = tokio::time::interval(Duration::from_secs(1));
                    loop {
                        tick.tick().await;
                        snap_l.lock().lan_peers = peers.lock().unwrap().len();
                    }
                });
                lan_out = Some(tx);
                lan_in = Some(mesh.incoming);
            }
            Err(e) => tracing::warn!("lan: {e}"),
        }
    }

    let engine = Engine::new(store.clone(), cfg.callsign.clone());
    let want_radio = cfg.mode.uses_radio();
    let want_hub = cfg.dials_hub();
    let want_peers = cfg.mode.uses_internet() && !cfg.hub.peers.is_empty();
    let cfg = Arc::new(Mutex::new(cfg));
    let txp_mail = txp.clone();
    let ports = Arc::new(move || {
        let g = txp_mail.lock();
        crate::mail::RadioPorts {
            kiss: g.kiss.clone(),
            control: g.control.clone(),
            air: g.air.clone(),
            sense: g.sense.clone(),
        }
    });
    let mail = crate::mail::start(
        cfg.clone(),
        snap.clone(),
        keys.clone(),
        ports,
        tel_tx.clone(),
    )?;
    *mail_slot.lock() = Some(mail.clone());
    let rt = Runtime {
        cfg,
        store: store.clone(),
        keys,
        engine,
        irc: irc.clone(),
        txp,
        io_swap,
        hub_in_tx: hub_in_tx.clone(),
        hub_flag: hub_flag.clone(),
        pending_inet: Arc::new(Mutex::new(Vec::new())),
        lan: lan_out,
        snap: snap.clone(),
        tel: tel_tx,
        dest_rungs: Arc::new(Mutex::new(HashMap::new())),
        assembler: Arc::new(Mutex::new(FragAssembler::new())),
        last_rx_snr,
        radio_lock: Arc::new(tokio::sync::Mutex::new(())),
        ack_lock: Arc::new(tokio::sync::Mutex::new(())),
        mail,
        ack_again: Arc::new(Mutex::new(HashMap::new())),
        relay_again: Arc::new(Mutex::new(HashMap::new())),
        downstream_heard: Arc::new(Mutex::new(HashSet::new())),
        carried_on: Arc::new(Mutex::new(HashSet::new())),
        relayed_ids: Arc::new(Mutex::new(HashSet::new())),
        hub_offered: Arc::new(Mutex::new(HashSet::new())),
        hub_marked: Arc::new(Mutex::new(HashSet::new())),
        gw_marked: Arc::new(Mutex::new(HashSet::new())),
        rl_painted: Arc::new(Mutex::new(HashSet::new())),
        exchanges: Arc::new(Mutex::new(relay::Exchanges::default())),
        beacon_keyed: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        heard_on_air: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        neighbours: Arc::new(Mutex::new(HashMap::new())),
        started: crate::proto::now_ts(),
        relay_copies: Arc::new(Mutex::new(HashMap::new())),
        rosters: Arc::new(Mutex::new(HashMap::new())),
        retries_heard: Arc::new(Mutex::new(HashMap::new())),
        own_receipts: Arc::new(Mutex::new(HashMap::new())),
    };
    refresh_group_prios(&rt);

    if want_radio {
        if let Err(e) = start_radio(&rt).await {
            tracing::warn!("{e}");
        }
    }
    {
        let rt_w = rt.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(8));
            tick.tick().await;
            loop {
                tick.tick().await;
                let want = rt_w.cfg.lock().mode.uses_radio();
                let modem_dead = {
                    let mut txp = rt_w.txp.lock();
                    txp.modem.as_mut().is_some_and(|m| m.exited())
                };
                if want && (rt_w.kiss().is_none() || modem_dead) {
                    if modem_dead {
                        let note = rt_w
                            .txp
                            .lock()
                            .modem
                            .as_mut()
                            .and_then(|m| m.exit_note())
                            .unwrap_or_else(|| "modem73 exited; restarting radio".into());
                        tracing::warn!("{note}");
                        stop_radio(&rt_w);
                    } else {
                        tracing::warn!("radio sound engine not connected; retrying");
                    }
                    if let Err(e) = start_radio(&rt_w).await {
                        tracing::warn!("{e}");
                    }
                }
            }
        });
    }
    if want_hub {
        if let Err(e) = start_hub(&rt).await {
            tracing::warn!("hub: {e}");
        }
    }
    if want_peers {
        if let Err(e) = start_peers(&rt).await {
            tracing::warn!("peers: {e}");
        }
    }
    drain_io_swap(&rt, &mut kiss_rx, &mut peer_in);

    // Hold-queue pump: relay other stations' frames, not our own (those use ARQ).
    {
        let rt_h = rt.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tick.tick().await;
                let now = crate::proto::now_ts();
                let our = rt_h.engine.our_call.clone();
                if let Ok(due) = rt_h.store.hold_due(now) {
                    for m in due {
                        // Own-origin rows are ARQ retries. may_rebroadcast is
                        // false for them; clearing the hold here cancels the
                        // retry and the queued copy. Leave them for the ARQ loop.
                        if relay::is_own_origin(&our, &m.env) {
                            continue;
                        }
                        let max_age = rt_h.cfg.lock().rf.outbound_max_age_secs;
                        if crate::store::outbound_expired(m.rx_time, now, max_age) {
                            let _ = rt_h.store.set_hold(&m.env.msg_id, 0, 0);
                            continue;
                        }
                        if !relay::may_rebroadcast(&our, &m.env, Some(m.delivery)) {
                            let _ = rt_h.store.set_hold(&m.env.msg_id, 0, 0);
                            if let Some(air) = rt_h.air() {
                                air.cancel(m.env.msg_id);
                            }
                            continue;
                        }
                        let retries = rt_h.store.retries_of(&m.env.msg_id).unwrap_or(0);
                        let heard = rt_h.downstream_heard.lock().contains(&m.env.msg_id)
                            || (m.env.kind != MsgType::Ack
                                && line_reached_neighbours(&rt_h, &m.env));
                        // A repeat that every neighbour no longer needs stays
                        // off the air, and so does one past the cap. The
                        // first relay always goes.
                        let spent = relay_copies_spent(&rt_h, &m.env);
                        if retries > 0 && (heard || spent) {
                            let _ = rt_h.store.set_hold(&m.env.msg_id, 0, m.env.hops_left);
                            continue;
                        }
                        // Our last copy may still be at the modem: this one
                        // waits for the next tick rather than keying behind it.
                        if relay_copy_too_soon(&rt_h, &m.env, now) {
                            continue;
                        }
                        let slot = dispatch_spread(&rt_h, &m.env).await;
                        let max = rt_h.cfg.lock().rf.max_retries;
                        let next_to_origin = relayed_next_to_origin(&rt_h.store, &m.env);
                        if relay::keep_relaying(&m.env, retries, max, heard)
                            || relay::group_walk_continues(&m.env, retries, max, heard)
                            || relay::ack_relay_walks(&m.env, next_to_origin, retries, max, heard)
                        {
                            // Relayed, not Queued, so the echo of this copy does
                            // not cancel the next send.
                            let _ = rt_h
                                .store
                                .advance_delivery(&m.env.msg_id, Delivery::Relayed);
                            let (jitter, preset) = {
                                let cfg = rt_h.cfg.lock();
                                (
                                    cfg.rf.retry_jitter,
                                    Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm),
                                )
                            };
                            // The walk timer starts when this copy keys.
                            let keyed = now.saturating_add(slot.as_secs() as u32);
                            let more = retries + 1;
                            let last = !(relay::keep_relaying(&m.env, more, max, heard)
                                || relay::group_walk_continues(&m.env, more, max, heard)
                                || relay::ack_relay_walks(
                                    &m.env,
                                    next_to_origin,
                                    more,
                                    max,
                                    heard,
                                ));
                            // After the last copy only a relay of it is awaited.
                            let next = if last {
                                keyed.saturating_add(relay::walk_last_wait_s(preset))
                            } else {
                                relay::next_retry_hold_jittered(
                                    retries,
                                    keyed,
                                    m.env.priority(),
                                    jitter,
                                )
                            };
                            let _ = rt_h.store.bump_retry(&m.env.msg_id, next);
                        } else {
                            let _ = rt_h.store.set_hold(&m.env.msg_id, 0, m.env.hops_left);
                        }
                    }
                }
                let max = rt_h.cfg.lock().rf.max_retries;
                if let Ok((o, h)) = rt_h.store.queue_depth(&our, max) {
                    let mut s = rt_h.snap.lock();
                    s.queue_out = o;
                    s.queue_hold = h;
                }
                if let Ok(next) = rt_h.store.next_hold(&our, max) {
                    let mut s = rt_h.snap.lock();
                    match next {
                        Some(h) => s.set_hold_due(h.due, h.kind, now),
                        None => s.set_hold_due(0, "", now),
                    }
                }
            }
        });
    }

    // ARQ: retransmit our unacked messages, stepping the modem down each try.
    {
        let rt_r = rt.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
            loop {
                tick.tick().await;
                let now = crate::proto::now_ts();
                let (max, our) = {
                    let cfg = rt_r.cfg.lock();
                    (cfg.rf.max_retries, rt_r.engine.our_call.clone())
                };
                let max_age = rt_r.cfg.lock().rf.outbound_max_age_secs;
                if let Ok(expired) = rt_r.store.expire_outbound(&our, now, max_age) {
                    for id in expired {
                        if let Some(air) = rt_r.air() {
                            air.cancel(id);
                        }
                        rt_r.irc.tagmsg_delivery(&id.hex(), "failed").await;
                    }
                }
                if let Ok(due) = rt_r.store.retry_due(now, &our, max) {
                    let held = relay::startup_hold_s(
                        now,
                        rt_r.started,
                        rt_r.cfg.lock().rf.startup_hold_s,
                        rt_r.heard_on_air.load(std::sync::atomic::Ordering::Relaxed),
                    );
                    let receipt_wait = {
                        let cfg = rt_r.cfg.lock();
                        relay::exchange_wait_s(
                            Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm),
                        )
                    };
                    for (m, retries) in due {
                        // A line's first copy waits out the start-up hold, so
                        // its retry counts from the hold's end; due now, it
                        // would key straight behind that copy.
                        if held > 0 && is_line(&m.env) {
                            let jitter = rt_r.cfg.lock().rf.retry_jitter;
                            let next = relay::next_retry_hold_jittered(
                                retries,
                                now.saturating_add(held),
                                m.env.priority(),
                                jitter,
                            );
                            let _ = rt_r.store.set_hold(&m.env.msg_id, next, m.env.hops_left);
                            continue;
                        }
                        // Its last copy is still in the air queue or on its
                        // way to the modem: the retry waits until that copy
                        // reaches the modem, which starts the timer again.
                        // QA v0.1.96: A's direct message showed retry-1
                        // before its first copy keyed; QA v0.1.97: the retry
                        // was handed over while the copy waited out carrier
                        // sense, and the modem keyed both back to back. An
                        // emergency line's extra copy, queued for later, is
                        // not waited for.
                        if (is_line(&m.env) || relay::walks_until_relayed(&m.env))
                            && rt_r.air().is_some_and(|a| {
                                a.holds_copy(m.env.msg_id, 0) || a.in_flight(m.env.msg_id)
                            })
                        {
                            continue;
                        }
                        // A due beacon is on its way: it goes first. Handed
                        // over in the same second, the retry took the modem
                        // and held the beacon until it had keyed.
                        if is_line(&m.env) && rt_r.air().is_some_and(|a| a.beacon_on_its_way()) {
                            continue;
                        }
                        // Its last copy reached the modem less than a
                        // receipt's round trip ago and nothing has answered
                        // it yet: the neighbour's receipt is still due, and a
                        // retry keyed now lands on it (QA v0.1.97: A's
                        // retry-1 keyed 3 s after B decoded the first copy).
                        // Emergency keeps its own 8 s pace.
                        if is_line(&m.env)
                            && m.env.priority() != Priority::Emergency
                            && rt_r.exchanges.lock().awaits_answer(
                                relay::ExchangeKey::Line(m.env.msg_id),
                                now,
                                receipt_wait,
                            )
                        {
                            continue;
                        }
                        let walking = relay::walks_until_relayed(&m.env);
                        if walking {
                            let heard = rt_r.downstream_heard.lock().contains(&m.env.msg_id);
                            if !relay::ack_walk_continues(heard, retries, max) {
                                finish_ack_walk(&rt_r, &m.env.msg_id);
                                continue;
                            }
                        } else if !still_awaiting_ack(&rt_r.store, &m.env.msg_id) {
                            let _ = rt_r.store.set_hold(&m.env.msg_id, 0, 0);
                            continue;
                        }
                        if channel_blocks_own_retry(rt_r.sense().map(|s| s.state())) {
                            continue;
                        }
                        let n = retries + 1;
                        if !walking {
                            let mut s = rt_r.snap.lock();
                            s.retries = n;
                        }
                        if !dispatch_rf_rung(&rt_r, &m.env, n).await.unwrap_or(false) {
                            continue;
                        }
                        // An ACK can land while this copy is still keying.
                        let done = if relay::is_channel_meta(&m.env) {
                            rt_r.downstream_heard.lock().contains(&m.env.msg_id)
                        } else {
                            !still_awaiting_ack(&rt_r.store, &m.env.msg_id)
                        };
                        if done {
                            let _ = rt_r.store.set_hold(&m.env.msg_id, 0, 0);
                            if !walking {
                                rt_r.snap.lock().retries = 0;
                            }
                            continue;
                        }
                        let delivery = rt_r.store.delivery_of(&m.env.msg_id).ok().flatten();
                        if !walking && retry_mark_shows(delivery) {
                            let tries = rt_r.store.rf_tx_of(&m.env.msg_id).ok();
                            rt_r.irc
                                .tagmsg_progress(
                                    &m.env.msg_id.hex(),
                                    &format!("retry-{n}/{max}"),
                                    tries,
                                )
                                .await;
                        }
                        // The last copy (n == max) still waits one full interval
                        // for an ACK. [no] is the ack-wait below, not this key.
                        // A walk's last copy awaits only a relay of it.
                        let (jitter, preset) = {
                            let cfg = rt_r.cfg.lock();
                            (
                                cfg.rf.retry_jitter,
                                Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm),
                            )
                        };
                        let next = if walking && n >= max {
                            now.saturating_add(relay::walk_last_wait_s(preset))
                        } else {
                            relay::next_retry_hold_jittered(n, now, m.env.priority(), jitter)
                        };
                        let _ = rt_r.store.bump_retry(&m.env.msg_id, next);
                    }
                }
                if let Ok(due) = rt_r.store.ack_wait_due(now, &our, max) {
                    for m in due {
                        if relay::walks_until_relayed(&m.env) {
                            finish_ack_walk(&rt_r, &m.env.msg_id);
                            continue;
                        }
                        let retries = rt_r.store.retries_of(&m.env.msg_id).unwrap_or(0);
                        if ack_wait_runs_on(m.delivery, retries, max) {
                            let jitter = rt_r.cfg.lock().rf.retry_jitter;
                            let next =
                                relay::next_retry_hold_jittered(max, now, m.env.priority(), jitter);
                            let _ = rt_r.store.bump_retry(&m.env.msg_id, next);
                            continue;
                        }
                        let _ = rt_r.store.set_hold(&m.env.msg_id, 0, 0);
                        if !still_awaiting_ack(&rt_r.store, &m.env.msg_id) {
                            continue;
                        }
                        // Retries are spent and someone ACKed, but not every
                        // member. The line stays [ok]; a late ACK still paints
                        // [all]. Marks never move back.
                        if m.delivery == Delivery::Delivered
                            && group_still_waiting(&rt_r.store, &m.env)
                        {
                            continue;
                        }
                        // A relay was heard carrying the line on. On a long or
                        // busy chain its receipt can take longer to come back
                        // than the retries wait, and [no] then flipped to [ok].
                        // It stays [rl] until the receipt comes, or until it
                        // is `outbound_max_age_secs` old ([no]).
                        if rt_r.carried_on.lock().contains(&m.env.msg_id) {
                            continue;
                        }
                        if rt_r
                            .store
                            .advance_delivery(&m.env.msg_id, Delivery::Failed)
                            .unwrap_or(false)
                        {
                            rt_r.irc
                                .tagmsg_delivery(&m.env.msg_id.hex(), "failed")
                                .await;
                        }
                    }
                }
            }
        });
    }

    let rt_hub = rt.clone();
    let hub_flag_s = hub_flag.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tick.tick().await;
            let pending = {
                let mut s = rt_hub.snap.lock();
                let was = s.hub_ok;
                s.hub_ok = hub_flag_s.get() && rt_hub.cfg.lock().mode.uses_internet();
                if s.hub_ok {
                    if !was {
                        drop(s);
                        Some(HubTick::Flush(
                            rt_hub.pending_inet.lock().drain(..).collect::<Vec<_>>(),
                        ))
                    } else {
                        s.hub_banner.clear();
                        None
                    }
                } else if rt_hub.cfg.lock().mode.uses_internet() {
                    let err = hub_flag_s.error();
                    let banner = if !err.is_empty() {
                        err
                    } else if was {
                        "Internet down, radio only".into()
                    } else {
                        s.hub_banner.clone()
                    };
                    let changed = s.hub_banner != banner;
                    let refused = changed && is_hub_refusal(&banner);
                    s.hub_banner = banner.clone();
                    if refused {
                        drop(s);
                        let queued = rt_hub.pending_inet.lock().drain(..).collect::<Vec<_>>();
                        Some(HubTick::Refused { banner, queued })
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            match pending {
                Some(HubTick::Flush(batch)) => {
                    if rt_hub.cfg.lock().mode.uses_internet() {
                        for env in batch {
                            if let Some(h) = rt_hub.hub() {
                                if h.send(&env).await.is_ok() {
                                    mark_hub_forward(&rt_hub, &env).await;
                                }
                            }
                        }
                    }
                }
                Some(HubTick::Refused { banner, queued }) => {
                    rt_hub.irc.notice_all(&banner).await;
                    for env in queued {
                        let delivery = rt_hub.store.delivery_of(&env.msg_id).ok().flatten();
                        if env.origin.as_str() == rt_hub.engine.our_call
                            && delivery.is_none_or(|d| d.can_advance(Delivery::Failed))
                        {
                            rt_hub
                                .irc
                                .tagmsg_delivery(&env.msg_id.hex(), "failed")
                                .await;
                        }
                    }
                }
                None => {}
            }
        }
    });

    // Beacon
    {
        let rt_b = rt.clone();
        tokio::spawn(async move {
            // When our last beacon reached the modem, and how many have.
            let mut last_beacon = None;
            let mut sent = 0;
            let mut pending: Option<PendingBeacon> = None;
            let mut pace = None;
            let mut busy_until = 0;
            // The one exchange this round's beacon waits out. Once chosen, a
            // later exchange does not hold the beacon again or take it back
            // from the air queue (QA v0.1.96: A's third warm-up beacon was
            // taken back for one invite, then another).
            let mut waits_on: Option<relay::ExchangeKey> = None;
            loop {
                let (jitter_s, congested, uses_rf, vox, preset, defer_s) = {
                    let cfg = rt_b.cfg.lock();
                    (
                        cfg.rf.beacon_jitter_s,
                        cfg.rf.congested_pct,
                        cfg.mode.uses_radio(),
                        cfg.modem.is_vox(),
                        Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm),
                        cfg.rf.max_defer_ms.div_ceil(1000),
                    )
                };
                {
                    let mut s = rt_b.snap.lock();
                    if !uses_rf {
                        s.beacon_due = 0;
                        s.beacon_span = 0;
                        s.beacon_note = "off".into();
                    } else if vox {
                        s.beacon_due = 0;
                        s.beacon_span = 0;
                        s.beacon_note = "vox".into();
                    }
                }
                // VOX beacons are a 1400 ms tone on the air; they walk on inbound frames.
                if !uses_rf || vox {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
                let now = crate::proto::now_ts();
                let ceiling = relay::beacon_ceiling(rt_b.started, last_beacon);
                // Our mail burst goes to the modem at once and keys for
                // minutes; the receiver answers after the lease ends.
                if let Some(a) = rt_b.air() {
                    let mail = if a.is_held_for_mail() {
                        now
                    } else {
                        a.last_mail()
                    };
                    if mail > 0 {
                        rt_b.exchanges
                            .lock()
                            .note(relay::ExchangeKey::Mail, false, mail);
                    }
                }
                let pace_s =
                    *pace.get_or_insert_with(|| relay::beacon_interval_secs(jitter_s) as u32);
                // When this beacon fell due, before any hold.
                let scheduled =
                    relay::next_beacon_at(rt_b.started, last_beacon, sent, pace_s).min(ceiling);
                // An exchange this station is part of. A due beacon waits for
                // one exchange at most, and for a mail until its acceptance.
                // Our own queued frames do not hold it: it goes ahead of them.
                let (wait_s, settle_s) = (
                    relay::exchange_wait_s(preset),
                    relay::exchange_settle_s(preset),
                );
                let open = rt_b.exchanges.lock().in_progress(now, wait_s, settle_s);
                let mail =
                    rt_b.exchanges
                        .lock()
                        .is_open(relay::ExchangeKey::Mail, now, wait_s, settle_s);
                // A warm-up beacon waits for the receipt, not for the rest
                // of a group's receipts after it, and never past two minutes
                // after the last beacon.
                let warm_up = sent < relay::BEACON_WARMUP_COUNT;
                let latest = if warm_up {
                    relay::warm_up_latest(last_beacon.unwrap_or(rt_b.started), defer_s)
                } else {
                    u32::MAX
                };
                let chosen_running = waits_on.is_some_and(|k| {
                    let x = rt_b.exchanges.lock();
                    if warm_up && k != relay::ExchangeKey::Mail {
                        x.awaits_answer(k, now, wait_s)
                    } else {
                        x.is_open(k, now, wait_s, settle_s)
                    }
                });
                let line = relay::beacon_exchange(waits_on, chosen_running, open).filter(|_| {
                    relay::beacon_waits_for_exchange(now, scheduled, preset, warm_up, latest)
                });
                let exchange = if mail && relay::beacon_waits_for_mail(now, ceiling, preset) {
                    Some(relay::ExchangeKey::Mail)
                } else {
                    line
                };
                // Handed to the air queue: the countdown stays at "pending"
                // until the beacon reaches the modem, and the next one counts
                // from then. One still queued when an exchange starts is
                // taken back, to wait for it like a beacon not yet due.
                if let Some(p) = pending.as_mut() {
                    let keyed = rt_b.beacon_keyed.load(std::sync::atomic::Ordering::Relaxed);
                    let air = rt_b.air();
                    let queued = air.as_ref().is_some_and(|a| a.on_its_way(p.id));
                    if keyed >= p.decided {
                        last_beacon = Some(keyed);
                        sent += 1;
                        pending = None;
                        pace = None;
                        busy_until = 0;
                        waits_on = None;
                        continue;
                    }
                    // Taken back once at most, and only by a beacon that has
                    // not waited out an exchange already. One already off the
                    // queue but still waiting out carrier sense is taken back
                    // too (QA v0.1.97: the gateway's beacon keyed between the
                    // chunks of a mail that started arriving as it waited).
                    if let (Some(x), Some(a)) = (exchange, air.as_ref()) {
                        if relay::beacon_taken_back(waits_on, exchange) && a.cancel(p.id) > 0 {
                            tracing::info!("beacon taken back from the air queue to wait for {x}");
                            waits_on = Some(x);
                            pending = None;
                            continue;
                        }
                    }
                    if queued {
                        p.queued_at = now;
                    } else if now.saturating_sub(p.queued_at) > BEACON_LOST_S {
                        tracing::warn!("beacon never reached the modem");
                        last_beacon = Some(p.decided);
                        pending = None;
                        pace = None;
                        waits_on = None;
                        continue;
                    }
                    {
                        let mut s = rt_b.snap.lock();
                        s.beacon_due = p.decided;
                        s.beacon_span = 1;
                        s.beacon_note = "pending".into();
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                let due = scheduled.max(busy_until).min(ceiling);
                let note = if now >= due && exchange.is_some() {
                    "exchange"
                } else if busy_until > now {
                    "busy"
                } else {
                    ""
                };
                {
                    let mut s = rt_b.snap.lock();
                    s.beacon_due = due;
                    s.beacon_span = due
                        .saturating_sub(last_beacon.unwrap_or(rt_b.started))
                        .max(1);
                    s.beacon_note = note.into();
                }
                // Look again every few seconds so the countdown and a mode
                // change show promptly.
                if now < due {
                    let wait = u64::from(due - now).min(5);
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    continue;
                }
                if let Some(x) = exchange {
                    if waits_on.is_none() {
                        tracing::info!("beacon waits for {x}");
                        waits_on = Some(x);
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                if let Some(s) = rt_b.sense() {
                    let occ = s.occupancy_pct();
                    let since_decode = s.since_last_decode().as_secs();
                    let wait =
                        relay::beacon_channel_wait(now, ceiling, occ >= congested, since_decode);
                    if wait > 0 {
                        busy_until = now.saturating_add(wait);
                        tracing::info!("beacon held: channel {occ}% busy, next try in {wait}s");
                        continue;
                    }
                    if occ >= congested {
                        let why = if now >= ceiling {
                            "10 minutes since the last beacon".to_string()
                        } else {
                            format!("no frame decoded in {}s", relay::BEACON_BUSY_DECODE_S)
                        };
                        tracing::info!("beacon goes although the channel reads {occ}% busy: {why}");
                    }
                }
                if let Some(x) = open {
                    tracing::info!(
                        "beacon goes during {x}: it waited {}s, one exchange",
                        now.saturating_sub(scheduled)
                    );
                }
                let cfg = rt_b.cfg.lock().clone();
                let Ok(seq) = rt_b.store.next_seq(&rt_b.engine.our_call) else {
                    continue;
                };
                let Ok(origin) = Callsign::parse(&rt_b.engine.our_call) else {
                    continue;
                };
                let dest = Callsign::from_raw("BEACON");
                let mut flags = Flags::new();
                apply_mode_flags(&mut flags, cfg.mode, false);
                if let Ok(mut env) = Envelope::new_msg(
                    origin,
                    dest,
                    seq,
                    format!("B|{}|{}", cfg.mode.as_str(), rt_b.snap.lock().freq_khz).into_bytes(),
                    1,
                    flags,
                ) {
                    env.kind = MsgType::Beacon;
                    pending = Some(PendingBeacon {
                        id: env.msg_id,
                        decided: now,
                        queued_at: now,
                    });
                    let _ = dispatch(&rt_b, &env).await;
                }
            }
        });
    }

    // Rigctl poll + heard refresh + hub frequency announce
    {
        let rt_f = rt.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            let mut last_hub_khz = u32::MAX;
            let mut last_hub = std::time::Instant::now()
                .checked_sub(Duration::from_secs(60))
                .unwrap_or_else(std::time::Instant::now);
            let mut last_rig = std::time::Instant::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(std::time::Instant::now);
            loop {
                tick.tick().await;
                if rt_f.cfg.lock().rig.enabled && last_rig.elapsed() >= Duration::from_secs(10) {
                    last_rig = std::time::Instant::now();
                    if let Some(c) = rt_f.control() {
                        if let Ok(resp) = c.rigctl("f").await {
                            if let Some(khz) = crate::band::parse_rigctl_hz(&resp) {
                                rt_f.snap.lock().set_freq(khz, "rig");
                            }
                        }
                    }
                }
                refresh_heard(&rt_f);
                refresh_group_prios(&rt_f);
                if let Some(h) = rt_f.hub() {
                    let (khz, heard, hub_ok) = {
                        let khz = rt_f.snap.lock().freq_khz;
                        let hub_ok = rt_f.snap.lock().hub_ok;
                        (khz, hub_register_names(&rt_f), hub_ok)
                    };
                    if hub_ok
                        && (khz != last_hub_khz || last_hub.elapsed() >= Duration::from_secs(60))
                    {
                        let body = serde_json::json!({"heard": heard, "freq_khz": khz}).to_string();
                        let _ = h.send_text(&body).await;
                        last_hub_khz = khz;
                        last_hub = std::time::Instant::now();
                    }
                }
            }
        });
    }

    let mut kiss_rx = kiss_rx;
    let mut lan_in = lan_in;
    let mut peer_in = peer_in;

    loop {
        tokio::select! {
            ev = irc_rx.recv() => {
                let Some(ev) = ev else { break };
                if let Err(e) = handle_irc(&rt, ev).await {
                    tracing::warn!("irc event: {e}");
                }
            }
            frame = recv_opt(&mut kiss_rx) => {
                if let Some(payload) = frame {
                    // A frame off KISS decoded here, mail included. The beacon's
                    // busy hold counts these, not the modem's occupancy alone.
                    if let Some(s) = rt.sense() {
                        s.note_decode();
                    }
                    rt.heard_on_air.store(true, std::sync::atomic::Ordering::Relaxed);
                    if let Some(incoming) = crate::mail::try_decode(&payload) {
                        note_exchange(&rt, None, Some(&incoming));
                        crate::mail::on_rf(&rt.mail, incoming).await;
                    } else if crate::mail::looks_like_mail(&payload) {
                        tracing::info!(
                            "mail frame dropped: {} bytes that do not decode",
                            payload.len()
                        );
                    } else if let Ok(env) = Envelope::decode(&payload) {
                        let snr = rt.last_rx_snr.lock().take();
                        let _ = on_envelope(&rt, env, "rf", snr).await;
                    }
                }
            }
            env = hub_in_rx.recv() => {
                if let Some(env) = env {
                    if rt.cfg.lock().mode.uses_internet() {
                        let _ = on_envelope(&rt, env, "inet", None).await;
                    }
                }
            }
            env = recv_lan(&mut lan_in) => {
                if let Some(env) = env {
                    if rt.cfg.lock().mode.uses_internet() {
                        let _ = on_envelope(&rt, env, "lan", None).await;
                    }
                }
            }
            env = recv_lan(&mut peer_in) => {
                if let Some(env) = env {
                    if rt.cfg.lock().mode.uses_internet() {
                        let _ = on_envelope(&rt, env, "inet", None).await;
                    }
                }
            }
        }
        drain_io_swap(&rt, &mut kiss_rx, &mut peer_in);
        if with_tui {
            // TUI runs in the caller; this loop is the node.
        }
    }
    Ok(())
}

async fn recv_opt(rx: &mut Option<mpsc::Receiver<Vec<u8>>>) -> Option<Vec<u8>> {
    match rx.as_mut() {
        Some(r) => r.recv().await,
        None => std::future::pending().await,
    }
}

async fn recv_lan(rx: &mut Option<mpsc::Receiver<Envelope>>) -> Option<Envelope> {
    match rx.as_mut() {
        Some(r) => r.recv().await,
        None => std::future::pending().await,
    }
}

fn apply_mode_flags(flags: &mut Flags, mode: Mode, third: bool) {
    flags.set(FLAG_INET_OK, mode.inet_ok_on_tx());
    flags.set(FLAG_NO_INET, mode.no_inet_on_tx());
    flags.set(FLAG_THIRD_PARTY, third);
}

fn stamp_own_mode_flags(env: &mut Envelope, our_call: &str, mode: Mode) {
    if env.origin.as_str() != our_call {
        return;
    }
    // The hub-forward notice is RF-only. Restamping it INET_OK would upload it.
    if hub_forward_notice(env).is_some() {
        return;
    }
    apply_mode_flags(&mut env.flags, mode, env.origin.is_guest());
}

async fn handle_irc(rt: &Runtime, ev: IrcEvent) -> Result<()> {
    match ev.kind {
        IrcEventKind::Privmsg { target, text, .. } => {
            if (target.starts_with('#') || target.starts_with('&')) && !dest_is_bulletin(&target) {
                if let Some(err) = crate::slash::channel_limit_error(&target) {
                    rt.irc.notice_all(&err).await;
                    return Ok(());
                }
                let name = crate::slash::group_name(&target);
                let members = rt.store.group_members(&name).map(|m| m.len()).unwrap_or(0);
                if let Some(warn) = crate::slash::group_hop_warning(members, BULLETIN_TTL) {
                    rt.irc.notice_all(&warn).await;
                }
            }
            rt.irc.begin_origin(ev.client_id, &target);
            let sent = send_chat(rt, &target, &text).await;
            rt.irc.end_origin();
            if let Err(e) = sent {
                rt.irc.notice_all(&e.to_string()).await;
                return Ok(());
            }
        }
        IrcEventKind::Radio { args } => {
            let reply = radio_cmd(rt, ev.client_id, &args).await;
            rt.irc.send_radio_reply(ev.client_id, &reply).await;
        }
        IrcEventKind::Part { channel } => {
            let _ = leave_channel(rt, &channel).await;
        }
        IrcEventKind::Invite { target, channel } => {
            let target = target.trim().to_ascii_uppercase();
            let radio = rt.cfg.lock().mode.uses_radio();
            let reply = if let Some(err) = crate::slash::channel_limit_error(&channel) {
                err
            } else if crate::slash::is_bulletin(&channel) {
                crate::slash::invite_over_air_reply(&channel, &target, radio, false)
            } else if target.is_empty() || Callsign::parse(&target).is_err() {
                format!("no over-air notice: '{target}' is not a callsign")
            } else {
                let name = crate::slash::group_name(&channel);
                let us = rt.cfg.lock().callsign.clone();
                let _ = rt.store.group_create(&name, &[us.clone(), target.clone()]);
                publish_hub_membership(rt).await;
                publish_member_roster(rt, &name).await;
                let notice = crate::slash::invite_notice(&channel, &us);
                rt.irc.begin_origin(ev.client_id, &target);
                let sent = send_chat(rt, &target, &notice).await;
                rt.irc.end_origin();
                match sent {
                    Ok(()) => {
                        if let Ok(origin) = Callsign::parse(&us) {
                            if let Ok(dest) = Callsign::parse(&target) {
                                if let Ok(Some(id)) = rt.store.msgid_of_body(
                                    origin.as_str(),
                                    dest.as_str(),
                                    notice.as_bytes(),
                                ) {
                                    rt.irc
                                        .broadcast_privmsg(
                                            origin.as_str(),
                                            &target,
                                            &notice,
                                            Some(&id.hex()),
                                            None,
                                        )
                                        .await;
                                }
                            }
                        }
                        crate::slash::invite_over_air_reply(&channel, &target, radio, true)
                    }
                    Err(e) => format!(
                        "{}: {e}",
                        crate::slash::invite_over_air_reply(&channel, &target, radio, false)
                    ),
                }
            };
            rt.irc.send_radio_reply(ev.client_id, &reply).await;
        }
        IrcEventKind::Join { channel } => {
            if let Some(err) = crate::slash::channel_limit_error(&channel) {
                rt.irc.notice_all(&err).await;
                return Ok(());
            }
            if !dest_is_bulletin(&channel) {
                let name = crate::slash::group_name(&channel);
                let us = rt.cfg.lock().callsign.to_ascii_uppercase();
                let _ = rt.store.group_create(&name, &[us]);
                publish_hub_membership(rt).await;
            }
            send_channel_names(rt, ev.client_id, &channel).await;
            let hist = rt.store.history(Some(&channel), 500)?;
            let irc_target = if channel.starts_with('#') || channel.starts_with('&') {
                channel.clone()
            } else {
                format!("#{channel}")
            };
            let lines: Vec<(
                String,
                String,
                String,
                String,
                String,
                String,
                String,
                String,
            )> = hist
                .into_iter()
                .filter(|m| m.env.kind == MsgType::Msg)
                .map(|m| {
                    let received = if m.rx_time > 0 { m.rx_time } else { m.env.ts };
                    let t = chrono::DateTime::<chrono::Utc>::from_timestamp(received as i64, 0)
                        .unwrap_or(chrono::Utc::now())
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                    let target = if m.env.flags.group()
                        || irc_target.starts_with('#')
                        || irc_target.starts_with('&')
                    {
                        irc_target.clone()
                    } else {
                        m.env.dest.to_string()
                    };
                    let via = rt
                        .store
                        .rx_medium(&m.env.msg_id)
                        .ok()
                        .flatten()
                        .unwrap_or_default();
                    let tries = rt.store.rf_tx_of(&m.env.msg_id).unwrap_or(0);
                    (
                        t,
                        m.env.origin.to_string(),
                        target,
                        m.env.body_text(),
                        m.env.msg_id.hex(),
                        m.delivery.as_str().to_string(),
                        via,
                        if tries > 0 {
                            tries.to_string()
                        } else {
                            String::new()
                        },
                    )
                })
                .collect();
            rt.irc.replay_history(ev.client_id, lines).await;
        }
        IrcEventKind::Names { channel } => {
            send_channel_names(rt, ev.client_id, &channel).await;
        }
        _ => {}
    }
    Ok(())
}

async fn send_channel_names(rt: &Runtime, client_id: u64, channel: &str) {
    let ch = if channel.starts_with('#') || channel.starts_with('&') {
        channel.to_ascii_lowercase()
    } else if dest_is_bulletin(channel) {
        BULLETIN_CHANNEL.to_string()
    } else {
        format!("#{}", channel.to_ascii_lowercase())
    };
    let us = rt.cfg.lock().callsign.to_ascii_uppercase();
    let mut nicks = if dest_is_bulletin(&ch) {
        Vec::new()
    } else {
        let name = crate::slash::group_name(&ch);
        rt.store.group_members(&name).unwrap_or_default()
    };
    if !nicks.iter().any(|n| n.eq_ignore_ascii_case(&us)) && !us.is_empty() {
        nicks.push(us);
    }
    nicks.sort_by_key(|n| n.to_ascii_uppercase());
    nicks.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    rt.irc.reply_names(client_id, &ch, &nicks).await;
}

async fn send_form(rt: &Runtime, target: &str, form: Form) -> Result<String> {
    let cfg_g = rt.cfg.lock().clone();
    let origin = Callsign::parse(&cfg_g.callsign)?;
    let is_group = target.starts_with('#') || target.starts_with('&');
    let dest = if is_group {
        Callsign::from_raw(crate::slash::channel_dest(target))
    } else {
        Callsign::parse(target)?
    };
    let seq = rt.store.next_seq(origin.as_str())?;
    let mut flags = Flags::new().with(FLAG_REQ_ACK);
    apply_mode_flags(&mut flags, cfg_g.mode, origin.is_guest());
    if is_group {
        flags.set(FLAG_GROUP, true);
    }
    let hops = if is_group {
        BULLETIN_TTL
    } else {
        Priority::Routine.default_ttl()
    };
    let body = form.encode();
    let mut env = Envelope::new_msg(origin, dest, seq, body.into_bytes(), hops, flags)?;
    env.kind = MsgType::Form;
    if cfg_g.mode.uses_internet() {
        rt.keys.sign_envelope(&mut env)?;
    }
    rt.store.insert(&env, Delivery::Queued)?;
    dispatch(rt, &env).await?;
    rt.store.set_delivery(&env.msg_id, Delivery::Sent)?;
    let tries = rt.store.rf_tx_of(&env.msg_id).ok().filter(|n| *n > 0);
    rt.irc
        .tagmsg_progress(&env.msg_id.hex(), "sent", tries)
        .await;
    let preview = form.render_text();
    rt.irc
        .broadcast_privmsg(
            &cfg_g.callsign,
            target,
            &preview,
            Some(&env.msg_id.hex()),
            None,
        )
        .await;
    Ok(env.msg_id.hex())
}

async fn send_chat(rt: &Runtime, target: &str, text: &str) -> Result<()> {
    let cfg_g = rt.cfg.lock().clone();
    let had_prefix = Priority::has_prefix(text);
    let (explicit, text) = Priority::parse_prefix(text);
    let origin = Callsign::parse(&cfg_g.callsign)?;
    let is_group = target.starts_with('#') || target.starts_with('&');
    let dest = if is_group {
        Callsign::from_raw(crate::slash::channel_dest(target))
    } else {
        Callsign::parse(target)?
    };
    let prio = if had_prefix {
        explicit
    } else if is_group {
        let name = dest.to_string().to_ascii_lowercase();
        if name == "bulletin" {
            Priority::Routine
        } else {
            match rt.store.group_prio(&name).unwrap_or(0) {
                2 => Priority::Emergency,
                1 => Priority::Priority,
                _ => Priority::Routine,
            }
        }
    } else {
        Priority::Routine
    };
    let mut flags = Flags::new().with(FLAG_REQ_ACK);
    apply_mode_flags(&mut flags, cfg_g.mode, origin.is_guest());
    flags.set_priority(prio);
    if is_group {
        flags.set(FLAG_GROUP, true);
    }
    // Closed groups, and invites to them, use the bulletin hop count. A
    // routine 3 dies before the far end of a five-station line; bulletin's 6
    // crosses that chain.
    let hops = if is_group || crate::slash::parse_invite_text(text).is_some() {
        BULLETIN_TTL
    } else {
        prio.default_ttl()
    };
    let max = MAX_BODY.min(cfg_g.relay.max_message_bytes.max(1));
    let chunks = split_body_chunks(text, max);
    if chunks.is_empty() {
        return Ok(());
    }
    for chunk in chunks {
        let seq = rt.store.next_seq(origin.as_str())?;
        let mut env = Envelope::new_msg(
            origin.clone(),
            dest.clone(),
            seq,
            chunk.into_bytes(),
            hops,
            flags,
        )?;
        if cfg_g.mode.uses_internet() {
            rt.keys.sign_envelope(&mut env)?;
        }
        rt.store.insert(&env, Delivery::Queued)?;
        if let Some(reason) = hub_refusal(rt) {
            if !cfg_g.mode.uses_radio() {
                rt.irc.notice_all(&reason).await;
                rt.irc.tagmsg_delivery(&env.msg_id.hex(), "failed").await;
                continue;
            }
            rt.irc
                .notice_all(&format!("{reason}. The radio copy was still keyed."))
                .await;
        }
        dispatch(rt, &env).await?;
        rt.store.set_delivery(&env.msg_id, Delivery::Sent)?;
        if env.flags.req_ack() && cfg_g.mode.uses_radio() {
            let now = crate::proto::now_ts();
            let jitter = cfg_g.rf.retry_jitter;
            let hold = relay::next_retry_hold_jittered(0, now, prio, jitter);
            let _ = rt.store.set_hold(&env.msg_id, hold, env.hops_left);
        }
        let tries = rt.store.rf_tx_of(&env.msg_id).ok().filter(|n| *n > 0);
        rt.irc
            .tagmsg_progress(&env.msg_id.hex(), "sent", tries)
            .await;
        let band = {
            let s = rt.snap.lock();
            if s.band.is_empty() {
                None
            } else {
                Some(s.band.clone())
            }
        };
        let _ = rt.tel.send(TelemetryEvent {
            ts: env.ts as u64,
            kind: "tx".into(),
            origin: Some(env.origin.to_string()),
            dest: Some(env.dest.to_string()),
            hops: Some(env.hops_left),
            snr: None,
            msgid: Some(env.msg_id.hex()),
            band,
        });
        if prio == Priority::Emergency && cfg_g.mode.uses_radio() {
            let delay = Duration::from_millis(cfg_g.rf.emergency_dup_ms as u64);
            let _ = dispatch_rf_at(rt, &env, 0, delay, 1).await;
        }
    }
    Ok(())
}

fn rf_copy(env: &Envelope, preset: Preset) -> Envelope {
    if preset.unsigned_on_rf() {
        env.without_signature()
    } else {
        env.clone()
    }
}

struct EnqueueOpts {
    rung: Option<Rung>,
    delay: Duration,
    copy: u8,
}

async fn dispatch_rf_rung(rt: &Runtime, env: &Envelope, retries: u32) -> Result<bool> {
    dispatch_rf_at(rt, env, retries, Duration::ZERO, 0).await
}

async fn dispatch_rf_at(
    rt: &Runtime,
    env: &Envelope,
    retries: u32,
    delay: Duration,
    copy: u8,
) -> Result<bool> {
    let cfg = rt.cfg.lock().clone();
    let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm);
    let mut env = env.clone();
    stamp_own_mode_flags(&mut env, &rt.engine.our_call, cfg.mode);
    let rf_env = rf_copy(&env, preset);
    let bytes = rf_env.encode()?;
    let stored = rt
        .dest_rungs
        .lock()
        .get(env.dest.as_str())
        .copied()
        .unwrap_or(preset.ladder_start());
    let frames = rf_frames(&rf_env, &bytes, preset, &cfg)?;
    // The rung must carry the largest frame we are about to key. Sizing it from
    // the pre-fragment envelope forces a split message onto OFDM, which a weak
    // or acoustic path never decodes.
    let widest = frames.iter().map(Vec::len).max().unwrap_or(bytes.len());
    let rung = presets::rung_for(preset, stored, retries, widest);
    let apply = env.origin.as_str() == rt.engine.our_call;
    enqueue_rf(
        rt,
        &rf_env,
        frames,
        preset,
        EnqueueOpts {
            rung: if apply { Some(rung) } else { None },
            delay,
            copy,
        },
    )
    .await
}

/// Encode `rf_env` into the frames that will go on the air, fragmenting so each
/// one fits the preset's PHY capacity.
fn rf_frames(
    rf_env: &Envelope,
    bytes: &[u8],
    preset: Preset,
    cfg: &Config,
) -> Result<Vec<Vec<u8>>> {
    let mtu = preset.payload_bytes();
    if frag::should_fragment(rf_env, bytes.len(), mtu) {
        frag::split_to_fit(rf_env, cfg.rf.frag_k, cfg.rf.frag_m, mtu)
    } else {
        Ok(vec![bytes.to_vec()])
    }
}

async fn enqueue_rf(
    rt: &Runtime,
    rf_env: &Envelope,
    frames: Vec<Vec<u8>>,
    preset: Preset,
    opts: EnqueueOpts,
) -> Result<bool> {
    if rf_env.kind == MsgType::Ack {
        // Key off the IRC/RX loop. Waiting out a busy channel must not stall
        // reading the next frame.
        let rt = rt.clone();
        let frames = frames.clone();
        let env = rf_env.clone();
        let delay = opts.delay;
        tokio::spawn(async move {
            let _ = key_ack_now(&rt, &env, &frames, preset, delay).await;
        });
        return Ok(true);
    }
    // Our own line waits out the start-up hold off the air queue, where a
    // queued copy would hold back everything behind it.
    if own_line_held(rt, rf_env) {
        tracing::info!(
            "line {} waits until another station is heard, at most {}s after start",
            rf_env.msg_id.hex(),
            rt.cfg.lock().rf.startup_hold_s
        );
        let rt = rt.clone();
        let env = rf_env.clone();
        tokio::spawn(async move {
            while own_line_held(&rt, &env) {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            // A roster a newer one replaced while it waited stays off the air.
            if relay::is_channel_meta(&env) && rt.store.hold_until(&env.msg_id).unwrap_or(0) == 0 {
                return;
            }
            if enqueue_air(&rt, &env, frames, preset, opts)
                .await
                .unwrap_or(false)
            {
                restart_retry_timer(&rt, &env);
            }
        });
        return Ok(true);
    }
    enqueue_air(rt, rf_env, frames, preset, opts).await
}

/// Hand a frame other than a receipt to the air queue.
async fn enqueue_air(
    rt: &Runtime,
    rf_env: &Envelope,
    frames: Vec<Vec<u8>>,
    preset: Preset,
    opts: EnqueueOpts,
) -> Result<bool> {
    let Some(air) = rt.air() else {
        if let Some(k) = rt.kiss() {
            for f in &frames {
                k.send(f).await?;
            }
            note_own_rf_tx(rt, rf_env);
        }
        return Ok(true);
    };
    // A relay used to key with no rung, in whatever mode the modem was left
    // in, and modem73 then dropped a frame wider than that mode.
    let rung = opts.rung.or_else(|| {
        if rf_env.origin.as_str() == rt.engine.our_call {
            None
        } else {
            Some(relay::relay_rung(preset))
        }
    });
    let mut item = AirItem::new(rf_env, &rt.engine.our_call, frames, preset).with_copy(opts.copy);
    if !opts.delay.is_zero() {
        item = item.with_delay(opts.delay);
    }
    if let Some(r) = rung {
        item = item.with_rung(r);
    }
    let queued = air.enqueue(item);
    if queued {
        note_own_rf_tx(rt, rf_env);
    }
    Ok(queued)
}

/// ACK frames skip the air queue. Pause it, then key this frame with modem
/// carrier sense off once `emergency_max_defer_ms` has elapsed, or as soon as
/// the modem reports undecodable busy. Carrier sense stays on while a frame
/// from another station is being received. It is restored afterwards.
async fn key_ack_now(
    rt: &Runtime,
    rf_env: &Envelope,
    frames: &[Vec<u8>],
    preset: Preset,
    delay: Duration,
) -> Result<bool> {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    let _key = rt.ack_lock.lock().await;
    // ACKs key one at a time. While this one waited its turn, a station
    // further along may already have been heard carrying it.
    if rt.downstream_heard.lock().contains(&rf_env.msg_id) {
        return Ok(false);
    }
    let rf = rt.cfg.lock().rf.clone();
    let sense = rt.sense();
    let control = rt.control();
    let kiss = rt.kiss();
    let air = rt.air();
    let sent = air::transmit_ack(air::AckTx {
        frames,
        preset,
        air: air.as_ref(),
        sense: sense.as_ref().map(|s| s.as_ref() as &dyn ChannelSense),
        control: control.as_ref(),
        kiss: kiss.as_ref(),
        cap: Duration::from_millis(u64::from(rf.emergency_max_defer_ms)),
        slot: Duration::from_millis(u64::from(rf.slot_ms.max(1))),
        quiet: Duration::from_millis(u64::from(rf.quiet_ms)),
        csma: rf.csma,
        drain_cap: Duration::from_secs(5),
    })
    .await;
    if sent {
        note_own_rf_tx(rt, rf_env);
    }
    Ok(sent)
}

/// Our own line, handed over before another station has been heard and
/// within `rf.startup_hold_s` of start. A neighbour started with us may not
/// be listening yet.
fn own_line_held(rt: &Runtime, env: &Envelope) -> bool {
    relay::is_own_origin(&rt.engine.our_call, env)
        && is_line(env)
        && relay::startup_hold_s(
            crate::proto::now_ts(),
            rt.started,
            rt.cfg.lock().rf.startup_hold_s,
            rt.heard_on_air.load(std::sync::atomic::Ordering::Relaxed),
        ) > 0
}

/// A copy of our line has just gone to the air queue or reached the modem:
/// its next retry counts from now, not from when it was sent, or it keys
/// straight behind.
fn restart_retry_timer(rt: &Runtime, env: &Envelope) {
    let id = env.msg_id;
    let Ok(Some(row)) = rt.store.get(&id) else {
        return;
    };
    if rt.store.hold_until(&id).unwrap_or(0) == 0 {
        return;
    }
    let retries = rt.store.retries_of(&id).unwrap_or(0);
    let jitter = rt.cfg.lock().rf.retry_jitter;
    let next =
        relay::next_retry_hold_jittered(retries, crate::proto::now_ts(), env.priority(), jitter);
    let _ = rt.store.set_hold(&id, next, row.env.hops_left);
}

fn is_line(env: &Envelope) -> bool {
    matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    )
}

/// A chat or mail frame keyed or heard here, as part of an exchange a due
/// beacon waits for.
fn note_exchange(rt: &Runtime, env: Option<&Envelope>, mail: Option<&crate::mail::Incoming>) {
    let step = match (env, mail) {
        (Some(env), _) => relay::exchange_step(env),
        (None, Some(m)) => Some((relay::ExchangeKey::Mail, m.answers())),
        (None, None) => None,
    };
    if let Some((key, answer)) = step {
        rt.exchanges
            .lock()
            .note(key, answer, crate::proto::now_ts());
    }
}

/// Frames as they reach the modem. The beacon's countdown and the exchanges
/// a due beacon waits for go by these: the air queue and the receipt path
/// hand frames over long before the modem gets them on a busy channel.
fn watch_keyed(rt: &Runtime, kiss: &KissClient, cancel: CancellationToken) {
    let mut keyed = kiss.keyed();
    let rt = rt.clone();
    tokio::spawn(async move {
        loop {
            let payload = tokio::select! {
                _ = cancel.cancelled() => return,
                got = keyed.recv() => match got {
                    Ok(p) => p,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                },
            };
            if let Some(a) = rt.air() {
                a.note_keyed(&payload);
            }
            if let Some(m) = crate::mail::try_decode(&payload) {
                note_exchange(&rt, None, Some(&m));
            } else if let Ok(env) = Envelope::decode(&payload) {
                if env.kind == MsgType::Beacon {
                    rt.beacon_keyed
                        .store(crate::proto::now_ts(), std::sync::atomic::Ordering::Relaxed);
                }
                // A line's next retry counts from when this copy reached the
                // modem, not from when it was handed over: one that waited
                // in the air queue would retry just behind itself. So does
                // our own receipt walking home from a far member (QA
                // v0.1.97: its first copy waited out the group dither, and
                // its walk keyed 17 s behind it).
                if (is_line(&env) || env.kind == MsgType::Ack)
                    && relay::is_own_origin(&rt.engine.our_call, &env)
                {
                    restart_retry_timer(&rt, &env);
                }
                note_exchange(&rt, Some(&env), None);
            }
        }
    });
}

/// Remember a station heard straight from itself on RF.
fn note_neighbour(rt: &Runtime, env: &Envelope) {
    if env
        .origin
        .as_str()
        .eq_ignore_ascii_case(&rt.engine.our_call)
    {
        return;
    }
    let line = env
        .acked_id()
        .filter(|_| env.kind == MsgType::Ack)
        .and_then(|id| rt.store.get(&id).ok().flatten())
        .map(|m| m.env);
    if relay::heard_direct(env, line.as_ref()) {
        rt.neighbours.lock().insert(
            env.origin.as_str().to_ascii_uppercase(),
            crate::proto::now_ts(),
        );
    }
}

/// Stations heard directly lately. Empty until every neighbour has had time
/// to beacon, so a cold start relays as if none were known.
fn current_neighbours(rt: &Runtime) -> Vec<String> {
    let now = crate::proto::now_ts();
    if now.saturating_sub(rt.started) < relay::NEIGHBOUR_WARMUP_S {
        return Vec::new();
    }
    let mut g = rt.neighbours.lock();
    g.retain(|_, t| now.saturating_sub(*t) < relay::NEIGHBOUR_TTL_S);
    g.keys().cloned().collect()
}

/// Every station we hear directly already has `line` (our stored copy).
fn line_reached_neighbours(rt: &Runtime, line: &Envelope) -> bool {
    let mut holders = vec![line.origin.as_str().to_string()];
    if let Some(name) = closed_group_name(line) {
        holders.extend(
            rt.store
                .receipts_for(&name, &line.msg_id)
                .unwrap_or_default(),
        );
    }
    // Fed by our only neighbour, we take ourselves for the chain end. Not for
    // a direct line: only its addressee's ACK says it arrived, and a station
    // beyond us whose beacons we all missed looks the same as none.
    let came_relayed = !relay::is_direct_line(line)
        && !relay::heard_from_origin(line, line.hops_left.saturating_add(1));
    relay::neighbours_hold(&current_neighbours(rt), &holders, came_relayed)
}

/// We have keyed our cap of copies of someone else's line or receipt.
fn relay_copies_spent(rt: &Runtime, env: &Envelope) -> bool {
    rt.relay_copies
        .lock()
        .get(&env.msg_id)
        .is_some_and(|c| c.count >= relay::relay_max_copies(env))
}

/// Our receipt for `line` has gone as often as its originator asked: once,
/// plus once per retry heard from its side on the air.
fn own_receipt_spent(rt: &Runtime, line: &MsgId) -> bool {
    let keyed = rt.own_receipts.lock().get(line).copied().unwrap_or(0);
    let asked = rt.retries_heard.lock().get(line).copied().unwrap_or(0);
    !relay::own_receipt_allowed(keyed, asked)
}

/// Our receipt for `line` is still walking home hop by hop, and its next
/// copy is still to come.
fn own_receipt_walking(rt: &Runtime, line: &MsgId) -> bool {
    let Ok(Some(ack)) = rt.store.own_ack_for(line, &rt.engine.our_call) else {
        return false;
    };
    if rt.store.hold_until(&ack.env.msg_id).unwrap_or(0) == 0 {
        return false;
    }
    let retries = rt.store.retries_of(&ack.env.msg_id).unwrap_or(0);
    let max = rt.cfg.lock().rf.max_retries;
    let heard = rt.downstream_heard.lock().contains(&ack.env.msg_id);
    relay::ack_walk_continues(heard, retries, max)
}

/// Our receipt for `line` went to the air.
fn note_own_receipt(rt: &Runtime, line: &MsgId) {
    let mut g = rt.own_receipts.lock();
    if g.len() > 4096 {
        g.clear();
    }
    *g.entry(*line).or_default() += 1;
}

/// Our last copy of someone else's line or receipt may still be waiting at
/// the modem. One handed over now would key straight after it and share its
/// collision.
fn relay_copy_too_soon(rt: &Runtime, env: &Envelope, now: u32) -> bool {
    rt.relay_copies.lock().get(&env.msg_id).is_some_and(|c| {
        !relay::answer_again(
            now,
            Some(c.last_at),
            relay::relay_copy_gap_s(env.priority()),
        )
    })
}

/// We have handed at least one copy of someone else's frame to the modem.
fn carried(rt: &Runtime, id: &MsgId) -> bool {
    rt.relay_copies.lock().get(id).is_some_and(|c| c.count > 0)
}

fn note_own_rf_tx(rt: &Runtime, env: &Envelope) {
    if env.origin.as_str() != rt.engine.our_call {
        return;
    }
    let _ = rt.store.bump_rf_tx(&env.msg_id);
}

async fn send_rf(
    rt: &Runtime,
    rf_env: &Envelope,
    bytes: &[u8],
    preset: Preset,
    cfg: &Config,
) -> Result<()> {
    let frames = rf_frames(rf_env, bytes, preset, cfg)?;
    enqueue_rf(
        rt,
        rf_env,
        frames,
        preset,
        EnqueueOpts {
            rung: None,
            delay: Duration::ZERO,
            copy: 0,
        },
    )
    .await
    .map(|_| ())
}

async fn dispatch(rt: &Runtime, env: &Envelope) -> Result<()> {
    dispatch_with(rt, env, false, Duration::ZERO).await
}

/// Key someone else's frame, or a group receipt, in a random frame slot so
/// two neighbours answering the same frame do not land on top of each other
/// at the station between them. Returns the slot's delay.
async fn dispatch_spread(rt: &Runtime, env: &Envelope) -> Duration {
    let preset = Preset::parse(&rt.cfg.lock().modem.preset).unwrap_or(Preset::VhfFm);
    let delay = relay::spread_delay(preset, env.priority());
    let _ = dispatch_with(rt, env, false, delay).await;
    delay
}

/// `delay` holds the radio copy back that long; the hub and LAN copies go now.
async fn dispatch_with(
    rt: &Runtime,
    env: &Envelope,
    force_inet: bool,
    delay: Duration,
) -> Result<()> {
    let cfg = rt.cfg.lock().clone();
    let mut env = env.clone();
    stamp_own_mode_flags(&mut env, &rt.engine.our_call, cfg.mode);
    if force_inet {
        env.flags.set(FLAG_NO_INET, false);
        env.flags.set(FLAG_INET_OK, true);
    }
    let foreign = !env
        .origin
        .as_str()
        .eq_ignore_ascii_case(&rt.engine.our_call);
    let mut relayed = false;
    if cfg.mode.uses_radio()
        && dispatch_rf_at(rt, &env, 0, delay, 0).await.unwrap_or(false)
        && foreign
    {
        relayed = true;
        if env.kind != MsgType::Ack {
            rt.engine
                .sides
                .lock()
                .relaying(env.msg_id, std::time::Instant::now() + delay);
        }
        let mut copies = rt.relay_copies.lock();
        if copies.len() > 4096 {
            copies.clear();
        }
        let c = copies.entry(env.msg_id).or_default();
        c.count += 1;
        c.last_at = crate::proto::now_ts().saturating_add(delay.as_secs() as u32);
    }
    let via_inet =
        force_inet || (cfg.mode.uses_internet() && env.flags.inet_ok() && inet_gap_for(rt, &env));
    if via_inet {
        if foreign {
            relayed = true;
        }
        if force_inet && !cfg.mode.uses_internet() {
            if let Some(h) = rt.hub() {
                if h.send(&env).await.is_ok() {
                    rt.hub_offered.lock().insert(env.msg_id);
                    mark_hub_forward(rt, &env).await;
                }
            }
            if let Some(p) = rt.peers_tx() {
                let _ = p.send(env.clone()).await;
            }
        } else if offer_hub(rt, &env).await {
            mark_hub_forward(rt, &env).await;
        }
        if let Some(l) = &rt.lan {
            let _ = l.send(env.clone()).await;
        }
    }
    if relayed {
        note_relayed(rt, &env);
    }
    Ok(())
}

fn note_relayed(rt: &Runtime, env: &Envelope) {
    tracing::info!("{}", relay_log_line(env));
    let mut snap = rt.snap.lock();
    snap.relayed = snap.relayed.saturating_add(1);
    if rt.relayed_ids.lock().insert(env.msg_id) {
        snap.relayed_unique = snap.relayed_unique.saturating_add(1);
    }
}

/// One line per frame this station forwards for someone else.
/// Text frames include a short preview. ACK and other binary frames do not.
pub(crate) fn relay_log_line(env: &Envelope) -> String {
    format!(
        "relayed {} {} -> {} id {}{}",
        env.kind.as_str(),
        env.origin,
        env.dest,
        env.msg_id,
        relay_preview(env)
    )
}

fn relay_preview(env: &Envelope) -> String {
    let text_kind = matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    );
    if !text_kind {
        return String::new();
    }
    let Ok(text) = std::str::from_utf8(&env.body) else {
        return String::new();
    };
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return String::new();
    }
    let preview: String = text.chars().take(80).collect();
    if preview.is_empty() {
        String::new()
    } else {
        format!(" {preview}")
    }
}

/// Hand `env` to the hub once. True only when this call's send succeeded.
/// A failed send is not recorded, so a later copy can try again. A down hub
/// queues the frame and returns false; the flush marks it when the socket accepts.
async fn offer_hub(rt: &Runtime, env: &Envelope) -> bool {
    if !rt.cfg.lock().mode.uses_internet() {
        return false;
    }
    if rt.hub_offered.lock().contains(&env.msg_id) {
        return false;
    }
    let sent = if rt.hub_flag.get() {
        if let Some(h) = rt.hub() {
            match h.send(env).await {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(
                        "hub send {} {} -> {} failed: {e}",
                        env.kind.as_str(),
                        env.origin,
                        env.dest
                    );
                    false
                }
            }
        } else {
            false
        }
    } else {
        false
    };
    if sent {
        rt.hub_offered.lock().insert(env.msg_id);
    } else if !rt.hub_flag.get() || rt.hub().is_none() {
        rt.pending_inet.lock().push(env.clone());
        rt.hub_offered.lock().insert(env.msg_id);
    }
    if let Some(p) = rt.peers_tx() {
        let _ = p.send(env.clone()).await;
    }
    sent
}

/// `gw <msgid hex>` on a NO_INET status frame. The gateway keys this at the
/// originator so a radio-plus sender can show [gw]. It is not chat.
fn hub_forward_notice(env: &Envelope) -> Option<MsgId> {
    if env.kind != MsgType::Status || env.flags.req_ack() || !env.flags.no_inet() {
        return None;
    }
    let text = std::str::from_utf8(&env.body).ok()?;
    let hex = text.strip_prefix("gw ")?;
    if hex.len() != 16 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    MsgId::parse_hex(hex)
}

async fn mark_hub_forward(rt: &Runtime, env: &Envelope) {
    if !matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    ) || hub_forward_notice(env).is_some()
    {
        return;
    }
    if !rt.hub_marked.lock().insert(env.msg_id) {
        return;
    }
    tracing::info!(
        "forwarded to hub {} {} -> {} id {}",
        env.kind.as_str(),
        env.origin,
        env.dest,
        env.msg_id
    );
    if env
        .origin
        .as_str()
        .eq_ignore_ascii_case(&rt.engine.our_call)
    {
        rt.gw_marked.lock().insert(env.msg_id);
        rt.irc.tagmsg_delivery(&env.msg_id.hex(), "gateway").await;
        return;
    }
    send_hub_forward_notice(rt, env).await;
}

/// One RF hop back to the station that wrote `env`. NO_INET so a gateway does
/// not upload the notice, and no ACK so it does not start its own retry.
async fn send_hub_forward_notice(rt: &Runtime, env: &Envelope) {
    let Ok(seq) = rt.store.next_seq(&rt.engine.our_call) else {
        return;
    };
    let Ok(origin) = Callsign::parse(&rt.engine.our_call) else {
        return;
    };
    let body = format!("gw {}", env.msg_id.hex());
    let Ok(mut notice) = Envelope::new_msg(
        origin,
        env.origin.clone(),
        seq,
        body.into_bytes(),
        1,
        Flags::new().with(FLAG_NO_INET),
    ) else {
        return;
    };
    notice.kind = MsgType::Status;
    let _ = dispatch_rf_at(rt, &notice, 0, Duration::ZERO, 0).await;
}

fn dest_is_bulletin(dest: &str) -> bool {
    dest.trim_start_matches('#')
        .trim_start_matches('&')
        .eq_ignore_ascii_case("bulletin")
}

fn inet_gap_for(rt: &Runtime, env: &Envelope) -> bool {
    let mode = rt.cfg.lock().mode;
    let dest = env.dest.as_str();
    let is_group = env.flags.group();
    let is_bulletin = dest_is_bulletin(dest);
    let freq = rt.snap.lock().freq_khz;
    let dest_heard = rt.store.recently_heard_rf(dest, 600, freq).unwrap_or(false);
    let group_heard = if is_group && !is_bulletin {
        group_member_heard_rf(&rt.store, dest, freq)
    } else {
        false
    };
    relay::needs_inet_gap(mode, is_group, is_bulletin, dest_heard, group_heard)
}

fn group_member_heard_rf(store: &Store, dest: &str, freq: u32) -> bool {
    let name = dest
        .trim_start_matches('#')
        .trim_start_matches('&')
        .to_ascii_lowercase();
    let members = store.group_members(&name).unwrap_or_default();
    members
        .iter()
        .any(|m| store.recently_heard_rf(m, 600, freq).unwrap_or(false))
}

/// Take the [rl] mark once. A later neighbour relay of the same message does
/// not take it again. After [ok] or [all] it is taken without painting, so
/// the mark does not move back.
fn take_relay_mark(
    painted: &mut HashSet<MsgId>,
    id: MsgId,
    advanced: bool,
    prior: Option<Delivery>,
) -> bool {
    let want = advanced || matches!(prior, Some(Delivery::Delivered | Delivery::All));
    want && painted.insert(id)
}

async fn on_envelope(rt: &Runtime, env: Envelope, medium: &str, snr: Option<f32>) -> Result<()> {
    // Safety net: mail frames are WCRM and demuxed before decode. A Mail
    // envelope must not enter chat delivery, retry, or the air queue.
    if env.kind == MsgType::Mail {
        return Ok(());
    }
    if medium == "rf" {
        note_exchange(rt, Some(&env), None);
        note_neighbour(rt, &env);
    }
    if env.origin.as_str() == rt.engine.our_call {
        // A relay closer to the destination has fewer hops than the copy we
        // keyed. That is how an ACK or roster walk stops: our own echo has
        // the same count.
        if let Ok(Some(stored)) = rt.store.get(&env.msg_id) {
            if relay::heard_further(env.hops_left, stored.env.hops_left) {
                rt.carried_on.lock().insert(env.msg_id);
            }
        }
        if relay::walks_until_relayed(&env) {
            if let Ok(Some(stored)) = rt.store.get(&env.msg_id) {
                if relay::heard_further(env.hops_left, stored.env.hops_left) {
                    rt.downstream_heard.lock().insert(env.msg_id);
                    let _ = rt.store.set_hold(&env.msg_id, 0, stored.env.hops_left);
                    let _ = rt.store.advance_delivery(&env.msg_id, Delivery::Delivered);
                    if let Some(air) = rt.air() {
                        air.cancel(env.msg_id);
                    }
                }
            }
        }
        // Our #bulletin carried on by another station: someone has it, which
        // is all the sender waits for. The retries stop; the line keeps
        // [rl] and still takes a late ACK, but it does not go to [no].
        if relay::bulletin_keeps_walking(&env) {
            if let Ok(Some(stored)) = rt.store.get(&env.msg_id) {
                if relay::heard_further(env.hops_left, stored.env.hops_left) {
                    let _ = rt.store.set_hold(&env.msg_id, 0, stored.env.hops_left);
                }
            }
        }
        // Acoustic echo or a relay of our own frame. Control frames (ACK,
        // beacon, …) must not paint [rl] onto the last chat line.
        if !matches!(
            env.kind,
            MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
        ) {
            if let Some(air) = rt.air() {
                air.cancel(env.msg_id);
            }
            return Ok(());
        }
        // [no] stays [no], and [ok] and [all] stay put: an echo heard after
        // the receipt is taken but not painted. A later copy of the same
        // message does not paint [rl] again. A real ACK is the only way off
        // [no], and that path does not come through our own origin.
        let cur = rt.store.delivery_of(&env.msg_id).ok().flatten();
        let advanced = rt
            .store
            .advance_delivery(&env.msg_id, Delivery::Relayed)
            .unwrap_or(false);
        if rt.gw_marked.lock().contains(&env.msg_id) {
            if let Some(air) = rt.air() {
                air.cancel(env.msg_id);
            }
        } else if take_relay_mark(&mut rt.rl_painted.lock(), env.msg_id, advanced, cur) {
            if advanced {
                rt.irc.tagmsg_delivery(&env.msg_id.hex(), "relayed").await;
            }
        } else if !rt.rl_painted.lock().contains(&env.msg_id) {
            if let Some(air) = rt.air() {
                air.cancel(env.msg_id);
            }
        }
        return Ok(());
    }
    if env.kind == MsgType::Frag {
        let reconstructed = rt.assembler.lock().push(&env)?;
        let freq = heard_freq(rt, &env, medium);
        let _ = rt.engine.on_rx(&env, medium, snr, freq)?;
        if let Some(full) = reconstructed {
            return Box::pin(on_envelope(rt, full, medium, snr)).await;
        }
        return Ok(());
    }
    let freq = heard_freq(rt, &env, medium);
    // `Engine::on_rx` marks the original Delivered as soon as it stores the
    // ACK. Read the previous state first: the first ACK must still paint [ok],
    // and a later ACK from another station must not paint it again.
    let acked_prior = if env.kind == MsgType::Ack {
        env.acked_id()
            .and_then(|id| rt.store.delivery_of(&id).ok().flatten())
    } else {
        None
    };
    let decision = rt.engine.on_rx(&env, medium, snr, freq)?;
    if decision.action == Action::Accept {
        if let Ok(Some(stored)) = rt.store.get(&env.msg_id) {
            if relay::may_rebroadcast(&rt.engine.our_call, &stored.env, Some(stored.delivery)) {
                rt.stamp_relay(&env.msg_id);
            }
        }
    }
    if decision.action == Action::Suppress
        && relay::duplicate_cancels_relay(
            medium,
            env.flags.group(),
            dest_is_bulletin(env.dest.as_str()),
        )
        && rt
            .store
            .get(&env.msg_id)
            .ok()
            .flatten()
            .is_none_or(|s| relay::duplicate_suppresses(&env, env.hops_left, s.env.hops_left))
    {
        if let Some(air) = rt.air() {
            air.cancel(env.msg_id);
        }
    }
    {
        let mut snap = rt.snap.lock();
        snap.clock_warn = crate::proto::clock_warn_after(
            snap.clock_warn,
            env.kind,
            env.ts,
            crate::proto::now_ts(),
        );
    }
    if !should_apply_local_effects(decision.action) {
        // A retry of a frame we already displayed or already relayed. The
        // first copy can reach the far end while its ACK never comes back;
        // ignoring every later copy leaves the sender at [rl] after 3/3.
        maybe_answer_retry(rt, &env, medium, snr).await;
        refresh_heard(rt);
        return Ok(());
    }
    match env.kind {
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status => {
            if let Some(id) = hub_forward_notice(&env) {
                // Display only. The retry row stays sent/relayed so [gw] does not
                // stop ARQ. A later [ok], [all], or [no] is not walked back.
                let show = env.dest.as_str().eq_ignore_ascii_case(&rt.engine.our_call)
                    && rt.store.delivery_of(&id).ok().flatten().is_some_and(|d| {
                        d.can_advance(Delivery::Gateway) && d != Delivery::Gateway
                    });
                if show {
                    rt.gw_marked.lock().insert(id);
                    rt.irc.tagmsg_delivery(&id.hex(), "gateway").await;
                }
            } else {
                let target = if env.flags.group() {
                    if dest_is_bulletin(env.dest.as_str()) {
                        BULLETIN_CHANNEL.to_string()
                    } else {
                        format!("#{}", env.dest.to_string().to_ascii_lowercase())
                    }
                } else if env.dest.as_str().eq_ignore_ascii_case(&rt.engine.our_call) {
                    // Incoming DM is `:<sender> PRIVMSG <our nick> :text`. Addressing it
                    // to the sender drops it: the local client nick is ours.
                    rt.engine.our_call.clone()
                } else {
                    env.origin.to_string()
                };
                let text = if env.kind == MsgType::Form {
                    Form::decode(&env.body_text())
                        .map(|f| f.render_text())
                        .unwrap_or_else(|| env.body_text())
                } else {
                    env.body_text()
                };
                let name = env.dest.to_string().to_ascii_lowercase();
                let leave_meta = env.kind == MsgType::Status
                    && env.flags.group()
                    && parse_chan_meta_leave(&text).is_some();
                let members_meta = env.kind == MsgType::Status
                    && env.flags.group()
                    && parse_chan_meta_members(&text).is_some();
                let closed = env.flags.group() && !dest_is_bulletin(env.dest.as_str());
                let member = rt
                    .store
                    .is_group_member(&name, &rt.engine.our_call)
                    .unwrap_or(false);
                let mute_closed = closed && !member && !leave_meta;
                if leave_meta {
                    apply_remote_leave(&rt.store, &name, env.origin.as_str());
                    rt.irc
                        .notice_all(&format!("{} left #{name}", env.origin))
                        .await;
                } else if members_meta {
                    if let Some(members) = parse_chan_meta_members(&text) {
                        let _ = rt.store.group_create(&name, &members);
                    }
                } else if mute_closed {
                    // Closed room we already left: relay only, do not show or ACK.
                } else if env.kind == MsgType::Status {
                    if let Some(prio) = parse_chan_meta_prio(&text) {
                        if env.flags.group() {
                            let name = env.dest.to_string().to_ascii_lowercase();
                            let _ = rt.store.group_set_prio(&name, prio as u8);
                            refresh_group_prios(rt);
                            let note =
                                format!("channel #{} default priority → {}", name, prio.as_str());
                            rt.irc.notice_all(&note).await;
                        }
                    } else if let Some(w) = Welfare::parse(&text) {
                        let _ = rt.store.welfare(env.origin.as_str(), w.as_str());
                        refresh_heard(rt);
                        rt.irc
                            .broadcast_privmsg(
                                env.origin.as_str(),
                                &target,
                                &text,
                                Some(&env.msg_id.hex()),
                                Some(medium),
                            )
                            .await;
                    } else {
                        rt.irc
                            .broadcast_privmsg(
                                env.origin.as_str(),
                                &target,
                                &text,
                                Some(&env.msg_id.hex()),
                                Some(medium),
                            )
                            .await;
                    }
                } else {
                    if env.kind == MsgType::Msg
                        && !env.flags.group()
                        && env.dest.as_str() == rt.engine.our_call
                    {
                        if let Some(ch) = record_remote_invite(
                            &rt.store,
                            &rt.engine.our_call,
                            env.origin.as_str(),
                            &text,
                        ) {
                            publish_hub_membership(rt).await;
                            rt.irc.send_invite(env.origin.as_str(), &ch).await;
                        }
                    }
                    rt.irc
                        .broadcast_privmsg(
                            env.origin.as_str(),
                            &target,
                            &text,
                            Some(&env.msg_id.hex()),
                            Some(medium),
                        )
                        .await;
                }
                if env.kind == MsgType::Checkin {
                    let _ = rt
                        .store
                        .checkin(env.origin.as_str(), &env.body_text(), None, None);
                }
                if !mute_closed
                    && env.flags.req_ack()
                    && (env.dest.as_str() == rt.engine.our_call || env.flags.group())
                    && relay::acks_line(&env)
                {
                    let _ = acknowledge(rt, &env, snr).await;
                }
                if !mute_closed && env.flags.group() {
                    if closed {
                        learn_member(&rt.store, &rt.engine.our_call, &name, env.origin.as_str());
                    }
                    let _ = rt.store.receipt(
                        &env.dest.to_string().to_ascii_lowercase(),
                        &env.msg_id,
                        env.origin.as_str(),
                    );
                    if rt
                        .store
                        .group_all_received(&env.dest.to_string().to_ascii_lowercase(), &env.msg_id)
                        .unwrap_or(false)
                    {
                        let _ = rt.store.set_delivery(&env.msg_id, Delivery::All);
                        rt.irc.tagmsg_delivery(&env.msg_id.hex(), "all").await;
                    }
                }
            }
        }
        MsgType::Ack => {
            if let Some(id) = env.acked_id() {
                let orig = rt.store.get(&id).ok().flatten();
                if !env
                    .origin
                    .as_str()
                    .eq_ignore_ascii_case(&rt.engine.our_call)
                    && relay::ack_ends_relay(env.origin.as_str(), orig.as_ref().map(|o| &o.env))
                {
                    rt.downstream_heard.lock().insert(id);
                }
                // on_rx already wrote Delivered before this runs, so a same-state
                // advance is false. Paint [ok] from the state before that write.
                let already = matches!(acked_prior, Some(Delivery::Delivered | Delivery::All));
                if orig
                    .as_ref()
                    .is_none_or(|o| relay::ack_marks_delivered(&rt.engine.our_call, &o.env))
                {
                    let _ = rt.store.advance_delivery(&id, Delivery::Delivered);
                }
                let now = rt.store.delivery_of(&id).ok().flatten();
                if !already && matches!(now, Some(Delivery::Delivered | Delivery::All)) {
                    rt.irc.tagmsg_delivery(&id.hex(), "delivered").await;
                }
                // A closed group is not done on the first ACK. Keep the retry
                // and the gateway's RF relay until every stored member has
                // ACKed, or the retry count runs out.
                let mut keep = false;
                if let Ok(Some(orig)) = rt.store.get(&id) {
                    if orig.env.flags.group() && !dest_is_bulletin(orig.env.dest.as_str()) {
                        let group = orig.env.dest.to_string().to_ascii_lowercase();
                        learn_member(&rt.store, &rt.engine.our_call, &group, env.origin.as_str());
                        let _ = rt.store.receipt(&group, &id, env.origin.as_str());
                        let _ = rt.store.receipt(&group, &id, &rt.engine.our_call);
                        if rt.store.group_all_received(&group, &id).unwrap_or(false)
                            && rt
                                .store
                                .advance_delivery(&id, Delivery::All)
                                .unwrap_or(false)
                        {
                            rt.irc.tagmsg_delivery(&id.hex(), "all").await;
                        }
                        keep = group_still_waiting(&rt.store, &orig.env);
                        // Our relay of someone else's line: once every
                        // neighbour has it, a queued repeat reaches nobody new.
                        // The first relay may be queued still: it always goes.
                        if !relay::is_own_origin(&rt.engine.our_call, &orig.env)
                            && relay::queued_repeat_unneeded(rt.store.retries_of(&id).unwrap_or(0))
                            && line_reached_neighbours(rt, &orig.env)
                        {
                            let _ = rt.store.set_hold(&id, 0, orig.env.hops_left);
                            if let Some(air) = rt.air() {
                                air.cancel(id);
                            }
                        }
                    }
                }
                // Our relay copy of someone else's bulletin keeps walking.
                // One station's ACK is not the far end hearing it.
                if orig.as_ref().is_some_and(|o| {
                    !relay::is_own_origin(&rt.engine.our_call, &o.env)
                        && relay::bulletin_keeps_walking(&o.env)
                }) {
                    keep = true;
                }
                if !keep {
                    // Our relay of someone else's line keeps the hops we heard
                    // it with. Zeroed, they said the line came from the far
                    // end: the receipts we carry for it then walked as if the
                    // originator were further on, and any later copy of the
                    // line looked like the originator's retry.
                    let hops = orig
                        .as_ref()
                        .filter(|o| !relay::is_own_origin(&rt.engine.our_call, &o.env))
                        .map_or(0, |o| o.env.hops_left);
                    let _ = rt.store.set_hold(&id, 0, hops);
                    if let Some(air) = rt.air() {
                        air.cancel(id);
                    }
                    rt.snap.lock().retries = 0;
                }
            }
            if let Some(snr_db) = env.acked_snr() {
                let preset = Preset::parse(&rt.cfg.lock().modem.preset).unwrap_or(Preset::VhfFm);
                let mut map = rt.dest_rungs.lock();
                let cur = map
                    .get(env.origin.as_str())
                    .copied()
                    .unwrap_or(preset.ladder_start());
                let next = presets::adjust_rung(cur, snr_db);
                map.insert(env.origin.to_string(), next);
                rt.snap.lock().tx_rung = Rung::from_index(next).as_str().into();
            }
        }
        MsgType::Have => {
            handle_have(rt, &env).await?;
        }
        MsgType::Want => {
            handle_want(rt, &env).await?;
        }
        MsgType::Beacon => {}
        MsgType::Ping | MsgType::File => {}
        MsgType::Frag => {}
        MsgType::Mail => {}
    }
    let band = {
        let s = rt.snap.lock();
        s.band.clone()
    };
    let _ = rt.tel.send(TelemetryEvent {
        ts: crate::proto::now_ts() as u64,
        kind: "rx".into(),
        origin: Some(env.origin.to_string()),
        dest: Some(env.dest.to_string()),
        hops: Some(env.hops_left),
        snr,
        msgid: Some(env.msg_id.hex()),
        band: if band.is_empty() { None } else { Some(band) },
    });
    refresh_heard(rt);
    forward_gateway(rt, &env, medium).await
}

fn should_apply_local_effects(action: Action) -> bool {
    action.applies_local_effects()
}

/// ACK a frame addressed to us. Stamps the retry gap first so an echo of the
/// same copy does not key a second receipt.
async fn acknowledge(rt: &Runtime, env: &Envelope, snr: Option<f32>) -> Result<()> {
    rt.stamp_ack(&env.msg_id);
    let seq = rt.store.next_seq(&rt.engine.our_call)?;
    let mut ack = Envelope::ack_for(
        env,
        Callsign::from_raw(rt.engine.our_call.clone()),
        seq,
        snr,
    );
    // Same hop budget as the message. A routine 3 dies before it walks back
    // along a five-station line; a group receipt, or an invite's, uses 6.
    ack.flags.set_priority(env.priority());
    ack.hops_left = relay::ack_hop_budget(
        env.flags.group() || relay::is_group_invite(env),
        env.priority(),
    );
    // Mode flags (including THIRD_PARTY on ~guest) must be set before the
    // signature. dispatch() stamps them again; a later mutation makes the hub
    // drop the ACK as unverified.
    let cfg_now = rt.cfg.lock().clone();
    stamp_own_mode_flags(&mut ack, &rt.engine.our_call, cfg_now.mode);
    if cfg_now.mode.uses_internet() {
        let _ = rt.keys.sign_envelope(&mut ack);
    }
    if cfg_now.mode.uses_radio() {
        note_own_receipt(rt, &env.msg_id);
        // Retry this ACK the way the message is retried, until a station
        // closer to the originator relays it. Next to the originator nobody
        // relays it; the originator's own retry asks for it again.
        if rt.store.insert(&ack, Delivery::Sent).unwrap_or(false)
            && !relay::heard_from_origin(env, env.hops_left)
        {
            let now = crate::proto::now_ts();
            let hold =
                relay::next_retry_hold_jittered(0, now, env.priority(), cfg_now.rf.retry_jitter);
            let _ = rt.store.set_hold(&ack.msg_id, hold, ack.hops_left);
        }
    }
    // Every member that heard the line answers it at once. Two of them on
    // either side of the sender cannot hear each other, so a sub-frame
    // dither still lands both receipts on the sender together.
    let dither = if env.flags.group() {
        let max = rt.cfg.lock().rf.ack_dither_ms as u64;
        let fine = if max > 0 {
            Duration::from_millis(rand::thread_rng().gen_range(0..=max))
        } else {
            Duration::ZERO
        };
        let preset = Preset::parse(&cfg_now.modem.preset).unwrap_or(Preset::VhfFm);
        relay::spread_delay(preset, env.priority()) + fine
    } else {
        Duration::ZERO
    };
    if dither.is_zero() {
        let _ = dispatch(rt, &ack).await;
    } else {
        let cfg_g = rt.cfg.lock().clone();
        if cfg_g.mode.uses_internet() && ack.flags.inet_ok() && inet_gap_for(rt, &ack) {
            offer_hub(rt, &ack).await;
            if let Some(l) = &rt.lan {
                let _ = l.send(ack.clone()).await;
            }
        }
        let _ = dispatch_rf_at(rt, &ack, 0, dither, 0).await;
    }
    Ok(())
}

/// Duplicate of a frame we already handled. Answer it again when the sender
/// is clearly still waiting, and relay it again when we are the middle hop
/// and the receipt has not landed.
async fn maybe_answer_retry(rt: &Runtime, env: &Envelope, medium: &str, snr: Option<f32>) {
    if env.kind == MsgType::Ack || relay::is_channel_meta(env) {
        relay_heard_retry(rt, env).await;
        return;
    }
    if !env.flags.req_ack() {
        return;
    }
    if !matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    ) {
        return;
    }
    if dest_is_bulletin(env.dest.as_str()) && !env.flags.group() {
        return;
    }
    if !relay::acks_line(env) {
        relay_heard_retry(rt, env).await;
        return;
    }
    let now = crate::proto::now_ts();
    let name = env.dest.to_string().to_ascii_lowercase();
    let closed = env.flags.group() && !dest_is_bulletin(env.dest.as_str());
    let member = rt
        .store
        .is_group_member(&name, &rt.engine.our_call)
        .unwrap_or(false);
    let for_us = env.dest.as_str().eq_ignore_ascii_case(&rt.engine.our_call)
        || (env.flags.group() && (!closed || member));
    let from_sender_side = rt
        .store
        .get(&env.msg_id)
        .ok()
        .flatten()
        .is_some_and(|s| relay::copy_wants_new_ack(env.hops_left, s.env.hops_left));
    // A retry heard on the air asks for one more receipt; a copy that came
    // some other way (the hub's echo of a line we answered on the air) does
    // not. Our receipt goes once, plus once per retry heard. One still
    // walking home hop by hop answers with its next copy, due within a
    // retry interval anyway (QA v0.1.97: the far end keyed its receipt's
    // walk and its answer 15 s apart).
    let walking = own_receipt_walking(rt, &env.msg_id);
    if for_us && from_sender_side && medium == "rf" && !walking {
        let mut g = rt.retries_heard.lock();
        if g.len() > 4096 {
            g.clear();
        }
        *g.entry(env.msg_id).or_default() += 1;
    }
    if for_us
        && from_sender_side
        && !walking
        && !own_receipt_spent(rt, &env.msg_id)
        && rt.ack_due(&env.msg_id, now)
    {
        answer_again(rt, env, snr).await;
    }
    if closed && !member {
        return;
    }
    relay_heard_retry(rt, env).await;
}

/// The sender is still waiting for our receipt. On RF, key the ACK we already
/// sent again. Relays that carried it toward the far end have heard it go
/// further and stay quiet; a brand-new ACK id would walk the whole line again.
async fn answer_again(rt: &Runtime, env: &Envelope, snr: Option<f32>) {
    let mode = rt.cfg.lock().mode;
    if mode.uses_radio() && !mode.uses_internet() {
        if let Ok(Some(ack)) = rt.store.own_ack_for(&env.msg_id, &rt.engine.our_call) {
            rt.downstream_heard.lock().remove(&ack.env.msg_id);
            note_own_receipt(rt, &env.msg_id);
            // Every member that heard the retry answers it at once.
            if env.flags.group() {
                let _ = dispatch_spread(rt, &ack.env).await;
            } else {
                let _ = dispatch(rt, &ack.env).await;
            }
            return;
        }
    }
    let _ = acknowledge(rt, env, snr).await;
}

/// Forward an originator retry of a frame we already relayed, until a station
/// further along has been heard. ACKs use this on the way back to the sender.
async fn relay_heard_retry(rt: &Runtime, env: &Envelope) {
    let now = crate::proto::now_ts();
    let Ok(Some(stored)) = rt.store.get(&env.msg_id) else {
        return;
    };
    if relay::heard_further(env.hops_left, stored.env.hops_left) {
        rt.downstream_heard.lock().insert(env.msg_id);
        if relay::keeps_walking(&stored.env)
            || relay::closed_group_walks(&stored.env)
            || stored.env.kind == MsgType::Ack
        {
            let _ = rt.store.set_hold(&env.msg_id, 0, stored.env.hops_left);
            if let Some(air) = rt.air() {
                air.cancel(env.msg_id);
            }
        }
        return;
    }
    if !relay::origin_retry(env.hops_left, stored.env.hops_left) {
        return;
    }
    if ack_from_upstream(rt, &stored.env) {
        return;
    }
    // A receipt keyed again further on is the same receipt. Ours is out,
    // and our own repeat, or the originator's retry, sends it again.
    if env.kind == MsgType::Ack && carried(rt, &env.msg_id) {
        return;
    }
    if env.kind != MsgType::Ack {
        resend_held_acks(rt, &env.msg_id, now).await;
    }
    // A closed group is not done downstream until every member has answered.
    // Hearing the line one hop further on does not bring those receipts back.
    let receipts_missing = closed_group_name(&stored.env).is_some_and(|name| {
        !rt.store
            .group_all_received(&name, &env.msg_id)
            .unwrap_or(false)
    });
    let heard_further = rt.downstream_heard.lock().contains(&env.msg_id);
    if !relay::retry_passes_on(&stored.env, heard_further, receipts_missing) {
        return;
    }
    // Every neighbour has the line, so passing the retry on reaches nobody
    // new. The receipts we hold for it went out above.
    // Our own copy that went out a moment ago carries this retry too.
    if line_reached_neighbours(rt, &stored.env)
        || relay_copies_spent(rt, &stored.env)
        || relay_copy_too_soon(rt, &stored.env, now)
    {
        return;
    }
    if relay::may_rebroadcast(&rt.engine.our_call, &stored.env, Some(stored.delivery))
        && rt.relay_due(&env.msg_id, now)
    {
        let _ = dispatch_spread(rt, &stored.env).await;
    }
}

/// The sender keyed `id` again, so a receipt we carried may not have reached
/// it. Key the ACKs we hold for it once more, unless a station closer to the
/// sender was already heard relaying that ACK. A receipt with a repeat still
/// to come sends that repeat now instead of one more copy, and none goes
/// within a retry interval of our last copy.
async fn resend_held_acks(rt: &Runtime, id: &MsgId, now: u32) {
    let Ok(acks) = rt.store.acks_for(id, &rt.engine.our_call) else {
        return;
    };
    for ack in acks {
        if ack.env.hops_left == 0
            || rt.downstream_heard.lock().contains(&ack.env.msg_id)
            || ack_from_upstream(rt, &ack.env)
        {
            continue;
        }
        if rt.store.hold_until(&ack.env.msg_id).unwrap_or(0) > 0 {
            let _ = rt.store.set_hold(&ack.env.msg_id, now, ack.env.hops_left);
            continue;
        }
        if relay::may_rebroadcast(&rt.engine.our_call, &ack.env, Some(ack.delivery))
            && !relay_copies_spent(rt, &ack.env)
            && !relay_copy_too_soon(rt, &ack.env, now)
            && rt.relay_due(&ack.env.msg_id, now)
        {
            let _ = dispatch_spread(rt, &ack.env).await;
        }
    }
}

/// A receipt from a station between us and the line's originator. It reaches
/// the originator without us.
fn ack_from_upstream(rt: &Runtime, ack: &Envelope) -> bool {
    ack.kind == MsgType::Ack
        && ack.acked_id().is_some_and(|line| {
            rt.engine
                .sides
                .lock()
                .is_upstream(&line, ack.origin.as_str())
        })
}

/// We heard the line `ack` answers straight from its originator. A receipt
/// we relay reaches the originator with no relay after us.
fn relayed_next_to_origin(store: &Store, ack: &Envelope) -> bool {
    if ack.kind != MsgType::Ack {
        return false;
    }
    ack.acked_id()
        .and_then(|line| store.get(&line).ok().flatten())
        .is_some_and(|line| {
            relay::heard_from_origin(&line.env, line.env.hops_left.saturating_add(1))
        })
}

async fn forward_gateway(rt: &Runtime, env: &Envelope, medium: &str) -> Result<()> {
    let cfg_g = rt.cfg.lock().clone();
    let upload = relay::may_inet_forward(
        cfg_g.mode.is_gateway(),
        env.flags.inet_ok(),
        env.flags.no_inet(),
    );
    if medium == "rf" && upload {
        if offer_hub(rt, env).await {
            mark_hub_forward(rt, env).await;
        }
    } else if medium == "rf" && cfg_g.mode.is_gateway() && dest_is_bulletin(env.dest.as_str()) {
        tracing::info!(
            "bulletin {} stays on RF (inet_ok={} no_inet={})",
            env.msg_id,
            env.flags.inet_ok(),
            env.flags.no_inet()
        );
    }
    if medium == "inet" || medium == "lan" {
        let freq = rt.snap.lock().freq_khz;
        let dest = env.dest.as_str();
        let is_group = env.flags.group();
        let is_bulletin = dest_is_bulletin(dest);
        let dest_heard = rt.store.recently_heard_rf(dest, 600, freq).unwrap_or(false);
        let member_heard = if is_group && !is_bulletin {
            group_member_heard_rf(&rt.store, dest, freq)
        } else {
            false
        };
        let any_on_dial = rt.store.recently_heard_any_rf(600, freq).unwrap_or(false);
        let heard =
            relay::rf_egress_heard(is_group, is_bulletin, dest_heard, member_heard, any_on_dial);
        if relay::may_rf_egress(
            cfg_g.mode.is_gateway(),
            cfg_g.gateway.rf_egress,
            heard,
            env.flags.third_party(),
            cfg_g.gateway.third_party_allow(),
            env.flags.inet_ok(),
            env.flags.no_inet(),
        ) {
            let preset = Preset::parse(&cfg_g.modem.preset).unwrap_or(Preset::VhfFm);
            let rf_env = rf_copy(env, preset);
            if let Ok(bytes) = rf_env.encode() {
                let _ = send_rf(rt, &rf_env, &bytes, preset, &cfg_g).await;
            }
        }
    }
    Ok(())
}

async fn handle_have(rt: &Runtime, env: &Envelope) -> Result<()> {
    let cfg_g = rt.cfg.lock().clone();
    let ids: Vec<&str> = std::str::from_utf8(&env.body)
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .collect();
    let dest = env.dest.to_string();
    let ours = rt.store.have_digest(
        &dest,
        cfg_g.group.history_max_msgs,
        cfg_g.group.history_max_age_hours,
    )?;
    let missing: Vec<&str> = ids
        .into_iter()
        .filter(|id| !ours.iter().any(|o| o == id))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let seq = rt.store.next_seq(&cfg_g.callsign)?;
    let mut flags = Flags::new();
    apply_mode_flags(&mut flags, cfg_g.mode, false);
    flags.set(FLAG_GROUP, true);
    let mut want = Envelope::new_msg(
        Callsign::parse(&cfg_g.callsign)?,
        env.origin.clone(),
        seq,
        missing.join(",").into_bytes(),
        2,
        flags,
    )?;
    want.kind = MsgType::Want;
    if cfg_g.mode.uses_internet() {
        let _ = rt.keys.sign_envelope(&mut want);
    }
    dispatch(rt, &want).await
}

async fn handle_want(rt: &Runtime, env: &Envelope) -> Result<()> {
    let ids = std::str::from_utf8(&env.body).unwrap_or("").split(',');
    for id in ids {
        if let Some(mid) = MsgId::parse_hex(id) {
            if let Some(stored) = rt.store.get(&mid)? {
                let _ = dispatch(rt, &stored.env).await;
            }
        }
    }
    Ok(())
}

fn heard_freq(rt: &Runtime, env: &Envelope, medium: &str) -> Option<u32> {
    if env.kind == MsgType::Beacon {
        if let Some(khz) = crate::band::beacon_khz(&env.body) {
            return Some(khz);
        }
    }
    if medium == "rf" {
        let khz = rt.snap.lock().freq_khz;
        if khz > 0 {
            return Some(khz);
        }
    }
    None
}

/// Callsigns this station has heard, plus closed-group names it belongs to.
/// The hub treats both as `serves`, so a group message is routed to members.
fn hub_register_names(rt: &Runtime) -> Vec<String> {
    let mut names: Vec<String> = rt
        .store
        .group_list()
        .unwrap_or_default()
        .into_iter()
        .map(|n| n.to_ascii_uppercase())
        .collect();
    names.extend(
        rt.snap
            .lock()
            .heard
            .iter()
            .map(|h| h.callsign.to_ascii_uppercase()),
    );
    names.sort();
    names.dedup();
    names
}

/// Push closed-group names to the hub immediately. The periodic heard refresh
/// is 60 s, and a group created or joined in that window is not routable yet,
/// so the first message stays at `sent`.
async fn publish_hub_membership(rt: &Runtime) {
    if !rt.cfg.lock().mode.uses_internet() {
        return;
    }
    let Some(h) = rt.hub() else {
        return;
    };
    let khz = rt.snap.lock().freq_khz;
    let heard = hub_register_names(rt);
    let body = serde_json::json!({ "heard": heard, "freq_khz": khz }).to_string();
    let _ = h.send_text(&body).await;
}

fn refresh_heard(rt: &Runtime) {
    let now = crate::proto::now_ts();
    let cutoff = now.saturating_sub(600);
    let Ok(list) = rt.store.heard_list() else {
        return;
    };
    let mut briefs = Vec::new();
    for h in list.into_iter().filter(|h| h.last_heard >= cutoff).take(64) {
        let channels = rt
            .store
            .channels_for(&h.callsign, cutoff)
            .unwrap_or_default();
        let band = h
            .band
            .clone()
            .filter(|b| !b.is_empty())
            .or_else(|| crate::band::band_for_khz(h.freq_khz).map(|s| s.to_string()))
            .unwrap_or_default();
        let welfare = rt
            .store
            .welfare_of(&h.callsign)
            .ok()
            .flatten()
            .and_then(|c| Welfare::parse(&c).map(|w| w.badge().to_string()))
            .unwrap_or_default();
        briefs.push(crate::status::HeardBrief {
            callsign: h.callsign,
            band,
            freq_khz: h.freq_khz,
            medium: h.medium,
            last_heard: h.last_heard,
            snr: h.snr,
            gateway: h.gateway,
            channels,
            welfare,
        });
    }
    rt.snap.lock().heard = briefs;
}

fn refresh_group_prios(rt: &Runtime) {
    let rows = rt.store.group_prios().unwrap_or_default();
    let briefs: Vec<crate::status::GroupPrioBrief> = rows
        .into_iter()
        .map(|(name, bits)| {
            let p = match bits {
                2 => Priority::Emergency,
                1 => Priority::Priority,
                _ => Priority::Routine,
            };
            crate::status::GroupPrioBrief {
                channel: format!("#{name}"),
                priority: p.as_str().to_string(),
            }
        })
        .collect();
    rt.snap.lock().group_prios = briefs;
}

/// A station that sends on a closed group, or answers one of its lines, is
/// in it. The roster that named it may never have reached us, and a JOIN
/// sends nothing over the air, so member lists would never match otherwise.
fn learn_member(store: &Store, our_call: &str, group: &str, call: &str) {
    if store.is_group_member(group, our_call).unwrap_or(false)
        && !store.is_group_member(group, call).unwrap_or(true)
    {
        let _ = store.group_create(group, &[call.to_ascii_uppercase()]);
    }
}

/// Remember a closed channel when another station invites us over the air.
fn record_remote_invite(store: &Store, our_call: &str, from: &str, body: &str) -> Option<String> {
    let ch = crate::slash::parse_invite_text(body)?;
    let name = crate::slash::group_name(&ch);
    let _ = store.group_create(&name, &[our_call.to_string(), from.to_ascii_uppercase()]);
    Some(ch)
}

fn apply_remote_leave(store: &Store, name: &str, who: &str) {
    let _ = store.group_remove_member(name, who);
    if store
        .group_members(name)
        .ok()
        .map(|m| m.is_empty())
        .unwrap_or(true)
    {
        let _ = store.group_forget(name);
    }
}

fn forget_channel_locally(store: &Store, name: &str, our_call: &str) {
    let _ = store.purge_channel(&format!("#{name}"), Some(our_call));
    let _ = store.group_forget(name);
}

fn chan_meta_members_body(members: &[String]) -> String {
    let mut list: Vec<String> = members
        .iter()
        .map(|m| m.trim().to_ascii_uppercase())
        .filter(|m| !m.is_empty())
        .collect();
    list.sort();
    list.dedup();
    format!("WCRMETA members={}", list.join(","))
}

fn parse_chan_meta_members(body: &str) -> Option<Vec<String>> {
    let rest = body.strip_prefix("WCRMETA ")?;
    for part in rest.split_whitespace() {
        if let Some(v) = part.strip_prefix("members=") {
            let list: Vec<String> = v
                .split(',')
                .map(|s| s.trim().to_ascii_uppercase())
                .filter(|s| !s.is_empty())
                .collect();
            if list.is_empty() {
                return None;
            }
            return Some(list);
        }
    }
    None
}

/// Send the channel's member list. It names every member, so it replaces
/// the one sent before: that one leaves the air queue and is not repeated.
/// QA v0.1.96: four invites in a row sent four rosters, priority traffic
/// repeated until relayed, and A's direct message and invites waited minutes
/// behind them.
async fn publish_member_roster(rt: &Runtime, name: &str) {
    let members = rt.store.group_members(name).unwrap_or_default();
    if members.len() < 2 {
        return;
    }
    let sent = send_group_status(
        rt,
        name,
        chan_meta_members_body(&members),
        false,
        BULLETIN_TTL,
    )
    .await;
    let Ok(id) = sent else {
        return;
    };
    let Some(old) = rt.rosters.lock().insert(name.to_string(), id) else {
        return;
    };
    if let Some(air) = rt.air() {
        air.cancel(old);
    }
    let _ = rt.store.set_hold(&old, 0, 0);
}

fn chan_meta_leave_body(callsign: &str) -> String {
    format!("WCRMETA leave={}", callsign.trim().to_ascii_uppercase())
}

fn parse_chan_meta_leave(body: &str) -> Option<String> {
    let rest = body.strip_prefix("WCRMETA ")?;
    for part in rest.split_whitespace() {
        if let Some(v) = part.strip_prefix("leave=") {
            let c = v.trim().to_ascii_uppercase();
            if !c.is_empty() {
                return Some(c);
            }
        }
    }
    None
}

async fn send_group_status(
    rt: &Runtime,
    name: &str,
    body: String,
    force_inet: bool,
    hops: u8,
) -> Result<MsgId> {
    let cfg_g = rt.cfg.lock().clone();
    let origin = Callsign::parse(&cfg_g.callsign)?;
    let dest = Callsign::from_raw(crate::slash::channel_dest(name));
    let seq = rt.store.next_seq(origin.as_str())?;
    let mut flags = Flags::new().with(FLAG_GROUP);
    apply_mode_flags(&mut flags, cfg_g.mode, origin.is_guest());
    flags.set_priority(Priority::Priority);
    let mut env = Envelope::new_msg(origin, dest, seq, body.into_bytes(), hops.max(1), flags)?;
    env.kind = MsgType::Status;
    if cfg_g.mode.uses_internet() || force_inet {
        let _ = rt.keys.sign_envelope(&mut env);
    }
    rt.store.insert(&env, Delivery::Queued)?;
    dispatch_with(rt, &env, force_inet, Duration::ZERO).await?;
    rt.store.set_delivery(&env.msg_id, Delivery::Sent)?;
    if cfg_g.mode.uses_radio() && relay::is_channel_meta(&env) {
        // Nobody ACKs a roster. Repeat it until a relay is heard carrying it.
        let hold = relay::next_retry_hold_jittered(
            0,
            crate::proto::now_ts(),
            env.priority(),
            cfg_g.rf.retry_jitter,
        );
        let _ = rt.store.set_hold(&env.msg_id, hold, env.hops_left);
    }
    Ok(env.msg_id)
}

async fn leave_channel(rt: &Runtime, channel: &str) -> String {
    if crate::slash::is_bulletin(channel) {
        return "#bulletin is public — you cannot leave it".into();
    }
    let name = crate::slash::group_name(channel);
    if name.is_empty() {
        return "usage: /part".into();
    }
    let us = rt.cfg.lock().callsign.clone();
    let _ = send_group_status(
        rt,
        &name,
        chan_meta_leave_body(&us),
        true,
        Priority::Priority.default_ttl(),
    )
    .await;
    forget_channel_locally(&rt.store, &name, &us);
    format!("left #{name}")
}

/// Channel-default priority control frame body (`WCRMETA prio=priority`).
fn chan_meta_prio_body(prio: Priority) -> String {
    format!("WCRMETA prio={}", prio.as_str())
}

fn parse_chan_meta_prio(body: &str) -> Option<Priority> {
    let rest = body.strip_prefix("WCRMETA ")?;
    for part in rest.split_whitespace() {
        if let Some(v) = part.strip_prefix("prio=") {
            return Priority::parse_name(v);
        }
    }
    None
}

fn persist_cfg(cfg: &Config) {
    let _ = cfg.save(&Config::default_path());
}

fn drain_io_swap(
    rt: &Runtime,
    kiss_rx: &mut Option<mpsc::Receiver<Vec<u8>>>,
    peer_in: &mut Option<mpsc::Receiver<Envelope>>,
) {
    let mut swap = rt.io_swap.lock();
    if swap.drop_kiss {
        *kiss_rx = None;
        swap.drop_kiss = false;
    }
    if let Some(rx) = swap.kiss_rx.take() {
        *kiss_rx = Some(rx);
    }
    if swap.drop_peer {
        *peer_in = None;
        swap.drop_peer = false;
    }
    if let Some(rx) = swap.peer_in.take() {
        *peer_in = Some(rx);
    }
}

async fn apply_mode_change(rt: &Runtime, old: Mode, mode: Mode) -> String {
    if old == mode {
        return format!("mode is now {}", mode.display_name());
    }
    let mut notes = Vec::new();
    if Mode::stop_radio(old, mode) {
        stop_radio(rt);
    } else if Mode::start_radio(old, mode) {
        if let Err(e) = start_radio(rt).await {
            notes.push(format!("radio: {e}"));
        }
    }
    if Mode::stop_hub(old, mode) {
        stop_hub(rt);
        stop_peers(rt);
        rt.pending_inet.lock().clear();
    } else if Mode::start_hub(old, mode) {
        if let Err(e) = start_hub(rt).await {
            notes.push(format!("hub: {e}"));
        }
        if let Err(e) = start_peers(rt).await {
            notes.push(format!("peers: {e}"));
        }
    }
    if notes.is_empty() {
        format!("mode is now {}", mode.display_name())
    } else {
        format!("mode is now {} ({})", mode.display_name(), notes.join("; "))
    }
}

async fn start_radio(rt: &Runtime) -> Result<()> {
    if rt.kiss().is_some() {
        return Ok(());
    }
    let _guard = rt.radio_lock.lock().await;
    if rt.kiss().is_some() {
        return Ok(());
    }
    let cfg = rt.cfg.lock().clone();
    let radio_tnc = cfg.modem.uses_tnc();
    let sense = Arc::new(if radio_tnc {
        ModemSense::passive()
    } else {
        ModemSense::new()
    });
    let cancel = CancellationToken::new();
    let mut modem = None;
    let mut control = None;
    let kiss;
    let kiss_rx;

    if radio_tnc {
        {
            let mut s = rt.snap.lock();
            s.ptt = "tnc".into();
            s.tnc = if cfg.modem.is_bluetooth() {
                format!("searching for {}…", cfg.tnc.bt_name)
            } else {
                format!("opening {}…", cfg.tnc.serial)
            };
        }
        let (k, rx) = crate::tnc::start_link(&cfg, rt.snap.clone(), sense.clone());
        kiss = Some(k);
        kiss_rx = Some(rx);
    } else {
        let kiss_addr = format!("{}:{}", cfg.modem.host, cfg.modem.kiss_port);
        let already = KissClient::connect(&kiss_addr).await.ok();
        {
            let mut txp = rt.txp.lock();
            if txp.modem.as_mut().is_some_and(|m| m.exited()) {
                txp.modem = None;
            }
        }
        let mut modem_err = None;
        if already.is_none() && cfg.modem.manage && rt.txp.lock().modem.is_none() {
            match ModemProcess::spawn(&cfg).await {
                Ok(c) => modem = Some(c),
                Err(e) => {
                    modem_err = Some(e.to_string());
                    tracing::warn!("{e}");
                }
            }
        }
        let connected = if let Some(pair) = already {
            Ok(pair)
        } else {
            connect_kiss_retry(&kiss_addr).await
        };
        match connected {
            Ok((k, rx)) => {
                kiss = Some(k);
                kiss_rx = Some(rx);
            }
            Err(e) => {
                // Keep a still-running child so a later retry can attach to KISS
                // (macOS often needs longer than the first connect window).
                if let Some(mut child) = modem {
                    if !child.exited() {
                        let mut txp = rt.txp.lock();
                        if txp.modem.is_none() {
                            txp.modem = Some(child);
                        }
                    }
                }
                if let Some(reason) = modem_err {
                    let mut s = rt.snap.lock();
                    s.audio_label = "no modem".into();
                    return Err(crate::error::Error::Modem(reason));
                }
                let mut s = rt.snap.lock();
                s.audio_label = "no modem".into();
                s.audio_db = presets::AUDIO_FLOOR_DB;
                s.audio_in_db = presets::AUDIO_FLOOR_DB;
                s.audio_out_db = presets::AUDIO_FLOOR_DB;
                return Err(e);
            }
        }
        let ctrl_addr = format!("{}:{}", cfg.modem.host, cfg.modem.control_port);
        if let Ok((c, mut ev)) = connect_control_retry(&ctrl_addr).await {
            if let Some(p) = Preset::parse(&cfg.modem.preset) {
                let _ = c.set_config(p.control_config()).await;
            }
            let _ = c.apply_ptt(&cfg.modem.ptt, &cfg.modem).await;
            if !cfg.modem.audio_input.is_empty() {
                let _ = c
                    .set_config(serde_json::json!({"capture_device": cfg.modem.audio_input}))
                    .await;
            }
            if !cfg.modem.audio_output.is_empty() {
                let _ = c
                    .set_config(serde_json::json!({"playback_device": cfg.modem.audio_output}))
                    .await;
            }
            let snap_c = rt.snap.clone();
            let snr_slot = rt.last_rx_snr.clone();
            let sense_ev = sense.clone();
            let cancel_ev = cancel.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel_ev.cancelled() => return,
                        frame = ev.recv() => {
                            let Some(frame) = frame else { return };
                            sense_ev.note_rx();
                            *snr_slot.lock() = Some(frame.snr);
                            let mut s = snap_c.lock();
                            s.snr = frame.snr;
                            s.ber = frame.ber_pct;
                            s.audio_db = frame.level_db;
                            s.audio_in_db = frame.level_db;
                            s.audio_label = presets::audio_level_label(frame.level_db).into();
                            s.channel = "rx".into();
                        }
                    }
                }
            });
            control = Some(c);
        }
        crate::audio_meter::spawn_input_meter(
            cfg.modem.audio_input.clone(),
            rt.snap.clone(),
            cancel.clone(),
        );
        {
            let mut s = rt.snap.lock();
            if matches!(s.audio_label.as_str(), "no modem" | "no audio") {
                s.audio_label = "—".into();
            }
        }
    }

    let air_q = AirQueue::new();
    if let Some(c) = &control {
        sense
            .clone()
            .spawn_poller(c.clone(), rt.snap.clone(), air_q.clone(), cancel.clone());
    }
    if let Some(k) = &kiss {
        let q = air_q.clone();
        let tx = k.tx.clone();
        let s = sense.clone();
        let ctrl = control.clone();
        let cfg_a = rt.cfg.clone();
        let snap_a = rt.snap.clone();
        let cancel_a = cancel.clone();
        tokio::spawn(async move {
            air::run_air_queue(q, tx, s, ctrl, cfg_a, snap_a, cancel_a).await;
        });
        watch_keyed(rt, k, cancel.clone());
    }

    {
        let mut txp = rt.txp.lock();
        if modem.is_some() {
            txp.modem = modem;
        }
        txp.kiss = kiss;
        txp.control = control;
        txp.air = Some(air_q);
        txp.sense = Some(sense);
        txp.radio_cancel = Some(cancel);
    }
    let mut swap = rt.io_swap.lock();
    swap.kiss_rx = kiss_rx;
    swap.drop_kiss = false;
    Ok(())
}

fn stop_radio(rt: &Runtime) {
    let mut txp = rt.txp.lock();
    if let Some(c) = txp.radio_cancel.take() {
        c.cancel();
    }
    txp.kiss = None;
    txp.control = None;
    txp.air = None;
    txp.sense = None;
    drop(txp.modem.take());
    rt.io_swap.lock().drop_kiss = true;
    let mut s = rt.snap.lock();
    s.tnc.clear();
    s.tnc_ok = false;
    s.channel = "idle".into();
    s.ptt_on = false;
    s.audio_db = presets::AUDIO_FLOOR_DB;
    s.audio_in_db = presets::AUDIO_FLOOR_DB;
    s.audio_out_db = presets::AUDIO_FLOOR_DB;
    s.audio_label = "—".into();
    s.queue_air = 0;
}

async fn start_hub(rt: &Runtime) -> Result<()> {
    if rt.hub().is_some() {
        return Ok(());
    }
    let cfg = rt.cfg.lock().clone();
    if !cfg.hub.enabled() {
        return Ok(());
    }
    match HubClient::connect(
        &cfg.hub.url,
        &cfg.callsign,
        &rt.keys,
        hub_register_names(rt),
        cfg.rf.frequency_khz,
        rt.hub_in_tx.clone(),
        rt.hub_flag.clone(),
    )
    .await
    {
        Ok(h) => {
            rt.txp.lock().hub = Some(h);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn stop_hub(rt: &Runtime) {
    if let Some(h) = rt.txp.lock().hub.take() {
        h.shutdown();
    }
    rt.hub_flag.set(false);
    let mut s = rt.snap.lock();
    s.hub_ok = false;
    s.hub_banner.clear();
}

async fn start_peers(rt: &Runtime) -> Result<()> {
    let peers = rt.cfg.lock().hub.peers.clone();
    if peers.is_empty() {
        return Ok(());
    }
    if rt.peers_tx().is_some() {
        return Ok(());
    }
    match DirectPeers::start(peers).await {
        Ok((mesh, rx, tx)) => {
            {
                let mut txp = rt.txp.lock();
                txp.peers = Some(tx);
                txp._peers_mesh = Some(mesh);
            }
            let mut swap = rt.io_swap.lock();
            swap.peer_in = Some(rx);
            swap.drop_peer = false;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn stop_peers(rt: &Runtime) {
    let mut txp = rt.txp.lock();
    txp.peers = None;
    txp._peers_mesh = None;
    rt.io_swap.lock().drop_peer = true;
}

fn set_manual_freq(rt: &Runtime, khz: u32) -> String {
    {
        let mut cfg = rt.cfg.lock();
        cfg.rf.frequency_khz = khz;
        persist_cfg(&cfg);
    }
    rt.snap.lock().set_freq(khz, "manual");
    crate::band::describe(khz)
}

async fn radio_cmd(rt: &Runtime, client_id: u64, args: &str) -> String {
    let cfg = &rt.cfg;
    let store = &rt.store;
    let snap = &rt.snap;
    let mut sp = args.split_whitespace();
    let cmd = sp.next().unwrap_or("").to_ascii_lowercase();
    match cmd.as_str() {
        "mode" => {
            if let Some(m) = sp.next() {
                match m.parse::<Mode>() {
                    Ok(mode) => {
                        if mode == Mode::Radio {
                            // confirmation is a TUI concern; IRC users pass `mode radio confirm`
                            if sp.next() != Some("confirm") {
                                return "Switching to Radio drops hub chat. Messages stay on RF. Type: /radio mode radio confirm".into();
                            }
                        }
                        let old = cfg.lock().mode;
                        cfg.lock().mode = mode;
                        let show_tower = cfg.lock().gateway.show_tower(mode);
                        {
                            let mut s = snap.lock();
                            s.mode = mode;
                            s.tower = show_tower;
                        }
                        persist_cfg(&cfg.lock());
                        apply_mode_change(rt, old, mode).await
                    }
                    Err(e) => e,
                }
            } else {
                format!("mode {}", cfg.lock().mode.display_name())
            }
        }
        "preset" => {
            if let Some(p) = sp.next() {
                if cfg.lock().modem.uses_tnc() {
                    return "preset is fixed at afsk-1200: the radio's own TNC does the modulation"
                        .into();
                }
                if let Some(pr) = Preset::parse(p) {
                    cfg.lock().modem.preset = pr.as_str().into();
                    snap.lock().preset = pr.as_str().into();
                    persist_cfg(&cfg.lock());
                    format!("preset {}", pr.as_str())
                } else {
                    "unknown preset. Use vhf-fm, hf-good, hf-poor, hf-weak, hf-deep, vox-safe, afsk-1200.".into()
                }
            } else {
                format!("preset {}", cfg.lock().modem.preset)
            }
        }
        "status" => {
            if let Some(code) = sp.next() {
                if let Some(w) = Welfare::parse(code) {
                    let _ = store.welfare(&cfg.lock().callsign, w.as_str());
                    return format!("status {}", w.badge());
                }
            }
            let s = snap.lock().clone();
            let tnc = if s.tnc.is_empty() {
                String::new()
            } else {
                format!(" | tnc {}", s.tnc)
            };
            let now = crate::status::unix_now_f64();
            format!(
                "{} | {} | {} {} | {} | SNR {:.1} | tx {} | retry {} | audio {} | q {}/{} | occ {}% air {} | beacon {} | hold {} | relayed {} unique {} | groups {} | hub {}{}",
                s.mode.display_name(),
                if s.deferred { "wait" } else { &s.channel },
                if s.frequency.is_empty() { "—" } else { s.frequency.trim_end_matches(" MHz") },
                if s.band.is_empty() { "—" } else { &s.band },
                s.preset,
                s.snr,
                if s.tx_rung.is_empty() { "—" } else { &s.tx_rung },
                s.retries,
                s.audio_label,
                s.queue_out,
                s.queue_hold,
                s.occupancy_pct,
                s.queue_air,
                s.beacon_lane(now).label,
                s.hold_lane(now).label,
                s.relayed,
                s.relayed_unique,
                crate::status::group_prio_summary(&s.group_prios),
                if s.hub_ok { "up" } else { "down" },
                tnc
            )
        }
        "group" => {
            let sub = sp.next().unwrap_or("");
            match sub {
                "create" => {
                    let name = crate::slash::group_name(sp.next().unwrap_or(""));
                    let members: Vec<String> = sp.map(|s| s.to_ascii_uppercase()).collect();
                    if name.is_empty() {
                        return "usage: /radio group create <name> <callsigns...>".into();
                    }
                    let _ = store.group_create(&name, &members);
                    publish_hub_membership(rt).await;
                    publish_member_roster(rt, &name).await;
                    format!("group {name} created")
                }
                "invite" | "add" => {
                    let name = crate::slash::group_name(sp.next().unwrap_or(""));
                    let members: Vec<String> = sp.map(|s| s.to_ascii_uppercase()).collect();
                    if name.is_empty() || members.is_empty() {
                        return "usage: /radio group invite <name> <callsigns...>".into();
                    }
                    let _ = store.group_create(&name, &members);
                    publish_hub_membership(rt).await;
                    publish_member_roster(rt, &name).await;
                    format!("invited {} to {name}", members.join(", "))
                }
                "list" => store.group_list().unwrap_or_default().join(", "),
                "members" => {
                    let name = sp.next().unwrap_or("");
                    store.group_members(name).unwrap_or_default().join(", ")
                }
                "leave" | "part" => {
                    let name = sp.next().unwrap_or("").to_string();
                    if name.is_empty() {
                        return "usage: /radio group leave <name>".into();
                    }
                    leave_channel(rt, &name).await
                }
                _ => "usage: /radio group create|list|members|invite|leave".into(),
            }
        }
        "queue" => {
            let our = rt.engine.our_call.clone();
            let max = cfg.lock().rf.max_retries;
            let (o, h) = store.queue_depth(&our, max).unwrap_or((0, 0));
            format!("outbound {o} hold {h}")
        }
        "flush" => {
            let our = cfg.lock().callsign.clone();
            let ids = store.flush_outbound(&our).unwrap_or_default();
            let holds = store.clear_relay_holds(&our).unwrap_or(0);
            for id in &ids {
                if let Some(air) = rt.air() {
                    air.cancel(*id);
                }
                rt.irc.tagmsg_delivery(&id.hex(), "failed").await;
            }
            format!(
                "flushed {} unacked outbound, dropped {} relay holds",
                ids.len(),
                holds
            )
        }
        "trace" => {
            let id = sp.next().unwrap_or("");
            if let Some(mid) = MsgId::parse_hex(id) {
                match store.trace(&mid) {
                    Ok(hops) if !hops.is_empty() => hops
                        .into_iter()
                        .map(|(c, m, t, snr)| {
                            format!("{c} {m} t={t} snr={}", snr.map(|s| format!("{s:.1}")).unwrap_or_else(|| "-".into()))
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => "no hops recorded".into(),
                }
            } else {
                "usage: /radio trace <msgid>".into()
            }
        }
        "clear" => {
            let t = sp.next().unwrap_or("");
            if t.is_empty() {
                return "usage: /radio clear <#channel|callsign>".into();
            }
            let us = cfg.lock().callsign.clone();
            let n = store.purge_channel(t, Some(&us)).unwrap_or(0);
            format!("cleared {n} local messages in {t}")
        }
        "part" | "leave" => {
            let ch = sp.next().unwrap_or("");
            leave_channel(rt, ch).await
        }
        "history" => {
            if sp.next() == Some("purge") {
                let t = sp.next();
                let n = store.purge(t).unwrap_or(0);
                format!("purged {n} messages")
            } else {
                "usage: /radio history purge [target]".into()
            }
        }
        "freq" => {
            if let Some(mhz) = sp.next() {
                match crate::band::parse_mhz(mhz) {
                    Some(khz) => set_manual_freq(rt, khz),
                    None => "usage: /radio freq <MHz>  (example: 144.950)".into(),
                }
            } else {
                let s = snap.lock();
                crate::band::describe(s.freq_khz)
            }
        }
        "prio" | "priority" => {
            let channel = sp.next().unwrap_or("");
            if channel.is_empty() {
                return "usage: /prio [routine|priority|emergency]  (on a channel)".into();
            }
            let name = crate::slash::group_name(channel);
            if name.is_empty() {
                return "usage: /prio [routine|priority|emergency]".into();
            }
            if name == "bulletin" {
                return "#bulletin is always routine — priority is fixed on the public channel"
                    .into();
            }
            let level = sp.next();
            match level {
                None => {
                    let bits = store.group_prio(&name).unwrap_or(0);
                    let p = match bits {
                        2 => Priority::Emergency,
                        1 => Priority::Priority,
                        _ => Priority::Routine,
                    };
                    format!("#{name} default priority is {}", p.as_str())
                }
                Some(raw) => {
                    let Some(prio) = Priority::parse_name(raw) else {
                        return "usage: /prio routine|priority|emergency".into();
                    };
                    let _ = store.group_set_prio(&name, prio as u8);
                    refresh_group_prios(rt);
                    // Sync over RF / hub so other stations pick up the default.
                    let cfg_g = cfg.lock().clone();
                    if let Ok(origin) = Callsign::parse(&cfg_g.callsign) {
                        let dest = Callsign::from_raw(crate::slash::channel_dest(&name));
                        if let Ok(seq) = store.next_seq(origin.as_str()) {
                            let mut flags = Flags::new().with(FLAG_GROUP);
                            apply_mode_flags(&mut flags, cfg_g.mode, origin.is_guest());
                            flags.set_priority(Priority::Priority);
                            let body = chan_meta_prio_body(prio).into_bytes();
                            if let Ok(mut env) = Envelope::new_msg(
                                origin,
                                dest,
                                seq,
                                body,
                                Priority::Priority.default_ttl(),
                                flags,
                            ) {
                                env.kind = MsgType::Status;
                                if cfg_g.mode.uses_internet() {
                                    let _ = rt.keys.sign_envelope(&mut env);
                                }
                                let _ = store.insert(&env, Delivery::Queued);
                                let _ = dispatch(rt, &env).await;
                                let _ = store.set_delivery(&env.msg_id, Delivery::Sent);
                            }
                        }
                    }
                    format!("#{name} default priority → {} (synced)", prio.as_str())
                }
            }
        }
        "qsy" => {
            if let Some(mhz) = sp.next() {
                let Some(khz) = crate::band::parse_mhz(mhz) else {
                    return "usage: /radio qsy <MHz>".into();
                };
                let rig_on = cfg.lock().rig.enabled;
                if rig_on {
                    if let Some(c) = rt.control() {
                        let hz = khz as u64 * 1000;
                        if c.rigctl(&format!("F {hz}")).await.is_ok() {
                            snap.lock().set_freq(khz, "rig");
                            return format!(
                                "qsy {} ({})",
                                crate::band::fmt_mhz(khz),
                                crate::band::band_label(khz)
                            );
                        }
                    }
                }
                set_manual_freq(rt, khz)
            } else {
                let s = snap.lock();
                crate::band::describe(s.freq_khz)
            }
        }
        "tower" => {
            let mode = cfg.lock().mode;
            if !mode.is_gateway() {
                return "tower map pin is only available in internet-radio".into();
            }
            let on = match sp.next().map(|s| s.to_ascii_lowercase()).as_deref() {
                None => {
                    let on = cfg.lock().gateway.tower;
                    return format!("tower {}", if on { "on" } else { "off" });
                }
                Some("on" | "true" | "1") => true,
                Some("off" | "false" | "0") => false,
                Some(_) => return "usage: /radio tower on|off".into(),
            };
            cfg.lock().gateway.tower = on;
            snap.lock().tower = on;
            persist_cfg(&cfg.lock());
            format!("tower {}", if on { "on" } else { "off" })
        }
        "ptt" => {
            if let Some(p) = sp.next() {
                cfg.lock().modem.ptt = p.to_string();
                format!("ptt {p}")
            } else {
                cfg.lock().modem.ptt.clone()
            }
        }
        "form" => {
            let kind_s = sp.next().unwrap_or("");
            let Some(kind) = FormKind::parse(kind_s) else {
                return "usage: /radio form ics213|radiogram <target> key=value …".into();
            };
            let target = sp.next().unwrap_or("#bulletin").to_string();
            let fields: Vec<(String, String)> = sp
                .filter_map(|p| {
                    p.split_once('=')
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                })
                .collect();
            if fields.is_empty() {
                return "usage: /radio form ics213 #net from=G4ABC to=NET msg=need generator"
                    .into();
            }
            let form = Form { kind, fields };
            rt.irc.begin_origin(client_id, &target);
            let sent = send_form(rt, &target, form).await;
            rt.irc.end_origin();
            match sent {
                Ok(id) => format!("form sent {id}"),
                Err(e) => format!("form failed: {e}"),
            }
        }
        "checkin" => {
            let note = sp.collect::<Vec<_>>().join(" ");
            let call = cfg.lock().callsign.clone();
            let _ = store.checkin(&call, &note, None, None);
            "checked in".into()
        }
        "net" => {
            match store.checkins() {
                Ok(rows) => rows
                    .into_iter()
                    .map(|(c, n, t, g, s)| {
                        format!("{c} t={t} grid={} snr={} {n}", g.unwrap_or_default(), s.unwrap_or(0.0))
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                Err(_) => "no checkins".into(),
            }
        }
        "mute" => {
            if let Some(t) = sp.next() {
                let _ = store.set_mute(t, true);
                format!("muted {t}")
            } else {
                "usage: /radio mute <target>".into()
            }
        }
        "theme" => {
            if let Some(t) = sp.next() {
                let mut c = cfg.lock();
                c.ui.theme = t.to_string();
                persist_cfg(&c);
                format!("theme {t}")
            } else {
                cfg.lock().ui.theme.clone()
            }
        }
        "activity" => {
            if let Some(v) = sp.next() {
                let on = matches!(v.to_ascii_lowercase().as_str(), "on" | "1" | "true" | "yes");
                {
                    let mut c = cfg.lock();
                    c.ui.activity_panel = on;
                    persist_cfg(&c);
                }
                snap.lock().activity_panel = on;
                format!("activity panel {}", if on { "on" } else { "off" })
            } else {
                format!(
                    "activity panel {}",
                    if cfg.lock().ui.activity_panel {
                        "on"
                    } else {
                        "off"
                    }
                )
            }
        }
        "update" => "run `wcr update` in a terminal".into(),
        "modem" | "tnc" => {
            let c = cfg.lock();
            if c.modem.is_bluetooth() {
                let s = snap.lock();
                format!(
                    "bluetooth kiss tnc {} {} — {}",
                    c.tnc.bt_name,
                    c.tnc.bt_addr,
                    if s.tnc.is_empty() { "starting" } else { &s.tnc }
                )
            } else if c.modem.uses_tnc() {
                let s = snap.lock();
                format!(
                    "serial kiss tnc {} — {}",
                    c.tnc.serial,
                    if s.tnc.is_empty() { "starting" } else { &s.tnc }
                )
            } else {
                format!("kiss {}:{}", c.modem.host, c.modem.kiss_port)
            }
        }
        "" | "help" => {
            "RADIO commands: mode preset tower status form group queue flush trace history freq qsy prio ptt checkin net mute theme activity update. Channels: /join #name (closed, +i)  /invite CALL (over-air notice from you)  /part /prio. /radio status shows relayed (every forwarded frame), unique (distinct messages), and groups (group_prios). /radio flush marks unacked outbound [no]."
                .into()
        }
        other => format!("unknown RADIO subcommand '{other}'. Try /radio help"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_mark_paints_once_per_message() {
        let mut painted = HashSet::new();
        let id = MsgId::from_bytes([1, 0, 0, 0, 0, 0, 0, 0]);
        assert!(take_relay_mark(
            &mut painted,
            id,
            true,
            Some(Delivery::Sent)
        ));
        assert!(!take_relay_mark(
            &mut painted,
            id,
            false,
            Some(Delivery::Relayed)
        ));
        let other = MsgId::from_bytes([2, 0, 0, 0, 0, 0, 0, 0]);
        assert!(take_relay_mark(
            &mut painted,
            other,
            false,
            Some(Delivery::Delivered)
        ));
        assert!(!take_relay_mark(
            &mut painted,
            other,
            false,
            Some(Delivery::Delivered)
        ));
        let failed = MsgId::from_bytes([3, 0, 0, 0, 0, 0, 0, 0]);
        assert!(!take_relay_mark(
            &mut painted,
            failed,
            false,
            Some(Delivery::Failed)
        ));
        assert!(!painted.contains(&failed));
    }

    #[test]
    fn a_member_learns_who_sends_or_answers_on_its_group() {
        let store = Store::open_memory().unwrap();
        learn_member(&store, "M0CCC", "qa", "G4EEE");
        assert!(
            store.group_members("qa").unwrap().is_empty(),
            "not our group"
        );
        store
            .group_create("qa", &["M0CCC".into(), "G4AAA".into()])
            .unwrap();
        learn_member(&store, "M0CCC", "qa", "g4eee");
        learn_member(&store, "M0CCC", "qa", "G4EEE");
        let mut members = store.group_members("qa").unwrap();
        members.sort();
        assert_eq!(members, ["G4AAA", "G4EEE", "M0CCC"]);
    }

    #[test]
    fn the_last_copy_waits_two_intervals_before_no() {
        assert!(ack_wait_runs_on(Delivery::Relayed, 3, 3));
        assert!(ack_wait_runs_on(Delivery::Sent, 3, 3));
        assert!(!ack_wait_runs_on(Delivery::Relayed, 4, 3), "only once");
        assert!(!ack_wait_runs_on(Delivery::Delivered, 3, 3));
    }

    #[test]
    fn retries_after_a_relay_or_receipt_do_not_paint() {
        assert!(retry_mark_shows(Some(Delivery::Sent)));
        assert!(retry_mark_shows(Some(Delivery::Queued)));
        for d in [
            Delivery::Relayed,
            Delivery::Gateway,
            Delivery::Delivered,
            Delivery::All,
            Delivery::Failed,
        ] {
            assert!(!retry_mark_shows(Some(d)), "{d:?}");
        }
    }

    #[test]
    fn relay_log_names_the_forwarded_frame() {
        let env = Envelope::new_msg(
            Callsign::parse("~RFSIMA").unwrap(),
            Callsign::parse("~RFSIMC").unwrap(),
            1,
            b"hello from A".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        let line = relay_log_line(&env);
        assert!(line.contains("~RFSIMA"));
        assert!(line.contains("~RFSIMC"));
        assert!(line.contains(&env.msg_id.hex()));
        assert!(line.contains("hello from A"));
        assert!(line.starts_with("relayed msg "));
        let ack = Envelope::ack_for(&env, Callsign::parse("~RFSIMC").unwrap(), 2, Some(4.0));
        let ack_line = relay_log_line(&ack);
        assert_eq!(
            ack_line,
            format!(
                "relayed ack {} -> {} id {}",
                ack.origin, ack.dest, ack.msg_id
            )
        );
        assert!(!ack_line.contains('\u{7}'));
    }

    #[test]
    fn rx_does_not_swallow_a_due_retry() {
        assert!(!channel_blocks_own_retry(None));
        assert!(!channel_blocks_own_retry(Some(ChannelState::Idle)));
        assert!(!channel_blocks_own_retry(Some(ChannelState::Rx)));
        assert!(!channel_blocks_own_retry(Some(ChannelState::Tx)));
    }

    #[test]
    fn ack_cancels_further_retries() {
        let store = Store::open_memory().unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"ping".to_vec(),
            3,
            Flags::new().with(FLAG_REQ_ACK),
        )
        .unwrap();
        store.insert(&env, Delivery::Sent).unwrap();
        assert!(still_awaiting_ack(&store, &env.msg_id));
        store
            .set_delivery(&env.msg_id, Delivery::Delivered)
            .unwrap();
        assert!(!still_awaiting_ack(&store, &env.msg_id));
        store.set_delivery(&env.msg_id, Delivery::All).unwrap();
        assert!(!still_awaiting_ack(&store, &env.msg_id));
    }

    #[test]
    fn long_message_keys_a_robust_rung_not_ofdm() {
        let cfg = Config::default();
        let preset = Preset::HfPoor;
        let env = Envelope::new_msg(
            Callsign::parse("M7TJF").unwrap(),
            Callsign::parse("G0ABC").unwrap(),
            1,
            vec![b'x'; 300],
            3,
            Flags::new(),
        )
        .unwrap();
        let bytes = env.encode().unwrap();
        assert!(bytes.len() > preset.payload_bytes() as usize);

        let frames = rf_frames(&env, &bytes, preset, &cfg).unwrap();
        let widest = frames.iter().map(Vec::len).max().unwrap();
        assert!(
            widest <= preset.payload_bytes() as usize,
            "widest frame {widest} exceeds {} byte capacity",
            preset.payload_bytes()
        );
        assert_eq!(
            presets::rung_for(preset, preset.ladder_start(), 0, widest),
            Rung::Rdm600S
        );
        // Sizing the rung from the unfragmented envelope is what forced OFDM.
        assert_eq!(
            presets::rung_for(preset, preset.ladder_start(), 0, bytes.len()),
            Rung::OfdmQpskHalf
        );
    }

    #[test]
    fn short_message_is_one_frame_and_unchanged() {
        let cfg = Config::default();
        let preset = Preset::HfPoor;
        let env = Envelope::new_msg(
            Callsign::parse("M7TJF").unwrap(),
            Callsign::parse("G0ABC").unwrap(),
            1,
            b"ETA 18:00".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        let bytes = env.encode().unwrap();
        let frames = rf_frames(&env, &bytes, preset, &cfg).unwrap();
        assert_eq!(frames, vec![bytes]);
    }

    #[test]
    fn radio_tx_flags_forbid_internet() {
        let mut flags = Flags::new().with(FLAG_INET_OK).with(FLAG_REQ_ACK);
        apply_mode_flags(&mut flags, Mode::Radio, false);
        assert!(flags.no_inet());
        assert!(!flags.inet_ok());
        apply_mode_flags(&mut flags, Mode::InternetRadio, false);
        assert!(!flags.no_inet());
        assert!(flags.inet_ok());
        apply_mode_flags(&mut flags, Mode::RadioPlus, false);
        assert!(!flags.no_inet());
        assert!(flags.inet_ok());
    }

    #[test]
    fn stamp_only_rewrites_our_frames() {
        let mut ours = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"hi".to_vec(),
            3,
            Flags::new().with(FLAG_INET_OK),
        )
        .unwrap();
        stamp_own_mode_flags(&mut ours, "G4ABC", Mode::Radio);
        assert!(ours.flags.no_inet());
        assert!(!ours.flags.inet_ok());

        let mut theirs = Envelope::new_msg(
            Callsign::parse("M0XYZ").unwrap(),
            Callsign::parse("G4ABC").unwrap(),
            2,
            b"ho".to_vec(),
            3,
            Flags::new().with(FLAG_INET_OK),
        )
        .unwrap();
        stamp_own_mode_flags(&mut theirs, "G4ABC", Mode::Radio);
        assert!(!theirs.flags.no_inet());
        assert!(theirs.flags.inet_ok());
    }

    #[test]
    fn remote_invite_creates_group() {
        let store = Store::open_memory().unwrap();
        assert!(record_remote_invite(
            &store,
            "TF101",
            "M7TJF",
            "You are invited to #qa85chain on WeeChat Radio. Join that channel to talk.",
        )
        .is_none());
        let ch = record_remote_invite(
            &store,
            "TF101",
            "M7TJF",
            "You are invited to #qa85chai on WeeChat Radio. Join that channel to talk.",
        )
        .unwrap();
        assert_eq!(ch, "#qa85chai");
        let members = store.group_members("qa85chai").unwrap();
        assert!(members.iter().any(|m| m == "TF101"));
        assert!(members.iter().any(|m| m == "M7TJF"));
        assert!(record_remote_invite(&store, "TF101", "M7TJF", "hello").is_none());
    }

    #[test]
    fn remote_leave_keeps_group_when_others_remain() {
        let store = Store::open_memory().unwrap();
        store
            .group_create("compatriots", &["M7TJF".into(), "TF101".into()])
            .unwrap();
        assert_eq!(
            parse_chan_meta_leave("WCRMETA leave=M7TJF").as_deref(),
            Some("M7TJF")
        );
        assert_eq!(
            parse_chan_meta_members("WCRMETA members=M7TJF,TF101,G4ABC").as_deref(),
            Some(
                [
                    "M7TJF".to_string(),
                    "TF101".to_string(),
                    "G4ABC".to_string()
                ]
                .as_slice()
            )
        );
        assert!(parse_chan_meta_members("WCRMETA leave=M7TJF").is_none());
        apply_remote_leave(&store, "compatriots", "M7TJF");
        assert!(!store.is_group_member("compatriots", "M7TJF").unwrap());
        assert!(store.is_group_member("compatriots", "TF101").unwrap());
        apply_remote_leave(&store, "compatriots", "TF101");
        assert!(store.group_members("compatriots").unwrap().is_empty());
        assert!(!store
            .group_list()
            .unwrap()
            .iter()
            .any(|n| n == "compatriots" || n == "compatri"));
    }
}
