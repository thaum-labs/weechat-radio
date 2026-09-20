//! SPDX-License-Identifier: Apache-2.0
//! Slash-command catalogue for the desktop GUI picker.

#[derive(Clone, Copy)]
struct Cmd {
    name: &'static str,
    usage: &'static str,
    summary: &'static str,
    args: &'static [Arg],
    send_bare: bool,
}

#[derive(Clone, Copy)]
struct Arg {
    value: &'static str,
    hint: &'static str,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub label: String,
    pub hint: String,
    pub insert: String,
    pub send_now: bool,
}

const MODE_ARGS: &[Arg] = &[
    Arg {
        value: "internet",
        hint: "Internet only — radio idle",
    },
    Arg {
        value: "internet-radio",
        hint: "Radio plus hub (this node is a gateway)",
    },
    Arg {
        value: "radio",
        hint: "RF chat only — hub chat off (needs confirm)",
    },
    Arg {
        value: "radio-plus",
        hint: "RF here; a gateway may forward you",
    },
];

const PRESET_ARGS: &[Arg] = &[
    Arg {
        value: "vhf-fm",
        hint: "VHF/UHF FM, clean local links",
    },
    Arg {
        value: "hf-good",
        hint: "Good HF SSB path",
    },
    Arg {
        value: "hf-poor",
        hint: "Fading HF / NVIS",
    },
    Arg {
        value: "hf-weak",
        hint: "Very weak HF (RDM-300S)",
    },
    Arg {
        value: "hf-deep",
        hint: "Deep-fade HF backup (MFSK-32R)",
    },
    Arg {
        value: "vox-safe",
        hint: "VOX audio cable — MFSK-32R + long lead",
    },
    Arg {
        value: "afsk-1200",
        hint: "Radio's own KISS TNC (VR-N76, UV-PRO) — fixed",
    },
];

const PTT_ARGS: &[Arg] = &[
    Arg {
        value: "vox",
        hint: "Radio VOX keys from PC audio",
    },
    Arg {
        value: "digirig",
        hint: "Digirig serial PTT (RTS)",
    },
    Arg {
        value: "cm108",
        hint: "CM108 GPIO PTT",
    },
    Arg {
        value: "rigctl",
        hint: "Hamlib rigctld CAT PTT",
    },
    Arg {
        value: "none",
        hint: "Do not key a radio",
    },
];

const THEME_ARGS: &[Arg] = &[
    Arg {
        value: "tron",
        hint: "Indigo / orange (default)",
    },
    Arg {
        value: "hacker",
        hint: "Green terminal",
    },
    Arg {
        value: "terminal",
        hint: "Plain terminal colours",
    },
];

const GROUP_ARGS: &[Arg] = &[
    Arg {
        value: "list",
        hint: "Show groups",
    },
    Arg {
        value: "create",
        hint: "create <name> <CALLSIGN…>",
    },
    Arg {
        value: "invite",
        hint: "invite <name> <CALLSIGN>",
    },
    Arg {
        value: "members",
        hint: "members <name>",
    },
    Arg {
        value: "leave",
        hint: "leave <name>",
    },
];

const PRIO_ARGS: &[Arg] = &[
    Arg {
        value: "routine",
        hint: "Normal traffic (TTL 3)",
    },
    Arg {
        value: "priority",
        hint: "Faster relays (TTL 4)",
    },
    Arg {
        value: "emergency",
        hint: "Highest — double TX on RF (TTL 5)",
    },
];

