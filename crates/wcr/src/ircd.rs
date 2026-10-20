//! SPDX-License-Identifier: Apache-2.0
//! Embedded localhost IRC server for WeeChat and the built-in TUI.

use crate::emcomm::BULLETIN_CHANNEL;
use crate::error::Result;
use chrono::Utc;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct IrcEvent {
    pub client_id: u64,
    pub nick: String,
    pub kind: IrcEventKind,
}

#[derive(Clone, Debug)]
pub enum IrcEventKind {
    Privmsg {
        target: String,
        text: String,
        tags: HashMap<String, String>,
    },
    Join {
        channel: String,
    },
    Part {
        channel: String,
    },
    Radio {
        args: String,
    },
    Invite {
        target: String,
        channel: String,
    },
    Names {
        channel: String,
    },
    Nick {
        nick: String,
    },
    Quit,
}

#[derive(Clone, Debug)]
pub struct Outgoing {
    pub to_nick: Option<String>,
    pub raw: String,
}

struct Client {
    id: u64,
    nick: String,
    user: String,
    caps: HashSet<String>,
    channels: HashSet<String>,
    registered: bool,
    tx: mpsc::Sender<String>,
    /// IRC PRIVMSG lines already delivered to this client (by msgid).
    delivered_msgids: HashSet<String>,
    delivered_order: VecDeque<String>,
    flood_tokens: f32,
    flood_last: Instant,
}

const FLOOD_CAPACITY: f32 = 24.0;
const FLOOD_REFILL_PER_SEC: f32 = 8.0;
const DELIVERED_MSGID_CAP: usize = 512;

const MAX_IRC_LINE: usize = 4096;

#[derive(Clone)]
pub struct IrcServer {
    inner: Arc<Mutex<Inner>>,
    events: mpsc::Sender<IrcEvent>,
}

struct Inner {
    next_id: u64,
    clients: HashMap<u64, Client>,
    server_name: String,
    /// Client currently inside `send_chat`, so the `[tx]` mark can be scoped
    /// before the handler returns.
    origin_client: Option<u64>,
    origin_target: String,
    /// msgid → (client that sent it, IRC target of that send).
    outbound: HashMap<String, (u64, String)>,
    outbound_order: VecDeque<String>,
    /// msgid → furthest delivery state shown for it. Marks never move back.
    shown: HashMap<String, String>,
}

const OFFERED_CAPS: &[&str] = &[
    "message-tags",
    "echo-message",
    "server-time",
    "msgid",
    "batch",
    "draft/chathistory",
];

