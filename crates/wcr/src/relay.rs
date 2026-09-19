//! SPDX-License-Identifier: Apache-2.0
//! Store-and-forward relay: dedupe, TTL, jitter + suppression, hold queue, ACKs.

use crate::error::Result;
use crate::modes::Mode;
use crate::proto::{Envelope, MsgType, Priority};
use crate::store::{Delivery, Store};
use rand::Rng;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct RelayDecision {
    pub action: Action,
    pub delay_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// First time we see it: deliver locally and maybe rebroadcast.
    Accept,
    /// Duplicate heard while we were waiting to TX: suppress our relay.
    Suppress,
    /// Duplicate after we already handled it: ignore.
    Ignore,
    /// hops_left is 0: deliver locally if dest matches, do not relay.
    DropRelay,
}

impl Action {
    /// First-seen frames get IRC, ACKs, and gateway work. Duplicates do not.
    pub fn applies_local_effects(self) -> bool {
        matches!(self, Self::Accept | Self::DropRelay)
    }
}

pub fn decide(store: &Store, env: &Envelope, we_are_waiting: bool) -> Result<RelayDecision> {
    let seen = store.seen_before(&env.msg_id)?;
    if seen {
        if we_are_waiting {
            return Ok(RelayDecision {
                action: Action::Suppress,
                delay_ms: 0,
            });
        }
        return Ok(RelayDecision {
            action: Action::Ignore,
            delay_ms: 0,
        });
    }
    if env.hops_left == 0 {
        return Ok(RelayDecision {
            action: Action::DropRelay,
            delay_ms: 0,
        });
    }
    let delay = jitter_ms(env.priority());
    Ok(RelayDecision {
        action: Action::Accept,
        delay_ms: delay,
    })
}

pub fn jitter_ms(priority: Priority) -> u64 {
    let range = priority.jitter_ms();
    rand::thread_rng().gen_range(range)
}

pub fn next_retry_hold_base(retries: u32, now: u32, priority: Priority) -> u32 {
    let base: u32 = match priority {
        Priority::Emergency => 8,
        Priority::Priority => 20,
        Priority::Routine => 45,
    };
    let delay = base.saturating_mul(1u32 << retries.min(4));
    now.saturating_add(delay)
}

/// ARQ hold with ±25% jitter so colliding stations do not retry in lockstep.
pub fn next_retry_hold(retries: u32, now: u32, priority: Priority) -> u32 {
    next_retry_hold_jittered(retries, now, priority, true)
}

pub fn next_retry_hold_jittered(retries: u32, now: u32, priority: Priority, jitter: bool) -> u32 {
    let target = next_retry_hold_base(retries, 0, priority);
    let delay = if jitter {
        let factor = rand::thread_rng().gen_range(0.75f64..=1.25);
        ((target as f64) * factor).round() as u32
    } else {
        target
    };
    now.saturating_add(delay)
}

/// Should this node put an internet-originated frame on RF?
pub fn may_rf_egress(
    mode_is_gateway: bool,
    rf_egress: bool,
    dest_recently_heard: bool,
    third_party: bool,
    third_party_allow: bool,
    inet_ok: bool,
    no_inet: bool,
) -> bool {
    if !mode_is_gateway || !rf_egress {
        return false;
    }
    if no_inet || !inet_ok {
        return false;
    }
    if third_party && !third_party_allow {
        return false;
    }
    dest_recently_heard
}

/// Should this node forward an RF frame to the internet?
/// `mode_is_gateway` is the live mode's [`Mode::is_gateway`] — leftover radio
/// after switching to `internet` must not upload overheard RF.
pub fn may_inet_forward(mode_is_gateway: bool, inet_ok: bool, no_inet: bool) -> bool {
    mode_is_gateway && inet_ok && !no_inet
}

/// In `internet-radio`, offer the hub only when RF will not reach the dest.
pub fn needs_inet_gap(
    mode: Mode,
    is_group: bool,
    is_bulletin: bool,
    dest_heard_rf_on_dial: bool,
    group_member_heard_rf_on_dial: bool,
) -> bool {
    if !mode.uses_internet() {
        return false;
    }
    if !mode.uses_radio() {
        return true;
    }
    if is_bulletin {
        return true;
    }
    if is_group {
        return !group_member_heard_rf_on_dial;
    }
    !dest_heard_rf_on_dial
}

