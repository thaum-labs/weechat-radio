//! SPDX-License-Identifier: Apache-2.0
//! Embedded localhost IRC server for WeeChat and the built-in TUI.

use crate::error::Result;
use chrono::Utc;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
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
}

#[derive(Clone)]
pub struct IrcServer {
    inner: Arc<Mutex<Inner>>,
    events: mpsc::Sender<IrcEvent>,
}

struct Inner {
    next_id: u64,
    clients: HashMap<u64, Client>,
    server_name: String,
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
            })),
            events,
        }
    }

    pub async fn listen(self, bind: &str) -> Result<()> {
        let listener = TcpListener::bind(bind).await?;
        tracing::info!("IRC listening on {bind}");
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
        let mut lines = BufReader::new(read).lines();
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
        while let Ok(Some(line)) = lines.next_line().await {
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
        match cmd.as_str() {
            "CAP" => self.cap(id, &parts).await,
            "NICK" => {
                if let Some(n) = parts.get(1) {
                    let nick = n.trim_start_matches(':').to_string();
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
                        let ch = ch.to_string();
                        {
                            let mut g = self.inner.lock();
                            if let Some(c) = g.clients.get_mut(&id) {
                                c.channels.insert(ch.clone());
                            }
                        }
                        let nick = self.nick(id);
                        self.send_raw(id, &format!(":{nick} JOIN {ch}")).await;
                        self.send_numeric(id, 332, &format!("{ch} :WeeChat Radio"))
                            .await;
                        self.send_numeric(id, 353, &format!("= {ch} :{nick}")).await;
                        self.send_numeric(id, 366, &format!("{ch} :End of /NAMES"))
                            .await;
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
            "PRIVMSG" => {
                if parts.len() >= 3 {
                    let target = parts[1].clone();
                    let text = trailing(&parts[2..]);
                    let nick = self.nick(id);
                    if self.has_cap(id, "echo-message") {
                        let tagged = self.tag_privmsg(&nick, &target, &text, None);
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
            "WHO" | "MODE" | "WHOIS" | "NAMES" => {}
            other => {
                tracing::debug!("IRC unhandled {other}");
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
        self.send_numeric(id, 4, &format!("{srv} 0.1.0 o o")).await;
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
        let srv = self.inner.lock().server_name.clone();
        self.send_raw(id, &format!(":{srv} {num:03} {nick} {rest}"))
            .await;
    }

    pub fn tag_privmsg(&self, from: &str, target: &str, text: &str, msgid: Option<&str>) -> String {
        let time = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut tags = format!("server-time={time}");
        if let Some(id) = msgid {
            tags.push_str(&format!(";msgid={id}"));
        }
        format!("@{tags} :{from} PRIVMSG {target} :{text}")
    }

    pub async fn broadcast_privmsg(
        &self,
        from: &str,
        target: &str,
        text: &str,
        msgid: Option<&str>,
    ) {
        let line = self.tag_privmsg(from, target, text, msgid);
        let plain = format!(":{from} PRIVMSG {target} :{text}");
        let clients: Vec<(u64, bool, bool)> = {
            let g = self.inner.lock();
            g.clients
                .values()
                .map(|c| {
                    let in_chan = target.starts_with('#')
                        || target.starts_with('&')
                        || c.channels.contains(target)
                        || c.nick.eq_ignore_ascii_case(target)
                        || c.nick.eq_ignore_ascii_case(from);
                    (
                        c.id,
                        in_chan || !target.starts_with('#') && !target.starts_with('&'),
                        c.caps.contains("message-tags"),
                    )
                })
                .collect()
        };
        for (id, include, tags) in clients {
            if include {
                self.send_raw(id, if tags { &line } else { &plain }).await;
            }
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

    pub async fn tagmsg_delivery(&self, msgid: &str, state: &str) {
        let line = format!("@+radio/delivery={state};+radio/msgid={msgid} TAGMSG *");
        let ids: Vec<(u64, bool)> = {
            let g = self.inner.lock();
            g.clients
                .values()
                .map(|c| (c.id, c.caps.contains("message-tags")))
                .collect()
        };
        for (id, tags) in ids {
            if tags {
                self.send_raw(id, &line).await;
            } else {
                self.send_raw(id, &format!(":wcr.local NOTICE * :[{state}] {msgid}"))
                    .await;
            }
        }
    }

    pub async fn replay_history(&self, id: u64, lines: Vec<(String, String, String, String)>) {
        // (time, from, target, text)
        let has_batch = self.has_cap(id, "batch");
        if has_batch {
            self.send_raw(id, ":wcr.local BATCH +hist chathistory")
                .await;
        }
        for (time, from, target, text) in lines {
            let tagged = format!("@server-time={time};batch=hist :{from} PRIVMSG {target} :{text}");
            self.send_raw(id, &tagged).await;
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

    pub async fn send_radio_reply(&self, id: u64, text: &str) {
        let nick = self.nick(id);
        for line in text.lines() {
            self.send_raw(id, &format!(":wcr.local NOTICE {nick} :{line}"))
                .await;
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
