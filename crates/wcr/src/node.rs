//! SPDX-License-Identifier: Apache-2.0
//! Station runtime: IRC + modem + hub + LAN + relay.

use crate::air::{self, AirItem, AirQueue, ChannelSense, ChannelState, ModemSense};
use crate::config::Config;
use crate::emcomm::{Form, FormKind, Welfare, BULLETIN_CHANNEL, BULLETIN_TTL};
use crate::error::Result;
use crate::ircd::{IrcEvent, IrcEventKind, IrcServer};
use crate::mail::{chunk_payloads, decode_chunk, encode_chunk, MailMeta, MailOp, MailWire};
use crate::mail_api::{MailApiState, MailNodeCmd};
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
use std::collections::HashMap;
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
    mail_parts: Arc<Mutex<HashMap<String, Vec<MailWire>>>>,
    mail_last_gateway: Arc<Mutex<String>>,
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
    }

    // Bind chat and status before opening the store. A locked SQLite (old
    // LaunchAgent, leftover node) is synchronous and would otherwise leave
    // the GUI on "station starting…" forever.
    let (irc_tx, mut irc_rx) = mpsc::channel(64);
    let irc = IrcServer::new(irc_tx);
    let irc_bind = cfg.irc.bind.clone();
    match tokio::net::TcpListener::bind(&irc_bind).await {
        Ok(listener) => {
            let irc_s = irc.clone();
            tokio::spawn(async move {
                if let Err(e) = irc_s.accept_loop(listener).await {
                    tracing::error!("irc: {e}");
                }
            });
        }
        Err(e) => tracing::error!("irc {irc_bind}: {e}"),
    }

    let status_bind = cfg.status.bind.clone();
    let status_listener = match tokio::net::TcpListener::bind(&status_bind).await {
        Ok(listener) => {
            tracing::info!("status HTTP on {status_bind}");
            Some(listener)
        }
        Err(e) => {
            tracing::warn!("status HTTP {status_bind}: {e}");
            None
        }
    };

    let keys = load_or_create(&Config::key_path())?;
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
    let (mail_cmd_tx, mut mail_cmd_rx) = mpsc::channel::<MailNodeCmd>(32);
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
    let rt = Runtime {
        cfg: Arc::new(Mutex::new(cfg)),
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
        mail_parts: Arc::new(Mutex::new(HashMap::new())),
        mail_last_gateway: Arc::new(Mutex::new(String::new())),
    };
    refresh_group_prios(&rt);
    let _ = store.ensure_mail_schema();

    if let Some(listener) = status_listener {
        let mail_api = MailApiState {
            store: store.clone(),
            cfg: rt.cfg.clone(),
            keys: rt.keys.clone(),
            snap: snap.clone(),
            mail_cmd: mail_cmd_tx.clone(),
            mail_last_gateway: rt.mail_last_gateway.clone(),
        };
        let snap_s = snap.clone();
        tokio::spawn(async move {
            status::serve_listener(listener, snap_s, Some(mail_api)).await;
        });
    }

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
                        tracing::warn!("modem73 exited; restarting radio");
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
                        if m.env.origin.as_str() == our {
                            continue;
                        }
                        let _ = dispatch(&rt_h, &m.env).await;
                        let _ = rt_h.store.set_hold(&m.env.msg_id, 0, m.env.hops_left);
                    }
                }
                if let Ok((o, h)) = rt_h.store.queue_depth() {
                    let mut s = rt_h.snap.lock();
                    s.queue_out = o;
                    s.queue_hold = h;
                }
                let max = rt_h.cfg.lock().rf.max_retries;
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
                if let Ok(due) = rt_r.store.retry_due(now, &our, max) {
                    for (m, retries) in due {
                        if let Some(s) = rt_r.sense() {
                            if s.state() != ChannelState::Idle {
                                continue;
                            }
                        }
                        let n = retries + 1;
                        {
                            let mut s = rt_r.snap.lock();
                            s.retries = n;
                        }
                        if !dispatch_rf_rung(&rt_r, &m.env, n).await.unwrap_or(false) {
                            continue;
                        }
                        let jitter = rt_r.cfg.lock().rf.retry_jitter;
                        let next =
                            relay::next_retry_hold_jittered(n, now, m.env.priority(), jitter);
                        let _ = rt_r.store.bump_retry(&m.env.msg_id, next);
                        let tries = rt_r.store.rf_tx_of(&m.env.msg_id).ok();
                        rt_r.irc
                            .tagmsg_progress(
                                &m.env.msg_id.hex(),
                                &format!("retry-{n}/{max}"),
                                tries,
                            )
                            .await;
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
                        Some(rt_hub.pending_inet.lock().drain(..).collect::<Vec<_>>())
                    } else {
                        s.hub_banner.clear();
                        None
                    }
                } else {
                    if rt_hub.cfg.lock().mode.uses_internet() {
                        let err = hub_flag_s.error();
                        s.hub_banner = if !err.is_empty() {
                            err
                        } else if was {
                            "Internet down, radio only".into()
                        } else {
                            s.hub_banner.clone()
                        };
                    }
                    None
                }
            };
            if let Some(batch) = pending {
                if rt_hub.cfg.lock().mode.uses_internet() {
                    for env in batch {
                        if let Some(h) = rt_hub.hub() {
                            let _ = h.send(&env).await;
                        }
                    }
                }
            }
        }
    });

    // Beacon
    {
        let rt_b = rt.clone();
        tokio::spawn(async move {
            loop {
                let (jitter_s, congested, uses_rf, vox) = {
                    let cfg = rt_b.cfg.lock();
                    (
                        cfg.rf.beacon_jitter_s,
                        cfg.rf.congested_pct,
                        cfg.mode.uses_radio(),
                        cfg.modem.is_vox(),
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
                let wait = relay::beacon_interval_secs(jitter_s);
                let due_at = crate::proto::now_ts().saturating_add(wait as u32);
                {
                    let mut s = rt_b.snap.lock();
                    s.beacon_due = due_at;
                    s.beacon_span = wait as u32;
                    s.beacon_note.clear();
                }
                tokio::time::sleep(Duration::from_secs(wait)).await;
                let (uses_rf, vox) = {
                    let cfg = rt_b.cfg.lock();
                    (cfg.mode.uses_radio(), cfg.modem.is_vox())
                };
                if !uses_rf || vox {
                    continue;
                }
                if let Some(s) = rt_b.sense() {
                    if s.occupancy_pct() >= congested {
                        continue;
                    }
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
                        let s = rt_f.snap.lock();
                        (
                            s.freq_khz,
                            s.heard
                                .iter()
                                .map(|x| x.callsign.clone())
                                .collect::<Vec<_>>(),
                            s.hub_ok,
                        )
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
            cmd = mail_cmd_rx.recv() => {
                if let Some(cmd) = cmd {
                    if let Err(e) = handle_mail_cmd(&rt, cmd).await {
                        tracing::warn!("mail: {e}");
                    }
                }
            }
            ev = irc_rx.recv() => {
                let Some(ev) = ev else { break };
                if let Err(e) = handle_irc(&rt, ev).await {
                    tracing::warn!("irc event: {e}");
                }
            }
            frame = recv_opt(&mut kiss_rx) => {
                if let Some(payload) = frame {
                    if let Ok(env) = Envelope::decode(&payload) {
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
    apply_mode_flags(&mut env.flags, mode, env.origin.is_guest());
}

async fn handle_irc(rt: &Runtime, ev: IrcEvent) -> Result<()> {
    match ev.kind {
        IrcEventKind::Privmsg { target, text, .. } => {
            if let Err(e) = send_chat(rt, &target, &text).await {
                rt.irc.notice_all(&e.to_string()).await;
                return Ok(());
            }
        }
        IrcEventKind::Radio { args } => {
            let reply = radio_cmd(rt, &args).await;
            rt.irc.send_radio_reply(ev.client_id, &reply).await;
        }
        IrcEventKind::Part { channel } => {
            let _ = leave_channel(rt, &channel).await;
        }
        IrcEventKind::Join { channel } => {
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
        _ => {}
    }
    Ok(())
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
    let hops = Priority::Routine.default_ttl();
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
    let hops = if target.eq_ignore_ascii_case(BULLETIN_CHANNEL) {
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
    let rung = presets::rung_for(preset, stored, retries, bytes.len());
    let apply = env.origin.as_str() == rt.engine.our_call;
    enqueue_rf(
        rt,
        &rf_env,
        &bytes,
        preset,
        &cfg,
        EnqueueOpts {
            rung: if apply { Some(rung) } else { None },
            delay,
            copy,
        },
    )
    .await
}

async fn enqueue_rf(
    rt: &Runtime,
    rf_env: &Envelope,
    bytes: &[u8],
    preset: Preset,
    cfg: &Config,
    opts: EnqueueOpts,
) -> Result<bool> {
    let Some(air) = rt.air() else {
        if let Some(k) = rt.kiss() {
            let mtu = preset.payload_bytes();
            if frag::should_fragment(rf_env, bytes.len(), mtu) {
                let frags = frag::split(rf_env, cfg.rf.frag_k, cfg.rf.frag_m)?;
                for f in frags {
                    k.send(&f.encode()?).await?;
                }
            } else {
                k.send(bytes).await?;
            }
            note_own_rf_tx(rt, rf_env);
        }
        return Ok(true);
    };
    let mtu = preset.payload_bytes();
    let frames = if frag::should_fragment(rf_env, bytes.len(), mtu) {
        let frags = frag::split(rf_env, cfg.rf.frag_k, cfg.rf.frag_m)?;
        let mut out = Vec::with_capacity(frags.len());
        for f in frags {
            out.push(f.encode()?);
        }
        out
    } else {
        vec![bytes.to_vec()]
    };
    let mut item = AirItem::new(rf_env, &rt.engine.our_call, frames, preset).with_copy(opts.copy);
    if !opts.delay.is_zero() {
        item = item.with_delay(opts.delay);
    }
    if let Some(r) = opts.rung {
        item = item.with_rung(r);
    }
    let queued = air.enqueue(item);
    if queued {
        note_own_rf_tx(rt, rf_env);
    }
    Ok(queued)
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
    enqueue_rf(
        rt,
        rf_env,
        bytes,
        preset,
        cfg,
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
    dispatch_with(rt, env, false).await
}

async fn dispatch_with(rt: &Runtime, env: &Envelope, force_inet: bool) -> Result<()> {
    let cfg = rt.cfg.lock().clone();
    let mut env = env.clone();
    stamp_own_mode_flags(&mut env, &rt.engine.our_call, cfg.mode);
    if force_inet {
        env.flags.set(FLAG_NO_INET, false);
        env.flags.set(FLAG_INET_OK, true);
    }
    if cfg.mode.uses_radio() {
        let _ = dispatch_rf_rung(rt, &env, 0).await;
    }
    let via_inet =
        force_inet || (cfg.mode.uses_internet() && env.flags.inet_ok() && inet_gap_for(rt, &env));
    if via_inet {
        if force_inet && !cfg.mode.uses_internet() {
            if let Some(h) = rt.hub() {
                let _ = h.send(&env).await;
            }
            if let Some(p) = rt.peers_tx() {
                let _ = p.send(env.clone()).await;
            }
        } else {
            offer_hub(rt, &env).await;
        }
        if let Some(l) = &rt.lan {
            let _ = l.send(env.clone()).await;
        }
    }
    Ok(())
}

async fn offer_hub(rt: &Runtime, env: &Envelope) {
    if !rt.cfg.lock().mode.uses_internet() {
        return;
    }
    if rt.hub_flag.get() {
        if let Some(h) = rt.hub() {
            let _ = h.send(env).await;
        }
    } else {
        rt.pending_inet.lock().push(env.clone());
    }
    if let Some(p) = rt.peers_tx() {
        let _ = p.send(env.clone()).await;
    }
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

async fn on_envelope(rt: &Runtime, env: Envelope, medium: &str, snr: Option<f32>) -> Result<()> {
    if env.origin.as_str() == rt.engine.our_call {
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
        // Do not pull [ok] back to [rl].
        if matches!(
            rt.store.delivery_of(&env.msg_id)?,
            Some(Delivery::Delivered) | Some(Delivery::All)
        ) {
            if let Some(air) = rt.air() {
                air.cancel(env.msg_id);
            }
            return Ok(());
        }
        rt.store.set_delivery(&env.msg_id, Delivery::Relayed)?;
        rt.irc.tagmsg_delivery(&env.msg_id.hex(), "relayed").await;
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
    let decision = rt.engine.on_rx(&env, medium, snr, freq)?;
    if decision.action == Action::Suppress {
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
        refresh_heard(rt);
        return Ok(());
    }
    match env.kind {
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status => {
            let target = if env.flags.group() {
                format!("#{}", env.dest)
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
            {
                let seq = rt.store.next_seq(&rt.engine.our_call)?;
                let mut ack = Envelope::ack_for(
                    &env,
                    Callsign::from_raw(rt.engine.our_call.clone()),
                    seq,
                    snr,
                );
                if rt.cfg.lock().mode.uses_internet() {
                    let _ = rt.keys.sign_envelope(&mut ack);
                }
                let dither = if env.flags.group() {
                    let max = rt.cfg.lock().rf.ack_dither_ms as u64;
                    if max > 0 {
                        Duration::from_millis(rand::thread_rng().gen_range(0..=max))
                    } else {
                        Duration::ZERO
                    }
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
            }
            if !mute_closed && env.flags.group() {
                let _ = rt.store.receipt(
                    &env.dest.to_string().to_ascii_lowercase(),
                    &env.msg_id,
                    env.origin.as_str(),
                );
                if rt
                    .store
                    .group_all_received(&env.dest.to_string().to_ascii_lowercase(), &env.msg_id)?
                {
                    rt.store.set_delivery(&env.msg_id, Delivery::All)?;
                    rt.irc.tagmsg_delivery(&env.msg_id.hex(), "all").await;
                }
            }
        }
        MsgType::Ack => {
            if let Some(id) = env.acked_id() {
                rt.store.set_delivery(&id, Delivery::Delivered)?;
                let _ = rt.store.set_hold(&id, 0, 0);
                if let Some(air) = rt.air() {
                    air.cancel(id);
                }
                rt.irc.tagmsg_delivery(&id.hex(), "delivered").await;
                rt.snap.lock().retries = 0;
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
        MsgType::Mail => {
            if let Err(e) = handle_mail_envelope(rt, &env, medium).await {
                tracing::warn!("mail envelope: {e}");
            }
        }
        MsgType::Ping | MsgType::File => {}
        MsgType::Frag => {}
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

async fn forward_gateway(rt: &Runtime, env: &Envelope, medium: &str) -> Result<()> {
    let cfg_g = rt.cfg.lock().clone();
    if medium == "rf"
        && relay::may_inet_forward(
            cfg_g.mode.is_gateway(),
            env.flags.inet_ok(),
            env.flags.no_inet(),
        )
    {
        if let Some(h) = rt.hub() {
            let _ = h.send(env).await;
        }
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

async fn send_group_status(rt: &Runtime, name: &str, body: String, force_inet: bool) -> Result<()> {
    let cfg_g = rt.cfg.lock().clone();
    let origin = Callsign::parse(&cfg_g.callsign)?;
    let dest = Callsign::from_raw(crate::slash::channel_dest(name));
    let seq = rt.store.next_seq(origin.as_str())?;
    let mut flags = Flags::new().with(FLAG_GROUP);
    apply_mode_flags(&mut flags, cfg_g.mode, origin.is_guest());
    flags.set_priority(Priority::Priority);
    let mut env = Envelope::new_msg(
        origin,
        dest,
        seq,
        body.into_bytes(),
        Priority::Priority.default_ttl(),
        flags,
    )?;
    env.kind = MsgType::Status;
    if cfg_g.mode.uses_internet() || force_inet {
        let _ = rt.keys.sign_envelope(&mut env);
    }
    rt.store.insert(&env, Delivery::Queued)?;
    dispatch_with(rt, &env, force_inet).await?;
    rt.store.set_delivery(&env.msg_id, Delivery::Sent)?;
    Ok(())
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
    let _ = send_group_status(rt, &name, chan_meta_leave_body(&us), true).await;
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
        if already.is_none() && cfg.modem.manage && rt.txp.lock().modem.is_none() {
            match ModemProcess::spawn(&cfg).await {
                Ok(c) => modem = Some(c),
                Err(e) => tracing::warn!("{e}"),
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
        vec![],
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

async fn radio_cmd(rt: &Runtime, args: &str) -> String {
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
                        snap.lock().mode = mode;
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
                "{} | {} | {} {} | {} | SNR {:.1} | tx {} | retry {} | audio {} | q {}/{} | occ {}% air {} | beacon {} | hold {} | hub {}{}",
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
                    format!("group {name} created")
                }
                "invite" | "add" => {
                    let name = crate::slash::group_name(sp.next().unwrap_or(""));
                    let members: Vec<String> = sp.map(|s| s.to_ascii_uppercase()).collect();
                    if name.is_empty() || members.is_empty() {
                        return "usage: /radio group invite <name> <callsigns...>".into();
                    }
                    let _ = store.group_create(&name, &members);
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
            let (o, h) = store.queue_depth().unwrap_or((0, 0));
            format!("outbound {o} hold {h}")
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
            match send_form(rt, &target, form).await {
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
            "RADIO commands: mode preset status form group queue trace history freq qsy prio ptt checkin net mute theme activity update. Channels: /join #name  /invite CALL /part /prio"
                .into()
        }
        other => format!("unknown RADIO subcommand '{other}'. Try /radio help"),
    }
}

async fn handle_mail_cmd(rt: &Runtime, cmd: MailNodeCmd) -> Result<()> {
    match cmd {
        MailNodeCmd::SendRf { mail_id } => mail_send_rf(rt, &mail_id).await,
        MailNodeCmd::CheckList => mail_check_list_rf(rt).await,
        MailNodeCmd::CheckGet { ids } => mail_check_get_rf(rt, &ids).await,
    }
}

async fn mail_send_rf(rt: &Runtime, mail_id: &str) -> Result<()> {
    let row = rt
        .store
        .mail_get(mail_id)?
        .ok_or_else(|| crate::error::Error::Msg("mail not found".into()))?;
    let cfg = rt.cfg.lock().clone();
    if cfg.mail.gateway.trim().is_empty() {
        return Err(crate::error::Error::config(
            "set [mail] gateway to an internet-radio callsign (Setup → Email gateway)",
        ));
    }
    let gateway = cfg.mail.gateway.clone();
    let dest = Callsign::parse(&gateway)?;
    let origin = Callsign::parse(&cfg.callsign)?;
    let meta = MailMeta {
        from: row.from_addr.clone(),
        to: row.to_addr.clone(),
        subject: row.subject.clone(),
        ids: vec![],
    };
    let third = crate::mail::is_third_party_to(&row.to_addr);
    let chunks = chunk_payloads(mail_id, &meta, &row.body);
    mail_tx_chunks(rt, origin, dest, chunks, cfg.mode, third).await?;
    rt.store.mail_set_delivery(mail_id, Delivery::Sent)?;
    rt.store.mail_move_folder(mail_id, "sent")?;
    Ok(())
}

async fn mail_check_list_rf(rt: &Runtime) -> Result<()> {
    let cfg = rt.cfg.lock().clone();
    if cfg.mode == Mode::Radio {
        return Ok(());
    }
    if cfg.mail.gateway.trim().is_empty() {
        return Err(crate::error::Error::config(
            "set [mail] gateway to an internet-radio callsign (Setup → Email gateway)",
        ));
    }
    let origin = Callsign::parse(&cfg.callsign)?;
    let gateway = Callsign::parse(&cfg.mail.gateway)?;
    let wire = MailWire {
        op: MailOp::ListReq,
        mail_id: "list".into(),
        idx: 0,
        count: 1,
        meta: MailMeta::default(),
        payload: String::new(),
    };
    mail_tx_one(rt, origin, gateway, encode_chunk(&wire), cfg.mode, false).await?;
    Ok(())
}

async fn mail_check_get_rf(rt: &Runtime, ids: &[String]) -> Result<()> {
    let cfg = rt.cfg.lock().clone();
    if cfg.mail.gateway.trim().is_empty() {
        return Err(crate::error::Error::config(
            "set [mail] gateway to an internet-radio callsign (Setup → Email gateway)",
        ));
    }
    let origin = Callsign::parse(&cfg.callsign)?;
    let gateway = Callsign::parse(&cfg.mail.gateway)?;
    let wire = MailWire {
        op: MailOp::GetReq,
        mail_id: "get".into(),
        idx: 0,
        count: 1,
        meta: MailMeta {
            from: String::new(),
            to: String::new(),
            subject: String::new(),
            ids: ids.to_vec(),
        },
        payload: String::new(),
    };
    mail_tx_one(rt, origin, gateway, encode_chunk(&wire), cfg.mode, false).await?;
    Ok(())
}

async fn mail_tx_chunks(
    rt: &Runtime,
    origin: Callsign,
    dest: Callsign,
    chunks: Vec<MailWire>,
    mode: Mode,
    third_party: bool,
) -> Result<()> {
    for ch in chunks {
        mail_tx_one(
            rt,
            origin.clone(),
            dest.clone(),
            encode_chunk(&ch),
            mode,
            third_party,
        )
        .await?;
    }
    Ok(())
}

async fn mail_tx_one(
    rt: &Runtime,
    origin: Callsign,
    dest: Callsign,
    body: Vec<u8>,
    mode: Mode,
    third_party: bool,
) -> Result<()> {
    let mut flags = Flags::new().with(FLAG_REQ_ACK);
    apply_mode_flags(&mut flags, mode, origin.is_guest());
    flags.set(FLAG_THIRD_PARTY, third_party);
    let seq = rt.store.next_seq(origin.as_str())?;
    let mut env = Envelope::new_msg(origin, dest, seq, body, 4, flags)?;
    env.kind = MsgType::Mail;
    if mode.uses_internet() {
        rt.keys.sign_envelope(&mut env)?;
    }
    if env.body.len() > crate::proto::MAX_BODY {
        return Err(crate::error::Error::protocol(format!(
            "mail chunk {} B over max {}",
            env.body.len(),
            crate::proto::MAX_BODY
        )));
    }
    rt.store.insert(&env, Delivery::Queued)?;
    dispatch(rt, &env).await?;
    let our = rt.engine.our_call.clone();
    if env.dest.as_str() != our {
        *rt.mail_last_gateway.lock() = env.dest.as_str().to_string();
    }
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
        kind: "mail".into(),
        origin: Some(env.origin.to_string()),
        dest: Some(env.dest.to_string()),
        hops: Some(env.hops_left),
        snr: None,
        msgid: Some(env.msg_id.hex()),
        band,
    });
    Ok(())
}

async fn handle_mail_envelope(rt: &Runtime, env: &Envelope, _via: &str) -> Result<()> {
    let chunk = decode_chunk(&env.body)?;
    let cfg = rt.cfg.lock().clone();
    let our = cfg.callsign.clone();

    if chunk.op == MailOp::ListReq && cfg.mode.is_gateway() && cfg.mode.uses_internet() {
        let headers = if rt.snap.lock().hub_ok {
            let base = crate::net::hub_mail::hub_api_base_from_telemetry(&cfg.telemetry.url);
            let url = format!("{}/api/v1/mail/inbox", base);
            if let Ok(v) = crate::net::hub_mail::signed_post(&url, &rt.keys, &our, b"{}").await {
                v.get("headers").cloned().unwrap_or_default()
            } else {
                serde_json::json!([])
            }
        } else {
            serde_json::json!([])
        };
        let origin = Callsign::parse(&our)?;
        let dest = env.origin.clone();
        if let Some(arr) = headers.as_array() {
            for (i, h) in arr
                .iter()
                .take(crate::mail::CHECK_MAIL_MAX_MSGS)
                .enumerate()
            {
                let wire = MailWire {
                    op: MailOp::ListHdr,
                    mail_id: h.get("id").and_then(|x| x.as_str()).unwrap_or("").into(),
                    idx: i as u16,
                    count: arr.len().min(crate::mail::CHECK_MAIL_MAX_MSGS) as u16,
                    meta: MailMeta {
                        from: h.get("from").and_then(|x| x.as_str()).unwrap_or("").into(),
                        to: String::new(),
                        subject: h
                            .get("subject")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .into(),
                        ids: vec![],
                    },
                    payload: h
                        .get("bytes")
                        .map(|b| b.to_string())
                        .unwrap_or_else(|| "0".into()),
                };
                mail_tx_one(
                    rt,
                    origin.clone(),
                    dest.clone(),
                    encode_chunk(&wire),
                    cfg.mode,
                    false,
                )
                .await?;
            }
        }
        return Ok(());
    }

    if chunk.op == MailOp::GetReq && cfg.mode.is_gateway() && cfg.mode.uses_internet() {
        let ids = chunk.meta.ids.clone();
        let base = crate::net::hub_mail::hub_api_base_from_telemetry(&cfg.telemetry.url);
        let url = format!("{}/api/v1/mail/fetch", base);
        let req = serde_json::json!({ "ids": ids });
        let raw = serde_json::to_vec(&req)?;
        if let Ok(v) = crate::net::hub_mail::signed_post(&url, &rt.keys, &our, &raw).await {
            if let Some(arr) = v.get("messages").and_then(|m| m.as_array()) {
                for m in arr {
                    let meta = MailMeta {
                        from: m.get("from").and_then(|x| x.as_str()).unwrap_or("").into(),
                        to: m.get("to").and_then(|x| x.as_str()).unwrap_or("").into(),
                        subject: m
                            .get("subject")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .into(),
                        ids: vec![],
                    };
                    let body = m.get("body").and_then(|x| x.as_str()).unwrap_or("");
                    let id = m.get("id").and_then(|x| x.as_str()).unwrap_or("in");
                    let parts = chunk_payloads(id, &meta, body);
                    let origin = Callsign::parse(&our)?;
                    let dest = env.origin.clone();
                    mail_tx_chunks(rt, origin, dest, parts, cfg.mode, false).await?;
                }
            }
        }
        return Ok(());
    }

    if chunk.op == MailOp::Data && env.dest.as_str() == our {
        let mut parts = rt.mail_parts.lock();
        let entry = parts.entry(chunk.mail_id.clone()).or_default();
        entry.push(chunk.clone());
        if entry.len() as u16 >= chunk.count {
            let assembled = entry.clone();
            parts.remove(&chunk.mail_id);
            let (meta, body) = crate::mail::assemble_chunks(assembled)?;
            let id = chunk.mail_id.clone();
            rt.store.mail_insert(
                &id,
                "inbox",
                &meta.from,
                &meta.to,
                &meta.subject,
                &body,
                Delivery::Delivered,
                Some(&id),
            )?;
            let band = {
                let s = rt.snap.lock();
                if s.band.is_empty() {
                    None
                } else {
                    Some(s.band.clone())
                }
            };
            let _ = rt.tel.send(TelemetryEvent {
                ts: crate::proto::now_ts() as u64,
                kind: "mail".into(),
                origin: Some(env.origin.to_string()),
                dest: Some(our.clone()),
                hops: Some(env.hops_left),
                snr: None,
                msgid: Some(env.msg_id.hex()),
                band,
            });
        }
        return Ok(());
    }

    if chunk.op == MailOp::ListHdr && env.dest.as_str() == our {
        // GUI polls /mail/check/list over HTTP; RF headers are informational only.
        return Ok(());
    }

    Ok(())
}

mod tests {
    use super::*;

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
        let ch = record_remote_invite(
            &store,
            "TF101",
            "M7TJF",
            "You are invited to #compatriots on WeeChat Radio. Join that channel to talk.",
        )
        .unwrap();
        assert_eq!(ch, "#compatri");
        let members = store.group_members("compatriots").unwrap();
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