const COMMANDS: &[Cmd] = &[
    Cmd {
        name: "help",
        usage: "/help",
        summary: "List station commands",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "status",
        usage: "/status",
        summary: "Mode, audio, queue, hub",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "mode",
        usage: "/mode internet|internet-radio|radio|radio-plus",
        summary: "How this station sends",
        args: MODE_ARGS,
        send_bare: false,
    },
    Cmd {
        name: "preset",
        usage: "/preset vhf-fm|hf-good|hf-poor|hf-weak|hf-deep|vox-safe|afsk-1200",
        summary: "Modem waveform",
        args: PRESET_ARGS,
        send_bare: false,
    },
    Cmd {
        name: "ptt",
        usage: "/ptt vox|digirig|cm108|rigctl|none",
        summary: "How the radio is keyed",
        args: PTT_ARGS,
        send_bare: false,
    },
    Cmd {
        name: "queue",
        usage: "/queue",
        summary: "Messages waiting to send",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "net",
        usage: "/net",
        summary: "Who checked in",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "form",
        usage: "/form ics213|radiogram <target> key=value …",
        summary: "Send ICS-213 or radiogram form",
        args: &[],
        send_bare: false,
    },
    Cmd {
        name: "checkin",
        usage: "/checkin [note]",
        summary: "Mark yourself present",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "freq",
        usage: "/freq [MHz]",
        summary: "Show or set this station's frequency",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "prio",
        usage: "/prio [routine|priority|emergency]",
        summary: "Channel default priority (synced)",
        args: PRIO_ARGS,
        send_bare: true,
    },
    Cmd {
        name: "qsy",
        usage: "/qsy [MHz]",
        summary: "Change frequency (CAT if available)",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "join",
        usage: "/join #channel",
        summary: "Open or create a chat channel",
        args: &[],
        send_bare: false,
    },
    Cmd {
        name: "invite",
        usage: "/invite CALLSIGN",
        summary: "Invite a station to this channel",
        args: &[],
        send_bare: false,
    },
    Cmd {
        name: "part",
        usage: "/part",
        summary: "Leave this channel (this computer only)",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "leave",
        usage: "/leave",
        summary: "Leave this channel (this computer only)",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "group",
        usage: "/group list|create|members|invite|leave",
        summary: "Named callsign lists",
        args: GROUP_ARGS,
        send_bare: false,
    },
    Cmd {
        name: "trace",
        usage: "/trace <msgid>",
        summary: "Where a message travelled",
        args: &[],
        send_bare: false,
    },
    Cmd {
        name: "clear",
        usage: "/clear",
        summary: "Clear this channel on this computer",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "history",
        usage: "/history purge [target]",
        summary: "Delete stored messages",
        args: &[Arg {
            value: "purge",
            hint: "purge [callsign]",
        }],
        send_bare: false,
    },
    Cmd {
        name: "mute",
        usage: "/mute <callsign>",
        summary: "Hide a station",
        args: &[],
        send_bare: false,
    },
    Cmd {
        name: "theme",
        usage: "/theme tron|hacker|terminal",
        summary: "TUI colours (GUI stays indigo)",
        args: THEME_ARGS,
        send_bare: false,
    },
    Cmd {
        name: "modem",
        usage: "/modem",
        summary: "KISS address",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "update",
        usage: "/update",
        summary: "How to apply a software update",
        args: &[],
        send_bare: true,
    },
];

/// Turn a typed `/…` line into the IRC `RADIO` argument string.
pub fn to_radio_args(raw: &str) -> Option<String> {
    let t = raw.trim();
    if !t.starts_with('/') {
        return None;
    }
    let rest = t[1..].trim();
    let rest = rest
        .strip_prefix("radio ")
        .or_else(|| rest.strip_prefix("RADIO "))
        .unwrap_or(rest);
    if rest.is_empty() || rest.eq_ignore_ascii_case("radio") {
        return Some("help".into());
    }
    let head = rest
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        head.as_str(),
        "join" | "j" | "part" | "leave" | "invite" | "prio" | "priority"
    ) {
        return None;
    }
    Some(rest.to_string())
}

/// Packed group dest is 8 characters (same as a callsign). Longer names
/// silently became a different room on the air (`compatriots` → `compatri`).
pub const CHANNEL_NAME_MAX: usize = 8;

pub fn normalize_channel(raw: &str) -> String {
    let t: String = raw
        .trim()
        .trim_start_matches('#')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(CHANNEL_NAME_MAX)
        .collect::<String>()
        .to_ascii_lowercase();
    format!("#{t}")
}

/// Wire dest for a `#channel` (uppercase, at most [`CHANNEL_NAME_MAX`]).
pub fn channel_dest(raw: &str) -> String {
    normalize_channel(raw)
        .trim_start_matches('#')
        .to_ascii_uppercase()
}

