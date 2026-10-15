//! SPDX-License-Identifier: Apache-2.0
//! Store-and-forward relay: dedupe, TTL, jitter + suppression, hold queue, ACKs.

use crate::error::Result;
use crate::modes::Mode;
use crate::presets::{Preset, Rung};
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

/// Whether this station should key `env` again for someone else.
///
/// A frame addressed to us stays here (the local client and the ACK path
/// handle it). A message that already reached `[ok]` or `[all]` is not
/// relayed again. An ACK toward another station still is: that is how a
/// receipt gets back through a middle station.
/// A second copy on RF means someone already relayed it, so cancel ours.
/// A hub or LAN echo of a closed group is the upload coming back. It must
/// not cancel the RF copy for members who only hear this gateway.
pub fn duplicate_cancels_relay(medium: &str, group: bool, bulletin: bool) -> bool {
    if group && !bulletin && !medium.eq_ignore_ascii_case("rf") {
        return false;
    }
    true
}

pub fn may_rebroadcast(our_call: &str, env: &Envelope, delivery: Option<Delivery>) -> bool {
    if env.hops_left == 0 || env.origin.as_str().eq_ignore_ascii_case(our_call) {
        return false;
    }
    if env.dest.as_str().eq_ignore_ascii_case(our_call) {
        return false;
    }
    if env.kind != MsgType::Ack {
        if matches!(delivery, Some(Delivery::All)) {
            return false;
        }
        // One member ACK is [ok], not done. Keep the RF copy until [all],
        // or a station that only hears this gateway never sees the line.
        let closed = env.flags.group() && !crate::slash::is_bulletin(env.dest.as_str());
        if matches!(delivery, Some(Delivery::Delivered)) && !closed {
            return false;
        }
    }
    true
}

/// Whether an ACK for `original` should mark our stored copy `[ok]`.
/// A relay's copy of someone else's bulletin stays open: any neighbour's ACK
/// says nothing about the stations further along, and `[ok]` would stop
/// this relay from carrying it on.
pub fn ack_marks_delivered(our_call: &str, original: &Envelope) -> bool {
    is_own_origin(our_call, original) || !bulletin_keeps_walking(original)
}

/// An ACK from the DM's addressee means the line arrived. An ACK for a
/// bulletin or group says only that one station got it, and must not stop a
/// relay from carrying the line on to the others.
pub fn ack_ends_relay(ack_origin: &str, original: Option<&Envelope>) -> bool {
    original.is_some_and(|o| !o.flags.group() && o.dest.as_str().eq_ignore_ascii_case(ack_origin))
}

/// The hold pump and the ARQ loop share one table. A row we originated is a
/// retry, not a relay: `may_rebroadcast` is false for it, and the pump must
/// leave the hold alone so the retry timer still fires.
pub fn is_own_origin(our_call: &str, env: &Envelope) -> bool {
    env.origin.as_str().eq_ignore_ascii_case(our_call)
}

/// Minimum gap before a duplicate of an unacked frame is answered or relayed
/// again. Short enough that an emergency retry (8 s) still gets a new ACK,
/// long enough that the echo of the copy we just sent does not.
pub const RETRY_ANSWER_GAP_S: u32 = 6;

/// `last` is when we last ACKed or relayed this id. `None` means we have not
/// answered a later copy yet, so the first retry should go out.
pub fn answer_again(now: u32, last: Option<u32>, gap: u32) -> bool {
    match last {
        Some(t) => now.saturating_sub(t) >= gap,
        None => true,
    }
}

/// A copy with fewer hops than the one we stored was relayed by a station
/// further from the origin. Our own echo has the same hop count.
pub fn heard_further(incoming_hops: u8, stored_hops: u8) -> bool {
    incoming_hops < stored_hops
}

/// The originator keyed this id again. The stored copy already had one hop
/// removed, so a fresh send still has more hops left than that copy.
pub fn origin_retry(incoming_hops: u8, stored_hops: u8) -> bool {
    incoming_hops > stored_hops
}

/// Hop budget for an ACK. A group receipt uses the same 6 hops as the message,
/// not the routine 3, so it can walk back along a five-station line.
pub fn ack_hop_budget(group: bool, priority: Priority) -> u8 {
    if group {
        crate::emcomm::BULLETIN_TTL
    } else {
        priority.default_ttl()
    }
}

/// The station that sent this ACK keeps keying it until a hop closer to the
/// originator relays it, or the retry count is used up.
pub fn ack_walk_continues(downstream_heard: bool, retries: u32, max_retries: u32) -> bool {
    !downstream_heard && retries < max_retries
}

