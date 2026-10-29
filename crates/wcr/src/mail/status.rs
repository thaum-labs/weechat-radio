//! SPDX-License-Identifier: Apache-2.0
//! One status per mail, the same for every client: the chip the desktop app
//! draws, the bracket mark the TUI and screen readers keep, the trailing
//! word, the full reason, and whether Retry or Re-send is offered.
//!
//! There is no "read" status: mail goes out as plain text, so nobody can
//! tell when it is opened.

use super::gateway;
use super::store::{Hop, MailRow};
use serde::{Deserialize, Serialize};

/// Sent mail without a final answer after this long says "no delivery report".
pub const NO_REPORT_AFTER_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailStatus {
    /// `queued`, `tx`, `keyed`, `gateway`, `sent`, `delivered`, `delayed`,
    /// `bounced`, `spam`, `failed`, `no_record` or `received`.
    pub key: String,
    /// The chip text: Queued, On air, At gateway, Sent, Delivered, …
    pub label: String,
    /// `[..]`, `[tx]`, `[gw]`, `[ok]`, `[no]`, or empty for inbound mail.
    pub mark: String,
    /// The short word after the chip, for example `with Resend`.
    pub word: String,
    /// The whole story when something went wrong or is still open.
    pub reason: String,
    /// One plain line for text clients: `[ok] delivered`.
    pub line: String,
    /// Failed here or at the gateway: offer Retry.
    pub retry: bool,
    /// Resend could not deliver it: offer Re-send (compose prefilled).
    pub resend: bool,
}

/// A mail row with its status and hop times, as the local mail API serves it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MailView {
    #[serde(flatten)]
    pub row: MailRow,
    #[serde(default)]
    pub status: MailStatus,
    #[serde(default)]
    pub hops: Vec<Hop>,
}

/// `online` is false when the last delivery check could not reach the hub.
pub fn mail_status(row: &MailRow, hops: &[Hop], now: i64, online: bool) -> MailStatus {
    // Inbound mail is Received whatever state an older version stored.
    let state = if row.folder == "inbox" {
        "received"
    } else {
        row.state.as_str()
    };
    let (key, label, word, reason) = match state {
        "queued" => ("queued", "Queued", "", String::new()),
        // The burst has finished keying; the gateway has not answered yet.
        "tx" if hops.iter().any(|h| h.stage == "keyed") => (
            "keyed",
            "Waiting for gateway",
            "",
            "The burst is done. Waiting for the gateway to hand it to the hub and answer."
                .to_string(),
        ),
        "tx" => ("tx", "On air", "", String::new()),
        "gateway" => (
            "gateway",
            "At gateway",
            "",
            "The gateway has every chunk and is handing it to the hub.".to_string(),
        ),
        "failed" => ("failed", "Failed", "", failed_reason(&row.detail, row.ack)),
        "sent" => sent_status(row, hops, now, online),
        _ => ("received", "Received", "", String::new()),
    };
    // Resend's bad news is a [no], although the row's state is `sent`.
    let mark = match key {
        "bounced" | "spam" | "failed" => "[no]",
        "delayed" | "keyed" => "[..]",
        "received" => "",
        _ => gateway::state_mark(state).trim(),
    }
    .to_string();
    let tail = if word.is_empty() {
        label.to_ascii_lowercase()
    } else {
        word.to_string()
    };
    let line = format!("{mark} {tail}").trim().to_string();
    MailStatus {
        key: key.into(),
        label: label.into(),
        mark,
        word: word.into(),
        reason,
        line,
        retry: state == "failed",
        resend: state == "sent"
            && matches!(row.delivery.as_str(), "bounced" | "failed" | "canceled"),
    }
}