impl IrcServer {
    pub fn new(events: mpsc::Sender<IrcEvent>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                next_id: 1,
                clients: HashMap::new(),
                server_name: "wcr.local".into(),
                origin_client: None,
                origin_target: String::new(),
                outbound: HashMap::new(),
                outbound_order: VecDeque::new(),
                shown: HashMap::new(),
            })),
            events,
        }
    }

    pub async fn listen(self, bind: &str) -> Result<()> {
        let listener = TcpListener::bind(bind).await?;
        self.accept_loop(listener).await
    }

    pub async fn accept_loop(self, listener: TcpListener) -> Result<()> {
        tracing::info!(
            "IRC listening on {}",
            listener
                .local_addr()
                .map(|a| a.to_string())
                .unwrap_or_else(|_| "irc".into())
        );
        loop {
            let (stream, addr) = listener.accept().await?;
            tracing::debug!("IRC client {addr}");
            let server = self.clone();
            tokio::spawn(async move {
                if let Err(e) = server.handle_conn(stream).await {
                    tracing::debug!("IRC client gone: {e}");
                }
            });
        }
    }

    async fn handle_conn(&self, stream: TcpStream) -> Result<()> {
        let (read, mut write) = stream.into_split();
        let (tx, mut rx) = mpsc::channel::<String>(128);
        let id = {
            let mut g = self.inner.lock();
            let id = g.next_id;
            g.next_id += 1;
            g.clients.insert(
                id,
                Client {
                    id,
                    nick: String::new(),
                    user: String::new(),
                    caps: HashSet::new(),
                    channels: HashSet::new(),
                    registered: false,
                    tx: tx.clone(),
                    delivered_msgids: HashSet::new(),
                    delivered_order: VecDeque::new(),
                    flood_tokens: FLOOD_CAPACITY,
                    flood_last: Instant::now(),
                },
            );
            id
        };
        let writer = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if write.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if write.write_all(b"\r\n").await.is_err() {
                    break;
                }
            }
        });
        let mut reader = tokio::io::BufReader::new(read);
        while let Some(line) = read_irc_line(&mut reader).await? {
            self.on_line(id, &line).await?;
        }
        let nick = {
            let mut g = self.inner.lock();
            g.clients.remove(&id).map(|c| c.nick).unwrap_or_default()
        };
        let _ = self
            .events
            .send(IrcEvent {
                client_id: id,
                nick,
                kind: IrcEventKind::Quit,
            })
            .await;
        drop(tx);
        let _ = writer.await;
        Ok(())
    }

    async fn on_line(&self, id: u64, line: &str) -> Result<()> {
        let line = line.trim_end();
        if line.is_empty() {
            return Ok(());
        }
        let (tags, rest) = split_tags(line);
        let parts = split_irc(rest);
        if parts.is_empty() {
            return Ok(());
        }
        let cmd = parts[0].to_ascii_uppercase();
        if !flood_allows(&self.inner, id, &cmd) {
            let nick = self.nick(id);
            let srv = self.inner.lock().server_name.clone();
            self.send_raw(
                id,
                &format!(
                    ":{srv} NOTICE {nick} :Slow down — message rate limited (try again in a moment)"
                ),
            )
            .await;
            return Ok(());
        }
        match cmd.as_str() {
            "CAP" => self.cap(id, &parts).await,
            "NICK" => {
                if let Some(n) = parts.get(1) {
                    let nick = n.trim_start_matches(':').to_string();
                    if crate::proto::Callsign::parse(&nick).is_err() {
                        self.send_numeric(id, 432, &format!("{nick} :Erroneous nickname"))
                            .await;
                        return Ok(());
                    }
                    {
                        let mut g = self.inner.lock();
                        if let Some(c) = g.clients.get_mut(&id) {
                            c.nick = nick.clone();
                        }
                    }
                    self.maybe_welcome(id).await;
                    let _ = self
                        .events
                        .send(IrcEvent {
                            client_id: id,
                            nick: nick.clone(),
                            kind: IrcEventKind::Nick { nick },
                        })
                        .await;
                }
            }
            "USER" => {
                if let Some(u) = parts.get(1) {
                    let mut g = self.inner.lock();
                    if let Some(c) = g.clients.get_mut(&id) {
                        c.user = u.clone();
                    }
                }
                self.maybe_welcome(id).await;
            }
            "PING" => {
                let token = parts.get(1).cloned().unwrap_or_else(|| "wcr".into());
                self.send_raw(id, &format!("PONG :{}", token.trim_start_matches(':')))
                    .await;
            }
            "JOIN" => {
                if let Some(list) = parts.get(1) {
                    for ch in list.split(',') {
                        if let Some(err) = crate::slash::channel_limit_error(ch) {
                            let nick = self.nick(id);
                            self.send_raw(id, &format!(":wcr.local NOTICE {nick} :{err}"))
                                .await;
                            continue;
                        }
                        let ch = ch.to_string();
                        let fresh = {
                            let mut g = self.inner.lock();
                            g.clients.get_mut(&id).is_some_and(|c| {
                                let fresh = !c.channels.iter().any(|j| j.eq_ignore_ascii_case(&ch));
                                c.channels.insert(ch.clone());
                                fresh
                            })
                        };
                        let nick = self.nick(id);
                        // #bulletin is joined at welcome. A client's own JOIN
                        // of it, or any repeat JOIN, gets no second echo. It
                        // still asks the node for history.
                        if fresh {
                            self.send_raw(id, &format!(":{nick} JOIN {ch}")).await;
                            self.send_numeric(id, 332, &format!("{ch} :WeeChat Radio"))
                                .await;
                        }
                        let _ = self
                            .events
                            .send(IrcEvent {
                                client_id: id,
                                nick,
                                kind: IrcEventKind::Join { channel: ch },
                            })
                            .await;
                    }
                }
            }
            "PART" => {
                if let Some(ch) = parts.get(1) {
                    let nick = self.nick(id);
                    {
                        let mut g = self.inner.lock();
                        if let Some(c) = g.clients.get_mut(&id) {
                            c.channels.remove(ch);
                        }
                    }
                    self.send_raw(id, &format!(":{nick} PART {ch}")).await;
                    let _ = self
                        .events
                        .send(IrcEvent {
                            client_id: id,
                            nick,
                            kind: IrcEventKind::Part {
                                channel: ch.clone(),
                            },
                        })
                        .await;
                }
            }
            "INVITE" => {
                if parts.len() >= 3 {
                    let target = parts[1].clone();
                    let ch = parts[2].clone();
                    if let Some(err) = crate::slash::channel_limit_error(&ch) {
                        let nick = self.nick(id);
                        self.send_raw(id, &format!(":wcr.local NOTICE {nick} :{err}"))
                            .await;
                    } else {
                        let from = self.nick(id);
                        self.send_numeric(id, 341, &format!("{target} {ch}")).await;
                        if let Some(tid) = self.id_by_nick(&target) {
                            self.send_raw(tid, &format!(":{from} INVITE {target} {ch}"))
                                .await;
                        }
                        let _ = self
                            .events
                            .send(IrcEvent {
                                client_id: id,
                                nick: from,
                                kind: IrcEventKind::Invite {
                                    target,
                                    channel: ch,
                                },
                            })
                            .await;
                    }
                }
            }
            "PRIVMSG" => {
                if parts.len() >= 3 {
                    let target = parts[1].clone();
                    let text = trailing(&parts[2..]);
                    if text.is_empty() {
                        return Ok(());
                    }
                    if let Some(err) = crate::slash::channel_limit_error(&target) {
                        let nick = self.nick(id);
                        self.send_raw(id, &format!(":wcr.local NOTICE {nick} :{err}"))
                            .await;
                    } else {
                        let nick = self.nick(id);
                        if self.has_cap(id, "echo-message") {
                            let tagged = self.tag_privmsg(&nick, &target, &text, None, None);
                            self.send_raw(id, &tagged).await;
                        }
                        let _ = self
                            .events
                            .send(IrcEvent {
                                client_id: id,
                                nick,
                                kind: IrcEventKind::Privmsg { target, text, tags },
                            })
                            .await;
                    }
                }
            }
            "RADIO" => {
                let args = parts[1..].join(" ");
                let nick = self.nick(id);
                let _ = self
                    .events
                    .send(IrcEvent {
                        client_id: id,
                        nick,
                        kind: IrcEventKind::Radio { args },
                    })
                    .await;
            }
            "QUIT" => {}
            "WHO" => {
                let nick = self.nick(id);
                self.send_numeric(id, 352, &format!("{nick} wcr irc :0 {nick}"))
                    .await;
                self.send_numeric(id, 315, &format!("{nick} :End of /WHO"))
                    .await;
            }
            "NAMES" => {
                if let Some(ch) = parts.get(1) {
                    let nick = self.nick(id);
                    let _ = self
                        .events
                        .send(IrcEvent {
                            client_id: id,
                            nick,
                            kind: IrcEventKind::Names {
                                channel: ch.clone(),
                            },
                        })
                        .await;
                }
            }
            "WHOIS" => {}
            "MODE" => self.reply_mode(id, &parts).await,
            "HELP" => self.reply_help(id).await,
            other => {
                self.send_numeric(id, 421, &format!("{other} :Unknown command"))
                    .await;
            }
        }
        Ok(())
    }

    async fn cap(&self, id: u64, parts: &[String]) {
        let sub = parts
            .get(1)
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_default();
        match sub.as_str() {
            "LS" => {
                self.send_raw(id, &format!("CAP * LS :{}", OFFERED_CAPS.join(" ")))
                    .await;
            }
            "REQ" => {
                let reqs = trailing(&parts[2..]);
                let mut ack = Vec::new();
                {
                    let mut g = self.inner.lock();
                    if let Some(c) = g.clients.get_mut(&id) {
                        for cap in reqs.split_whitespace() {
                            if OFFERED_CAPS.contains(&cap) {
                                c.caps.insert(cap.to_string());
                                ack.push(cap.to_string());
                            }
                        }
                    }
                }
                self.send_raw(id, &format!("CAP * ACK :{}", ack.join(" ")))
                    .await;
            }
            "END" => self.maybe_welcome(id).await,
            "LIST" => {
                let caps = {
                    let g = self.inner.lock();
                    g.clients
                        .get(&id)
                        .map(|c| c.caps.iter().cloned().collect::<Vec<_>>())
                        .unwrap_or_default()
                };
                self.send_raw(id, &format!("CAP * LIST :{}", caps.join(" ")))
                    .await;
            }
            _ => {}
        }
    }

    async fn maybe_welcome(&self, id: u64) {
        let (nick, already) = {
            let mut g = self.inner.lock();
            match g.clients.get_mut(&id) {
                Some(c) if !c.nick.is_empty() && !c.registered => {
                    c.registered = true;
                    (c.nick.clone(), false)
                }
                _ => return,
            }
        };
        if already {
            return;
        }
        let srv = self.inner.lock().server_name.clone();
        self.send_numeric(id, 1, &format!("Welcome to WeeChat Radio, {nick}"))
            .await;
        self.send_numeric(id, 2, &format!("Your host is {srv}"))
            .await;
        self.send_numeric(
            id,
            3,
            "This server is a radio node, not the public internet",
        )
        .await;
        self.send_numeric(id, 4, &format!("{srv} {} o o", env!("CARGO_PKG_VERSION")))
            .await;
        self.send_numeric(id, 376, ":End of MOTD").await;
        // Auto-join bulletin
        self.send_raw(id, &format!(":{nick} JOIN #bulletin")).await;
        {
            let mut g = self.inner.lock();
            if let Some(c) = g.clients.get_mut(&id) {
                c.channels.insert("#bulletin".into());
            }
        }
    }

    fn nick(&self, id: u64) -> String {
        self.inner
            .lock()
            .clients
            .get(&id)
            .map(|c| c.nick.clone())
            .unwrap_or_else(|| "*".into())
    }

    fn id_by_nick(&self, nick: &str) -> Option<u64> {
        self.inner
            .lock()
            .clients
            .iter()
            .find(|(_, c)| c.nick.eq_ignore_ascii_case(nick))
            .map(|(id, _)| *id)
    }

    fn has_cap(&self, id: u64, cap: &str) -> bool {
        self.inner
            .lock()
            .clients
            .get(&id)
            .map(|c| c.caps.contains(cap))
            .unwrap_or(false)
    }

    pub async fn send_raw(&self, id: u64, line: &str) {
        let tx = self.inner.lock().clients.get(&id).map(|c| c.tx.clone());
        if let Some(tx) = tx {
            let _ = tx.send(line.to_string()).await;
        }
    }

    async fn send_numeric(&self, id: u64, num: u16, rest: &str) {
        let nick = self.nick(id);
        let who = if nick.is_empty() { "*" } else { nick.as_str() };
        let srv = self.inner.lock().server_name.clone();
        self.send_raw(id, &format!(":{srv} {num:03} {who} {rest}"))
            .await;
    }

    pub fn tag_privmsg(
        &self,
        from: &str,
        target: &str,
        text: &str,
        msgid: Option<&str>,
        via: Option<&str>,
    ) -> String {
        let time = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut tags = format!("server-time={time}");
        if let Some(id) = msgid {
            tags.push_str(&format!(";msgid={id}"));
        }
        if let Some(v) = crate::store::normalize_via(via) {
            tags.push_str(&format!(";+radio/via={v}"));
        }
        format!("@{tags} :{from} PRIVMSG {target} :{text}")
    }

    pub async fn broadcast_privmsg(
        &self,
        from: &str,
        target: &str,
        text: &str,
        msgid: Option<&str>,
        via: Option<&str>,
    ) {
        let target = normalize_irc_channel(target);
        let line = self.tag_privmsg(from, &target, text, msgid, via);
        let via_mark = crate::store::via_bracket(via.unwrap_or(""));
        let plain = if via_mark.is_empty() {
            format!(":{from} PRIVMSG {target} :{text}")
        } else {
            format!(":{from} PRIVMSG {target} :{via_mark} {text}")
        };
        let clients: Vec<(u64, bool, bool)> = {
            let g = self.inner.lock();
            g.clients
                .values()
                .map(|c| {
                    let in_chan = client_receives_target(c, &target, from);
                    (c.id, in_chan, c.caps.contains("message-tags"))
                })
                .collect()
        };
        for (id, include, tags) in clients {
            if !include {
                continue;
            }
            if let Some(mid) = msgid {
                if !remember_msgid(&self.inner, id, mid) {
                    continue;
                }
            }
            self.send_raw(id, if tags { &line } else { &plain }).await;
        }
    }

    /// Tell every local IRC client they were invited (remote RF invite).
    pub async fn send_invite(&self, from: &str, channel: &str) {
        let clients: Vec<(u64, String)> = {
            let g = self.inner.lock();
            g.clients.values().map(|c| (c.id, c.nick.clone())).collect()
        };
        for (id, nick) in clients {
            self.send_raw(id, &format!(":{from} INVITE {nick} {channel}"))
                .await;
        }
    }

    pub async fn notice_all(&self, text: &str) {
        let srv = self.inner.lock().server_name.clone();
        let ids: Vec<u64> = self.inner.lock().clients.keys().copied().collect();
        for id in ids {
            let nick = self.nick(id);
            self.send_raw(id, &format!(":{srv} NOTICE {nick} :{text}"))
                .await;
        }
    }

    /// Remember which client is inside a send, so the mark can name that client
    /// and the channel or nick the line was addressed to.
    pub fn begin_origin(&self, client_id: u64, target: &str) {
        let mut g = self.inner.lock();
        g.origin_client = Some(client_id);
        g.origin_target = normalize_irc_channel(target);
    }

    pub fn end_origin(&self) {
        let mut g = self.inner.lock();
        g.origin_client = None;
        g.origin_target.clear();
    }

    pub async fn reply_names(&self, id: u64, channel: &str, nicks: &[String]) {
        let ch = normalize_irc_channel(channel);
        let body = nicks.join(" ");
        self.send_numeric(id, 353, &format!("= {ch} :{body}")).await;
        self.send_numeric(
            id,
            366,
            &format!("{ch} :End of /NAMES. Members stored on this station, not the whole chain."),
        )
        .await;
    }

    pub async fn tagmsg_delivery(&self, msgid: &str, state: &str) {
        self.tagmsg_progress(msgid, state, None).await;
    }

    pub async fn tagmsg_progress(&self, msgid: &str, state: &str, tries: Option<u32>) {
        let Some((id, target)) = self.bind_delivery(msgid) else {
            return;
        };
        if !self.take_mark(msgid, state) {
            return;
        }
        let mut tags = format!("msgid={msgid};+radio/delivery={state};+radio/msgid={msgid}");
        if let Some(n) = tries.filter(|n| *n > 0) {
            tags.push_str(&format!(";+radio/tries={n}"));
        }
        let line = format!("@{tags} TAGMSG {target}");
        let (has_tags, nick) = {
            let g = self.inner.lock();
            match g.clients.get(&id) {
                Some(c) => (c.caps.contains("message-tags"), c.nick.clone()),
                None => return,
            }
        };
        if has_tags {
            self.send_raw(id, &line).await;
        } else if !nick.is_empty() {
            let notice = if let Some(n) = tries.filter(|n| *n > 0) {
                format!(":wcr.local NOTICE {nick} :[{state} x{n}] {msgid}")
            } else {
                format!(":wcr.local NOTICE {nick} :[{state}] {msgid}")
            };
            self.send_raw(id, &notice).await;
        }
    }

    /// True when `state` is the same mark or further along than the one
    /// already shown. A late `[tx]` after `[gw]` is dropped.
    fn take_mark(&self, msgid: &str, state: &str) -> bool {
        let mut g = self.inner.lock();
        if let Some(prev) = g.shown.get(msgid) {
            if mark_rank(state) < mark_rank(prev) {
                return false;
            }
        }
        // A retry is a try count, not a mark of its own.
        if !state.starts_with("retry") {
            g.shown.insert(msgid.to_string(), state.to_string());
        }
        true
    }

    /// Scope a delivery mark to the client that sent `msgid`.
    /// Returns nothing when this station did not originate the line.
    fn bind_delivery(&self, msgid: &str) -> Option<(u64, String)> {
        if msgid.is_empty() {
            return None;
        }
        let mut g = self.inner.lock();
        if let Some(id) = g.origin_client {
            let target = if g.origin_target.is_empty() {
                g.clients
                    .get(&id)
                    .map(|c| c.nick.clone())
                    .unwrap_or_default()
            } else {
                g.origin_target.clone()
            };
            if target.is_empty() {
                return None;
            }
            g.outbound.insert(msgid.to_string(), (id, target.clone()));
            g.outbound_order.push_back(msgid.to_string());
            while g.outbound_order.len() > DELIVERED_MSGID_CAP {
                if let Some(old) = g.outbound_order.pop_front() {
                    g.outbound.remove(&old);
                    g.shown.remove(&old);
                }
            }
            return Some((id, target));
        }
        g.outbound.get(msgid).cloned()
    }

    pub async fn replay_history(
        &self,
        id: u64,
        lines: Vec<(
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        )>,
    ) {
        // (time, from, target, text, msgid, delivery, via, tries)
        let has_batch = self.has_cap(id, "batch");
        if has_batch {
            self.send_raw(id, ":wcr.local BATCH +hist chathistory")
                .await;
        }
        for (time, from, target, text, msgid, delivery, via, tries) in lines {
            // [gw] is display only, so the store still says sent. Replay the
            // furthest mark this station showed, not the stored one.
            let delivery = match self.inner.lock().shown.get(&msgid) {
                Some(shown) if mark_rank(shown) > mark_rank(&delivery) => shown.clone(),
                _ => delivery,
            };
            let mut tags =
                format!("server-time={time};batch=hist;msgid={msgid};+radio/delivery={delivery}");
            if let Some(v) = crate::store::normalize_via(Some(via.as_str())) {
                tags.push_str(&format!(";+radio/via={v}"));
            }
            if !tries.is_empty() && tries != "0" {
                tags.push_str(&format!(";+radio/tries={tries}"));
            }
            let target = normalize_irc_channel(&target);
            let tagged = format!("@{tags} :{from} PRIVMSG {target} :{text}");
            if remember_msgid(&self.inner, id, &msgid) {
                self.send_raw(id, &tagged).await;
            }
        }
        if has_batch {
            self.send_raw(id, ":wcr.local BATCH -hist").await;
        }
    }

    pub fn client_count(&self) -> usize {
        self.inner.lock().clients.len()
    }

    pub fn first_client_id(&self) -> Option<u64> {
        self.inner.lock().clients.keys().next().copied()
    }

    async fn reply_mode(&self, id: u64, parts: &[String]) {
        let target = parts
            .get(1)
            .map(|s| s.trim_start_matches(':'))
            .unwrap_or("");
        if target.is_empty() {
            self.send_numeric(id, 461, "MODE :Not enough parameters")
                .await;
            return;
        }
        if target.starts_with('#') || target.starts_with('&') {
            let ch = normalize_irc_channel(target);
            let modes = crate::slash::channel_mode(&ch);
            self.send_numeric(id, 324, &format!("{ch} {modes}")).await;
            if parts.len() > 2 {
                let note = if crate::slash::is_bulletin(&ch) {
                    "#bulletin is public. Channel mode stays +."
                } else {
                    "Closed group. Channel mode stays +i (invite only). Use /invite CALL."
                };
                let nick = self.nick(id);
                let srv = self.inner.lock().server_name.clone();
                self.send_raw(id, &format!(":{srv} NOTICE {nick} :{note}"))
                    .await;
            }
        } else {
            self.send_numeric(id, 221, "+").await;
        }
    }

    async fn reply_help(&self, id: u64) {
        let lines = [
            "WeeChat Radio commands:",
            "/join #name — closed group. #bulletin is the public channel.",
            "/invite CALL — add them and send an over-air notice.",
            "/part — leave the current closed group.",
            "/mode #chan — 324 reply. +i means a closed group.",
            "/msg CALL text — direct message.",
            "/radio help — mode, preset, status, group, prio.",
            "Marks: [tx] sent, [rl] heard own frame on RF, [gw] a gateway accepted it onto the hub, [ok] delivered, [all] every closed-group member, [no] one wait after the last copy, [xN] radio copies.",
            "/radio flush — mark this station's unacked outbound [no] and drop relay holds.",
            "#bulletin stops at [ok]. It has no member list, so it never shows [all].",
        ];
        self.send_numeric(id, 704, ":wcr HELP").await;
        for line in lines {
            self.send_numeric(id, 705, &format!(":{line}")).await;
        }
        self.send_numeric(id, 706, ":End of /HELP").await;
    }

    pub async fn send_radio_reply(&self, id: u64, text: &str) {
        let nick = self.nick(id);
        for line in text.lines() {
            self.send_raw(id, &format!(":wcr.local NOTICE {nick} :{line}"))
                .await;
        }
    }
}