/// `#bulletin` has no member list, so the sender stops at the first ACK.
/// A relay keeps the copy moving until a station further along is heard.
pub fn bulletin_keeps_walking(env: &Envelope) -> bool {
    matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    ) && crate::slash::is_bulletin(env.dest.as_str())
}

/// Channel control frame (`WCRMETA members=…`, `leave=…`, `prio=…`). It has
/// no ACK, so one lost copy used to leave every station past it with a
/// short roster.
pub fn is_channel_meta(env: &Envelope) -> bool {
    env.kind == MsgType::Status
        && env.flags.group()
        && !crate::slash::is_bulletin(env.dest.as_str())
        && env.body.starts_with(b"WCRMETA ")
}

/// Relays key this frame again until a station further along is heard.
pub fn keeps_walking(env: &Envelope) -> bool {
    bulletin_keeps_walking(env) || is_channel_meta(env)
}

/// Our own frame that nobody ACKs. The ARQ loop repeats it until a relay
/// is heard carrying it on, then stops without a retry tag or `[no]`.
pub fn walks_until_relayed(env: &Envelope) -> bool {
    env.kind == MsgType::Ack || is_channel_meta(env)
}

/// Schedule another relay of a bulletin or channel roster that has not been
/// heard further on.
pub fn keep_relaying(
    env: &Envelope,
    retries: u32,
    max_retries: u32,
    downstream_heard: bool,
) -> bool {
    keeps_walking(env)
        && env.hops_left > 0
        && ack_walk_continues(downstream_heard, retries, max_retries)
}

/// A later copy of a frame we already answered. Only one that came from
/// the originator's side (as many hops or more than ours) is a retry worth a
/// new ACK. A copy with fewer hops is a relay further along; answering it
/// sends another receipt down the whole line for nothing.
pub fn copy_wants_new_ack(incoming_hops: u8, stored_hops: u8) -> bool {
    !heard_further(incoming_hops, stored_hops)
}

/// A duplicate from the originator's side is not a relay further along.
/// It must not cancel our pending copy of a frame that walks hop by hop.
pub fn duplicate_suppresses(env: &Envelope, incoming_hops: u8, stored_hops: u8) -> bool {
    !((keeps_walking(env) || closed_group_walks(env)) && origin_retry(incoming_hops, stored_hops))
}

/// A closed-group line relayed for its members. One copy is often lost at
/// the next station, and the originator's retries only reach it when every
/// hop between carries them, so the relay walks it like a bulletin.
pub fn closed_group_walks(env: &Envelope) -> bool {
    matches!(
        env.kind,
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
    ) && env.flags.group()
        && !crate::slash::is_bulletin(env.dest.as_str())
}

/// Walk a closed-group relay again until a station further along is heard,
/// every member's receipt is known here (the pump then stops on `[all]`),
/// or the retries are spent.
pub fn group_walk_continues(
    env: &Envelope,
    retries: u32,
    max_retries: u32,
    downstream_heard: bool,
) -> bool {
    closed_group_walks(env)
        && env.hops_left > 0
        && ack_walk_continues(downstream_heard, retries, max_retries)
}

/// Rung for the `step`-th copy this station relays without hearing it go
/// further. Starts at the preset's ladder start, never faster. Steps only to
/// a rung that still carries the preset's full frame: modem73 has one TX
/// mode, and a narrower mode drops any wider frame still in its queue.
pub fn relay_rung(preset: Preset, step: u32, widest: usize) -> Rung {
    let mut idx = preset.ladder_start();
    for _ in 0..step {
        let next = idx + 1;
        if next >= Rung::COUNT
            || next > Rung::max_for_size(widest)
            || Rung::from_index(next).payload_bytes() < preset.payload_bytes()
        {
            break;
        }
        idx = next;
    }
    Rung::from_index(idx)
}

/// Airtime of one full frame on the preset. modem73 frames are a fixed
/// length per mode, so a short frame holds the channel this long too.
pub fn frame_slot(preset: Preset) -> std::time::Duration {
    std::time::Duration::from_secs_f64(crate::presets::airtime_secs(
        preset,
        preset.payload_bytes() as usize,
        0,
    ))
}

/// Frame slots a relay or group receipt picks from. Two neighbours that
/// cannot hear each other both answer the frame between them at once; a
/// station between them decodes neither. Carrier sense cannot help.
pub fn spread_slots(priority: Priority) -> u32 {
    match priority {
        Priority::Emergency => 2,
        Priority::Priority => 3,
        Priority::Routine => 4,
    }
}

