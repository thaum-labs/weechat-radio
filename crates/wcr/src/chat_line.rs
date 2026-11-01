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
    Some(ChatLine {
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
    })
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