fn sent_status(
    row: &MailRow,
    hops: &[Hop],
    now: i64,
    online: bool,
) -> (&'static str, &'static str, &'static str, String) {
    let d = row.delivery.as_str();
    match d {
        "delivered" | "opened" | "clicked" => ("delivered", "Delivered", "", String::new()),
        "delivery_delayed" => (
            "delayed",
            "Delayed",
            "",
            "Resend says delivery is delayed. It is still trying.".into(),
        ),
        "bounced" => (
            "bounced",
            "Bounced",
            "",
            "The recipient's mail server refused it. Check the address, then Re-send.".into(),
        ),
        "complained" => (
            "spam",
            "Spam",
            "",
            "The recipient marked it as spam.".into(),
        ),
        "failed" => (
            "failed",
            "Failed",
            "",
            "Resend accepted it but could not send it.".into(),
        ),
        "canceled" => ("failed", "Failed", "", "Resend canceled it.".into()),
        "unknown" => (
            "no_record",
            "No record",
            "",
            "The hub has no delivery record for this mail, for example when an older gateway posted it."
                .into(),
        ),
        _ => {
            let sent_at = hops
                .iter()
                .find(|h| h.stage == "sent")
                .map(|h| h.ts)
                .unwrap_or(row.ts);
            if now - sent_at >= NO_REPORT_AFTER_SECS {
                (
                    "sent",
                    "Sent",
                    "no delivery report",
                    "Resend accepted it, but sent no delivery report within a day.".into(),
                )
            } else if !online {
                (
                    "sent",
                    "Sent",
                    "delivery unknown (offline)",
                    "Resend accepted it. The hub cannot be reached from here to ask about delivery."
                        .into(),
                )
            } else {
                ("sent", "Sent", "with Resend", String::new())
            }
        }
    }
}

