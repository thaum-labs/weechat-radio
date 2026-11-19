//! SPDX-License-Identifier: Apache-2.0
//! Chat lines as an IRC client of the station sees them, and the delivery
//! marks on them. Shared by the desktop app and `wcr web`, so a line's marks
//! mean the same thing in both.

use std::collections::HashMap;

pub(crate) struct ChatLine {
    pub(crate) nick: String,
    pub(crate) text: String,
    pub(crate) sys: bool,
    pub(crate) channel: String,
    pub(crate) msgid: String,
    pub(crate) ticks: String,
    pub(crate) via: String,
    pub(crate) tries: u32,
    pub(crate) when: String,
    /// SNR in dB a radio line was decoded at; `None` off the air.
    pub(crate) snr: Option<f32>,
    /// File transfer bar, such as `[████░░░░] 3/15`; empty for other lines.
    pub(crate) progress: String,
    /// This line, as parsed, is piece `i` of `n` of a long message (#365).
    /// [`push_grouped`] folds it into the message's one entry.
    pub(crate) piece: Option<(usize, usize)>,
    /// A long message received as pieces: one entry, updated in place.
    pub(crate) parts: Option<crate::chat_parts::Parts>,
    /// A long message we send as pieces: pieces on the air of all.
    pub(crate) sending: Option<(usize, usize)>,
    /// Ids of the later pieces folded into this entry (the first piece's id
    /// is `msgid`), so a history replay does not add them again.
    pub(crate) piece_ids: Vec<String>,
}

impl ChatLine {
    pub(crate) fn sys(text: impl Into<String>, channel: impl Into<String>) -> Self {
        Self {
            nick: String::new(),
            text: text.into(),
            sys: true,
            channel: channel.into(),
            msgid: String::new(),
            ticks: String::new(),
            via: String::new(),
            tries: 0,
            when: String::new(),
            snr: None,
            progress: String::new(),
            piece: None,
            parts: None,
            sending: None,
            piece_ids: Vec::new(),
        }
    }

    pub(crate) fn user(
        nick: impl Into<String>,
        text: impl Into<String>,
        channel: impl Into<String>,
    ) -> Self {
        Self {
            nick: nick.into(),
            text: text.into(),
            sys: false,
            channel: channel.into(),
            msgid: String::new(),
            ticks: String::new(),
            via: String::new(),
            tries: 0,
            when: crate::store::chat_stamp(None),
            snr: None,
            progress: String::new(),
            piece: None,
            parts: None,
            sending: None,
            piece_ids: Vec::new(),
        }
    }
}

pub(crate) fn irc_tags(line: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(rest) = line.strip_prefix('@') else {
        return map;
    };
    let tagstr = if let Some(i) = rest.find(" TAGMSG") {
        &rest[..i]
    } else if let Some(i) = rest.find(" :") {
        &rest[..i]
    } else if let Some((t, _)) = rest.split_once(' ') {
        t
    } else {
        return map;
    };
    for t in tagstr.split(';') {
        let (raw_k, v) = t.split_once('=').unwrap_or((t, ""));
        let k = raw_k.trim_start_matches('+');
        map.insert(k.to_string(), v.to_string());
    }
    map
}

pub(crate) fn apply_delivery_to(
    chat: &mut [ChatLine],
    me: &str,
    msgid: &str,
    state: &str,
    tries: u32,
) {
    let Some(ticks) = delivery_ticks(state) else {
        return;
    };
    let next = crate::store::Delivery::parse(state);
    if !msgid.is_empty() {
        if let Some(line) = chat
            .iter_mut()
            .rev()
            .find(|l| !l.msgid.is_empty() && l.msgid.eq_ignore_ascii_case(msgid))
        {
            apply_line_delivery(line, ticks, next, tries, state);
            return;
        }
    }
    // Two sends in flight both start with an empty id. Stamp the latest
    // unmarked line, and skip a line that already carries a different id so
    // the two receipts cannot collapse onto one message.
    if let Some(line) = chat.iter_mut().rev().find(|l| {
        !l.sys
            && l.nick.eq_ignore_ascii_case(me)
            && (l.msgid.is_empty() || (!msgid.is_empty() && l.msgid.eq_ignore_ascii_case(msgid)))
    }) {
        if line.msgid.is_empty() && !msgid.is_empty() {
            line.msgid = msgid.to_string();
        }
        apply_line_delivery(line, ticks, next, tries, state);
    }
}

