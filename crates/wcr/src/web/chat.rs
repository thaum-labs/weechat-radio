//! SPDX-License-Identifier: Apache-2.0
//! Live Chat for the web page. `wcr web` is one more IRC client on the
//! station's own loopback IRC server, exactly as the desktop app is: a line
//! from the browser goes out as the same `PRIVMSG` the desktop app sends,
//! and the station's delivery marks come back to this connection because it
//! sent the line. The marks are applied with the desktop app's own code
//! ([`crate::chat_line`]), so they mean the same thing in both.
//!
//! Open pages follow along over Server-Sent Events: a snapshot on connect,
//! then each new or changed line.

use crate::chat_line::{self, ChatLine};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};

/// Lines kept per conversation for the page.
const LINES_PER_CONV: usize = 200;
/// Conversations kept; the quietest goes first.
const MAX_CONVS: usize = 40;
const BULLETIN: &str = "#bulletin";

/// One line as the page draws it. `seq` is stable, so a later mark on the
/// same line replaces it in place.
#[derive(Clone, Debug, Serialize)]
pub struct LineView {
    pub seq: u64,
    pub conv: String,
    pub nick: String,
    pub text: String,
    pub sys: bool,
    pub mine: bool,
    pub ticks: String,
    pub tries: u32,
    pub via: String,
    pub when: String,
    pub snr: Option<f32>,
}

struct Conv {
    id: String,
    lines: VecDeque<ChatLine>,
    /// The page's id for each line in `lines`.
    seqs: VecDeque<u64>,
}

impl Conv {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            lines: VecDeque::new(),
            seqs: VecDeque::new(),
        }
    }

    fn views(&self, me: &str) -> Vec<LineView> {
        self.lines
            .iter()
            .zip(&self.seqs)
            .map(|(l, &seq)| view(me, &self.id, seq, l))
            .collect()
    }
}

struct State {
    me: String,
    /// True while connected to the station's IRC server.
    up: bool,
    next_seq: u64,
    /// Most recently active first.
    convs: Vec<Conv>,
    /// Channels this page has joined; kept across restarts.
    joined: Vec<String>,
}

pub struct Chat {
    state: Mutex<State>,
    events: broadcast::Sender<Value>,
    out: mpsc::UnboundedSender<String>,
    joined_path: PathBuf,
}