/// Delay before keying a relay or group receipt: a whole random slot, so
/// two stations that picked different slots do not overlap.
pub fn spread_delay(preset: Preset, priority: Priority) -> std::time::Duration {
    let slot = frame_slot(preset);
    let pick = rand::thread_rng().gen_range(0..spread_slots(priority));
    slot * pick
}

/// How long a neighbour heard keying its own frame counts as present.
pub const NEIGHBOUR_FRESH_S: u32 = 900;
/// Uptime before a lone known neighbour is taken as the keyer of every frame.
pub const COVERAGE_WARMUP_S: u32 = 120;
const COVERAGE_KEEP_S: u32 = 3_600;

/// Which neighbours already hold a frame, from what was decoded on RF.
///
/// A neighbour is a station heard keying its own frame (a beacon, or a frame
/// still at the top hop budget). A frame's origin holds it; an ACK's sender
/// holds the ACK and the line it answers. When one neighbour is known and
/// nothing suggests a second, every frame decoded came from that neighbour.
/// Each station keys one id at one hop count, so copies of one id at two
/// counts (other than our own) mean two keyers: a second neighbour exists
/// even if it has never keyed its own frame.
#[derive(Default)]
pub struct Coverage {
    started: u32,
    neighbours: std::collections::HashMap<String, u32>,
    holders: std::collections::HashMap<crate::proto::MsgId, (u32, Vec<String>)>,
    hops: std::collections::HashMap<crate::proto::MsgId, (u32, Vec<u8>)>,
    keyers_seen: (u32, usize),
}

impl Coverage {
    pub fn new(now: u32) -> Self {
        Self {
            started: now,
            ..Self::default()
        }
    }

    fn hold(&mut self, id: crate::proto::MsgId, call: &str, now: u32) {
        let e = self.holders.entry(id).or_insert((now, Vec::new()));
        e.0 = now;
        if !e.1.iter().any(|c| c.eq_ignore_ascii_case(call)) {
            e.1.push(call.to_ascii_uppercase());
        }
    }

    /// One frame decoded on RF. `ours` is the hop count we stored for it
    /// before this copy, which is what our own relay keys.
    pub fn heard(&mut self, env: &Envelope, ours: Option<u8>, now: u32) {
        let origin = env.origin.as_str();
        if env.kind == MsgType::Beacon
            || (env.kind != MsgType::Frag && env.hops_left >= crate::emcomm::BULLETIN_TTL)
        {
            self.neighbours.insert(origin.to_ascii_uppercase(), now);
        }
        self.hold(env.msg_id, origin, now);
        if env.kind == MsgType::Ack {
            if let Some(id) = env.acked_id() {
                self.hold(id, origin, now);
            }
        }
        if ours != Some(env.hops_left) && env.kind != MsgType::Beacon {
            let e = self.hops.entry(env.msg_id).or_insert((now, Vec::new()));
            e.0 = now;
            if !e.1.contains(&env.hops_left) {
                e.1.push(env.hops_left);
            }
            let n = e.1.len();
            if n >= self.keyers_floor(now) {
                self.keyers_seen = (now, n);
            }
        }
        self.prune(now);
    }

    fn prune(&mut self, now: u32) {
        if self.holders.len() < 512 && self.hops.len() < 512 {
            return;
        }
        self.holders
            .retain(|_, (t, _)| now.saturating_sub(*t) < COVERAGE_KEEP_S);
        self.hops
            .retain(|_, (t, _)| now.saturating_sub(*t) < COVERAGE_KEEP_S);
        self.neighbours
            .retain(|_, t| now.saturating_sub(*t) < COVERAGE_KEEP_S);
    }

    fn fresh(&self, now: u32) -> Vec<&str> {
        self.neighbours
            .iter()
            .filter(|(_, t)| now.saturating_sub(**t) < NEIGHBOUR_FRESH_S)
            .map(|(c, _)| c.as_str())
            .collect()
    }

    /// Most keyers seen for one id lately: a floor on how many neighbours exist.
    fn keyers_floor(&self, now: u32) -> usize {
        if now.saturating_sub(self.keyers_seen.0) < NEIGHBOUR_FRESH_S {
            self.keyers_seen.1
        } else {
            0
        }
    }