async fn read_irc_line<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Result<Option<String>> {
    let mut line = Vec::new();
    loop {
        if line.len() >= MAX_IRC_LINE {
            discard_irc_line_remainder(reader).await?;
            return Ok(Some(finish_irc_line(&line)));
        }
        let buf = reader.fill_buf().await?;
        if buf.is_empty() {
            return Ok(if line.is_empty() {
                None
            } else {
                Some(finish_irc_line(&line))
            });
        }
        if let Some(nl) = buf.iter().position(|&b| b == b'\n') {
            let chunk = &buf[..nl];
            let room = MAX_IRC_LINE - line.len();
            if chunk.len() > room {
                line.extend_from_slice(&chunk[..room]);
            } else {
                line.extend_from_slice(chunk);
            }
            reader.consume(nl + 1);
            return Ok(Some(finish_irc_line(&line)));
        }
        let room = MAX_IRC_LINE - line.len();
        if buf.len() > room {
            line.extend_from_slice(&buf[..room]);
            reader.consume(room);
            discard_irc_line_remainder(reader).await?;
            return Ok(Some(finish_irc_line(&line)));
        }
        let n = buf.len();
        line.extend_from_slice(buf);
        reader.consume(n);
    }
}

fn finish_irc_line(line: &[u8]) -> String {
    let mut end = line.len();
    while end > 0 && line[end - 1] == b'\r' {
        end -= 1;
    }
    String::from_utf8_lossy(&line[..end]).into_owned()
}

