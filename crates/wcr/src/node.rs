//! SPDX-License-Identifier: Apache-2.0
//! Station runtime: IRC + modem + hub + LAN + relay.

use crate::air::{self, AirItem, AirQueue, ChannelSense, ChannelState, ModemSense};
use crate::config::Config;
use crate::emcomm::{Form, Welfare, BULLETIN_CHANNEL, BULLETIN_TTL};
use crate::error::Result;
use crate::ircd::{IrcEvent, IrcEventKind, IrcServer};
use crate::modem::{ControlClient, KissClient, ModemProcess};
use crate::modes::Mode;
use crate::net::hub_client::{ArcFlag, HubClient};
use crate::net::lan::LanMesh;
use crate::presets::{self, Preset, Rung};
use crate::proto::frag::{self, FragAssembler};
use crate::proto::{
    load_or_create, Callsign, Envelope, Flags, IdentityKeys, MsgId, MsgType, Priority, FLAG_GROUP,
    FLAG_INET_OK, FLAG_NO_INET, FLAG_REQ_ACK, FLAG_THIRD_PARTY,
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

#[derive(Clone)]
struct Runtime {
    cfg: Arc<Mutex<Config>>,
    store: Arc<Store>,
    keys: IdentityKeys,
    engine: Engine,
    irc: IrcServer,
    kiss: Option<KissClient>,
    hub: Option<HubClient>,
    lan: Option<mpsc::Sender<Envelope>>,
    snap: Arc<SharedStatus>,
    tel: broadcast::Sender<TelemetryEvent>,
    control: Option<ControlClient>,
    dest_rungs: Arc<Mutex<HashMap<String, usize>>>,
    assembler: Arc<Mutex<FragAssembler>>,
    last_rx_snr: Arc<Mutex<Option<f32>>>,
    air: Option<AirQueue>,
    sense: Option<Arc<ModemSense>>,
}

pub async fn run_node(mut cfg: Config, with_tui: bool) -> Result<()> {
    cfg.normalize();
    crate::config::ensure_dirs()?;
    if cfg.callsign.is_empty() {
        return Err(crate::error::Error::config(
            "no callsign set. Run `wcr setup` first.",
        ));
    }
    let keys = load_or_create(&Config::key_path())?;
    let store = Arc::new(Store::open(
        &cfg.store.path,
        cfg.store.max_age_hours,
        cfg.store.max_msgs,
    )?);
    let snap = status::new_shared();
    {
        let mut s = snap.lock();
        s.callsign = cfg.callsign.clone();
        s.grid = cfg.grid.clone();
        s.mode = cfg.mode;
        s.ptt = cfg.modem.ptt.clone();
        s.preset = cfg.modem.preset.clone();
    }

    let (irc_tx, mut irc_rx) = mpsc::channel(64);
    let irc = IrcServer::new(irc_tx);
    let irc_bind = cfg.irc.bind.clone();
    let irc_s = irc.clone();
    tokio::spawn(async move {
        if let Err(e) = irc_s.listen(&irc_bind).await {
            tracing::error!("irc: {e}");
        }
    });

    let status_bind = cfg.status.bind.clone();
    let snap_s = snap.clone();
    tokio::spawn(async move { status::serve(status_bind, snap_s).await });

    let (tel_tx, tel_rx) = broadcast::channel::<TelemetryEvent>(64);
    if cfg.mode.uses_internet() {
        let url = cfg.telemetry.url.clone();
        let keys_t = keys.clone();
        let call = cfg.callsign.clone();
        let snap_t = snap.clone();
        tokio::spawn(telemetry::reporter_loop(url, keys_t, call, snap_t, tel_rx));
    }

    let mut _modem_child: Option<ModemProcess> = None;
    let mut kiss: Option<KissClient> = None;
    let mut kiss_rx: Option<mpsc::Receiver<Vec<u8>>> = None;
    let mut control: Option<ControlClient> = None;
    let last_rx_snr = Arc::new(Mutex::new(None::<f32>));
    let air_q = AirQueue::new();
    let radio_tnc = cfg.modem.uses_tnc();
    let sense = Arc::new(if radio_tnc {
        ModemSense::passive()
    } else {
        ModemSense::new()
    });
    if cfg.mode.uses_radio() && radio_tnc {
        // Radio with its own KISS TNC (VR-N76 / UV-PRO / GA-5WB over Bluetooth,
        // or any TNC on a serial port). No modem73, no control port.
        {
            let mut s = snap.lock();
            s.ptt = "tnc".into();
            s.tnc = if cfg.modem.is_bluetooth() {
                format!("searching for {}…", cfg.tnc.bt_name)
            } else {
                format!("opening {}…", cfg.tnc.serial)
            };
        }
        let (k, rx) = crate::tnc::start_link(&cfg, snap.clone(), sense.clone());
        kiss = Some(k);
        kiss_rx = Some(rx);
    } else if cfg.mode.uses_radio() {
        if cfg.modem.manage {
            match ModemProcess::spawn(&cfg).await {
                Ok(c) => _modem_child = Some(c),
                Err(e) => tracing::warn!("{e}"),
            }
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        }
        let kiss_addr = format!("{}:{}", cfg.modem.host, cfg.modem.kiss_port);
        match KissClient::connect(&kiss_addr).await {
            Ok((k, rx)) => {
                kiss = Some(k);
                kiss_rx = Some(rx);
            }
            Err(e) => tracing::warn!("{e}"),
        }
        let ctrl_addr = format!("{}:{}", cfg.modem.host, cfg.modem.control_port);
        if let Ok((c, mut ev)) = ControlClient::connect(&ctrl_addr).await {
            if let Some(p) = Preset::parse(&cfg.modem.preset) {
                let _ = c.set_config(p.control_config()).await;
            }
            let _ = c.apply_ptt(&cfg.modem.ptt, &cfg.modem).await;
            let snap_c = snap.clone();
            let snr_slot = last_rx_snr.clone();
            let sense_ev = sense.clone();
            tokio::spawn(async move {
                while let Some(frame) = ev.recv().await {
                    sense_ev.note_rx();
                    *snr_slot.lock() = Some(frame.snr);
                    let mut s = snap_c.lock();
                    s.snr = frame.snr;
                    s.ber = frame.ber_pct;
                    s.audio_db = frame.level_db;
                    s.audio_label = presets::audio_level_label(frame.level_db).into();
                    s.channel = "rx".into();
                }
            });
            control = Some(c);
        }
    }

    let hub_flag = ArcFlag::new();
    let (hub_in_tx, mut hub_in_rx) = mpsc::channel::<Envelope>(64);
    let mut hub: Option<HubClient> = None;
    if cfg.mode.uses_internet() {
        match HubClient::connect(
            &cfg.hub.url,
            &cfg.callsign,
            &keys,
            vec![],
            hub_in_tx,
            hub_flag.clone(),
        )
        .await
        {
            Ok(h) => hub = Some(h),
            Err(e) => tracing::warn!("hub: {e}"),
        }
    }

    let mut lan_out: Option<mpsc::Sender<Envelope>> = None;
    let mut lan_in: Option<mpsc::Receiver<Envelope>> = None;
    if cfg.lan.discovery {
        match LanMesh::start(&cfg.callsign, cfg.lan.port).await {
            Ok((mesh, tx)) => {
                let _ = mesh.bind_port;
                lan_out = Some(tx);
                lan_in = Some(mesh.incoming);
            }
            Err(e) => tracing::warn!("lan: {e}"),
        }
    }

    let engine = Engine::new(store.clone(), cfg.callsign.clone());
    let uses_radio = cfg.mode.uses_radio();
    let have_kiss = kiss.is_some();
    let rt = Runtime {
        cfg: Arc::new(Mutex::new(cfg)),
        store: store.clone(),
        keys,
        engine,
        irc: irc.clone(),
        kiss,
        hub,
        lan: lan_out,
        snap: snap.clone(),
        tel: tel_tx,
        control,
        dest_rungs: Arc::new(Mutex::new(HashMap::new())),
        assembler: Arc::new(Mutex::new(FragAssembler::new())),
        last_rx_snr,
        air: if uses_radio && have_kiss {
            Some(air_q.clone())
        } else {
            None
        },
        sense: if uses_radio {
            Some(sense.clone())
        } else {
            None
        },
    };

    if uses_radio {
        if let Some(c) = &rt.control {
            sense
                .clone()
                .spawn_poller(c.clone(), snap.clone(), air_q.clone());
        }
        if let Some(k) = &rt.kiss {
            let q = air_q.clone();
            let tx = k.tx.clone();
            let s = sense.clone();
            let ctrl = rt.control.clone();
            let cfg_a = rt.cfg.clone();
            let snap_a = snap.clone();
            tokio::spawn(async move {
                air::run_air_queue(q, tx, s, ctrl, cfg_a, snap_a).await;
            });
        }
    }

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
                        if let Some(s) = &rt_r.sense {
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
                        rt_r.irc
                            .tagmsg_delivery(&m.env.msg_id.hex(), &format!("retry {n}/{max}"))
                            .await;
                    }
                }
            }
        });
    }

    let hub_flag_s = hub_flag.clone();
    let snap_b = snap.clone();
    let cfg_b = rt.cfg.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tick.tick().await;
            let mut s = snap_b.lock();
            let was = s.hub_ok;
            s.hub_ok = hub_flag_s.get();
            if s.hub_ok {
                s.hub_banner.clear();
            } else if cfg_b.lock().mode.uses_internet() {
                let err = hub_flag_s.error();
                s.hub_banner = if !err.is_empty() {
                    err
                } else if was {
                    "Internet down, radio only".into()
                } else {
                    s.hub_banner.clone()
                };
            }
        }
    });

    // Beacon
    {
        let rt_b = rt.clone();
        tokio::spawn(async move {
            loop {
                let (jitter_s, congested, uses_rf) = {
                    let cfg = rt_b.cfg.lock();
                    (
                        cfg.rf.beacon_jitter_s,
                        cfg.rf.congested_pct,
                        cfg.mode.uses_radio(),
                    )
                };
                let j = jitter_s as i64;
                let wait = (60i64 + rand::thread_rng().gen_range(-j..=j)).clamp(15, 120) as u64;
                tokio::time::sleep(Duration::from_secs(wait)).await;
                if !uses_rf {
                    continue;
                }
                if let Some(s) = &rt_b.sense {
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
                    format!("B|{}", cfg.mode.as_str()).into_bytes(),
                    1,
                    flags,
                ) {
                    env.kind = MsgType::Beacon;
                    let _ = dispatch(&rt_b, &env).await;
                }
            }
        });
    }

    let mut kiss_rx = kiss_rx;
    let mut lan_in = lan_in;

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
                    if let Ok(env) = Envelope::decode(&payload) {
                        let snr = rt.last_rx_snr.lock().take();
                        let _ = on_envelope(&rt, env, "rf", snr).await;
                    }
                }
            }
            env = hub_in_rx.recv() => {
                if let Some(env) = env {
                    let _ = on_envelope(&rt, env, "inet", None).await;
                }
            }
            env = recv_lan(&mut lan_in) => {
                if let Some(env) = env {
                    let _ = on_envelope(&rt, env, "lan", None).await;
                }
            }
        }
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
    if mode.inet_ok_on_tx() {
        flags.set(FLAG_INET_OK, true);
    }
    if mode.no_inet_on_tx() {
        flags.set(FLAG_NO_INET, true);
    }
    if third {
        flags.set(FLAG_THIRD_PARTY, true);
    }
}