pub(crate) fn apply_line_delivery(
    line: &mut ChatLine,
    ticks: &str,
    next: crate::store::Delivery,
    tries: u32,
    state: &str,
) {
    if state == "relayed" {
        if line.ticks.contains("[no]") || line.ticks.contains("[rl]") {
            return;
        }
        if line.ticks.contains("[ok]")
            || line.ticks.contains("[all]")
            || line.ticks.contains("[gw]")
        {
            line.ticks = format!("[rl] {}", line.ticks);
            return;
        }
    }
    if state == "failed" {
        if line.ticks.contains("[all]") {
            return;
        }
        line.ticks = "[no]".into();
        return;
    }
    if state == "waiting" {
        if line.ticks.contains("[all]") {
            return;
        }
        line.ticks = "waiting on acks".into();
        return;
    }
    // "retry-*" parses as Queued. That rank cannot advance a line already on
    // [tx] or [rl], so the try count never moved and a keyed retry stayed [x1].
    if state.starts_with("retry") {
        if matches!(line.ticks.as_str(), "[ok]" | "[all]" | "[no]") {
            return;
        }
        if tries > 0 {
            line.tries = tries;
        }
        if line.ticks.is_empty() || line.ticks == "[..]" {
            line.ticks = "[tx]".into();
        }
        return;
    }
    if let Some(cur) = ticks_delivery(&line.ticks) {
        if !cur.can_advance(next) {
            return;
        }
    }
    let had_rl = line.ticks.contains("[rl]");
    line.ticks = if had_rl && ticks != "[rl]" {
        format!("[rl] {ticks}")
    } else {
        ticks.into()
    };
    if tries > 0 {
        line.tries = tries;
    }
}

pub(crate) fn ticks_delivery(ticks: &str) -> Option<crate::store::Delivery> {
    if ticks.contains("[all]") {
        return Some(crate::store::Delivery::All);
    }
    if ticks.contains("[ok]") {
        return Some(crate::store::Delivery::Delivered);
    }
    if ticks.contains("[no]") {
        return Some(crate::store::Delivery::Failed);
    }
    if ticks.contains("[gw]") {
        return Some(crate::store::Delivery::Gateway);
    }
    if ticks.contains("[rl]") {
        return Some(crate::store::Delivery::Relayed);
    }
    if ticks.contains("[tx]") {
        return Some(crate::store::Delivery::Sent);
    }
    if ticks.contains("[..]") {
        return Some(crate::store::Delivery::Queued);
    }
    None
}

pub(crate) fn delivery_ticks(state: &str) -> Option<&'static str> {
    let d = crate::store::Delivery::parse(state);
    match state {
        "queued" | "sent" | "relayed" | "gateway" | "delivered" | "all" => Some(d.ticks_bracket()),
        "failed" => Some("[no]"),
        "waiting" => Some("waiting on acks"),
        s if s.starts_with("retry") => Some(crate::store::Delivery::Queued.ticks_bracket()),
        _ => None,
    }
}

/// How the receipt behind an `[ok]` reached the station (`rf`, `inet`,
/// `lan`), from the mark's `+radio/via` tag (#226).
pub(crate) fn delivery_via(line: &str) -> Option<String> {
    irc_tags(line)
        .get("radio/via")
        .filter(|v| !v.is_empty())
        .cloned()
}

/// Note on our own line `msgid` how its first receipt came back. The first
/// receipt wins: a later one by another path does not change it.
pub(crate) fn apply_receipt_via(chat: &mut [ChatLine], msgid: &str, via: &str) {
    if msgid.is_empty() || via.is_empty() {
        return;
    }
    if let Some(line) = chat
        .iter_mut()
        .rev()
        .find(|l| !l.sys && l.msgid.eq_ignore_ascii_case(msgid))
    {
        if line.via.is_empty() {
            line.via = via.to_string();
        }
    }
}