pub fn is_bulletin(channel: &str) -> bool {
    channel.eq_ignore_ascii_case("#bulletin") || channel.eq_ignore_ascii_case("bulletin")
}

/// Human + wire body for a channel invite (also sent as a 1:1 PRIVMSG).
pub fn invite_notice(channel: &str, nick: &str) -> String {
    let ch = normalize_channel(channel);
    let nick = nick.trim();
    if nick.is_empty() {
        format!("You are invited to {ch} on WeeChat Radio. Join that channel to talk.")
    } else {
        format!("You are invited to {ch} ({nick}) on WeeChat Radio. Join that channel to talk.")
    }
}

/// `:from INVITE nick #channel` (optional `@tags`).
pub fn parse_irc_invite(line: &str) -> Option<(String, String)> {
    let rest = if let Some(tagged) = line.strip_prefix('@') {
        tagged.find(" :").map(|i| &tagged[i + 1..]).unwrap_or(line)
    } else {
        line
    };
    let rest = rest.strip_prefix(':')?;
    let (prefix, cmd) = rest.split_once(' ')?;
    let rest = cmd.strip_prefix("INVITE ")?;
    let mut bits = rest.split_whitespace();
    bits.next()?;
    let ch = normalize_channel(bits.next()?);
    if ch.len() < 3 || is_bulletin(&ch) {
        return None;
    }
    let from = prefix.split('!').next().unwrap_or(prefix).to_string();
    Some((from, ch))
}

/// Pull `#channel` out of [`invite_notice`] (and the older wording without a nick).
pub fn parse_invite_text(text: &str) -> Option<String> {
    let mut t = text.trim();
    for prefix in ["[rf] ", "[lan] ", "[inet] ", "[net] "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest;
            break;
        }
    }
    let rest = t.strip_prefix("You are invited to ")?;
    let raw = rest.split_whitespace().next()?;
    let ch = normalize_channel(raw);
    if ch.len() < 3 || is_bulletin(&ch) {
        return None;
    }
    Some(ch)
}

/// Shown in the channel when a station leaves (not the RF control body).
pub fn leave_notice(channel: &str) -> String {
    format!("left {}", normalize_channel(channel))
}

pub fn parse_leave_text(text: &str) -> Option<String> {
    let mut t = text.trim();
    for prefix in ["[rf] ", "[lan] ", "[inet] ", "[net] "] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest;
            break;
        }
    }
    let rest = t.strip_prefix("left ")?;
    let ch = normalize_channel(rest.split_whitespace().next()?);
    if ch.len() < 3 || is_bulletin(&ch) {
        return None;
    }
    Some(ch)
}

/// Server notice `{CALL} left #channel`.
pub fn parse_peer_left_notice(text: &str) -> Option<(String, String)> {
    let (nick, rest) = text.trim().split_once(" left #")?;
    let nick = nick.trim();
    if nick.is_empty() || nick.contains(' ') {
        return None;
    }
    let ch = normalize_channel(rest);
    if ch.len() < 3 || is_bulletin(&ch) {
        return None;
    }
    Some((nick.to_string(), ch))
}

