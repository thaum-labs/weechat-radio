//! SPDX-License-Identifier: Apache-2.0
//! Station runtime: IRC + modem + hub + LAN + relay.

use crate::config::Config;
use crate::emcomm::{Form, Welfare, BULLETIN_CHANNEL, BULLETIN_TTL};
use crate::error::Result;
use crate::ircd::{IrcEvent, IrcEventKind, IrcServer};
use crate::modem::{ControlClient, KissClient, ModemProcess};
use crate::modes::Mode;
use crate::net::hub_client::{ArcFlag, HubClient};
use crate::net::lan::LanMesh;
use crate::presets::{self, Preset};
use crate::proto::{
    load_or_create, Callsign, Envelope, Flags, IdentityKeys, MsgId, MsgType, Priority, FLAG_GROUP,
    FLAG_INET_OK, FLAG_NO_INET, FLAG_REQ_ACK, FLAG_THIRD_PARTY,
};
use crate::relay::{self, Engine};
use crate::status::{self, SharedStatus};
use crate::store::{Delivery, Store};
use crate::telemetry::{self, TelemetryEvent};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

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
    let mut _control: Option<ControlClient> = None;
    if cfg.mode.uses_radio() {
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
            tokio::spawn(async move {
                while let Some(frame) = ev.recv().await {
                    let mut s = snap_c.lock();
                    s.snr = frame.snr;
                    s.ber = frame.ber_pct;
                    s.audio_db = frame.level_db;
                    s.audio_label = presets::audio_level_label(frame.level_db).into();
                    s.channel = "rx".into();
                }
            });
            _control = Some(c);
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
    let cfg = Arc::new(Mutex::new(cfg));
    let our = engine.our_call.clone();

    // Hold-queue pump
    let store_h = store.clone();
    let kiss_h = kiss.clone();
    let hub_h = hub.as_ref().map(|h| h.tx.clone());
    let lan_h = lan_out.clone();
    let snap_h = snap.clone();
    let our_h = our.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tick.tick().await;
            let now = crate::proto::now_ts();
            if let Ok(due) = store_h.hold_due(now) {
                for m in due {
                    if m.env.origin.as_str() == our_h {
                        continue;
                    }
                    if let Ok(bytes) = m.env.encode() {
                        if let Some(k) = &kiss_h {
                            let _ = k.send(&bytes).await;
                        }
                        if let Some(h) = &hub_h {
                            let _ = h.send(bytes.clone()).await;
                        }
                        if let Some(l) = &lan_h {
                            let _ = l.send(m.env.clone()).await;
                        }
                    }
                    let _ = store_h.set_hold(&m.env.msg_id, 0, m.env.hops_left);
                }
            }
            if let Ok((o, h)) = store_h.queue_depth() {
                let mut s = snap_h.lock();
                s.queue_out = o;
                s.queue_hold = h;
                s.hub_ok = false; // updated below via flag
            }
        }
    });

    let hub_flag_s = hub_flag.clone();
    let snap_b = snap.clone();
    let cfg_b = cfg.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tick.tick().await;
            let mut s = snap_b.lock();
            let was = s.hub_ok;
            s.hub_ok = hub_flag_s.get();
            if cfg_b.lock().mode.uses_internet() && was && !s.hub_ok {
                s.hub_banner = "Internet down, radio only".into();
            }
            if s.hub_ok {
                s.hub_banner.clear();
            }
        }
    });

    // Beacon
    let store_b = store.clone();
    let kiss_b = kiss.clone();
    let our_b = our.clone();
    let cfg_be = cfg.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tick.tick().await;
            let cfg = cfg_be.lock().clone();
            if !cfg.mode.uses_radio() {
                continue;
            }
            let Ok(seq) = store_b.next_seq(&our_b) else {
                continue;
            };
            let Ok(origin) = Callsign::parse(&our_b) else {
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
                if let Ok(bytes) = env.encode() {
                    if let Some(k) = &kiss_b {
                        let _ = k.send(&bytes).await;
                    }
                }
            }
        }
    });

    let mut kiss_rx = kiss_rx;
    let mut lan_in = lan_in;

    loop {
        tokio::select! {
            ev = irc_rx.recv() => {
                let Some(ev) = ev else { break };
                if let Err(e) = handle_irc(&cfg, &store, &keys, &engine, &irc, &kiss, &hub, &lan_out, &snap, &tel_tx, ev).await {
                    tracing::warn!("irc event: {e}");
                }
            }
            frame = recv_opt(&mut kiss_rx) => {
                if let Some(payload) = frame {
                    if let Ok(env) = Envelope::decode(&payload) {
                        let _ = on_envelope(&cfg, &store, &keys, &engine, &irc, &kiss, &hub, &lan_out, &snap, &tel_tx, env, "rf").await;
                    }
                }
            }
            env = hub_in_rx.recv() => {
                if let Some(env) = env {
                    let _ = on_envelope(&cfg, &store, &keys, &engine, &irc, &kiss, &hub, &lan_out, &snap, &tel_tx, env, "inet").await;
                }
            }
            env = recv_lan(&mut lan_in) => {
                if let Some(env) = env {
                    let _ = on_envelope(&cfg, &store, &keys, &engine, &irc, &kiss, &hub, &lan_out, &snap, &tel_tx, env, "lan").await;
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

async fn handle_irc(
    cfg: &Arc<Mutex<Config>>,
    store: &Arc<Store>,
    keys: &IdentityKeys,
    _engine: &Engine,
    irc: &IrcServer,
    kiss: &Option<KissClient>,
    hub: &Option<HubClient>,
    lan: &Option<mpsc::Sender<Envelope>>,
    snap: &Arc<SharedStatus>,
    tel: &broadcast::Sender<TelemetryEvent>,
    ev: IrcEvent,
) -> Result<()> {
    match ev.kind {
        IrcEventKind::Privmsg { target, text, .. } => {
            send_chat(
                cfg, store, keys, irc, kiss, hub, lan, snap, tel, &target, &text,
            )
            .await?;
        }
        IrcEventKind::Radio { args } => {
            let reply = radio_cmd(cfg, store, snap, &args, ev.client_id).await;
            irc.send_radio_reply(ev.client_id, &reply).await;
        }
        IrcEventKind::Join { channel } => {
            let hist = store.history(Some(&channel), 100)?;
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
            irc.replay_history(ev.client_id, lines).await;
        }
        _ => {}
    }
    Ok(())
}

async fn send_chat(
    cfg: &Arc<Mutex<Config>>,
    store: &Arc<Store>,
    keys: &IdentityKeys,
    irc: &IrcServer,
    kiss: &Option<KissClient>,
    hub: &Option<HubClient>,
    lan: &Option<mpsc::Sender<Envelope>>,
    snap: &Arc<SharedStatus>,
    tel: &broadcast::Sender<TelemetryEvent>,
    target: &str,
    text: &str,
) -> Result<()> {
    let cfg_g = cfg.lock().clone();
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
    let seq = store.next_seq(origin.as_str())?;
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
        keys.sign_envelope(&mut env)?;
    }
    store.insert(&env, Delivery::Queued)?;
    dispatch(cfg, store, kiss, hub, lan, &env).await?;
    store.set_delivery(&env.msg_id, Delivery::Sent)?;
    irc.tagmsg_delivery(&env.msg_id.hex(), "sent").await;
    let _ = tel.send(TelemetryEvent {
        ts: env.ts as u64,
        kind: "tx".into(),
        origin: Some(env.origin.to_string()),
        dest: Some(env.dest.to_string()),
        hops: Some(env.hops_left),
        snr: None,
        msgid: Some(env.msg_id.hex()),
    });
    let _ = snap;
    Ok(())
}

async fn dispatch(
    cfg: &Arc<Mutex<Config>>,
    store: &Arc<Store>,
    kiss: &Option<KissClient>,
    hub: &Option<HubClient>,
    lan: &Option<mpsc::Sender<Envelope>>,
    env: &Envelope,
) -> Result<()> {
    let cfg = cfg.lock().clone();
    let bytes = env.encode()?;
    if cfg.mode.uses_radio() {
        if let Some(k) = kiss {
            k.send(&bytes).await?;
        }
    }
    if cfg.mode.uses_internet() && env.flags.inet_ok() {
        if let Some(h) = hub {
            let _ = h.send(env).await;
        }
    } else if cfg.mode.uses_internet() && !hub.is_none() {
        // hold for flush when hub returns: already in store
    }
    if let Some(l) = lan {
        let _ = l.send(env.clone()).await;
    }
    let _ = store;
    Ok(())
}

async fn on_envelope(
    cfg: &Arc<Mutex<Config>>,
    store: &Arc<Store>,
    keys: &IdentityKeys,
    engine: &Engine,
    irc: &IrcServer,
    kiss: &Option<KissClient>,
    hub: &Option<HubClient>,
    lan: &Option<mpsc::Sender<Envelope>>,
    snap: &Arc<SharedStatus>,
    tel: &broadcast::Sender<TelemetryEvent>,
    env: Envelope,
    medium: &str,
) -> Result<()> {
    if env.origin.as_str() == engine.our_call {
        // our own echo
        store.set_delivery(&env.msg_id, Delivery::Relayed)?;
        irc.tagmsg_delivery(&env.msg_id.hex(), "relayed").await;
        return Ok(());
    }
    let decision = engine.on_rx(&env, medium, None)?;
    if env.ts.abs_diff(crate::proto::now_ts()) > 300 {
        snap.lock().clock_warn = true;
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
            irc.broadcast_privmsg(env.origin.as_str(), &target, &text, Some(&env.msg_id.hex()))
                .await;
            if env.kind == MsgType::Checkin {
                let _ = store.checkin(env.origin.as_str(), &env.body_text(), None, None);
            }
            if env.kind == MsgType::Status {
                if let Some(w) = Welfare::parse(&env.body_text()) {
                    let _ = store.welfare(env.origin.as_str(), w.as_str());
                }
            }
            if env.flags.req_ack() && (env.dest.as_str() == engine.our_call || env.flags.group()) {
                let seq = store.next_seq(&engine.our_call)?;
                let mut ack =
                    Envelope::ack_for(&env, Callsign::from_raw(engine.our_call.clone()), seq);
                if cfg.lock().mode.uses_internet() {
                    let _ = keys.sign_envelope(&mut ack);
                }
                let _ = dispatch(cfg, store, kiss, hub, lan, &ack).await;
            }
            if env.flags.group() {
                let _ = store.receipt(
                    &env.dest.to_string().to_ascii_lowercase(),
                    &env.msg_id,
                    env.origin.as_str(),
                );
                if store
                    .group_all_received(&env.dest.to_string().to_ascii_lowercase(), &env.msg_id)?
                {
                    store.set_delivery(&env.msg_id, Delivery::All)?;
                    irc.tagmsg_delivery(&env.msg_id.hex(), "all").await;
                }
            }
        }
        MsgType::Ack => {
            if let Some(id) = env.acked_id() {
                store.set_delivery(&id, Delivery::Delivered)?;
                irc.tagmsg_delivery(&id.hex(), "delivered").await;
            }
        }
        MsgType::Have => {
            handle_have(cfg, store, keys, kiss, hub, lan, &env).await?;
        }
        MsgType::Want => {
            handle_want(cfg, store, kiss, hub, lan, &env).await?;
        }
        MsgType::Beacon | MsgType::Ping | MsgType::File => {}
    }
    let _ = decision;
    let _ = tel.send(TelemetryEvent {
        ts: crate::proto::now_ts() as u64,
        kind: "rx".into(),
        origin: Some(env.origin.to_string()),
        dest: Some(env.dest.to_string()),
        hops: Some(env.hops_left),
        snr: None,
        msgid: Some(env.msg_id.hex()),
    });

    // Gateway forward RF -> internet
    let cfg_g = cfg.lock().clone();
    if medium == "rf"
        && relay::may_inet_forward(
            cfg_g.mode.uses_internet(),
            env.flags.inet_ok(),
            env.flags.no_inet(),
        )
    {
        if let Some(h) = hub {
            let _ = h.send(&env).await;
        }
    }
    // Gateway internet -> RF
    if medium == "inet" || medium == "lan" {
        let heard = store.recently_heard(env.dest.as_str(), 600)?;
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
            if let Some(k) = kiss {
                if let Ok(bytes) = env.encode() {
                    let _ = k.send(&bytes).await;
                }
            }
        }
    }
    Ok(())
}

async fn handle_have(
    cfg: &Arc<Mutex<Config>>,
    store: &Arc<Store>,
    keys: &IdentityKeys,
    kiss: &Option<KissClient>,
    hub: &Option<HubClient>,
    lan: &Option<mpsc::Sender<Envelope>>,
    env: &Envelope,
) -> Result<()> {
    let cfg_g = cfg.lock().clone();
    let ids: Vec<&str> = std::str::from_utf8(&env.body)
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .collect();
    let dest = env.dest.to_string();
    let ours = store.have_digest(
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
    let seq = store.next_seq(&cfg_g.callsign)?;
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
        let _ = keys.sign_envelope(&mut want);
    }
    dispatch(cfg, store, kiss, hub, lan, &want).await
}

async fn handle_want(
    cfg: &Arc<Mutex<Config>>,
    store: &Arc<Store>,
    kiss: &Option<KissClient>,
    hub: &Option<HubClient>,
    lan: &Option<mpsc::Sender<Envelope>>,
    env: &Envelope,
) -> Result<()> {
    let ids = std::str::from_utf8(&env.body).unwrap_or("").split(',');
    for id in ids {
        if let Some(mid) = MsgId::parse_hex(id) {
            if let Some(stored) = store.get(&mid)? {
                let _ = dispatch(cfg, store, kiss, hub, lan, &stored.env).await;
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
                if let Some(pr) = Preset::parse(p) {
                    cfg.lock().modem.preset = pr.as_str().into();
                    snap.lock().preset = pr.as_str().into();
                    format!("preset {}", pr.as_str())
                } else {
                    "unknown preset. Use vhf-fm, hf-good, hf-poor, hf-weak, vox-safe.".into()
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
            format!(
                "{} | {} | {} | {} | SNR {:.1} | audio {} | q {}/{} | hub {}",
                s.mode.display_name(),
                s.channel,
                s.frequency,
                s.preset,
                s.snr,
                s.audio_label,
                s.queue_out,
                s.queue_hold,
                if s.hub_ok { "up" } else { "down" }
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
                "list" => store.group_list().unwrap_or_default().join(", "),
                "members" => {
                    let name = sp.next().unwrap_or("");
                    store.group_members(name).unwrap_or_default().join(", ")
                }
                _ => "usage: /radio group create|list|members".into(),
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
        "modem" => format!("kiss {}:{}", cfg.lock().modem.host, cfg.lock().modem.kiss_port),
        "" | "help" => {
            "RADIO commands: mode preset status group queue trace history qsy ptt checkin net mute theme update"
                .into()
        }
        other => format!("unknown RADIO subcommand '{other}'. Try /radio help"),
    }
}