pub(crate) fn parse_tries(raw: Option<&str>) -> u32 {
    raw.and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// A "📎 name (size) saved to PATH" line: the text before the path, and
/// the path, so a UI can make the path a link.
pub(crate) fn saved_file(text: &str) -> Option<(&str, &str)> {
    if !text.starts_with("📎 ") {
        return None;
    }
    let i = text.rfind(" saved to ")?;
    let path = text[i + " saved to ".len()..].trim();
    (!path.is_empty()).then_some((&text[..i + " saved to ".len()], path))
}

/// A file progress TAGMSG (`+radio/file=have/count`): the chat line's id
/// and the bar to show on it.
pub(crate) fn parse_file_progress(line: &str) -> Option<(String, String)> {
    if !line.contains(" TAGMSG ") {
        return None;
    }
    let tags = irc_tags(line);
    let bar = crate::proto::file::progress_bar_from_tag(tags.get("radio/file")?)?;
    let msgid = tags
        .get("radio/msgid")
        .or_else(|| tags.get("msgid"))
        .cloned()
        .unwrap_or_default();
    (!msgid.is_empty()).then_some((msgid, bar))
}

/// Put a file progress bar on the line with `msgid`. True when one changed.
pub(crate) fn apply_file_progress(lines: &mut [ChatLine], msgid: &str, bar: &str) -> bool {
    let mut changed = false;
    for l in lines
        .iter_mut()
        .filter(|l| l.msgid.eq_ignore_ascii_case(msgid))
    {
        if l.progress != bar {
            l.progress = bar.to_string();
            changed = true;
        }
    }
    changed
}

/// Folds a parsed piece of a long message into its message's entry in
/// `lines` (updated in place, so other lines never land inside it) and
/// returns that entry's index. A first piece, or a piece with no open
/// message, becomes the entry itself: `line` is turned into it and None is
/// returned for the caller to add. Ordinary lines return None unchanged.
pub(crate) fn group_into(lines: &mut [ChatLine], line: &mut ChatLine) -> Option<usize> {
    let (i, n) = line.piece.take()?;
    let now = crate::chat_parts::unix_now();
    let found = crate::chat_parts::find_group(
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.channel.eq_ignore_ascii_case(&line.channel))
            .map(|(k, l)| (k, l.nick.as_str(), l.parts.as_ref())),
        &line.nick,
        i,
        n,
    );
    if let Some(k) = found {
        let entry = &mut lines[k];
        if let Some(p) = entry.parts.as_mut() {
            p.add(i, n, &line.text, now);
            entry.text = p.text();
        }
        if entry.snr.is_none() {
            entry.snr = line.snr;
        }
        if !line.msgid.is_empty() {
            entry.piece_ids.push(line.msgid.clone());
        }
        return Some(k);
    }
    let mut parts = crate::chat_parts::Parts::new(n, now);
    parts.add(i, n, &line.text, now);
    line.text = parts.text();
    line.parts = Some(parts);
    None
}

/// Adds a parsed line to `chat`, a piece of a long message to its
/// message's one entry. Returns the index of the entry.
pub(crate) fn push_grouped(chat: &mut Vec<ChatLine>, mut line: ChatLine) -> usize {
    if let Some(k) = group_into(chat, &mut line) {
        return k;
    }
    chat.push(line);
    chat.len() - 1
}

/// True when `msgid` is this line's, or one of its later pieces'.
pub(crate) fn has_msgid(l: &ChatLine, msgid: &str) -> bool {
    l.msgid.eq_ignore_ascii_case(msgid) || l.piece_ids.iter().any(|p| p.eq_ignore_ascii_case(msgid))
}

/// How a grouped line's footer reads, for its colour: amber while pieces
/// are coming or going out, green when done, red when a piece is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FooterTone {
    InProgress,
    Done,
    Missing,
}

/// The badge (`1/3`) and footer line under a long message, received or
/// sent as pieces (#365). None for an ordinary line.
pub(crate) fn group_footer(line: &ChatLine, now: u64) -> Option<(String, String, FooterTone)> {
    use crate::chat_parts::Status;
    if let Some(p) = &line.parts {
        let tone = match p.status(now) {
            Status::Arriving => FooterTone::InProgress,
            Status::Complete => FooterTone::Done,
            Status::Incomplete => FooterTone::Missing,
        };
        let badge = format!("{}/{}", p.received(), p.total());
        return Some((badge, p.footer(now), tone));
    }
    let (k, n) = line.sending?;
    let delivered = crate::chat_line::ticks_delivery(&line.ticks)
        .is_some_and(|d| d == crate::store::Delivery::Delivered);
    let failed = line.ticks.contains("[no]");
    let tone = if failed {
        FooterTone::Missing
    } else if k >= n && delivered {
        FooterTone::Done
    } else {
        FooterTone::InProgress
    };
    let tail = if failed {
        "  ⚠ not all pieces delivered"
    } else if delivered {
        "  delivered"
    } else {
        "  … awaiting receipt"
    };
    Some((
        format!("{k}/{n}"),
        format!("{}{tail}", crate::chat_parts::sending_line(k, n)),
        tone,
    ))
}

