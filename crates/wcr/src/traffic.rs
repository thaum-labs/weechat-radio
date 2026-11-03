//! SPDX-License-Identifier: Apache-2.0
//! All-traffic monitor (#243): every frame this station hears or sends,
//! kept in memory for the web page, the desktop app and `/traffic`.
//!
//! Amateur radio is open, so the monitor shows other stations' DMs,
//! receipts, beacons, bulletins and mail chunks as they pass. It is read
//! only: nothing is ever posted into it, and a reply from it opens a
//! separate DM with that station.

use crate::proto::{Envelope, MsgType};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;

/// Rows kept. Older rows fall off the end.
pub const KEEP: usize = 400;

/// One frame heard or sent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    /// Rising number, so a client can ask only for newer rows.
    pub seq: u64,
    /// Unix seconds.
    pub ts: i64,
    /// `heard` or `sent`.
    pub dir: &'static str,
    /// `rf`, `inet` or `lan`.
    pub via: String,
    /// `dm`, `bulletin`, `group`, `receipt`, `beacon`, `mail`, `status`,
    /// `form`, `checkin`, `ping`, `fragment` or another frame kind.
    pub kind: &'static str,
    pub from: String,
    pub to: String,
    /// The text of a chat line, or a short note (`receipt for 1a2b…`).
    pub text: String,
    /// SNR of an RF frame we heard, dB.
    pub snr: Option<f32>,
    /// Short message id, to match a receipt with its line.
    pub id: String,
    /// Sent by this station for another (a relay), not its own (#247).
    pub relayed: bool,
    /// A mail receiver's answer (chunk ack, complete ack, list reply). Only
    /// used to name its sender; not sent to clients.
    #[serde(skip)]
    pub mail_answer: bool,
}

/// Mail exchanges remembered, so later frames can name their stations.
const MAIL_KEEP: usize = 32;

/// Who sent a mail and which station it went to over RF, learned from the
/// first chunk (the only one with addresses).
#[derive(Debug)]
struct MailExchange {
    id: String,
    sender: String,
    receiver: String,
}

#[derive(Debug, Default)]
pub struct Log {
    next: u64,
    rows: VecDeque<Row>,
    mail: VecDeque<MailExchange>,
}

pub type Shared = Arc<Mutex<Log>>;

pub fn new_shared() -> Shared {
    Arc::new(Mutex::new(Log::default()))
}

impl Log {
    pub fn push(&mut self, mut row: Row) {
        if row.kind == "mail" {
            self.name_mail_stations(&mut row);
        }
        self.next += 1;
        row.seq = self.next;
        self.rows.push_back(row);
        while self.rows.len() > KEEP {
            self.rows.pop_front();
        }
    }

    /// Only a mail's first chunk carries addresses: later chunks and the
    /// receiver's acks showed "?" as the sender (#266). Fill them in from the
    /// first chunk of the same mail. Display only.
    fn name_mail_stations(&mut self, row: &mut Row) {
        if row.id.is_empty() {
            return;
        }
        let known = self.mail.iter().find(|m| m.id == row.id);
        if row.mail_answer {
            // The station the chunks went to answers the mail's sender.
            if let Some(m) = known {
                let to_sender = row.to.is_empty() || row.to.eq_ignore_ascii_case(&m.sender);
                if row.from.is_empty() && to_sender {
                    row.from = m.receiver.clone();
                }
                if row.to.is_empty() {
                    row.to = m.sender.clone();
                }
            }
            return;
        }
        if let Some(m) = known {
            if row.from.is_empty() {
                row.from = m.sender.clone();
            }
            if row.to.is_empty() {
                row.to = m.receiver.clone();
            }
        } else if !row.from.is_empty() && !row.to.is_empty() {
            self.mail.push_back(MailExchange {
                id: row.id.clone(),
                sender: row.from.clone(),
                receiver: row.to.clone(),
            });
            while self.mail.len() > MAIL_KEEP {
                self.mail.pop_front();
            }
        }
    }

    /// Rows newer than `seq`, oldest first, at most `limit`.
    pub fn since(&self, seq: u64, limit: usize) -> Vec<Row> {
        let newer: Vec<&Row> = self.rows.iter().filter(|r| r.seq > seq).collect();
        let skip = newer.len().saturating_sub(limit);
        newer.into_iter().skip(skip).cloned().collect()
    }
}

/// A short id for display and matching.
fn short_id(env: &Envelope) -> String {
    let hex = env.msg_id.hex();
    hex.chars().take(8).collect()
}

