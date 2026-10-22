//! SPDX-License-Identifier: Apache-2.0
//! The Email tab's send progress and Resend's delivery word.
//!
//! Progress follows the real stages of a send, not a timer: hub mail is
//! handed over, then accepted by Resend, then delivered. Radio+ mail is keyed
//! (the only part estimated from airtime), its chunks are acknowledged by the
//! gateway, the gateway hands it to the hub, Resend accepts it, and Resend
//! reports delivery.

/// How long the send window waits for Resend's delivery report after Sent.
pub const DELIVERY_WAIT_SECS: f64 = 30.0;

/// Resend's `last_event` values that will not change again, plus `unknown`
/// (the hub has no record of the mail, for example an older gateway sent it).
pub fn delivery_final(delivery: &str) -> bool {
    matches!(
        delivery,
        "delivered"
            | "opened"
            | "clicked"
            | "bounced"
            | "complained"
            | "failed"
            | "canceled"
            | "unknown"
    )
}

/// Resend reported the mail reached the recipient's mail server.
pub fn delivery_ok(delivery: &str) -> bool {
    matches!(delivery, "delivered" | "opened" | "clicked")
}

/// Plain words for the Email tab. Empty until the hub has been asked.
pub fn delivery_word(delivery: &str) -> &'static str {
    match delivery {
        "" => "",
        "delivered" | "opened" | "clicked" => "delivered",
        "sent" | "queued" | "scheduled" => "with Resend",
        "delivery_delayed" => "delayed",
        "bounced" => "bounced",
        "complained" => "marked as spam",
        "failed" => "Resend failed to send it",
        "canceled" => "canceled",
        "unknown" => "no delivery record",
        _ => "with Resend",
    }
}

/// When the station asks the hub again. Young mail often, then less, and
/// not at all after a day or once Resend's answer is final.
pub fn delivery_check_due(age_secs: i64, since_check_secs: i64, delivery: &str) -> bool {
    if delivery_final(delivery) {
        return false;
    }
    let every = if age_secs < 120 {
        10
    } else if age_secs < 30 * 60 {
        60
    } else if age_secs < 24 * 60 * 60 {
        15 * 60
    } else {
        return false;
    };
    since_check_secs >= every
}

/// One mail as the send window sees it.
pub struct SendView<'a> {
    pub state: &'a str,
    pub chunks: u32,
    pub got: &'a str,
    pub delivery: &'a str,
    pub detail: &'a str,
    /// Radio+ (chunks over the air to a gateway) rather than straight to the hub.
    pub rf: bool,
    /// Seconds since Send was pressed.
    pub elapsed_secs: f64,
    /// Planned airtime of the first burst, from the compose hint.
    pub airtime_secs: f64,
    /// Seconds since the row turned Sent, when it has.
    pub since_sent_secs: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendStep {
    /// 0.0 to 1.0.
    pub fraction: f32,
    pub label: String,
    /// The window may close: Sent and settled, or Failed.
    pub done: bool,
    /// Sent (Resend accepted it). Delivery is reported separately.
    pub ok: bool,
}

/// The Radio+ Get window: asking, chunks received H of T, fetched, or why
/// it stopped. The caller keeps the bar from going back.
pub fn get_step(p: &super::GetProgress) -> SendStep {
    let step = |fraction: f32, label: String, done: bool, ok: bool| SendStep {
        fraction,
        label,
        done,
        ok,
    };
    match p.state.as_str() {
        "asking" => step(0.1, "Asking the gateway…".into(), false, false),
        "receiving" => {
            let part = if p.total == 0 {
                0.0
            } else {
                p.got as f32 / p.total as f32
            };
            step(
                0.1 + 0.85 * part,
                format!("Chunks received {} of {}", p.got, p.total),
                false,
                false,
            )
        }
        "fetched" => step(1.0, "Fetched. It is in your Inbox.".into(), true, true),
        "failed" => {
            let why = if p.detail.is_empty() {
                "No reply from the gateway."
            } else {
                p.detail.as_str()
            };
            step(1.0, why.to_string(), true, false)
        }
        _ => step(0.0, "Asking the gateway…".into(), false, false),
    }
}

