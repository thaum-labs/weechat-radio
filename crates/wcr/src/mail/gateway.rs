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

/// A 48-chunk hf-poor mail keys for over three minutes. The stuck timer
/// scales with the planned airtime so a long burst is not failed mid-key.
pub fn tx_stuck_after_secs(airtime_secs: f64) -> u64 {
    let scaled = (airtime_secs.max(0.0) * 3.0).ceil() as u64 + 60;
    scaled.max(TX_STUCK_AFTER_SECS)
}

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

/// `try 1/4` .. `try 4/4` when the cap is 3: the first key plus that many retries.
/// `retries` is the 0-based count stored on the row at key time.
pub fn mail_try_label(retries: u32, cap: u32) -> String {
    format!(
        "try {}/{}",
        retries.saturating_add(1),
        cap.saturating_add(1)
    )
}

/// A partial "has N/M" ack starts another send only after this burst has
/// finished keying. Chunks still on the air are not missing yet.
pub fn progress_ack_retries(during_burst: bool, have: u32, total: u32) -> bool {
    !during_burst && total > 0 && have < total
}

/// 8s, 16s, 32s, 64s, then 128s. Added after the keyed burst has finished.
pub fn ack_backoff_secs(retries: u32) -> u64 {
    8u64.saturating_mul(1u64 << retries.min(4))
}

/// The gateway's hub post and its own radio lease, between the end of its
/// quiet gap and its reply keying.
pub const REPLY_SETUP_SECS: f64 = 6.0;

/// Wait until the keyed audio should be over, then the gateway's quiet gap,
/// its hub post and lease, and one reply frame, then the backoff. A retry
/// that keys before the reply lands leaves the sender deaf to it.
pub fn retry_after_secs(airtime_secs: f64, spent_secs: f64, retries: u32, frame_secs: f64) -> f64 {
    let remain = (airtime_secs - spent_secs).max(0.0);
    let frame = frame_secs.max(0.0);
    remain + quiet_gap_secs(frame) + REPLY_SETUP_SECS + frame + ack_backoff_secs(retries) as f64
}

/// Gateway stays quiet this long after the last chunk before it keys an ack.
/// Longer than one frame, so a chunk still on the air resets the wait.
pub fn quiet_gap_secs(frame_secs: f64) -> f64 {
    (frame_secs * 2.0).max(3.0)
}

pub const MAX_QUIET_GAP_SECS: f64 = 30.0;

/// A retry on a slower rung spaces its frames wider than the ladder-start
/// frame. Wait half as long again as the spacing actually heard, so the
/// gateway does not key between two chunks of one burst.
pub fn quiet_gap_heard_secs(frame_secs: f64, heard_spacing_secs: f64) -> f64 {
    quiet_gap_secs(frame_secs)
        .max(heard_spacing_secs * 1.5)
        .min(MAX_QUIET_GAP_SECS)
}

/// Longest the gateway waits for the rest of a burst it has heard part of.
pub const MAX_TAIL_WAIT_SECS: f64 = 180.0;

/// Time the rest of a burst would take: the chunks past the highest index
/// heard, at the spacing heard. Until then the sender may still be keying
/// them, and a have-report keyed now lands on top of them.
pub fn tail_wait_secs(count: u16, highest: u16, spacing_secs: f64) -> f64 {
    let left = count.saturating_sub(highest.saturating_add(1));
    (f64::from(left) * spacing_secs).min(MAX_TAIL_WAIT_SECS)
}

/// A Radio+ sender holds its burst this long after another station's mail
/// chunk: the rest of that burst, the gateway's quiet gap, its hub post and
/// its reply. Keying sooner lands on top of them at the gateway.
pub fn busy_hold_secs(frame_secs: f64) -> f64 {
    let frame = frame_secs.max(0.0);
    quiet_gap_secs(frame) + REPLY_SETUP_SECS + frame
}

/// A gateway hearing chunk `idx` of `count` keys no chat or relay for this
/// long: the rest of the burst, then the same hold a sender keeps.
pub fn heard_mail_hold_secs(count: u16, idx: u16, frame_secs: f64) -> f64 {
    tail_wait_secs(count, idx, frame_secs) + busy_hold_secs(frame_secs)
}