/// The frame kind in plain words.
fn kind_of(env: &Envelope) -> &'static str {
    let dest = env.dest.as_str();
    match env.kind {
        MsgType::Msg if crate::slash::is_bulletin(dest) => "bulletin",
        MsgType::Msg if env.flags.group() => "group",
        MsgType::Msg => "dm",
        MsgType::Ack => "receipt",
        MsgType::Frag => "fragment",
        other => other.as_str(),
    }
}

/// Printable text of a line, or empty for binary bodies.
fn text_of(env: &Envelope) -> String {
    let text_kind = matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    );
    if !text_kind {
        return String::new();
    }
    let body = if env.flags.compressed() {
        match crate::proto::compress::decompress(&env.body) {
            Ok(b) => b,
            Err(_) => return String::new(),
        }
    } else {
        env.body.clone()
    };
    let Ok(text) = std::str::from_utf8(&body) else {
        return String::new();
    };
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return String::new();
    }
    text.chars()
        .take(200)
        .collect::<String>()
        .replace('\n', " ")
}

/// A chat frame (any envelope) as a row.
pub fn envelope_row(env: &Envelope, dir: &'static str, via: &str, snr: Option<f32>) -> Row {
    let text = match env.kind {
        MsgType::Ack => env
            .acked_id()
            .map(|id| {
                let h = id.hex();
                format!("receipt for {}", &h[..h.len().min(8)])
            })
            .unwrap_or_default(),
        _ => text_of(env),
    };
    Row {
        seq: 0,
        ts: chrono::Utc::now().timestamp(),
        dir,
        via: via.to_string(),
        kind: kind_of(env),
        from: env.origin.to_string(),
        to: env.dest.to_string(),
        text,
        snr: if via == "rf" { snr } else { None },
        id: short_id(env),
        relayed: false,
        mail_answer: false,
    }
}

/// A mail frame on the air as a row.
pub fn mail_row(m: &crate::mail::Incoming, dir: &'static str, snr: Option<f32>) -> Row {
    let mail_answer = m.answers();
    let (from, to, text, id) = match m {
        crate::mail::Incoming::Wire(w) => {
            let what = format!("{:?}", w.op).to_ascii_lowercase();
            let part = if w.count > 1 {
                format!(" {}/{}", w.idx + 1, w.count)
            } else {
                String::new()
            };
            let subject = if w.meta.subject.is_empty() {
                String::new()
            } else {
                format!(" \u{b7} {}", w.meta.subject)
            };
            // A station's own WCR address is shown as its callsign (#266).
            let from = if w.meta.from.is_empty() {
                w.meta.via.clone()
            } else {
                crate::mail::callsign_from_wcr(&w.meta.from).unwrap_or_else(|| w.meta.from.clone())
            };
            let to = if w.meta.dest.is_empty() {
                w.meta.to.clone()
            } else {
                w.meta.dest.clone()
            };
            (
                from,
                to,
                format!("mail {what}{part}{subject}"),
                w.mail_id.chars().take(8).collect(),
            )
        }
        crate::mail::Incoming::Slice {
            group, part, parts, ..
        } => (
            String::new(),
            String::new(),
            format!("mail slice {}/{}", part + 1, parts),
            group.chars().take(8).collect(),
        ),
    };
    Row {
        seq: 0,
        ts: chrono::Utc::now().timestamp(),
        dir,
        via: "rf".into(),
        kind: "mail",
        from,
        to,
        text,
        snr,
        id,
        relayed: false,
        mail_answer,
    }
}

/// `GET /traffic?since=N&limit=M` on the station's loopback HTTP.
pub fn router(log: Shared) -> axum::Router {
    use axum::{extract::Query, routing::get, Json};
    #[derive(serde::Deserialize)]
    struct Q {
        #[serde(default)]
        since: u64,
        #[serde(default)]
        limit: Option<usize>,
    }
    axum::Router::new().route(
        "/traffic",
        get(move |Query(q): Query<Q>| {
            let log = log.clone();
            async move {
                let g = log.lock();
                Json(serde_json::json!({
                    "rows": g.since(q.since, q.limit.unwrap_or(KEEP).min(KEEP)),
                    "last": g.next,
                }))
            }
        }),
    )
}

/// `/traffic [N]`: the last N frames (default 15) as text lines.
pub fn lines(log: &Log, n: usize) -> Vec<String> {
    log.since(0, n.clamp(1, 100))
        .into_iter()
        .map(|r| line(&r))
        .collect()
}

