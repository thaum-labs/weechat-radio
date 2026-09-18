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
        hint: "RF only — drops the internet (needs confirm)",
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
        hint: "VOX audio cable — extra lead/tail",
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
        summary: "Leave this channel",
        args: &[],
        send_bare: true,
    },
    Cmd {
        name: "group",
        usage: "/group list|create|members|invite",
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
        "join" | "j" | "part" | "invite" | "prio" | "priority"
    ) {
        return None;
    }
    Some(rest.to_string())
}

pub fn normalize_channel(raw: &str) -> String {
    let t = raw
        .trim()
        .trim_start_matches('#')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
        .to_ascii_lowercase();
    format!("#{t}")
}

pub fn is_bulletin(channel: &str) -> bool {
    channel.eq_ignore_ascii_case("#bulletin") || channel.eq_ignore_ascii_case("bulletin")
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
        "part" => vec![format!("PART {channel}")],
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
                format!(
                    "PRIVMSG {nick} :You are invited to {channel} on WeeChat Radio. Join that channel to talk."
                ),
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
    fn join_is_irc_not_radio() {
        assert_eq!(to_radio_args("/join #ops"), None);
        assert_eq!(
            to_wire("/join ops", "#bulletin"),
            vec!["JOIN #ops".to_string()]
        );
        assert!(to_wire("hello", "#ops")[0].starts_with("PRIVMSG #ops"));
    }
}