async fn discard_irc_line_remainder<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Result<()> {
    loop {
        let (consume, done) = {
            let buf = reader.fill_buf().await?;
            if buf.is_empty() {
                return Ok(());
            }
            if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                (pos + 1, true)
            } else {
                (buf.len(), false)
            }
        };
        reader.consume(consume);
        if done {
            return Ok(());
        }
    }
}

fn split_tags(line: &str) -> (HashMap<String, String>, &str) {
    if let Some(rest) = line.strip_prefix('@') {
        if let Some((tagstr, rest)) = rest.split_once(' ') {
            let mut map = HashMap::new();
            for t in tagstr.split(';') {
                if let Some((k, v)) = t.split_once('=') {
                    map.insert(k.to_string(), v.to_string());
                } else {
                    map.insert(t.to_string(), String::new());
                }
            }
            return (map, rest);
        }
    }
    (HashMap::new(), line)
}

fn split_irc(line: &str) -> Vec<String> {
    if let Some((head, trail)) = line.split_once(" :") {
        let mut v: Vec<String> = head.split_whitespace().map(|s| s.to_string()).collect();
        v.push(trail.to_string());
        v
    } else {
        line.split_whitespace().map(|s| s.to_string()).collect()
    }
}

fn trailing(parts: &[String]) -> String {
    parts.join(" ")
}