/// Longest a sender holds one mail for other stations' mail.
pub const MAX_BUSY_HOLD_SECS: f64 = MAX_TAIL_WAIT_SECS;

/// Seconds left to hold, or `None` to key now. `since_heard` is the time
/// since another station's mail chunk, `held_for` how long this mail has
/// held already.
pub fn busy_hold_left(since_heard: Option<f64>, held_for: f64, frame_secs: f64) -> Option<f64> {
    let left = busy_hold_secs(frame_secs) - since_heard?;
    (left > 0.0 && held_for < MAX_BUSY_HOLD_SECS).then_some(left)
}

/// A retry with no gateway progress at all. More than this many chunks
/// still missing, and the sender keys one chunk to ask what the gateway
/// holds instead of the whole mail again.
pub const PROBE_MIN_MISSING: usize = 4;

/// The gateway answers one chunk with its acceptance, or a have-report the
/// sender resends from. Nothing heard back keeps the full first try's cost
/// to one frame per retry.
pub fn probe_only(retries: u32, gateway_reported: bool, missing: usize) -> bool {
    retries > 0 && !gateway_reported && missing > PROBE_MIN_MISSING
}

/// How many times the gateway holds its reply because a frame is still
/// being received when the quiet gap ends.
pub const MAX_QUIET_DEFERS: u8 = 4;