pub fn send_step(v: &SendView<'_>) -> SendStep {
    let step = |fraction: f32, label: String, done: bool, ok: bool| SendStep {
        fraction,
        label,
        done,
        ok,
    };
    match v.state {
        "failed" => {
            let why = if v.detail.is_empty() {
                "Failed."
            } else {
                v.detail
            };
            step(1.0, why.to_string(), true, false)
        }
        "sent" => {
            if delivery_ok(v.delivery) {
                return step(1.0, "Success: delivered.".into(), true, true);
            }
            if delivery_final(v.delivery) {
                let label = format!("Sent, but {}.", delivery_word(v.delivery));
                return step(1.0, label, true, true);
            }
            if v.since_sent_secs >= DELIVERY_WAIT_SECS {
                let label = "Success: sent. Resend has not confirmed delivery yet; the Sent folder will show it.";
                return step(1.0, label.into(), true, true);
            }
            let mut label = "Sent. Resend accepted it; checking delivery".to_string();
            if v.delivery == "delivery_delayed" {
                label.push_str(" (Resend says delayed)");
            }
            label.push('…');
            let wait = (v.since_sent_secs / DELIVERY_WAIT_SECS).clamp(0.0, 1.0) as f32;
            step(0.9 + 0.09 * wait, label, false, true)
        }
        "gateway" => step(
            0.8,
            "The gateway has every chunk and is handing it to the hub…".into(),
            false,
            false,
        ),
        _ if !v.rf => step(0.3, "Handing it to the hub…".into(), false, false),
        "tx" | "queued" => {
            let have = got_count(v.got);
            if v.chunks > 0 && have > 0 {
                let part = (have as f32 / v.chunks as f32).min(1.0);
                return step(
                    0.5 + 0.3 * part,
                    format!("The gateway has {have} of {} chunks…", v.chunks),
                    false,
                    false,
                );
            }
            if v.state == "queued" {
                return step(0.02, "Waiting for the radio…".into(), false, false);
            }
            let air = v.airtime_secs.max(1.0);
            let part = (v.elapsed_secs / air).clamp(0.0, 1.0) as f32;
            let label = if part < 1.0 {
                let left = (air - v.elapsed_secs).max(0.0).ceil();
                format!("On the air, about {left:.0} s left…")
            } else {
                "Keyed. Waiting for the gateway to answer…".into()
            };
            step(0.05 + 0.45 * part, label, false, false)
        }
        _ => step(0.02, "Waiting for the radio…".into(), false, false),
    }
}

fn got_count(got: &str) -> u32 {
    got.split(',')
        .filter(|s| s.trim().parse::<u16>().is_ok())
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_window_walks_asking_chunks_fetched() {
        let mut p = super::super::GetProgress {
            state: "asking".into(),
            ..Default::default()
        };
        let a = get_step(&p);
        assert_eq!(a.label, "Asking the gateway…");
        assert!(!a.done);
        p.state = "receiving".into();
        p.got = 3;
        p.total = 6;
        let r = get_step(&p);
        assert_eq!(r.label, "Chunks received 3 of 6");
        assert!(r.fraction > a.fraction && !r.done);
        p.state = "fetched".into();
        let f = get_step(&p);
        assert!(f.done && f.ok);
        p.state = "failed".into();
        p.detail = String::new();
        let x = get_step(&p);
        assert!(x.done && !x.ok);
        assert_eq!(x.label, "No reply from the gateway.");
    }

    fn view<'a>(state: &'a str, got: &'a str, delivery: &'a str) -> SendView<'a> {
        SendView {
            state,
            chunks: 4,
            got,
            delivery,
            detail: "",
            rf: true,
            elapsed_secs: 10.0,
            airtime_secs: 20.0,
            since_sent_secs: 0.0,
        }
    }

    #[test]
    fn radio_plus_progress_moves_forward_through_the_real_stages() {
        let order = [
            send_step(&view("queued", "", "")),
            send_step(&view("tx", "", "")),
            send_step(&view("tx", "0,1", "")),
            send_step(&view("gateway", "0,1,2,3", "")),
            send_step(&view("sent", "0,1,2,3", "sent")),
            send_step(&view("sent", "0,1,2,3", "delivered")),
        ];
        for pair in order.windows(2) {
            assert!(pair[0].fraction < pair[1].fraction, "{pair:?}");
        }
        assert_eq!(order[2].label, "The gateway has 2 of 4 chunks…");
        assert!(order[5].done && order[5].ok);
        assert_eq!(order[5].label, "Success: delivered.");
        assert!(!order[4].done);
    }

    #[test]
    fn a_send_without_a_delivery_report_still_ends_as_sent() {
        let mut v = view("sent", "", "sent");
        v.since_sent_secs = DELIVERY_WAIT_SECS;
        let s = send_step(&v);
        assert!(s.done && s.ok);
        assert!(s.label.starts_with("Success: sent."));
    }

    #[test]
    fn a_bounce_and_a_failure_close_the_window_with_the_reason() {
        let s = send_step(&view("sent", "", "bounced"));
        assert!(s.done);
        assert_eq!(s.label, "Sent, but bounced.");
        let mut f = view("failed", "", "");
        f.detail = "Gateway did not acknowledge.";
        let s = send_step(&f);
        assert!(s.done && !s.ok);
        assert_eq!(s.label, "Gateway did not acknowledge.");
    }

    #[test]
    fn delivery_checks_slow_down_and_stop() {
        assert!(delivery_check_due(5, 10, ""));
        assert!(!delivery_check_due(5, 9, "sent"));
        assert!(delivery_check_due(600, 60, "sent"));
        assert!(!delivery_check_due(600, 30, "sent"));
        assert!(delivery_check_due(3600, 900, "delivery_delayed"));
        assert!(!delivery_check_due(25 * 3600, 99_999, "sent"));
        assert!(!delivery_check_due(5, 99_999, "delivered"));
        assert!(!delivery_check_due(5, 99_999, "unknown"));
    }
}