async fn handle_irc(rt: &Runtime, ev: IrcEvent) -> Result<()> {
    match ev.kind {
        IrcEventKind::Privmsg { target, text, .. } => {
            send_chat(rt, &target, &text).await?;
        }
        IrcEventKind::Radio { args } => {
            let reply = radio_cmd(&rt.cfg, &rt.store, &rt.snap, &args, ev.client_id).await;
            rt.irc.send_radio_reply(ev.client_id, &reply).await;
        }
        IrcEventKind::Join { channel } => {
            let hist = rt.store.history(Some(&channel), 100)?;
            let lines: Vec<(String, String, String, String)> = hist
                .into_iter()
                .filter(|m| m.env.kind == MsgType::Msg)
                .map(|m| {
                    let t = chrono::DateTime::<chrono::Utc>::from_timestamp(m.env.ts as i64, 0)
                        .unwrap_or(chrono::Utc::now())
                        .to_rfc3339();
                    (
                        t,
                        m.env.origin.to_string(),
                        m.env.dest.to_string(),
                        m.env.body_text(),
                    )
                })
                .collect();
            rt.irc.replay_history(ev.client_id, lines).await;
        }
        _ => {}
    }
    Ok(())
}

async fn send_chat(rt: &Runtime, target: &str, text: &str) -> Result<()> {
    let cfg_g = rt.cfg.lock().clone();
    let (prio, text) = Priority::parse_prefix(text);
    let origin = Callsign::parse(&cfg_g.callsign)?;
    let is_group = target.starts_with('#') || target.starts_with('&');
    let dest = if is_group {
        Callsign::from_raw(
            target
                .trim_start_matches('#')
                .trim_start_matches('&')
                .to_ascii_uppercase(),
        )
    } else {
        Callsign::parse(target)?
    };
    let seq = rt.store.next_seq(origin.as_str())?;
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
    let mut env = Envelope::new_msg(origin, dest, seq, text.as_bytes().to_vec(), hops, flags)?;
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
    rt.irc.tagmsg_delivery(&env.msg_id.hex(), "sent").await;
    let _ = rt.tel.send(TelemetryEvent {
        ts: env.ts as u64,
        kind: "tx".into(),
        origin: Some(env.origin.to_string()),
        dest: Some(env.dest.to_string()),
        hops: Some(env.hops_left),
        snr: None,
        msgid: Some(env.msg_id.hex()),
    });
    if prio == Priority::Emergency && cfg_g.mode.uses_radio() {
        let delay = Duration::from_millis(cfg_g.rf.emergency_dup_ms as u64);
        let _ = dispatch_rf_at(rt, &env, 0, delay, 1).await;
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
    let rf_env = rf_copy(env, preset);
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
    let Some(air) = &rt.air else {
        if let Some(k) = &rt.kiss {
            let mtu = preset.payload_bytes();
            if frag::should_fragment(rf_env, preset.is_hf(), bytes.len(), mtu) {
                let frags = frag::split(rf_env, cfg.rf.frag_k, cfg.rf.frag_m)?;
                for f in frags {
                    k.send(&f.encode()?).await?;
                }
            } else {
                k.send(bytes).await?;
            }
        }
        return Ok(true);
    };
    let mtu = preset.payload_bytes();
    let frames = if frag::should_fragment(rf_env, preset.is_hf(), bytes.len(), mtu) {
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
    Ok(air.enqueue(item))
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
    let cfg = rt.cfg.lock().clone();
    if cfg.mode.uses_radio() {
        let _ = dispatch_rf_rung(rt, env, 0).await;
    }
    if cfg.mode.uses_internet() && env.flags.inet_ok() {
        if let Some(h) = &rt.hub {
            let _ = h.send(env).await;
        }
    }
    if let Some(l) = &rt.lan {
        let _ = l.send(env.clone()).await;
    }
    Ok(())
}

async fn on_envelope(rt: &Runtime, env: Envelope, medium: &str, snr: Option<f32>) -> Result<()> {
    if env.origin.as_str() == rt.engine.our_call {
        rt.store.set_delivery(&env.msg_id, Delivery::Relayed)?;
        rt.irc.tagmsg_delivery(&env.msg_id.hex(), "relayed").await;
        return Ok(());
    }
    if env.kind == MsgType::Frag {
        let reconstructed = rt.assembler.lock().push(&env)?;
        let _ = rt.engine.on_rx(&env, medium, snr)?;
        forward_gateway(rt, &env, medium).await?;
        if let Some(full) = reconstructed {
            return Box::pin(on_envelope(rt, full, medium, snr)).await;
        }
        return Ok(());
    }
    let decision = rt.engine.on_rx(&env, medium, snr)?;
    if decision.action == Action::Suppress {
        if let Some(air) = &rt.air {
            air.cancel(env.msg_id);
        }
    }
    if env.ts.abs_diff(crate::proto::now_ts()) > 300 {
        rt.snap.lock().clock_warn = true;
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
            rt.irc
                .broadcast_privmsg(env.origin.as_str(), &target, &text, Some(&env.msg_id.hex()))
                .await;
            if env.kind == MsgType::Checkin {
                let _ = rt
                    .store
                    .checkin(env.origin.as_str(), &env.body_text(), None, None);
            }
            if env.kind == MsgType::Status {
                if let Some(w) = Welfare::parse(&env.body_text()) {
                    let _ = rt.store.welfare(env.origin.as_str(), w.as_str());
                }
            }
            if env.flags.req_ack() && (env.dest.as_str() == rt.engine.our_call || env.flags.group())
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
                    if cfg_g.mode.uses_internet() && ack.flags.inet_ok() {
                        if let Some(h) = &rt.hub {
                            let _ = h.send(&ack).await;
                        }
                    }
                    if let Some(l) = &rt.lan {
                        let _ = l.send(ack.clone()).await;
                    }
                    let _ = dispatch_rf_at(rt, &ack, 0, dither, 0).await;
                }
            }
            if env.flags.group() {
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
                if let Some(air) = &rt.air {
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
        MsgType::Beacon | MsgType::Ping | MsgType::File => {}
        MsgType::Frag => {}
    }
    let _ = rt.tel.send(TelemetryEvent {
        ts: crate::proto::now_ts() as u64,
        kind: "rx".into(),
        origin: Some(env.origin.to_string()),
        dest: Some(env.dest.to_string()),
        hops: Some(env.hops_left),
        snr,
        msgid: Some(env.msg_id.hex()),
    });
    forward_gateway(rt, &env, medium).await
}

async fn forward_gateway(rt: &Runtime, env: &Envelope, medium: &str) -> Result<()> {
    let cfg_g = rt.cfg.lock().clone();
    if medium == "rf"
        && relay::may_inet_forward(
            cfg_g.mode.uses_internet(),
            env.flags.inet_ok(),
            env.flags.no_inet(),
        )
    {
        if let Some(h) = &rt.hub {
            let _ = h.send(env).await;
        }
    }
    if medium == "inet" || medium == "lan" {
        let heard = rt.store.recently_heard(env.dest.as_str(), 600)?;
        let group_heard = env.flags.group();
        if relay::may_rf_egress(
            cfg_g.mode.is_gateway(),
            cfg_g.gateway.rf_egress,
            heard || group_heard,
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

async fn radio_cmd(
    cfg: &Arc<Mutex<Config>>,
    store: &Arc<Store>,
    snap: &Arc<SharedStatus>,
    args: &str,
    _id: u64,
) -> String {
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
                                return "Switching to Radio drops the internet. Type: /radio mode radio confirm".into();
                            }
                        }
                        cfg.lock().mode = mode;
                        snap.lock().mode = mode;
                        format!("mode is now {}", mode.display_name())
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
            format!(
                "{} | {} | {} | {} | SNR {:.1} | tx {} | retry {} | audio {} | q {}/{} | occ {}% air {} | hub {}{}",
                s.mode.display_name(),
                if s.deferred { "wait" } else { &s.channel },
                s.frequency,
                s.preset,
                s.snr,
                if s.tx_rung.is_empty() { "—" } else { &s.tx_rung },
                s.retries,
                s.audio_label,
                s.queue_out,
                s.queue_hold,
                s.occupancy_pct,
                s.queue_air,
                if s.hub_ok { "up" } else { "down" },
                tnc
            )
        }
        "group" => {
            let sub = sp.next().unwrap_or("");
            match sub {
                "create" => {
                    let name = sp.next().unwrap_or("").to_string();
                    let members: Vec<String> = sp.map(|s| s.to_ascii_uppercase()).collect();
                    if name.is_empty() {
                        return "usage: /radio group create <name> <callsigns...>".into();
                    }
                    let _ = store.group_create(&name, &members);
                    format!("group {name} created")
                }
                "invite" | "add" => {
                    let name = sp.next().unwrap_or("").to_string();
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
                _ => "usage: /radio group create|list|members|invite".into(),
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
        "history" => {
            if sp.next() == Some("purge") {
                let t = sp.next();
                let n = store.purge(t).unwrap_or(0);
                format!("purged {n} messages")
            } else {
                "usage: /radio history purge [target]".into()
            }
        }
        "qsy" => {
            if let Some(mhz) = sp.next() {
                format!("qsy requested to {mhz} MHz (send via rigctl if configured)")
            } else {
                snap.lock().frequency.clone()
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
                cfg.lock().ui.theme = t.to_string();
                format!("theme {t}")
            } else {
                cfg.lock().ui.theme.clone()
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
            "RADIO commands: mode preset status group queue trace history qsy ptt checkin net mute theme update. Channels: /join #name  /invite CALL"
                .into()
        }
        other => format!("unknown RADIO subcommand '{other}'. Try /radio help"),
    }
}