/// One row as a text line: `19:03:12 heard rf DM M7TJF/M -> M7TJF 12 dB: hello`.
pub fn line(r: &Row) -> String {
    let when = chrono::DateTime::from_timestamp(r.ts, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();
    let snr = r.snr.map(|s| format!(" {s:.0} dB")).unwrap_or_default();
    let to = if r.to.is_empty() {
        String::new()
    } else {
        format!(" -> {}", r.to)
    };
    let text = if r.text.is_empty() {
        String::new()
    } else {
        format!(": {}", r.text)
    };
    format!(
        "{when} {} {} {} {}{to}{snr}{text}",
        if r.relayed { "relayed" } else { r.dir },
        r.via,
        r.kind,
        if r.from.is_empty() { "?" } else { &r.from }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Callsign, Flags};

    fn dm(from: &str, to: &str, text: &str) -> Envelope {
        Envelope::new_msg(
            Callsign::parse(from).unwrap(),
            Callsign::parse(to).unwrap(),
            1,
            text.as_bytes().to_vec(),
            5,
            Flags::new(),
        )
        .unwrap()
    }

    #[test]
    fn rows_keep_order_and_drop_the_oldest() {
        let mut log = Log::default();
        for i in 0..(KEEP + 5) {
            log.push(envelope_row(
                &dm("M7TJF", "M0BBB", &format!("line {i}")),
                "heard",
                "rf",
                Some(9.0),
            ));
        }
        let all = log.since(0, KEEP);
        assert_eq!(all.len(), KEEP);
        assert_eq!(all[0].text, "line 5");
        assert_eq!(all.last().unwrap().seq, (KEEP + 5) as u64);
        let newer = log.since(all.last().unwrap().seq - 2, KEEP);
        assert_eq!(newer.len(), 2);
        assert_eq!(log.since(0, 3).len(), 3);
    }

    #[test]
    fn other_stations_dms_and_receipts_are_shown_with_who_and_how() {
        let line = dm("M7TJF", "M0BBB", "hello there");
        let r = envelope_row(&line, "heard", "rf", Some(12.4));
        assert_eq!(r.kind, "dm");
        assert_eq!((r.from.as_str(), r.to.as_str()), ("M7TJF", "M0BBB"));
        assert_eq!(r.text, "hello there");
        assert_eq!(r.snr, Some(12.4));
        let ack = Envelope::ack_for(&line, Callsign::parse("M0BBB").unwrap(), 2, Some(8.0));
        let a = envelope_row(&ack, "sent", "inet", Some(3.0));
        assert_eq!(a.kind, "receipt");
        assert!(a.text.starts_with("receipt for "), "{}", a.text);
        // SNR belongs to RF only.
        assert_eq!(a.snr, None);
        assert!(super::line(&r).contains("heard rf dm M7TJF -> M0BBB 12 dB: hello there"));
    }

    /// A mail frame as it comes off the air.
    fn mail(op: u8, idx: u16, count: u16, meta: &str) -> crate::mail::Incoming {
        let id = "ab12cd34".repeat(8);
        let raw = format!("WCRM\t{op}\t{id}\t{idx}\t{count}\t{meta}\tbody");
        crate::mail::try_decode(raw.as_bytes()).expect("mail frame")
    }

    #[test]
    fn mail_rows_name_both_stations_after_the_first_chunk() {
        // #266: only the first chunk carries addresses. The second chunk and
        // the gateway's complete ack showed "?" as the sender.
        let mut log = Log::default();
        let first = r#"{"f":"M7TJF/M@mail.weechatradio.com","t":"qa@example.com","s":"rc7","d":"M7TJF","v":"M7TJF"}"#;
        log.push(mail_row(&mail(5, 0, 2, first), "heard", Some(8.0)));
        log.push(mail_row(&mail(5, 1, 2, "{}"), "heard", Some(8.0)));
        log.push(mail_row(&mail(7, 0, 1, r#"{"d":"M7TJF/M"}"#), "sent", None));
        let rows = log.since(0, KEEP);
        let who: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| (r.from.as_str(), r.to.as_str()))
            .collect();
        assert_eq!(
            who,
            [
                ("M7TJF/M", "M7TJF"),
                ("M7TJF/M", "M7TJF"),
                ("M7TJF", "M7TJF/M")
            ]
        );
        assert_eq!(rows[1].text, "mail voxdata 2/2");
        assert_eq!(rows[2].text, "mail completeack");
        // Another station's address stays as it is.
        let out = r#"{"f":"someone@example.com","d":"M7TJF/M","v":"M7TJF"}"#;
        assert_eq!(
            mail_row(&mail(0, 0, 1, out), "heard", None).from,
            "someone@example.com"
        );
        // An ack for a mail never seen keeps an empty sender ("?").
        let mut fresh = Log::default();
        fresh.push(mail_row(
            &mail(7, 0, 1, r#"{"d":"M7TJF/M"}"#),
            "heard",
            None,
        ));
        assert_eq!(fresh.since(0, 1)[0].from, "");
    }
}