/// The full reason for a mail that Failed here or at the gateway.
/// `rf` is Radio+ mail (keyed to a gateway).
pub fn failed_reason(detail: &str, rf: bool) -> String {
    let detail = detail.trim();
    match detail {
        "" => "Failed.".into(),
        gateway::NO_GATEWAY_ACK => {
            "The gateway never acknowledged it. It may not have heard the mail.".into()
        }
        gateway::GATEWAY_NOT_ACCEPTED => "The gateway has it, but the hub refused it.".into(),
        gateway::RADIO_NOT_RUNNING | gateway::TX_STUCK => detail.into(),
        "Hub did not accept the message." => {
            "The hub refused it: Resend did not accept the message.".into()
        }
        // Already a whole sentence from the hub path, Radio+ or not.
        other if other.starts_with("The hub") => other.into(),
        other if rf => format!("The gateway has it, but the hub refused it: {other}"),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(state: &str, delivery: &str) -> MailRow {
        MailRow {
            id: "m1".into(),
            folder: if state == "sent" { "sent" } else { "outbox" }.into(),
            state: state.into(),
            delivery: delivery.into(),
            ts: 1_000,
            ..MailRow::default()
        }
    }

    fn hop(stage: &str, ts: i64) -> Hop {
        Hop {
            stage: stage.into(),
            ts,
        }
    }

    #[test]
    fn each_stage_has_one_chip() {
        let chip =
            |state: &str, delivery: &str| mail_status(&row(state, delivery), &[], 1_010, true);
        assert_eq!(chip("queued", "").label, "Queued");
        assert_eq!(chip("tx", "").label, "On air");
        let keyed = mail_status(&row("tx", ""), &[hop("air", 1), hop("keyed", 9)], 10, true);
        assert_eq!(
            (keyed.label.as_str(), keyed.line.as_str()),
            ("Waiting for gateway", "[..] waiting for gateway")
        );
        assert_eq!(chip("gateway", "").label, "At gateway");
        assert_eq!(chip("sent", "").label, "Sent");
        assert_eq!(chip("sent", "delivered").label, "Delivered");
        assert_eq!(chip("sent", "opened").label, "Delivered");
        assert_eq!(chip("sent", "delivery_delayed").label, "Delayed");
        assert_eq!(chip("sent", "bounced").label, "Bounced");
        assert_eq!(chip("sent", "complained").label, "Spam");
        assert_eq!(chip("sent", "failed").label, "Failed");
        assert_eq!(chip("sent", "unknown").label, "No record");
        assert_eq!(chip("failed", "").label, "Failed");
        assert_eq!(chip("received", "").label, "Received");
        assert_eq!(chip("received", "").mark, "");
    }

    #[test]
    fn text_clients_get_a_bracket_line() {
        let s = mail_status(&row("sent", "delivered"), &[], 1_010, true);
        assert_eq!(s.line, "[ok] delivered");
        let s = mail_status(&row("sent", ""), &[], 1_010, true);
        assert_eq!(s.line, "[ok] with Resend");
        let s = mail_status(&row("tx", ""), &[], 1_010, true);
        assert_eq!(s.line, "[tx] on air");
    }

    #[test]
    fn delivery_that_never_resolves_says_so() {
        let r = row("sent", "sent");
        let day = NO_REPORT_AFTER_SECS;
        assert_eq!(mail_status(&r, &[], 1_000 + 60, true).word, "with Resend");
        assert_eq!(
            mail_status(&r, &[], 1_000 + 60, false).word,
            "delivery unknown (offline)"
        );
        assert_eq!(
            mail_status(&r, &[], 1_000 + day, true).word,
            "no delivery report"
        );
        // The day counts from Resend's acceptance, not from Send.
        let hops = [hop("sent", 5_000)];
        assert_eq!(
            mail_status(&r, &hops, 1_000 + day, true).word,
            "with Resend"
        );
    }

    #[test]
    fn bad_news_from_resend_is_a_no() {
        let mark = |delivery: &str| mail_status(&row("sent", delivery), &[], 1_010, true);
        assert_eq!(mark("bounced").line, "[no] bounced");
        assert_eq!(mark("complained").mark, "[no]");
        assert_eq!(mark("failed").mark, "[no]");
        assert_eq!(mark("canceled").mark, "[no]");
        assert_eq!(mark("delivery_delayed").mark, "[..]");
        assert_eq!(mark("delivered").mark, "[ok]");
    }

    #[test]
    fn inbox_mail_is_always_received() {
        // An older version stored inbound mail with state `sent`.
        let mut r = row("sent", "");
        r.folder = "inbox".into();
        let s = mail_status(&r, &[], 1_000 + NO_REPORT_AFTER_SECS, true);
        assert_eq!((s.label.as_str(), s.mark.as_str()), ("Received", ""));
        assert!(!s.retry && !s.resend);
    }

    #[test]
    fn retry_for_failed_and_resend_for_bounced() {
        let failed = mail_status(&row("failed", ""), &[], 0, true);
        assert!(failed.retry && !failed.resend);
        let bounced = mail_status(&row("sent", "bounced"), &[], 0, true);
        assert!(!bounced.retry && bounced.resend);
        let delivered = mail_status(&row("sent", "delivered"), &[], 0, true);
        assert!(!delivered.retry && !delivered.resend);
    }

    #[test]
    fn failure_reasons_say_where_it_stopped() {
        assert_eq!(
            failed_reason(gateway::NO_GATEWAY_ACK, true),
            "The gateway never acknowledged it. It may not have heard the mail."
        );
        assert_eq!(
            failed_reason(gateway::GATEWAY_NOT_ACCEPTED, true),
            "The gateway has it, but the hub refused it."
        );
        assert_eq!(
            failed_reason("resend 422: invalid to", true),
            "The gateway has it, but the hub refused it: resend 422: invalid to"
        );
        assert_eq!(
            failed_reason("Hub did not accept the message.", false),
            "The hub refused it: Resend did not accept the message."
        );
        assert_eq!(failed_reason("", false), "Failed.");
        let down = "The hub is down. Retry when it is back.";
        assert_eq!(failed_reason(down, true), down);
    }

    #[test]
    fn a_view_reads_back_as_a_plain_row() {
        let view = MailView {
            row: row("sent", "delivered"),
            status: mail_status(&row("sent", "delivered"), &[], 0, true),
            hops: vec![hop("sent", 7)],
        };
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["state"], "sent");
        assert_eq!(json["status"]["label"], "Delivered");
        let back: MailRow = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(back, view.row);
        let again: MailView = serde_json::from_value(json).unwrap();
        assert_eq!(again, view);
    }
}