impl Chat {
    /// Start the IRC client. It keeps trying while the station is down.
    pub fn start(irc_addr: String, me: String, joined_path: PathBuf) -> Arc<Self> {
        let (out, out_rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(256);
        let joined = load_joined(&joined_path);
        let chat = Arc::new(Self {
            state: Mutex::new(State {
                me: me.to_ascii_uppercase(),
                up: false,
                next_seq: 1,
                convs: joined.iter().map(|id| Conv::new(id)).collect(),
                joined,
            }),
            events,
            out,
            joined_path,
        });
        tokio::spawn(chat.clone().run(irc_addr, out_rx));
        chat
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    /// Everything the page needs to draw from scratch.
    pub fn snapshot(&self) -> Value {
        let st = self.state.lock();
        let convs: Vec<Value> = st
            .convs
            .iter()
            .map(|c| json!({ "id": c.id, "lines": c.views(&st.me) }))
            .collect();
        json!({ "type": "hello", "me": st.me, "up": st.up, "convs": convs })
    }

    /// Change the station's mode, preset or levels with the same commands the
    /// desktop app sends (`/mode X`, `/preset X`, #226; `/level rx|tx X`,
    /// #241). Nothing else is accepted, so the page cannot run other commands.
    pub fn radio_setting(&self, what: &str, value: &str) -> Result<(), String> {
        let cmd = match what {
            "mode" => {
                let mode: crate::modes::Mode = value.parse()?;
                // The page asks first, as the desktop app does.
                if mode == crate::modes::Mode::Radio {
                    "/mode radio confirm".to_string()
                } else {
                    format!("/mode {}", mode.as_str())
                }
            }
            "preset" => {
                let p = crate::presets::Preset::parse(value)
                    .ok_or_else(|| format!("unknown preset {value}"))?;
                format!("/preset {}", p.as_str())
            }
            // The level sliders (#241): the same /level the desktop app sends.
            "rx_gain" | "tx_level" => {
                let db: f32 = value
                    .trim()
                    .parse()
                    .ok()
                    .filter(|d: &f32| d.is_finite())
                    .ok_or_else(|| format!("{value} is not a number of dB"))?;
                let knob = if what == "rx_gain" { "rx" } else { "tx" };
                format!("/level {knob} {db:.1}")
            }
            _ => return Err("only mode, preset and the levels can be changed here".into()),
        };
        if !self.state.lock().up {
            return Err("the station is not running".into());
        }
        for wire in crate::slash::to_wire(&cmd, "#bulletin") {
            let _ = self.out.send(wire);
        }
        Ok(())
    }

    /// Send what was typed into `to` (`#channel` or a callsign), through the
    /// same steps as the desktop app's compose box.
    pub fn send(&self, to: &str, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("nothing to send".into());
        }
        let to = conv_id(to);
        if to.is_empty() {
            return Err("need a channel name or callsign".into());
        }
        if text.starts_with('/') {
            return Err("slash commands are for the desktop app".into());
        }
        if let Some(err) = crate::slash::line_error(text, &to) {
            return Err(err);
        }
        if !self.state.lock().up {
            return Err("the station is not running".into());
        }
        let chunks = crate::proto::split_body_chunks(text, crate::proto::MAX_BODY);
        for chunk in chunks {
            let wires = crate::slash::to_wire(&chunk, &to);
            if wires.is_empty() {
                return Err("need a channel name or callsign".into());
            }
            let me = self.state.lock().me.clone();
            let mut line = ChatLine::user(me, chunk.clone(), to.clone());
            line.ticks = crate::store::Delivery::Queued.ticks_bracket().into();
            self.push(&to, line);
            for wire in wires {
                let _ = self.out.send(wire);
            }
        }
        Ok(())
    }

    /// Open a channel: join it on the station (its history follows).
    pub fn join(&self, channel: &str) -> Result<String, String> {
        let ch = crate::slash::normalize_channel(channel);
        if let Some(err) = crate::slash::channel_limit_error(&ch) {
            return Err(err);
        }
        if ch.len() < 3 {
            return Err("channel name is too short".into());
        }
        self.remember_join(&ch);
        let _ = self.out.send(format!("JOIN {ch}"));
        Ok(ch)
    }

    fn remember_join(&self, ch: &str) {
        let mut st = self.state.lock();
        if !st.joined.iter().any(|c| c.eq_ignore_ascii_case(ch)) {
            st.joined.push(ch.to_string());
            save_joined(&self.joined_path, &st.joined);
        }
        conv_mut(&mut st, ch);
    }

    fn set_up(&self, up: bool) {
        let me = {
            let mut st = self.state.lock();
            if st.up == up {
                return;
            }
            st.up = up;
            st.me.clone()
        };
        let _ = self
            .events
            .send(json!({ "type": "link", "up": up, "me": me }));
    }

    /// Add a line and tell the pages.
    fn push(&self, conv: &str, line: ChatLine) {
        let v = {
            let mut st = self.state.lock();
            // History replays on every reconnect; a line seen already is skipped.
            if !line.msgid.is_empty() {
                let dup = st.convs.iter().any(|c| {
                    c.id.eq_ignore_ascii_case(conv)
                        && c.lines
                            .iter()
                            .any(|l| l.msgid.eq_ignore_ascii_case(&line.msgid))
                });
                if dup {
                    return;
                }
            }
            let seq = st.next_seq;
            st.next_seq += 1;
            let me = st.me.clone();
            let c = conv_mut(&mut st, conv);
            if c.lines.len() >= LINES_PER_CONV {
                c.lines.pop_front();
                c.seqs.pop_front();
            }
            let v = view(&me, &c.id, seq, &line);
            c.lines.push_back(line);
            c.seqs.push_back(seq);
            v
        };
        let _ = self.events.send(json!({ "type": "line", "line": v }));
    }

    /// A delivery mark from the station, applied as the desktop app does.
    fn mark(&self, msgid: &str, state: &str, tries: u32, via: Option<&str>) {
        let changed: Vec<LineView> = {
            let mut st = self.state.lock();
            let me = st.me.clone();
            let mut out = Vec::new();
            // The mark belongs to one of our own lines. Look for its id
            // first, then let the shared code stamp the newest unmarked one.
            let pos = st.convs.iter().position(|c| {
                !msgid.is_empty() && c.lines.iter().any(|l| l.msgid.eq_ignore_ascii_case(msgid))
            });
            if let Some(i) = pos.or_else(|| newest_unmarked(&st.convs, &me)) {
                let conv = &mut st.convs[i];
                let before: Vec<_> = conv.lines.iter().map(marks).collect();
                chat_line::apply_delivery_to(
                    conv.lines.make_contiguous(),
                    &me,
                    msgid,
                    state,
                    tries,
                );
                if let Some(via) = via {
                    chat_line::apply_receipt_via(conv.lines.make_contiguous(), msgid, via);
                }
                for (j, was) in before.into_iter().enumerate() {
                    if marks(&conv.lines[j]) != was {
                        out.push(view(&me, &conv.id, conv.seqs[j], &conv.lines[j]));
                    }
                }
            }
            out
        };
        for v in changed {
            let _ = self.events.send(json!({ "type": "line", "line": v }));
        }
    }

    /// The station replays a channel's history after JOIN: start it clean.
    fn joined(&self, ch: &str) {
        let ch = crate::slash::normalize_channel(ch);
        {
            let mut st = self.state.lock();
            // As the desktop app does: the replay that follows is the
            // channel's history, our own lines included.
            let c = conv_mut(&mut st, &ch);
            c.lines.clear();
            c.seqs.clear();
        }
        let _ = self.events.send(json!({ "type": "reset", "conv": ch }));
    }

    fn handle(&self, raw: &str) {
        let me = self.state.lock().me.clone();
        if let Some((msgid, state, tries)) = chat_line::parse_delivery(raw) {
            let via = chat_line::delivery_via(raw);
            self.mark(&msgid, &state, tries, via.as_deref());
        } else if let Some((from, ch)) = chat_line::parse_invite(raw) {
            let ch = crate::slash::normalize_channel(&ch);
            if !crate::slash::is_bulletin(&ch) && ch.len() >= 3 {
                self.remember_join(&ch);
                let _ = self.out.send(format!("JOIN {ch}"));
                self.push(
                    &ch,
                    ChatLine::sys(format!("{from} invited you to {ch}"), &ch),
                );
            }
        } else if let Some((nick, ch)) = chat_line::parse_part(raw) {
            if !nick.eq_ignore_ascii_case(&me) {
                self.push(&ch, ChatLine::sys(format!("{nick} left {ch}"), &ch));
            }
        } else if let Some(ch) = chat_line::parse_join(raw) {
            self.joined(&ch);
        } else if let Some(line) = chat_line::parse_privmsg(raw) {
            let conv = match dm_target(raw) {
                // A direct line: the conversation is the other station.
                Some(to) if line.nick.eq_ignore_ascii_case(&me) => to,
                Some(_) => line.nick.to_ascii_uppercase(),
                None => line.channel.clone(),
            };
            self.push(&conv, line);
        } else if let Some(note) = chat_line::parse_notice(raw) {
            let _ = self.events.send(json!({ "type": "notice", "text": note }));
        }
    }

    async fn run(self: Arc<Self>, addr: String, mut out_rx: mpsc::UnboundedReceiver<String>) {
        loop {
            match TcpStream::connect(&addr).await {
                Ok(stream) => {
                    self.session(stream, &mut out_rx).await;
                    self.set_up(false);
                }
                Err(_) => {
                    // Lines typed while the station is down are not kept.
                    while out_rx.try_recv().is_ok() {}
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn session(&self, stream: TcpStream, out_rx: &mut mpsc::UnboundedReceiver<String>) {
        let _ = stream.set_nodelay(true);
        let (read, mut write) = stream.into_split();
        let (me, joined) = {
            let st = self.state.lock();
            (st.me.clone(), st.joined.clone())
        };
        let mut hello = format!(
            "CAP LS\r\nNICK {me}\r\nUSER {me} 0 * :WeeChat Radio web\r\n\
             CAP REQ :message-tags server-time msgid\r\nCAP END\r\nJOIN {BULLETIN}\r\n"
        );
        for ch in joined.iter().filter(|c| !crate::slash::is_bulletin(c)) {
            hello.push_str(&format!("JOIN {ch}\r\n"));
        }
        if write.write_all(hello.as_bytes()).await.is_err() {
            return;
        }
        self.set_up(true);
        let mut lines = BufReader::new(read).lines();
        loop {
            tokio::select! {
                next = lines.next_line() => match next {
                    Ok(Some(raw)) => {
                        let raw = raw.trim_end();
                        if let Some(token) = raw.strip_prefix("PING ") {
                            let pong = format!("PONG {}\r\n", token.trim());
                            if write.write_all(pong.as_bytes()).await.is_err() {
                                return;
                            }
                        } else if !raw.contains(" CAP ") {
                            self.handle(raw);
                        }
                    }
                    _ => return,
                },
                Some(msg) = out_rx.recv() => {
                    if !chat_line::irc_command(&msg) {
                        continue;
                    }
                    let line = format!("{}\r\n", msg.trim_end());
                    if write.write_all(line.as_bytes()).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// The callsign a `PRIVMSG` is addressed to, when it is a direct line and
/// not a channel. (The shared parser files both under a channel name.)
fn dm_target(raw: &str) -> Option<String> {
    let rest = raw.split_once(" PRIVMSG ")?.1;
    let target = rest.split_whitespace().next()?;
    if target.starts_with('#') || target.starts_with('&') {
        return None;
    }
    crate::proto::Callsign::parse(target)
        .ok()
        .map(|_| target.to_ascii_uppercase())
}

/// What a mark can change on a line.
fn marks(l: &ChatLine) -> (String, String, u32, String) {
    (l.msgid.clone(), l.ticks.clone(), l.tries, l.via.clone())
}

fn view(me: &str, conv: &str, seq: u64, l: &ChatLine) -> LineView {
    LineView {
        seq,
        conv: conv.to_string(),
        nick: l.nick.clone(),
        text: l.text.clone(),
        sys: l.sys,
        mine: !l.sys && l.nick.eq_ignore_ascii_case(me),
        ticks: l.ticks.clone(),
        tries: l.tries,
        via: l.via.clone(),
        when: l.when.clone(),
        snr: l.snr,
    }
}

/// The conversation holding our newest line that has no id yet.
fn newest_unmarked(convs: &[Conv], me: &str) -> Option<usize> {
    convs
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            c.lines
                .iter()
                .zip(&c.seqs)
                .filter(|(l, _)| !l.sys && l.msgid.is_empty() && l.nick.eq_ignore_ascii_case(me))
                .map(|(_, &seq)| (seq, i))
                .next_back()
        })
        .max()
        .map(|(_, i)| i)
}

/// Find or open a conversation; it moves to the top.
fn conv_mut<'a>(st: &'a mut State, id: &str) -> &'a mut Conv {
    let pos = st.convs.iter().position(|c| c.id.eq_ignore_ascii_case(id));
    let conv = match pos {
        Some(i) => st.convs.remove(i),
        None => Conv::new(id),
    };
    st.convs.insert(0, conv);
    st.convs.truncate(MAX_CONVS);
    &mut st.convs[0]
}

/// `#Net` → `#net`, `m7abc` → `M7ABC`.
pub fn conv_id(raw: &str) -> String {
    let t = raw.trim();
    if t.starts_with('#') || t.starts_with('&') {
        crate::slash::normalize_channel(t)
    } else {
        t.to_ascii_uppercase()
    }
}

fn load_joined(path: &PathBuf) -> Vec<String> {
    let mut joined: Vec<String> = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["joined"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(crate::slash::normalize_channel))
        .collect();
    if !joined.iter().any(|c| crate::slash::is_bulletin(c)) {
        joined.insert(0, BULLETIN.into());
    }
    joined
}

fn save_joined(path: &PathBuf, joined: &[String]) {
    let _ = std::fs::write(path, json!({ "joined": joined }).to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat() -> Arc<Chat> {
        let (out, _rx) = mpsc::unbounded_channel();
        let (events, _) = broadcast::channel(16);
        Arc::new(Chat {
            state: Mutex::new(State {
                me: "M7TJF".into(),
                up: true,
                next_seq: 1,
                convs: Vec::new(),
                joined: vec![BULLETIN.into()],
            }),
            events,
            out,
            joined_path: std::env::temp_dir().join("wcr-web-chat-test.json"),
        })
    }

    fn lines(c: &Chat, conv: &str) -> Vec<LineView> {
        let snap = c.snapshot();
        let conv = snap["convs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == conv)
            .cloned()
            .unwrap();
        serde_json::from_value::<Vec<Value>>(conv["lines"].clone())
            .unwrap()
            .into_iter()
            .map(|v| LineView {
                seq: v["seq"].as_u64().unwrap(),
                conv: v["conv"].as_str().unwrap().into(),
                nick: v["nick"].as_str().unwrap().into(),
                text: v["text"].as_str().unwrap().into(),
                sys: v["sys"].as_bool().unwrap(),
                mine: v["mine"].as_bool().unwrap(),
                ticks: v["ticks"].as_str().unwrap().into(),
                tries: v["tries"].as_u64().unwrap() as u32,
                via: v["via"].as_str().unwrap().into(),
                when: v["when"].as_str().unwrap().into(),
                snr: v["snr"].as_f64().map(|f| f as f32),
            })
            .collect()
    }

    #[test]
    fn a_sent_line_takes_the_stations_marks() {
        let c = chat();
        c.send("#bulletin", "hello").unwrap();
        assert_eq!(lines(&c, "#bulletin")[0].ticks, "[..]");
        c.handle("@msgid=ab12;+radio/delivery=sent;+radio/msgid=ab12 TAGMSG #bulletin");
        let l = &lines(&c, "#bulletin")[0];
        assert_eq!(l.ticks, "[tx]");
        assert!(l.mine);
        c.handle("@msgid=ab12;+radio/delivery=delivered;+radio/msgid=ab12 TAGMSG #bulletin");
        assert_eq!(lines(&c, "#bulletin")[0].ticks, "[ok]");
        // A late, lower mark never walks it back.
        c.handle("@msgid=ab12;+radio/delivery=sent;+radio/msgid=ab12 TAGMSG #bulletin");
        assert_eq!(lines(&c, "#bulletin")[0].ticks, "[ok]");
    }

    #[test]
    fn incoming_lines_land_in_their_conversation_once() {
        let c = chat();
        let dm = "@server-time=2026-10-31T22:00:00.000Z;msgid=x1;+radio/via=rf;+radio/snr=-4.0 :G4ABC PRIVMSG M7TJF :hi there";
        c.handle(dm);
        c.handle(dm);
        let l = lines(&c, "G4ABC");
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].via, "rf");
        assert!(!l[0].mine);
        c.handle("@msgid=x2 :G4ABC PRIVMSG #bulletin :cq");
        assert_eq!(lines(&c, "#bulletin")[0].text, "cq");
    }

    #[test]
    fn sends_are_checked_before_they_go() {
        let c = chat();
        assert!(c.send("#bulletin", "  ").is_err());
        assert!(c.send("", "hi").is_err());
        assert!(c.send("#bulletin", "/part").is_err());
        c.state.lock().up = false;
        assert!(c.send("#bulletin", "hi").is_err());
    }

    #[test]
    fn conversation_ids_are_normalised() {
        assert_eq!(conv_id("m7abc"), "M7ABC");
        assert_eq!(conv_id("#bulletin"), "#bulletin");
    }
}