/// Turn a GUI line into one or more IRC commands (no trailing CRLF).
pub fn to_wire(raw: &str, channel: &str) -> Vec<String> {
    let t = raw.trim();
    if t.is_empty() {
        return Vec::new();
    }
    if !t.starts_with('/') {
        return vec![format!("PRIVMSG {channel} :{t}")];
    }
    let rest = t[1..].trim();
    let rest = rest
        .strip_prefix("radio ")
        .or_else(|| rest.strip_prefix("RADIO "))
        .unwrap_or(rest);
    let (head, tail) = match rest.split_once(char::is_whitespace) {
        Some((h, a)) => (h.to_ascii_lowercase(), a.trim().to_string()),
        None => (rest.to_ascii_lowercase(), String::new()),
    };
    match head.as_str() {
        "join" | "j" => {
            let ch = normalize_channel(&tail);
            if ch.len() < 2 {
                return Vec::new();
            }
            vec![format!("JOIN {ch}")]
        }
        "clear" => {
            let target = if tail.is_empty() {
                channel.to_string()
            } else {
                let raw = tail.split_whitespace().next().unwrap_or("");
                if raw.starts_with('~') || raw.chars().any(|c| c.is_ascii_digit()) {
                    raw.to_ascii_uppercase()
                } else {
                    normalize_channel(raw)
                }
            };
            if target.is_empty() {
                return Vec::new();
            }
            vec![format!("RADIO clear {target}")]
        }
        "part" | "leave" => {
            if is_bulletin(channel) {
                return Vec::new();
            }
            vec![format!("PART {channel}")]
        }
        "invite" => {
            let nick = tail
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if nick.is_empty() {
                return Vec::new();
            }
            let g = channel.trim_start_matches('#');
            vec![
                format!("RADIO group invite {g} {nick}"),
                format!("INVITE {nick} {channel}"),
                format!("PRIVMSG {nick} :{}", invite_notice(channel, &nick)),
            ]
        }
        "prio" | "priority" => {
            if channel.starts_with('#') || channel.starts_with('&') {
                if tail.is_empty() {
                    vec![format!("RADIO prio {channel}")]
                } else {
                    vec![format!("RADIO prio {channel} {tail}")]
                }
            } else {
                Vec::new()
            }
        }
        _ => {
            if let Some(args) = to_radio_args(t) {
                vec![format!("RADIO {args}")]
            } else {
                vec![format!("PRIVMSG {channel} :{t}")]
            }
        }
    }
}

pub fn suggestions(draft: &str) -> Vec<Suggestion> {
    let t = draft.trim_start();
    if !t.starts_with('/') {
        return Vec::new();
    }
    let mut rest = t[1..].to_string();
    if let Some(r) = rest
        .strip_prefix("radio ")
        .or_else(|| rest.strip_prefix("RADIO "))
    {
        rest = r.to_string();
    }
    let rest = rest.trim_start();
    if rest.is_empty() {
        return COMMANDS.iter().map(cmd_suggestion).collect();
    }
    let (head, tail) = match rest.split_once(char::is_whitespace) {
        Some((h, a)) => (h.to_ascii_lowercase(), a.trim_start().to_string()),
        None => (rest.to_ascii_lowercase(), String::new()),
    };
    if tail.is_empty() && !draft.ends_with(' ') && !draft.ends_with('\t') {
        return COMMANDS
            .iter()
            .filter(|c| c.name.starts_with(&head) || c.usage.contains(&head))
            .map(cmd_suggestion)
            .collect();
    }
    let Some(cmd) = COMMANDS.iter().find(|c| c.name == head) else {
        return COMMANDS
            .iter()
            .filter(|c| c.name.starts_with(&head))
            .map(cmd_suggestion)
            .collect();
    };
    if cmd.args.is_empty() {
        return vec![cmd_suggestion(cmd)];
    }
    let needle = tail.to_ascii_lowercase();
    cmd.args
        .iter()
        .filter(|a| needle.is_empty() || a.value.starts_with(&needle) || a.hint.contains(&needle))
        .map(|a| {
            let insert = if cmd.name == "mode" && a.value == "radio" {
                "/mode radio confirm".into()
            } else if cmd.name == "group" && a.value == "create" {
                "/group create ".into()
            } else if cmd.name == "group" && a.value == "invite" {
                "/group invite ".into()
            } else if cmd.name == "group" && a.value == "members" {
                "/group members ".into()
            } else if cmd.name == "history" {
                "/history purge ".into()
            } else {
                format!("/{} {}", cmd.name, a.value)
            };
            let send_now = !insert.ends_with(' ');
            Suggestion {
                label: insert.trim().to_string(),
                hint: a.hint.to_string(),
                insert,
                send_now,
            }
        })
        .collect()
}