/// Hold the reply while a chunk is on the air. Keying over it deafens the
/// gateway to that chunk. Bounded, so a busy channel cannot hold it forever.
pub fn quiet_defers(receiving: bool, defers: u8) -> bool {
    receiving && defers < MAX_QUIET_DEFERS
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

/// Accepted or given up. Nothing is keyed for it again.
pub fn is_finished(state: &str) -> bool {
    matches!(state, "sent" | "failed")
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

/// The Failed reason when posting to the hub went wrong: the hub's own
/// error when it answered (`hub mail 502: {"error":"…"}`), else unreachable.
pub fn hub_send_error(err: &str) -> String {
    let Some(rest) = err.strip_prefix("hub mail ") else {
        return "The hub could not be reached. Retry when it is back.".into();
    };
    let (_, text) = rest.split_once(": ").unwrap_or(("", rest));
    let why = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_else(|| text.trim().to_string());
    if why.is_empty() {
        "The hub refused it.".into()
    } else {
        format!("The hub refused it: {why}")
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
    fn a_hub_send_error_names_the_hubs_reason() {
        assert_eq!(
            hub_send_error(r#"hub mail 502: {"error":"resend: invalid key"}"#),
            "The hub refused it: resend: invalid key"
        );
        assert_eq!(
            hub_send_error("hub mail 400: bad"),
            "The hub refused it: bad"
        );
        assert_eq!(
            hub_send_error("error sending request"),
            "The hub could not be reached. Retry when it is back."
        );
    }

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
        assert!(!progress_ack_retries(true, 24, 48));
        assert!(progress_ack_retries(false, 24, 48));
        assert!(!progress_ack_retries(false, 48, 48));
        assert!(!progress_ack_retries(false, 0, 0));
        assert_eq!(ack_backoff_secs(0), 8);
        assert_eq!(ack_backoff_secs(1), 16);
        assert_eq!(ack_backoff_secs(3), 64);
        assert!((retry_after_secs(22.0, 1.0, 0, 4.0) - 47.0).abs() < 0.01);
        assert!((retry_after_secs(5.0, 9.0, 1, 4.0) - 34.0).abs() < 0.01);
        assert!((quiet_gap_secs(3.6) - 7.2).abs() < 0.01);
        assert!((quiet_gap_secs(1.0) - 3.0).abs() < 0.01);
        // Chunk 1 of 3 heard: two more chunks, then the sender's hold.
        assert!((heard_mail_hold_secs(3, 0, 4.0) - (8.0 + busy_hold_secs(4.0))).abs() < 0.01);
        assert!((heard_mail_hold_secs(3, 2, 4.0) - busy_hold_secs(4.0)).abs() < 0.01);
        assert!((quiet_gap_heard_secs(4.0, 0.0) - 8.0).abs() < 0.01);
        assert!((quiet_gap_heard_secs(4.0, 7.5) - 11.25).abs() < 0.01);
        assert!((quiet_gap_heard_secs(4.0, 60.0) - MAX_QUIET_GAP_SECS).abs() < 0.01);
        assert_eq!(
            radio_down_text("modem: Radio is not running."),
            Some(RADIO_NOT_RUNNING)
        );
        assert_eq!(radio_down_text("Chat is still using the radio."), None);
        assert_eq!(mail_attempt_cap(0), 3);
        assert_eq!(mail_try_label(0, 3), "try 1/4");
        assert_eq!(mail_try_label(3, 3), "try 4/4");
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

    #[test]
    fn a_long_burst_is_not_called_stuck_while_it_keys() {
        assert_eq!(tx_stuck_after_secs(4.0), TX_STUCK_AFTER_SECS);
        assert_eq!(tx_stuck_after_secs(0.0), TX_STUCK_AFTER_SECS);
        let long = 200.0;
        assert!(tx_stuck_after_secs(long) as f64 > long * 3.0);
        assert_eq!(tx_stuck_after_secs(200.0), 660);
    }

    /// v0.1.89 big mail: the gateway's ack keyed 9 s after the last chunk,
    /// the sender's retry 13 s after, and the sender keyed over the ack.
    #[test]
    fn the_sender_waits_out_the_gateway_reply_before_a_retry() {
        let frame = 4.0;
        let gateway_reply = quiet_gap_secs(frame) + REPLY_SETUP_SECS + frame;
        for retries in 0..4 {
            assert!(retry_after_secs(0.0, 0.0, retries, frame) > gateway_reply);
        }
        let slow_rung = quiet_gap_heard_secs(frame, 7.5) + REPLY_SETUP_SECS + 7.5;
        assert!(retry_after_secs(0.0, 0.0, 1, frame) > slow_rung);
    }

    #[test]
    fn a_retry_with_no_gateway_progress_keys_one_probe() {
        assert!(!probe_only(0, false, 56));
        assert!(probe_only(1, false, 56));
        assert!(!probe_only(1, true, 56));
        assert!(!probe_only(2, false, PROBE_MIN_MISSING));
        assert!(probe_only(2, false, PROBE_MIN_MISSING + 1));
    }

    #[test]
    fn a_sender_holds_while_another_station_sends_mail() {
        let hold = busy_hold_secs(4.0);
        assert_eq!(hold, 8.0 + REPLY_SETUP_SECS + 4.0);
        assert_eq!(busy_hold_left(None, 0.0, 4.0), None);
        assert_eq!(busy_hold_left(Some(hold), 0.0, 4.0), None);
        assert_eq!(busy_hold_left(Some(2.0), 0.0, 4.0), Some(hold - 2.0));
        // A channel that never goes quiet does not hold the mail forever.
        assert_eq!(busy_hold_left(Some(2.0), MAX_BUSY_HOLD_SECS, 4.0), None);
    }

    #[test]
    fn the_gateway_holds_its_reply_while_a_chunk_is_on_the_air() {
        assert_eq!(tail_wait_secs(56, 55, 4.0), 0.0, "the tail was heard");
        assert_eq!(
            tail_wait_secs(48, 46, 2.0),
            2.0,
            "only the last chunk is due"
        );
        assert_eq!(tail_wait_secs(56, 32, 4.0), 92.0, "23 chunks still due");
        assert_eq!(tail_wait_secs(500, 0, 4.0), MAX_TAIL_WAIT_SECS);
        assert!(quiet_defers(true, 0));
        assert!(quiet_defers(true, MAX_QUIET_DEFERS - 1));
        assert!(!quiet_defers(true, MAX_QUIET_DEFERS));
        assert!(!quiet_defers(false, 0));
    }
}