    /// Every neighbour we know of already holds `id`, so keying it again
    /// reaches nobody new. False until a neighbour is known.
    pub fn covered(&self, id: &crate::proto::MsgId, now: u32) -> bool {
        let fresh = self.fresh(now);
        if fresh.is_empty() || fresh.len() < self.keyers_floor(now) {
            return false;
        }
        let holders = self
            .holders
            .get(id)
            .map(|(_, h)| h.as_slice())
            .unwrap_or(&[]);
        let named = fresh
            .iter()
            .all(|n| holders.iter().any(|h| h.eq_ignore_ascii_case(n)));
        if named {
            return true;
        }
        // One neighbour, nothing suggesting another: it keyed what we decoded.
        fresh.len() == 1
            && self.keyers_floor(now) <= 1
            && now.saturating_sub(self.started) >= COVERAGE_WARMUP_S
            && self.hops.contains_key(id)
    }
}

pub fn jitter_ms(priority: Priority) -> u64 {
    let range = priority.jitter_ms();
    rand::thread_rng().gen_range(range)
}

/// `sent` is how many retransmissions have gone out (the `n` in `retry-n/M`).
/// The line has given up once that reaches `max`.
pub fn retries_exhausted(sent: u32, max: u32) -> bool {
    max > 0 && sent >= max
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

pub const BEACON_BASE_S: i64 = 60;
pub const BEACON_MIN_S: i64 = 15;
pub const BEACON_MAX_S: i64 = 120;

/// Next identity-beacon wait, 60 s ± jitter, clamped to 15–120 s.
pub fn beacon_interval_secs(jitter_s: u32) -> u64 {
    let j = jitter_s as i64;
    let roll = if j <= 0 {
        0
    } else {
        rand::thread_rng().gen_range(-j..=j)
    };
    (BEACON_BASE_S + roll).clamp(BEACON_MIN_S, BEACON_MAX_S) as u64
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

/// In `internet-radio`, offer the hub and LAN mesh only when RF will not reach the dest.
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
        let prior = self.store.get(&env.msg_id)?;
        let waiting = prior
            .as_ref()
            .map(|m| m.delivery == Delivery::Queued || m.delivery == Delivery::Sent)
            .unwrap_or(false);
        let stored_hops = prior.as_ref().map(|m| m.env.hops_left).unwrap_or(0);
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
                        // Failed -> delivered is allowed. Delivered and [all]
                        // do not move backwards.
                        let marks = self
                            .store
                            .get(&id)?
                            .is_none_or(|o| ack_marks_delivered(&self.our_call, &o.env));
                        if marks {
                            let _ = self.store.advance_delivery(&id, Delivery::Delivered)?;
                        }
                    }
                }
                if stored.kind == MsgType::Msg && stored.dest.as_str() == self.our_call {
                    // local delivery; ACK is produced by the node runtime
                }
                let ack_for_us =
                    stored.kind == MsgType::Ack && stored.dest.as_str() == self.our_call;
                if ack_for_us {
                    let _ = self
                        .store
                        .advance_delivery(&stored.msg_id, Delivery::Relayed)?;
                }
                let delivery = self.store.delivery_of(&stored.msg_id).ok().flatten();
                if may_rebroadcast(&self.our_call, &stored, delivery) {
                    let now = crate::proto::now_ts();
                    self.store.set_hold(
                        &stored.msg_id,
                        now + (decision.delay_ms / 1000) as u32,
                        stored.hops_left,
                    )?;
                }
            }
            Action::Suppress => {
                // A hub echo of a closed group is not another RF relay.
                if duplicate_cancels_relay(
                    medium,
                    env.flags.group(),
                    crate::slash::is_bulletin(env.dest.as_str()),
                ) && duplicate_suppresses(env, env.hops_left, stored_hops)
                {
                    self.store.suppress(&env.msg_id)?;
                    let _ = self
                        .store
                        .advance_delivery(&env.msg_id, Delivery::Relayed)?;
                }
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
    fn own_origin_is_not_a_relay_hold() {
        let env = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("M0BBB").unwrap(),
            1,
            b"ping".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert!(is_own_origin("G4AAA", &env));
        assert!(is_own_origin("g4aaa", &env));
        assert!(!is_own_origin("M0BBB", &env));
        assert!(!may_rebroadcast("G4AAA", &env, Some(Delivery::Sent)));
        assert!(!may_rebroadcast("G4AAA", &env, Some(Delivery::Relayed)));
    }

    #[test]
    fn origin_retry_is_a_higher_hop_count() {
        assert!(origin_retry(6, 5));
        assert!(!origin_retry(5, 5));
        assert!(!origin_retry(4, 5));
        assert!(heard_further(4, 5));
        assert!(!heard_further(5, 5));
        assert!(!heard_further(6, 5));
    }

    #[test]
    fn ack_uses_the_message_hop_budget_and_walks_until_relayed() {
        assert_eq!(
            ack_hop_budget(true, Priority::Routine),
            crate::emcomm::BULLETIN_TTL
        );
        assert_eq!(
            ack_hop_budget(true, Priority::Emergency),
            crate::emcomm::BULLETIN_TTL
        );
        assert_eq!(ack_hop_budget(false, Priority::Routine), 3);
        assert_eq!(ack_hop_budget(false, Priority::Emergency), 5);
        assert!(ack_walk_continues(false, 0, 3));
        assert!(ack_walk_continues(false, 2, 3));
        assert!(!ack_walk_continues(true, 0, 3));
        assert!(!ack_walk_continues(false, 3, 3));
    }

    #[test]
    fn bulletin_relay_repeats_until_a_further_station_is_heard() {
        let bulletin = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("BULLETIN"),
            1,
            b"b1".to_vec(),
            5,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        assert!(bulletin_keeps_walking(&bulletin));
        assert!(keep_relaying(&bulletin, 0, 3, false));
        assert!(keep_relaying(&bulletin, 2, 3, false));
        assert!(!keep_relaying(&bulletin, 0, 3, true));
        assert!(!keep_relaying(&bulletin, 3, 3, false));
        let mut done = bulletin.clone();
        done.hops_left = 0;
        assert!(!keep_relaying(&done, 0, 3, false));

        let group = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("QARP"),
            1,
            b"g1".to_vec(),
            5,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        assert!(!bulletin_keeps_walking(&group));
        assert!(!keep_relaying(&group, 0, 3, false));

        let ack = Envelope::ack_for(&group, Callsign::parse("M0BBB").unwrap(), 2, None);
        assert!(!bulletin_keeps_walking(&ack));
        assert!(!keep_relaying(&ack, 0, 3, false));
    }

    #[test]
    fn roster_walks_hop_by_hop_like_a_bulletin() {
        let roster = |body: &[u8], dest: &str| {
            let mut env = Envelope::new_msg(
                Callsign::parse("G4AAA").unwrap(),
                Callsign::from_raw(dest),
                1,
                body.to_vec(),
                6,
                Flags::new().with(crate::proto::FLAG_GROUP),
            )
            .unwrap();
            env.kind = MsgType::Status;
            env
        };
        let members = roster(b"WCRMETA members=G4AAA,M0BBB", "NET");
        assert!(is_channel_meta(&members));
        assert!(keeps_walking(&members));
        assert!(walks_until_relayed(&members));
        assert!(keep_relaying(&members, 0, 3, false));
        assert!(!keep_relaying(&members, 0, 3, true));
        assert!(!keep_relaying(&members, 3, 3, false));
        assert!(is_channel_meta(&roster(b"WCRMETA leave=M0BBB", "NET")));

        let chat = roster(b"hello", "NET");
        assert!(!is_channel_meta(&chat));
        assert!(!keeps_walking(&chat));
        let mut dm = members.clone();
        dm.flags = Flags::new();
        assert!(!is_channel_meta(&dm));
        assert!(!is_channel_meta(&roster(b"WCRMETA members=X", "BULLETIN")));

        let ack = Envelope::ack_for(&chat, Callsign::parse("M0BBB").unwrap(), 2, None);
        assert!(walks_until_relayed(&ack));
        assert!(!keeps_walking(&ack));
    }

    #[test]
    fn only_the_dm_addressee_ack_ends_a_relay() {
        let dm = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("2E0CCC").unwrap(),
            1,
            b"dm".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert!(ack_ends_relay("2E0CCC", Some(&dm)));
        assert!(ack_ends_relay("2e0ccc", Some(&dm)));
        assert!(!ack_ends_relay("M0BBB", Some(&dm)));
        assert!(!ack_ends_relay("2E0CCC", None));
        let bulletin = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("BULLETIN"),
            2,
            b"b".to_vec(),
            6,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        assert!(!ack_ends_relay("M0BBB", Some(&bulletin)));
        let mut group = bulletin.clone();
        group.dest = Callsign::from_raw("NET");
        assert!(!ack_ends_relay("NET", Some(&group)));
    }

    #[test]
    fn neighbour_ack_does_not_close_a_relayed_bulletin() {
        let bulletin = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("BULLETIN"),
            1,
            b"b".to_vec(),
            6,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        assert!(!ack_marks_delivered("2E0CCC", &bulletin));
        assert!(ack_marks_delivered("G4AAA", &bulletin));
        let mut dm = bulletin.clone();
        dm.dest = Callsign::parse("M0DDD").unwrap();
        dm.flags = Flags::new();
        assert!(ack_marks_delivered("2E0CCC", &dm));
        let mut group = bulletin.clone();
        group.dest = Callsign::from_raw("NET");
        assert!(ack_marks_delivered("2E0CCC", &group));
    }

    #[test]
    fn retry_copies_and_further_relays_are_told_apart() {
        // We stored 4. The sender's side keys 5 or more; a relay further on keys 3.
        assert!(copy_wants_new_ack(5, 4));
        assert!(copy_wants_new_ack(4, 4));
        assert!(!copy_wants_new_ack(3, 4));
        let bulletin = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("BULLETIN"),
            1,
            b"b".to_vec(),
            6,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        assert!(!duplicate_suppresses(&bulletin, 5, 4));
        assert!(duplicate_suppresses(&bulletin, 4, 4));
        assert!(duplicate_suppresses(&bulletin, 3, 4));
        let mut dm = bulletin.clone();
        dm.dest = Callsign::parse("M0DDD").unwrap();
        dm.flags = Flags::new();
        assert!(duplicate_suppresses(&dm, 5, 4));
    }

    #[test]
    fn relay_keeps_its_bulletin_open_after_a_neighbour_ack() {
        let store = Arc::new(Store::open_memory().unwrap());
        let engine = Engine::new(store.clone(), "2E0CCC".into());
        let bulletin = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("BULLETIN"),
            1,
            b"b".to_vec(),
            5,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        assert_eq!(
            engine.on_rx(&bulletin, "rf", None, None).unwrap().action,
            Action::Accept
        );
        let ack = Envelope::ack_for(&bulletin, Callsign::parse("M0BBB").unwrap(), 7, None);
        engine.on_rx(&ack, "rf", None, None).unwrap();
        let row = store.get(&bulletin.msg_id).unwrap().unwrap();
        assert_eq!(row.delivery, Delivery::Queued);
        assert!(may_rebroadcast("2E0CCC", &row.env, Some(row.delivery)));

        // The sender's side keys it again before our hold fires: not a
        // relay further on, so our copy is not suppressed.
        let mut again = bulletin.clone();
        again.hops_left = 5;
        assert_eq!(
            engine.on_rx(&again, "rf", None, None).unwrap().action,
            Action::Suppress
        );
        assert!(store
            .hold_due(u32::MAX)
            .unwrap()
            .iter()
            .any(|m| m.env.msg_id == bulletin.msg_id));
    }

    #[test]
    fn hub_echo_keeps_a_closed_group_relay() {
        assert!(!duplicate_cancels_relay("inet", true, false));
        assert!(!duplicate_cancels_relay("lan", true, false));
        assert!(duplicate_cancels_relay("rf", true, false));
        assert!(duplicate_cancels_relay("inet", false, false));
        assert!(duplicate_cancels_relay("inet", true, true));
    }

    #[test]
    fn closed_group_still_relays_after_one_member_ack() {
        let mut env = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("QARP"),
            1,
            b"group".to_vec(),
            3,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap();
        assert!(may_rebroadcast("M0BBB", &env, Some(Delivery::Delivered)));
        assert!(!may_rebroadcast("M0BBB", &env, Some(Delivery::All)));
        env.dest = Callsign::from_raw("BULLETIN");
        assert!(!may_rebroadcast("M0BBB", &env, Some(Delivery::Delivered)));
    }

    #[test]
    fn last_retry_is_a_failure() {
        assert!(!retries_exhausted(2, 3));
        assert!(retries_exhausted(3, 3));
        assert!(!retries_exhausted(0, 0));
    }

    #[test]
    fn last_copy_still_waits_one_interval() {
        // retry-3/3 is n=3. The next hold is the ACK wait, routine 360s, not zero.
        assert_eq!(
            next_retry_hold_base(3, 1_000, Priority::Routine),
            1_000 + 360
        );
        assert_eq!(
            next_retry_hold_jittered(3, 1_000, Priority::Routine, false),
            1_000 + 360
        );
    }

    #[test]
    fn failed_stays_until_a_real_ack() {
        let store = Store::open_memory().unwrap();
        let engine = Engine::new(std::sync::Arc::new(store), "G4AAA".into());
        let original = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("M0BBB").unwrap(),
            1,
            b"ping".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        engine.store.insert(&original, Delivery::Relayed).unwrap();
        assert!(engine
            .store
            .advance_delivery(&original.msg_id, Delivery::Failed)
            .unwrap());
        assert_eq!(engine.store.retries_of(&original.msg_id).unwrap(), 0);
        engine.store.bump_retry(&original.msg_id, 0).unwrap();
        engine.store.bump_retry(&original.msg_id, 0).unwrap();
        engine.store.bump_retry(&original.msg_id, 0).unwrap();
        assert_eq!(engine.store.retries_of(&original.msg_id).unwrap(), 3);
        // The relay of the final copy must not walk the row back to relayed.
        assert!(!engine
            .store
            .advance_delivery(&original.msg_id, Delivery::Relayed)
            .unwrap());
        assert_eq!(
            engine.store.delivery_of(&original.msg_id).unwrap(),
            Some(Delivery::Failed)
        );
        let ack = Envelope::ack_for(&original, Callsign::parse("M0BBB").unwrap(), 4, None);
        engine.on_rx(&ack, "rf", None, None).unwrap();
        assert_eq!(
            engine.store.delivery_of(&original.msg_id).unwrap(),
            Some(Delivery::Delivered)
        );
        assert!(!engine
            .store
            .advance_delivery(&original.msg_id, Delivery::Relayed)
            .unwrap());
        assert_eq!(
            engine.store.delivery_of(&original.msg_id).unwrap(),
            Some(Delivery::Delivered)
        );
    }

    #[test]
    fn answer_again_waits_out_the_echo() {
        assert!(answer_again(100, None, RETRY_ANSWER_GAP_S));
        assert!(!answer_again(100, Some(96), RETRY_ANSWER_GAP_S));
        assert!(answer_again(100, Some(94), RETRY_ANSWER_GAP_S));
        assert!(answer_again(8, Some(0), RETRY_ANSWER_GAP_S));
    }

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
    fn ack_addressed_to_us_is_not_put_on_relay_hold() {
        let store = Store::open_memory().unwrap();
        let engine = Engine::new(std::sync::Arc::new(store), "M7TJF".into());
        let original = Envelope::new_msg(
            Callsign::parse("M7TJF").unwrap(),
            Callsign::from_raw("BULLETIN"),
            1,
            b"hello".to_vec(),
            6,
            Flags::new(),
        )
        .unwrap();
        engine.store.insert(&original, Delivery::Sent).unwrap();
        let ack = Envelope::ack_for(&original, Callsign::parse("TF101").unwrap(), 2, Some(1.0));
        let d = engine.on_rx(&ack, "rf", Some(1.0), None).unwrap();
        assert_eq!(d.action, Action::Accept);
        assert_eq!(
            engine.store.delivery_of(&original.msg_id).unwrap(),
            Some(Delivery::Delivered)
        );
        assert_eq!(
            engine.store.delivery_of(&ack.msg_id).unwrap(),
            Some(Delivery::Relayed)
        );
        let due = engine.store.hold_due(u32::MAX).unwrap();
        assert!(due.iter().all(|m| m.env.msg_id != ack.msg_id));
    }

    #[test]
    fn dm_to_us_is_not_rebroadcast_and_acked_frames_stop() {
        let us = "M0BBB";
        let dm = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse(us).unwrap(),
            1,
            b"for-b".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert!(!may_rebroadcast(us, &dm, Some(Delivery::Queued)));
        let store = Store::open_memory().unwrap();
        let engine = Engine::new(std::sync::Arc::new(store), us.into());
        engine.on_rx(&dm, "rf", None, None).unwrap();
        assert!(engine.store.hold_due(u32::MAX).unwrap().is_empty());

        let onward = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("2E0CCC").unwrap(),
            2,
            b"for-c".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        assert!(may_rebroadcast(us, &onward, Some(Delivery::Queued)));
        assert!(!may_rebroadcast(us, &onward, Some(Delivery::Delivered)));
        let ack = Envelope::ack_for(&onward, Callsign::parse("2E0CCC").unwrap(), 3, None);
        assert!(may_rebroadcast(us, &ack, Some(Delivery::Queued)));
        assert!(!may_rebroadcast("2E0CCC", &ack, Some(Delivery::Queued)));
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
    fn beacon_interval_stays_in_bounds() {
        for j in [0u32, 15, 60, 200] {
            for _ in 0..40 {
                let w = beacon_interval_secs(j);
                assert!(
                    (BEACON_MIN_S as u64..=BEACON_MAX_S as u64).contains(&w),
                    "jitter {j}: wait {w}"
                );
            }
        }
        assert_eq!(beacon_interval_secs(0), BEACON_BASE_S as u64);
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

    fn group_line() -> Envelope {
        Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("NET"),
            1,
            b"g".to_vec(),
            6,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap()
    }

    #[test]
    fn a_closed_group_retry_does_not_cancel_the_walk() {
        let group = group_line();
        assert!(closed_group_walks(&group));
        assert!(group_walk_continues(&group, 0, 3, false));
        assert!(group_walk_continues(&group, 2, 3, false));
        assert!(!group_walk_continues(&group, 0, 3, true));
        assert!(!group_walk_continues(&group, 3, 3, false));
        let mut spent = group.clone();
        spent.hops_left = 0;
        assert!(!group_walk_continues(&spent, 0, 3, false));
        // Incoming 5 against a stored 4 is the originator keying again.
        assert!(!duplicate_suppresses(&group, 5, 4));
        assert!(duplicate_suppresses(&group, 3, 4));
        let mut dm = group.clone();
        dm.flags = Flags::new();
        assert!(!closed_group_walks(&dm));
        assert!(duplicate_suppresses(&dm, 5, 4));
        assert!(!closed_group_walks(&{
            let mut b = group.clone();
            b.dest = Callsign::from_raw("BULLETIN");
            b
        }));
    }

    #[test]
    fn a_repeated_relay_steps_one_rung_and_keeps_the_preset_frame() {
        assert_eq!(relay_rung(Preset::HfPoor, 0, 40), Rung::Rdm600S);
        assert_eq!(relay_rung(Preset::HfPoor, 1, 40), Rung::Rdm300S);
        assert_eq!(relay_rung(Preset::HfPoor, 4, 40), Rung::Rdm300S);
        assert_eq!(relay_rung(Preset::HfPoor, 1, 200), Rung::Rdm600S);
        assert_eq!(relay_rung(Preset::HfWeak, 3, 40), Rung::Rdm300S);
        assert_eq!(relay_rung(Preset::VhfFm, 3, 40), Rung::OfdmQpskHalf);
        assert_eq!(relay_rung(Preset::HfDeep, 2, 40), Rung::Mfsk32R);
        let slot = frame_slot(Preset::HfPoor);
        assert!(slot > std::time::Duration::from_secs(3));
        for _ in 0..12 {
            let delay = spread_delay(Preset::HfPoor, Priority::Routine);
            let n = (delay.as_secs_f64() / slot.as_secs_f64()).round() as u32;
            assert!(n < spread_slots(Priority::Routine));
            assert!((delay.as_secs_f64() - slot.as_secs_f64() * n as f64).abs() < 0.01);
        }
    }

    fn heard_line(origin: &str, hops: u8) -> Envelope {
        Envelope::new_msg(
            Callsign::parse(origin).unwrap(),
            Callsign::from_raw("NET"),
            1,
            b"g".to_vec(),
            hops,
            Flags::new().with(crate::proto::FLAG_GROUP),
        )
        .unwrap()
    }

    #[test]
    fn an_end_station_stops_once_its_only_neighbour_holds_the_frame() {
        let line = heard_line("G4AAA", 5);
        let mut beacon = heard_line("M0BBB", 1);
        beacon.kind = MsgType::Beacon;
        let mut cover = Coverage::new(1_000);
        cover.heard(&beacon, None, 1_000);
        // Relayed by the one neighbour: a new hop count, origin still G4AAA.
        cover.heard(&line, Some(6), 1_050);
        assert!(!cover.covered(&line.msg_id, 1_050));
        assert!(cover.covered(&line.msg_id, 1_000 + COVERAGE_WARMUP_S));
        let mut other_hop = line.clone();
        other_hop.hops_left = 3;
        cover.heard(&other_hop, Some(6), 1_400);
        assert!(
            !cover.covered(&line.msg_id, 1_500),
            "two hop counts means a second neighbour, even unnamed"
        );

        let mut two = Coverage::new(1_000);
        two.heard(&beacon, None, 1_000);
        let mut east = heard_line("2E0CCC", 1);
        east.kind = MsgType::Beacon;
        two.heard(&east, None, 1_000);
        two.heard(&line, Some(6), 1_100);
        let from_b = Envelope::ack_for(&line, Callsign::parse("M0BBB").unwrap(), 2, None);
        two.heard(&from_b, Some(5), 1_200);
        assert!(!two.covered(&line.msg_id, 1_250));
        let from_c = Envelope::ack_for(&line, Callsign::parse("2E0CCC").unwrap(), 3, None);
        two.heard(&from_c, Some(5), 1_300);
        assert!(two.covered(&line.msg_id, 1_350));
    }
}