/// Gateway RF-egress "heard" gate: dest on this dial over RF, or a group member,
/// or (for bulletin) any RF hear on this dial.
pub fn rf_egress_heard(
    is_group: bool,
    is_bulletin: bool,
    dest_heard_rf_on_dial: bool,
    group_member_heard_rf_on_dial: bool,
    any_rf_hear_on_dial: bool,
) -> bool {
    if is_bulletin {
        return any_rf_hear_on_dial;
    }
    if is_group {
        return group_member_heard_rf_on_dial;
    }
    dest_heard_rf_on_dial
}

#[derive(Clone)]
pub struct Engine {
    pub store: Arc<Store>,
    pub our_call: String,
}

impl Engine {
    pub fn new(store: Arc<Store>, our_call: String) -> Self {
        Self { store, our_call }
    }

    pub fn on_rx(
        &self,
        env: &Envelope,
        medium: &str,
        snr: Option<f32>,
        freq_khz: Option<u32>,
    ) -> Result<RelayDecision> {
        self.store.heard_touch(
            env.origin.as_str(),
            snr,
            None,
            env.kind == MsgType::Beacon,
            medium,
            freq_khz,
        )?;
        self.store
            .add_hop(&env.msg_id, env.origin.as_str(), medium, snr)?;
        let waiting = self
            .store
            .get(&env.msg_id)?
            .map(|m| m.delivery == Delivery::Queued || m.delivery == Delivery::Sent)
            .unwrap_or(false);
        let decision = decide(&self.store, env, waiting)?;
        match decision.action {
            Action::Accept => {
                let mut stored = env.clone();
                if stored.hops_left > 0 {
                    stored.hops_left -= 1;
                }
                self.store.insert(&stored, Delivery::Queued)?;
                if stored.kind == MsgType::Ack {
                    if let Some(id) = stored.acked_id() {
                        self.store.set_delivery(&id, Delivery::Delivered)?;
                    }
                }
                if stored.kind == MsgType::Msg && stored.dest.as_str() == self.our_call {
                    // local delivery; ACK is produced by the node runtime
                }
                let now = crate::proto::now_ts();
                if stored.hops_left > 0 && stored.origin.as_str() != self.our_call {
                    self.store.set_hold(
                        &stored.msg_id,
                        now + (decision.delay_ms / 1000) as u32,
                        stored.hops_left,
                    )?;
                }
            }
            Action::Suppress => {
                self.store.suppress(&env.msg_id)?;
                self.store.set_delivery(&env.msg_id, Delivery::Relayed)?;
            }
            Action::Ignore => {}
            Action::DropRelay => {
                let _ = self.store.insert(env, Delivery::Queued);
            }
        }
        Ok(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Callsign, Envelope, Flags};

    #[test]
    fn ttl_and_dedupe() {
        let store = Store::open_memory().unwrap();
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"ping".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        let d1 = decide(&store, &env, false).unwrap();
        assert_eq!(d1.action, Action::Accept);
        store.insert(&env, Delivery::Queued).unwrap();
        let d2 = decide(&store, &env, false).unwrap();
        assert_eq!(d2.action, Action::Ignore);
        let d3 = decide(&store, &env, true).unwrap();
        assert_eq!(d3.action, Action::Suppress);
    }

    #[test]
    fn local_effects_only_on_first_seen() {
        assert!(Action::Accept.applies_local_effects());
        assert!(Action::DropRelay.applies_local_effects());
        assert!(!Action::Ignore.applies_local_effects());
        assert!(!Action::Suppress.applies_local_effects());
    }

    #[test]
    fn ignore_does_not_insert_again() {
        let store = Store::open_memory().unwrap();
        let engine = Engine::new(std::sync::Arc::new(store), "M0XYZ".into());
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"ping".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        let d1 = engine.on_rx(&env, "rf", None, None).unwrap();
        assert_eq!(d1.action, Action::Accept);
        assert!(d1.action.applies_local_effects());
        assert_eq!(engine.store.hear_count(&env.msg_id).unwrap(), 1);
        let d2 = engine.on_rx(&env, "rf", None, None).unwrap();
        assert_eq!(d2.action, Action::Suppress);
        assert!(!d2.action.applies_local_effects());
        assert_eq!(engine.store.hear_count(&env.msg_id).unwrap(), 1);
        engine
            .store
            .set_delivery(&env.msg_id, Delivery::Relayed)
            .unwrap();
        let d3 = engine.on_rx(&env, "rf", None, None).unwrap();
        assert_eq!(d3.action, Action::Ignore);
        assert!(!d3.action.applies_local_effects());
        assert_eq!(engine.store.hear_count(&env.msg_id).unwrap(), 1);
        assert_eq!(
            engine.store.delivery_of(&env.msg_id).unwrap(),
            Some(Delivery::Relayed)
        );
    }

    #[test]
    fn drop_relay_still_stores_first_seen() {
        let store = Store::open_memory().unwrap();
        let engine = Engine::new(std::sync::Arc::new(store), "M0XYZ".into());
        let mut env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"late".to_vec(),
            1,
            Flags::new(),
        )
        .unwrap();
        env.hops_left = 0;
        let d = engine.on_rx(&env, "rf", None, None).unwrap();
        assert_eq!(d.action, Action::DropRelay);
        assert!(d.action.applies_local_effects());
        assert!(engine.store.get(&env.msg_id).unwrap().is_some());
    }