/// `+radio/parts=k/n` on a delivery mark: k of the n pieces of our long
/// message are on the air (#365).
pub(crate) fn delivery_parts(line: &str) -> Option<(usize, usize)> {
    irc_tags(line)
        .get("radio/parts")
        .and_then(|v| crate::chat_parts::parse_progress(v))
}

/// Notes the pieces-on-air count on our long message `msgid`.
pub(crate) fn apply_sending(chat: &mut [ChatLine], msgid: &str, parts: (usize, usize)) {
    if let Some(line) = chat
        .iter_mut()
        .rev()
        .find(|l| !l.sys && !l.msgid.is_empty() && l.msgid.eq_ignore_ascii_case(msgid))
    {
        line.sending = Some(parts);
    }
}

pub(crate) fn parse_delivery(line: &str) -> Option<(String, String, u32)> {
    let payload = irc_payload(line);
    let cmd = if let Some(rest) = payload.strip_prefix(':') {
        rest.split_once(' ').map(|(_, c)| c).unwrap_or(rest)
    } else {
        payload
    };
    if cmd.starts_with("TAGMSG ") || line.contains(" TAGMSG ") {
        let tags = irc_tags(line);
        if let Some(state) = tags.get("radio/delivery") {
            let msgid = tags
                .get("radio/msgid")
                .or_else(|| tags.get("msgid"))
                .cloned()
                .unwrap_or_default();
            return Some((
                msgid,
                state.clone(),
                parse_tries(tags.get("radio/tries").map(String::as_str)),
            ));
        }
        return None;
    }
    let notice = parse_notice(line)?;
    let rest = notice.strip_prefix('[')?;
    let (inside, msgid) = rest.split_once("] ")?;
    let (state, tries) = if let Some((s, n)) = inside.rsplit_once(" x") {
        (s.to_string(), n.parse().unwrap_or(0))
    } else {
        (inside.to_string(), 0)
    };
    if delivery_ticks(&state).is_none() && !state.starts_with("retry") {
        return None;
    }
    Some((msgid.trim().to_string(), state, tries))
}

pub(crate) fn irc_payload(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix('@') {
        rest.split_once(' ').map(|(_, c)| c).unwrap_or(rest)
    } else {
        line
    }
}

pub(crate) fn irc_command(msg: &str) -> bool {
    let head = msg
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        head.as_str(),
        "PRIVMSG" | "JOIN" | "PART" | "INVITE" | "RADIO" | "PONG" | "NICK" | "USER" | "QUIT"
    )
}

pub(crate) fn parse_notice(line: &str) -> Option<String> {
    let rest = irc_payload(line).strip_prefix(':')?;
    let (_, cmd) = rest.split_once(' ')?;
    if !cmd.starts_with("NOTICE ") {
        return None;
    }
    Some(cmd.split_once(" :")?.1.to_string())
}

pub(crate) fn parse_privmsg(line: &str) -> Option<ChatLine> {
    let tags = irc_tags(line);
    let rest = irc_payload(line).strip_prefix(':')?;
    let (prefix, cmd) = rest.split_once(' ')?;
    if !cmd.starts_with("PRIVMSG ") {
        return None;
    }
    let nick = prefix.split('!').next().unwrap_or(prefix).to_string();
    let (target, text) = cmd.strip_prefix("PRIVMSG ")?.split_once(" :")?;
    let channel = if target.starts_with('#') || target.starts_with('&') {
        crate::slash::normalize_channel(target)
    } else if target.len() <= 8 && target.chars().all(|c| c.is_ascii_alphanumeric()) {
        // Group destinations sometimes arrive without '#'.
        crate::slash::normalize_channel(target)
    } else {
        target.to_ascii_uppercase()
    };
    let (piece, text) = match crate::chat_parts::read(text) {
        crate::chat_parts::Body::Piece { i, n, text } => (Some((i, n)), text),
        crate::chat_parts::Body::Whole(t) => (None, t),
    };
    Some(ChatLine {
        piece,
        parts: None,
        sending: None,
        piece_ids: Vec::new(),
        nick,
        text: text.to_string(),
        sys: false,
        channel,
        msgid: tags.get("msgid").cloned().unwrap_or_default(),
        ticks: tags
            .get("radio/delivery")
            .and_then(|s| delivery_ticks(s))
            .unwrap_or("")
            .to_string(),
        via: crate::store::normalize_via(tags.get("radio/via").map(String::as_str))
            .unwrap_or("")
            .to_string(),
        tries: parse_tries(tags.get("radio/tries").map(String::as_str)),
        when: crate::store::chat_stamp(tags.get("server-time").map(String::as_str)),
        snr: tags.get("radio/snr").and_then(|s| s.parse().ok()),
        progress: tags
            .get("radio/file")
            .and_then(|t| crate::proto::file::progress_bar_from_tag(t))
            .unwrap_or_default(),
    })
}