fn cmd_suggestion(c: &Cmd) -> Suggestion {
    Suggestion {
        label: format!("/{}", c.name),
        hint: format!("{} — {}", c.usage, c.summary),
        insert: if c.send_bare {
            format!("/{}", c.name)
        } else {
            format!("/{} ", c.name)
        },
        send_now: c.send_bare,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_prefix_strips() {
        assert_eq!(
            to_radio_args("/radio mode internet").as_deref(),
            Some("mode internet")
        );
        assert_eq!(to_radio_args("/help").as_deref(), Some("help"));
        assert_eq!(to_radio_args("hello"), None);
    }

    #[test]
    fn slash_lists_commands() {
        let s = suggestions("/");
        assert!(s.iter().any(|x| x.label == "/mode"));
        assert!(s.iter().any(|x| x.label == "/preset"));
    }

    #[test]
    fn mode_args_after_space() {
        let s = suggestions("/mode ");
        assert!(s.iter().any(|x| x.label.contains("internet-radio")));
    }

    #[test]
    fn long_channel_names_fit_the_radio_dest() {
        assert_eq!(normalize_channel("#compatriots"), "#compatri");
        assert_eq!(normalize_channel("friends"), "#friends");
        assert_eq!(channel_dest("#compatriots"), "COMPATRI");
        assert_eq!(CHANNEL_NAME_MAX, 8);
    }

    #[test]
    fn join_is_irc_not_radio() {
        assert_eq!(to_radio_args("/join #ops"), None);
        assert_eq!(
            to_wire("/join ops", "#bulletin"),
            vec!["JOIN #ops".to_string()]
        );
        assert!(to_wire("hello", "#ops")[0].starts_with("PRIVMSG #ops"));
    }

    #[test]
    fn invite_sends_group_and_privmsg() {
        let wires = to_wire("/invite TF101", "#compatriots");
        assert_eq!(wires[0], "RADIO group invite compatriots TF101");
        assert_eq!(wires[1], "INVITE TF101 #compatriots");
        assert_eq!(
            wires[2],
            format!("PRIVMSG TF101 :{}", invite_notice("#compatriots", "TF101"))
        );
        assert_eq!(
            parse_invite_text(&invite_notice("#compatriots", "TF101")).as_deref(),
            Some("#compatriots")
        );
        assert_eq!(
            parse_invite_text(
                "You are invited to #compatriots on WeeChat Radio. Join that channel to talk."
            )
            .as_deref(),
            Some("#compatriots")
        );
        assert_eq!(
            parse_invite_text(
                "[rf] You are invited to #ops on WeeChat Radio. Join that channel to talk."
            )
            .as_deref(),
            Some("#ops")
        );
        assert!(parse_invite_text("hello there").is_none());
        assert!(parse_invite_text("You are invited to #bulletin on WeeChat Radio.").is_none());
        let (from, ch) = parse_irc_invite(":M7TJF INVITE TF101 #compatriots").unwrap();
        assert_eq!(from, "M7TJF");
        assert_eq!(ch, "#compatriots");
    }

    #[test]
    fn part_and_leave_are_irc_part() {
        assert_eq!(to_wire("/part", "#compatriots"), vec!["PART #compatriots"]);
        assert_eq!(to_wire("/leave", "#ops"), vec!["PART #ops"]);
        assert!(to_wire("/part", "#bulletin").is_empty());
        assert_eq!(
            parse_leave_text("left #compatriots").as_deref(),
            Some("#compatriots")
        );
        assert_eq!(parse_leave_text("[rf] left #ops").as_deref(), Some("#ops"));
        assert!(parse_leave_text("hello").is_none());
        assert!(parse_leave_text("left #bulletin").is_none());
        let (nick, ch) = parse_peer_left_notice("M7TJF left #compatriots").unwrap();
        assert_eq!(nick, "M7TJF");
        assert_eq!(ch, "#compatriots");
        assert!(parse_peer_left_notice("channel default left alone").is_none());
    }

    #[test]
    fn clear_uses_active_channel() {
        assert_eq!(
            to_wire("/clear", "#bulletin"),
            vec!["RADIO clear #bulletin".to_string()]
        );
        assert_eq!(
            to_wire("/clear ops", "#bulletin"),
            vec!["RADIO clear #ops".to_string()]
        );
        assert_eq!(
            to_wire("/clear M0XYZ", "#bulletin"),
            vec!["RADIO clear M0XYZ".to_string()]
        );
    }
}
