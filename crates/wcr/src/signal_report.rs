//! SPDX-License-Identifier: Apache-2.0
//! Signal reports: `/report CALL` asks, and the far station answers on its own.
//!
//! Both are plain 1:1 chat lines, so an older station shows the request as
//! text and simply does not answer.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The request line. A station that sees a 1:1 line starting `SNR?` answers.
pub const REQUEST_TEXT: &str = "SNR? Please send me a signal report.";

/// One automatic answer per callsign in this window, so nobody can keep a
/// station transmitting by asking over and over.
pub const ANSWER_EVERY: Duration = Duration::from_secs(5 * 60);

static LAST_ANSWER: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// True when a received 1:1 line asks for a signal report.
pub fn is_request(text: &str) -> bool {
    text.trim_start()
        .get(..4)
        .is_some_and(|head| head.eq_ignore_ascii_case("SNR?"))
}

/// The answer to a request heard over `medium` at `snr` dB.
pub fn answer_text(medium: &str, snr: Option<f32>) -> String {
    match (medium, snr) {
        ("rf", Some(db)) => format!("SNR {db:.0} dB: your request reached me by radio at {db:.0} dB."),
        ("rf", None) => "SNR report: your request reached me by radio, but the modem gave no reading."
            .into(),
        _ => "SNR report: your request reached me over the internet, so there is no radio signal to report."
            .into(),
    }
}

/// True when `call` may get an automatic answer now; records the answer.
pub fn may_answer(call: &str, now: Instant) -> bool {
    let mut g = LAST_ANSWER.lock();
    let map = g.get_or_insert_with(HashMap::new);
    let key = call.to_ascii_uppercase();
    if let Some(last) = map.get(&key) {
        if now.saturating_duration_since(*last) < ANSWER_EVERY {
            return false;
        }
    }
    map.insert(key, now);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_recognised() {
        assert!(is_request(REQUEST_TEXT));
        assert!(is_request("  snr? anyone"));
        assert!(!is_request("SNR -4 dB: your request reached me"));
        assert!(!is_request("hello"));
        assert!(!is_request("SN"));
    }

    #[test]
    fn answers_are_not_requests() {
        // An answer must never trigger another answer.
        for a in [
            answer_text("rf", Some(-4.4)),
            answer_text("rf", None),
            answer_text("inet", None),
        ] {
            assert!(!is_request(&a), "{a}");
        }
        assert!(answer_text("rf", Some(-4.4)).starts_with("SNR -4 dB"));
    }

    #[test]
    fn one_answer_per_call_every_five_minutes() {
        let t0 = Instant::now();
        assert!(may_answer("T3SRA", t0));
        assert!(!may_answer("t3sra", t0 + Duration::from_secs(60)));
        assert!(may_answer("T3SRB", t0 + Duration::from_secs(60)));
        assert!(may_answer("T3SRA", t0 + ANSWER_EVERY));
    }
}