/// The callsign a `PRIVMSG` is addressed to, when it is a direct line and
/// not a channel. (The shared parser files both under a channel name.)
pub(crate) fn dm_target(raw: &str) -> Option<String> {
    let rest = raw.split_once(" PRIVMSG ")?.1;
    let target = rest.split_whitespace().next()?;
    if target.starts_with('#') || target.starts_with('&') {
        return None;
    }
    crate::proto::Callsign::parse(target)
        .ok()
        .map(|_| target.to_ascii_uppercase())
}

/// The direct chat a line belongs to: the other station's callsign, or
/// `None` for a channel line. `me` is our own callsign.
pub(crate) fn dm_peer(raw: &str, nick: &str, me: &str) -> Option<String> {
    let to = dm_target(raw)?;
    Some(if nick.eq_ignore_ascii_case(me) {
        to
    } else {
        nick.to_ascii_uppercase()
    })
}

/// A conversation name that is a callsign, not a `#channel`.
pub(crate) fn is_direct(conv: &str) -> bool {
    !conv.is_empty() && !conv.starts_with('#') && !conv.starts_with('&')
}

pub(crate) fn parse_join(line: &str) -> Option<String> {
    let rest = irc_payload(line).strip_prefix(':')?;
    let (_, cmd) = rest.split_once(' ')?;
    let ch = cmd.strip_prefix("JOIN ")?;
    Some(ch.split_whitespace().next()?.to_string())
}

pub(crate) fn parse_part(line: &str) -> Option<(String, String)> {
    let rest = irc_payload(line).strip_prefix(':')?;
    let (prefix, cmd) = rest.split_once(' ')?;
    let rest = cmd.strip_prefix("PART ")?;
    let ch = rest.split_whitespace().next()?;
    let nick = prefix.split('!').next().unwrap_or(prefix).to_string();
    Some((nick, crate::slash::normalize_channel(ch)))
}

pub(crate) fn parse_invite(line: &str) -> Option<(String, String)> {
    crate::slash::parse_irc_invite(line)
}

#[cfg(test)]
mod file_tests {
    use super::*;

    #[test]
    fn a_file_progress_tag_becomes_a_bar() {
        let (id, bar) =
            parse_file_progress("@msgid=ab12;+radio/msgid=ab12;+radio/file=3/15 TAGMSG G4ABC")
                .unwrap();
        assert_eq!(id, "ab12");
        assert_eq!(bar, crate::proto::file::progress_bar(3, 15));
        // A delivery mark is not file progress.
        assert!(parse_file_progress("@+radio/delivery=sent;+radio/msgid=ab TAGMSG *").is_none());
        let mut lines = vec![ChatLine::user("G4ABC", "📎 a", "G4ABC")];
        lines[0].msgid = "AB12".into();
        assert!(apply_file_progress(&mut lines, "ab12", &bar));
        assert!(!apply_file_progress(&mut lines, "ab12", &bar));
        assert_eq!(lines[0].progress, bar);
    }

    #[test]
    fn the_saved_path_is_found() {
        let t = "📎 map.gpx (1.2 KB) saved to /home/pi/files/G4ABC/map.gpx";
        assert_eq!(
            saved_file(t),
            Some((
                "📎 map.gpx (1.2 KB) saved to ",
                "/home/pi/files/G4ABC/map.gpx"
            ))
        );
        assert!(saved_file("hello saved to x").is_none());
    }
}
