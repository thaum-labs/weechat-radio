//! SPDX-License-Identifier: Apache-2.0
//! Keyed rate limits for hub telemetry, WebSocket hello and hub mail.

use governor::clock::DefaultClock;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::Arc;

pub type KeyedLimiter = RateLimiter<String, DashMapStateStore<String>, DefaultClock>;

pub fn per_minute(limit: u32) -> Arc<KeyedLimiter> {
    let q = NonZeroU32::new(limit.max(1)).expect("limit");
    Arc::new(RateLimiter::keyed(Quota::per_minute(q)))
}

/// `limit` per hour, all of it usable at once.
pub fn per_hour(limit: u32) -> Arc<KeyedLimiter> {
    over(limit, 3600)
}

/// `limit` per day, all of it usable at once.
pub fn per_day(limit: u32) -> Arc<KeyedLimiter> {
    over(limit, 86_400)
}

fn over(limit: u32, secs: u64) -> Arc<KeyedLimiter> {
    let n = NonZeroU32::new(limit.max(1)).expect("limit");
    let every = std::time::Duration::from_secs(secs) / n.get();
    let q = Quota::with_period(every).expect("period").allow_burst(n);
    Arc::new(RateLimiter::keyed(q))
}

pub fn allow(lim: &KeyedLimiter, key: &str) -> bool {
    lim.check_key(&key.to_ascii_lowercase()).is_ok()
}
