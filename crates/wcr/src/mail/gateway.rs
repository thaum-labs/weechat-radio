//! SPDX-License-Identifier: Apache-2.0
//! When a gateway may hand mail to the hub. Chat's third-party flag does not apply.

use crate::modes::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendPath {
    /// Pure radio cannot reach Resend.
    Blocked,
    /// Radio-plus always keys to the named gateway. No hub shortcut.
    Rf,
    /// Internet and internet-radio post when the hub is up.
    Hub,
}

pub fn local_send_path(mode: Mode) -> SendPath {
    match mode {
        Mode::Radio => SendPath::Blocked,
        Mode::RadioPlus => SendPath::Rf,
        Mode::Internet | Mode::InternetRadio => SendPath::Hub,
    }
}

/// Inbound RF mail at an internet-radio gateway may be posted even when
/// `[gateway] third_party = deny`. That flag is chat-only.
pub fn gateway_may_post(mode: Mode, third_party_allow: bool) -> bool {
    let _ = third_party_allow;
    matches!(mode, Mode::InternetRadio)
}

/// Mail is Sent only after Resend has accepted it. A hub error is not Sent.
pub fn resend_accepted(http_ok: bool) -> bool {
    http_ok
}

pub const NO_GATEWAY_ACK: &str =
    "Gateway did not acknowledge. The message may not have been heard.";
pub const RADIO_NOT_RUNNING: &str = "Radio is not running.";
pub const GATEWAY_NOT_ACCEPTED: &str = "Gateway has the message, but it was not accepted.";
pub const TX_STUCK: &str = "Timed out while transmitting. The message stayed at tx.";

/// Longer than one hf-poor VOX pass (idle wait, drain, lead, and tail).
pub const TX_STUCK_AFTER_SECS: u64 = 300;

/// Error when a row is still `tx` after `TX_STUCK_AFTER_SECS` and nothing superseded that wait.
pub fn tx_stuck_error(state: &str) -> Option<&'static str> {
    if state == "tx" {
        Some(TX_STUCK)
    } else {
        None
    }
}

/// `max_retries` of 0 means "no cap" for chat. Mail still has to stop.
pub fn mail_attempt_cap(max_retries: u32) -> u32 {
    if max_retries == 0 {
        3
    } else {
        max_retries
    }
}

/// 8s, 16s, 32s, 64s, then 128s. Added after the keyed burst has finished.
pub fn ack_backoff_secs(retries: u32) -> u64 {
    8u64.saturating_mul(1u64 << retries.min(4))
}

/// Wait until the keyed audio should be over, then the backoff, then one
/// frame so the gateway's ack can land before the next key-up.
pub fn retry_after_secs(airtime_secs: f64, spent_secs: f64, retries: u32, frame_secs: f64) -> f64 {
    let remain = (airtime_secs - spent_secs).max(0.0);
    remain + ack_backoff_secs(retries) as f64 + frame_secs.max(0.0)
}

/// Gateway stays quiet this long after the last chunk before it keys an ack.
/// Longer than one frame, so a chunk still on the air resets the wait.
pub fn quiet_gap_secs(frame_secs: f64) -> f64 {
    (frame_secs * 2.0).max(3.0)
}

/// The modem never opened. Say so immediately instead of burning the retries.
pub fn radio_down_text(err: &str) -> Option<&'static str> {
    if err.contains("Radio is not running") {
        Some(RADIO_NOT_RUNNING)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckTimeout {
    /// Key the missing chunks again.
    Retry,
    GiveUp {
        error: &'static str,
    },
}

/// What to do when the ack wait expires. `None` means the row is already finished.
pub fn on_ack_timeout(state: &str, retries: u32, cap: u32) -> Option<AckTimeout> {
    match state {
        "sent" | "failed" => None,
        "gateway" => Some(AckTimeout::GiveUp {
            error: GATEWAY_NOT_ACCEPTED,
        }),
        _ => {
            if retries >= cap {
                Some(AckTimeout::GiveUp {
                    error: NO_GATEWAY_ACK,
                })
            } else {
                Some(AckTimeout::Retry)
            }
        }
    }
}

pub fn check_notice(waiting: usize) -> &'static str {
    if waiting == 0 {
        "no new mail"
    } else {
        ""
    }
}

/// `None` when this CompleteAck means Resend accepted the message.
pub fn failure_text(payload: &str) -> Option<String> {
    let text = payload.trim();
    let Some(rest) = text.strip_prefix("fail") else {
        return None;
    };
    let rest = rest.trim().trim_start_matches(':').trim();
    if rest.is_empty() {
        Some(GATEWAY_NOT_ACCEPTED.to_string())
    } else {
        Some(rest.to_string())
    }
}

pub fn state_mark(state: &str) -> &'static str {
    match state {
        "queued" => "[..]",
        "tx" => "[tx]",
        "gateway" => "[gw]",
        "sent" => "[ok]",
        "failed" => "[no]",
        _ => "    ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_tx_is_not_accepted_and_a_missed_ack_fails() {
        assert_eq!(on_ack_timeout("sent", 0, 3), None);
        assert_eq!(on_ack_timeout("tx", 0, 3), Some(AckTimeout::Retry));
        assert_eq!(on_ack_timeout("tx", 2, 3), Some(AckTimeout::Retry));
        assert_eq!(
            on_ack_timeout("tx", 3, 3),
            Some(AckTimeout::GiveUp {
                error: NO_GATEWAY_ACK
            })
        );
        assert_eq!(
            on_ack_timeout("gateway", 1, 3),
            Some(AckTimeout::GiveUp {
                error: GATEWAY_NOT_ACCEPTED
            })
        );
        assert_eq!(ack_backoff_secs(0), 8);
        assert_eq!(ack_backoff_secs(1), 16);
        assert_eq!(ack_backoff_secs(3), 64);
        assert!((retry_after_secs(22.0, 1.0, 0, 4.0) - 33.0).abs() < 0.01);
        assert!((retry_after_secs(5.0, 9.0, 1, 4.0) - 20.0).abs() < 0.01);
        assert!((quiet_gap_secs(3.6) - 7.2).abs() < 0.01);
        assert!((quiet_gap_secs(1.0) - 3.0).abs() < 0.01);
        assert_eq!(
            radio_down_text("modem: Radio is not running."),
            Some(RADIO_NOT_RUNNING)
        );
        assert_eq!(radio_down_text("Chat is still using the radio."), None);
        assert_eq!(mail_attempt_cap(0), 3);
        assert_eq!(check_notice(0), "no new mail");
        assert_eq!(check_notice(2), "");
        assert!(failure_text("ok").is_none());
        assert_eq!(
            failure_text("fail Hub did not accept the message.").as_deref(),
            Some("Hub did not accept the message.")
        );
        assert_eq!(state_mark("gateway"), "[gw]");
        assert_eq!(state_mark("failed"), "[no]");
        assert_eq!(state_mark("sent"), "[ok]");
        assert_eq!(tx_stuck_error("tx"), Some(TX_STUCK));
        assert_eq!(tx_stuck_error("gateway"), None);
        assert_eq!(tx_stuck_error("sent"), None);
        assert_eq!(TX_STUCK_AFTER_SECS, 300);
    }
}
