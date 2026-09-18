//! SPDX-License-Identifier: Apache-2.0
//! Keyed rate limits for hub telemetry and WebSocket hello.

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

pub fn allow(lim: &KeyedLimiter, key: &str) -> bool {
    lim.check_key(&key.to_ascii_lowercase()).is_ok()
}