fn normalize_irc_channel(target: &str) -> String {
    if crate::slash::is_bulletin(target) {
        return BULLETIN_CHANNEL.to_string();
    }
    if target.starts_with('#') || target.starts_with('&') {
        return target.to_ascii_lowercase();
    }
    target.to_string()
}

/// Order of delivery marks: [..] < [tx] < [rl] < [gw] < [no] < [ok] < [all].
/// A retry only updates the try count of a [tx] or [rl] line, so it ranks
/// with [rl]. "waiting on acks" sits with [ok].
fn mark_rank(state: &str) -> u8 {
    if state.starts_with("retry") {
        return crate::store::Delivery::Relayed.rank();
    }
    if state == "waiting" {
        return crate::store::Delivery::Delivered.rank();
    }
    crate::store::Delivery::parse(state).rank()
}

fn client_receives_target(c: &Client, target: &str, from: &str) -> bool {
    if target.starts_with('#') || target.starts_with('&') {
        return c.channels.iter().any(|ch| ch.eq_ignore_ascii_case(target));
    }
    c.nick.eq_ignore_ascii_case(target) || c.nick.eq_ignore_ascii_case(from)
}

fn remember_msgid(inner: &Arc<Mutex<Inner>>, client_id: u64, msgid: &str) -> bool {
    if msgid.is_empty() {
        return true;
    }
    let mut g = inner.lock();
    let Some(c) = g.clients.get_mut(&client_id) else {
        return false;
    };
    if c.delivered_msgids.contains(msgid) {
        return false;
    }
    c.delivered_msgids.insert(msgid.to_string());
    c.delivered_order.push_back(msgid.to_string());
    while c.delivered_order.len() > DELIVERED_MSGID_CAP {
        if let Some(old) = c.delivered_order.pop_front() {
            c.delivered_msgids.remove(&old);
        }
    }
    true
}

fn flood_allows(inner: &Arc<Mutex<Inner>>, id: u64, cmd: &str) -> bool {
    if matches!(
        cmd,
        "CAP" | "NICK" | "USER" | "PASS" | "PING" | "PONG" | "QUIT" | "AUTH"
    ) {
        return true;
    }
    let mut g = inner.lock();
    let Some(c) = g.clients.get_mut(&id) else {
        return false;
    };
    let now = Instant::now();
    let elapsed = now.duration_since(c.flood_last).as_secs_f32();
    c.flood_last = now;
    c.flood_tokens = (c.flood_tokens + elapsed * FLOOD_REFILL_PER_SEC).min(FLOOD_CAPACITY);
    if c.flood_tokens < 1.0 {
        return false;
    }
    c.flood_tokens -= 1.0;
    true
}