    #[test]
    fn gateway_rules() {
        assert!(may_rf_egress(true, true, true, false, false, true, false));
        assert!(!may_rf_egress(true, true, true, true, false, true, false));
        assert!(may_rf_egress(true, true, true, true, true, true, false));
        assert!(!may_rf_egress(true, true, true, false, false, true, true));
        assert!(!may_inet_forward(true, true, true));
        assert!(may_inet_forward(true, true, false));
        assert!(!may_inet_forward(false, true, false));
    }

    #[test]
    fn inet_gap_is_hub_only_when_rf_cannot_reach() {
        use crate::modes::Mode;
        assert!(needs_inet_gap(Mode::Internet, false, false, true, false));
        assert!(!needs_inet_gap(Mode::Radio, false, false, false, false));
        assert!(!needs_inet_gap(Mode::RadioPlus, false, false, false, false));
        assert!(!needs_inet_gap(
            Mode::InternetRadio,
            false,
            false,
            true,
            false
        ));
        assert!(needs_inet_gap(
            Mode::InternetRadio,
            false,
            false,
            false,
            false
        ));
        assert!(!needs_inet_gap(
            Mode::InternetRadio,
            true,
            false,
            false,
            true
        ));
        assert!(needs_inet_gap(
            Mode::InternetRadio,
            true,
            false,
            false,
            false
        ));
        assert!(needs_inet_gap(Mode::InternetRadio, true, true, true, true));
    }

    #[test]
    fn rf_egress_heard_requires_dial_members() {
        assert!(rf_egress_heard(false, false, true, false, false));
        assert!(!rf_egress_heard(false, false, false, true, true));
        assert!(rf_egress_heard(true, false, false, true, false));
        assert!(!rf_egress_heard(true, false, true, false, true));
        assert!(rf_egress_heard(true, true, false, false, true));
        assert!(!rf_egress_heard(true, true, true, true, false));
    }

    #[test]
    fn retry_hold_is_monotonic_without_jitter() {
        let a = next_retry_hold_base(0, 0, Priority::Routine);
        let b = next_retry_hold_base(1, 0, Priority::Routine);
        let c = next_retry_hold_base(2, 0, Priority::Routine);
        assert!(b > a && c > b);
        assert_eq!(a, 45);
        assert_eq!(b, 90);
    }

    #[test]
    fn retry_hold_jitter_stays_in_bounds() {
        let now = 1_000u32;
        for retries in 0..5u32 {
            let base = next_retry_hold_base(retries, 0, Priority::Routine);
            let lo = ((base as f64) * 0.75).floor() as u32;
            let hi = ((base as f64) * 1.25).ceil() as u32;
            for _ in 0..40 {
                let t = next_retry_hold(retries, now, Priority::Routine);
                let delay = t.saturating_sub(now);
                assert!(
                    delay >= lo && delay <= hi,
                    "retry {retries}: delay {delay} not in {lo}..={hi}"
                );
            }
        }
        assert_eq!(
            next_retry_hold_jittered(0, 100, Priority::Emergency, false),
            108
        );
    }
}
