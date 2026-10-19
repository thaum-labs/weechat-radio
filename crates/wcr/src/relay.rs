//! SPDX-License-Identifier: Apache-2.0
//! Store-and-forward relay: dedupe, TTL, jitter + suppression, hold queue, ACKs.

use crate::error::Result;
use crate::modes::Mode;
use crate::presets::{Preset, Rung};
use crate::proto::{Envelope, MsgId, MsgType, Priority};
use crate::store::{Delivery, Store};
use parking_lot::Mutex;
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

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
/// It must not cancel our pending copy, of a frame that walks hop by hop or
/// of a direct line: a modem whose carrier sense sticks keys a line and its
/// first retry back to back, before our copy has gone.
pub fn duplicate_suppresses(env: &Envelope, incoming_hops: u8, stored_hops: u8) -> bool {
    !((keeps_walking(env) || closed_group_walks(env) || is_direct_line(env))
        && origin_retry(incoming_hops, stored_hops))
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

/// A direct invite to a closed group. It has to reach every station the
/// group's own lines reach, so it travels on the group's hop budget.
pub fn is_group_invite(env: &Envelope) -> bool {
    env.kind == MsgType::Msg
        && !env.flags.group()
        && crate::slash::parse_invite_text(&env.body_text()).is_some()
}

/// Hops a line leaves its originator with: 6 for `#bulletin`, closed groups
/// and invites to them, the priority's budget for a direct message.
pub fn initial_hops(env: &Envelope) -> u8 {
    if env.flags.group() || is_group_invite(env) {
        crate::emcomm::BULLETIN_TTL
    } else {
        env.priority().default_ttl()
    }
}

/// A copy with `hops_heard` came straight from its originator, no relay between.
pub fn heard_from_origin(env: &Envelope, hops_heard: u8) -> bool {
    hops_heard >= initial_hops(env)
}

/// A neighbour not heard for this long has gone: three beacon gaps.
pub const NEIGHBOUR_TTL_S: u32 = 3 * BEACON_MAX_GAP_S;

/// Listening this long after start, every neighbour has had time to beacon
/// (the first beacon goes within `BEACON_MAX_S`, then waits out a busy
/// modem). Before that the neighbour list is not trusted.
pub const NEIGHBOUR_WARMUP_S: u32 = BEACON_MAX_S as u32 + 60;

/// A copy heard with the hops its origin keyed it with came straight from
/// that station, with no relay between: the origin is a neighbour. `line` is
/// the line an ACK answers, when we hold it. Beacons are never relayed.
pub fn heard_direct(env: &Envelope, line: Option<&Envelope>) -> bool {
    let keyed = match env.kind {
        MsgType::Beacon => return true,
        MsgType::Ack => match line {
            Some(l) => ack_hop_budget(l.flags.group() || is_group_invite(l), l.priority()),
            None => crate::emcomm::BULLETIN_TTL,
        },
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status => initial_hops(env),
        _ => return false,
    };
    env.hops_left >= keyed
}

/// Every station we hear directly already has the line, so another copy
/// from us reaches nobody new: we are the end of the chain for it.
/// `holders` have it (its originator, and every member whose receipt we
/// hold). With one neighbour and a relayed copy, that neighbour is the one
/// that relayed it to us. With one neighbour and a first-hand copy, it is
/// the originator, and a station beyond us whose beacons we lost would look
/// the same, so we cannot tell. Nor with no neighbour known.
pub fn neighbours_hold(neighbours: &[String], holders: &[String], came_relayed: bool) -> bool {
    if neighbours.is_empty() {
        return false;
    }
    if neighbours.len() == 1 {
        return came_relayed;
    }
    neighbours
        .iter()
        .all(|n| holders.iter().any(|h| h.eq_ignore_ascii_case(n)))
}

/// A receipt shows every neighbour has a line we relay, and `handed` copies
/// of it have gone to our air queue (`retries` on its row). Only a repeat
/// queued there may then be dropped: the first copy can still be waiting
/// too, and the first relay always goes. A station beyond us whose beacons
/// we all lost looks the same as the end of the chain.
pub fn queued_repeat_unneeded(handed: u32) -> bool {
    handed > 1
}

/// Most copies of one line a relay keys: its relay, one repeat, and one
/// answer to the originator's retry. Receipts are not capped: each hop's
/// repeats are what carries a far member's receipt home.
pub const RELAY_MAX_COPIES: u32 = 3;

/// Least gap between a relay's copies of one line: the line's own first
/// retry interval (45 s routine). A modem whose carrier sense sticks holds a
/// frame up to 30 s and then keys what is queued behind it in the same
/// burst, so two copies handed over closer than that go out back to back
/// and one collision takes both.
pub fn relay_copy_gap_s(priority: Priority) -> u32 {
    next_retry_hold_base(0, 0, priority)
}

/// A line to one station (a direct message or an invite), not a channel line
/// or a receipt. Only its addressee's ACK says it arrived.
pub fn is_direct_line(env: &Envelope) -> bool {
    env.kind != MsgType::Ack && !env.flags.group()
}

/// A relay passes an originator's retry on unless a station further along
/// already carried the line. A direct message is the exception: hearing it
/// one hop on says nothing about its addressee, whose ACK alone ends it (and
/// marks our copy delivered, which stops the relay anyway).
pub fn retry_passes_on(line: &Envelope, heard_further: bool, receipts_missing: bool) -> bool {
    !heard_further || receipts_missing || is_direct_line(line)
}

/// Only stations that heard a `#bulletin` from its originator ACK it. The
/// originator stops at the first ACK, so a receipt from further away is
/// airtime that the relays need.
pub fn acks_line(env: &Envelope) -> bool {
    !crate::slash::is_bulletin(env.dest.as_str()) || heard_from_origin(env, env.hops_left)
}

/// Repeats of a receipt we carry next to the originator. Nobody relays it on
/// to tell us it arrived, and the originator's retries ask for it again; but
/// its last retry can key over our copy, and then nothing asks.
pub const NEXT_TO_ORIGIN_ACK_REPEATS: u32 = 1;

/// Our relay of someone else's receipt keys again until a station closer to
/// the originator is heard carrying it. Next to the originator it repeats
/// `NEXT_TO_ORIGIN_ACK_REPEATS` times, a retry interval apart.
pub fn ack_relay_walks(
    ack: &Envelope,
    next_to_origin: bool,
    retries: u32,
    max_retries: u32,
    downstream_heard: bool,
) -> bool {
    let max = if next_to_origin {
        max_retries.min(NEXT_TO_ORIGIN_ACK_REPEATS)
    } else {
        max_retries
    };
    ack.kind == MsgType::Ack
        && ack.hops_left > 0
        && ack_walk_continues(downstream_heard, retries, max)
}

/// A receipt that says where its sender heard the line (v0.1.90 on), against
/// where we did. True when the sender is no further from the originator than
/// we are, so carrying its receipt takes it the wrong way. A station that does
/// not hold the line is not on its path. None for an older receipt.
pub fn ack_upstream_by_hops(ack: &Envelope, our_line: Option<&Envelope>) -> Option<bool> {
    let theirs = ack.acked_line_hops()?;
    let Some(line) = our_line else {
        return Some(true);
    };
    // Stored copies hold one hop less than the copy we heard.
    Some(line.hops_left.saturating_add(1) <= theirs)
}

/// Which receipts this station carries toward a line's originator.
///
/// A receipt from the originator's side of us, carried on, walked to the far
/// end of a chain and back, collided with the receipts that needed the air,
/// and made its sender stop repeating it. A receipt now says where its sender
/// heard the line ([`ack_upstream_by_hops`]). For an older receipt, a station
/// that ACKed before our relay of the line went out is taken as upstream.
#[derive(Default)]
pub struct AckSides {
    lines: HashMap<MsgId, LineSide>,
}

#[derive(Default)]
struct LineSide {
    /// When our first relay of the line keys.
    relay_at: Option<Instant>,
    /// Ackers heard before that.
    upstream: Vec<String>,
}

impl AckSides {
    fn side(&mut self, line: MsgId) -> &mut LineSide {
        if self.lines.len() > 4096 && !self.lines.contains_key(&line) {
            self.lines.clear();
        }
        self.lines.entry(line).or_default()
    }

    /// Our relay of `line` keys at `at`.
    pub fn relaying(&mut self, line: MsgId, at: Instant) {
        let side = self.side(line);
        if side.relay_at.is_none_or(|t| at < t) {
            side.relay_at = Some(at);
        }
    }

    /// `acker` sent a receipt for `line`, heard on the air at `now`. True when
    /// it came from the originator's side and is not ours to carry.
    pub fn from_upstream(&mut self, line: MsgId, acker: &str, now: Instant) -> bool {
        let side = self.side(line);
        if side.upstream.iter().any(|c| c.eq_ignore_ascii_case(acker)) {
            return true;
        }
        if side.relay_at.is_some_and(|t| now >= t) {
            return false;
        }
        side.upstream.push(acker.to_ascii_uppercase());
        true
    }

    /// `acker` sits on the originator's side of us for `line`.
    pub fn mark_upstream(&mut self, line: MsgId, acker: &str) {
        let side = self.side(line);
        if !side.upstream.iter().any(|c| c.eq_ignore_ascii_case(acker)) {
            side.upstream.push(acker.to_ascii_uppercase());
        }
    }

    /// `acker` is known to sit on the originator's side of us for `line`.
    pub fn is_upstream(&self, line: &MsgId, acker: &str) -> bool {
        self.lines
            .get(line)
            .is_some_and(|s| s.upstream.iter().any(|c| c.eq_ignore_ascii_case(acker)))
    }
}

/// Rung a relay keys on: the preset's own. A relay used to key in whatever
/// mode the modem was left in, and modem73 dropped a frame wider than that
/// mode. A slower rung would hold the channel longer on a busy chain, where
/// the copies are lost to collisions, not to SNR.
pub fn relay_rung(preset: Preset) -> Rung {
    Rung::from_index(preset.ladder_start())
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
/// station between them decodes neither. Carrier sense cannot help. On a
/// group retry each side answers with its own receipt and the ones it holds,
/// so a routine line leaves room for several frames.
pub fn spread_slots(priority: Priority) -> u32 {
    match priority {
        Priority::Emergency => 2,
        Priority::Priority => 4,
        Priority::Routine => 6,
    }
}

/// Delay before keying a relay or group receipt: a whole random slot, so
/// two stations that picked different slots do not overlap. A slot is a
/// quarter longer than the frame: an ACK can key a moment late.
pub fn spread_delay(preset: Preset, priority: Priority) -> std::time::Duration {
    let slot = frame_slot(preset).mul_f64(1.25);
    let pick = rand::thread_rng().gen_range(0..spread_slots(priority));
    slot * pick
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

/// Longest a station goes without a beacon while traffic keeps it quiet.
pub const BEACON_MAX_GAP_S: u32 = 600;

/// Gap between beacons on a quiet channel once the neighbour warm-up is over.
pub const BEACON_IDLE_S: u32 = 300;

/// A station's first beacons go a pace apart, so one lost beacon still
/// leaves its neighbours time to learn it.
pub const BEACON_WARMUP_COUNT: u32 = 3;

/// When the next beacon is due. `pace` is this round's `beacon_interval_secs`
/// roll (about a minute). The first beacon goes `pace` after `started`, and
/// the next ones `pace` after the one before until `BEACON_WARMUP_COUNT`
/// have reached the modem (`sent`). After that a station beacons every
/// `BEACON_IDLE_S` plus the round's jitter, never less. `last_beacon` is when the last one
/// reached the modem. Traffic does not move the schedule: a beacon that falls
/// due while an exchange is in progress waits for it (see [`Exchanges`]).
pub fn next_beacon_at(started: u32, last_beacon: Option<u32>, sent: u32, pace: u32) -> u32 {
    let Some(last) = last_beacon else {
        return started.saturating_add(pace);
    };
    let gap = if sent < BEACON_WARMUP_COUNT {
        pace
    } else {
        BEACON_IDLE_S + pace.abs_diff(BEACON_BASE_S as u32)
    };
    last.saturating_add(gap)
}

/// Latest a beacon may go: `BEACON_MAX_GAP_S` after the last one, or after
/// start for the first. A busy channel holds a beacon until then, no later.
pub fn beacon_ceiling(started: u32, last_beacon: Option<u32>) -> u32 {
    last_beacon
        .unwrap_or(started)
        .saturating_add(BEACON_MAX_GAP_S)
}

/// A busy channel holds a due beacon until a frame decoded here is this old.
pub const BEACON_BUSY_DECODE_S: u64 = 10;

/// How many seconds a beacon that has fallen due waits for the channel, 0 to
/// go now. The modem's occupancy alone reads near 100% on a steady noise
/// floor or with its carrier sense stuck high, and that held every beacon for
/// good. So `congested` holds it only while traffic is on the air: until
/// `BEACON_BUSY_DECODE_S` after the last frame we decoded (`since_decode_s`
/// ago), and never past `ceiling`.
pub fn beacon_channel_wait(now: u32, ceiling: u32, congested: bool, since_decode_s: u64) -> u32 {
    if now >= ceiling || !congested || since_decode_s >= BEACON_BUSY_DECODE_S {
        return 0;
    }
    ((BEACON_BUSY_DECODE_S - since_decode_s) as u32).min(ceiling - now)
}

/// modem73 holds a frame this long when its carrier sense is stuck busy on
/// the noise floor, then keys it anyway.
pub const MODEM_STUCK_HOLD_S: u32 = 30;

/// Allowance for a receipt's carrier-sense cap (3 s) and dither.
const RECEIPT_CAP_S: f64 = 5.0;

/// Spread window of a routine relay or group receipt: every slot it picks from.
fn spread_window_secs(preset: Preset) -> f64 {
    frame_slot(preset).as_secs_f64() * 1.25 * f64::from(spread_slots(Priority::Routine))
}

/// How long a line or mail frame keyed or heard here waits for its answer:
/// a receipt's round trip. The line can wait `MODEM_STUCK_HOLD_S` at the
/// modem, takes a frame on the air, and its receipt picks a slot of the
/// spread window, waits out its carrier-sense cap and takes another frame.
/// About 70 s at hf-poor.
pub fn exchange_wait_s(preset: Preset) -> u32 {
    let slot = frame_slot(preset).as_secs_f64();
    (f64::from(MODEM_STUCK_HOLD_S) + 2.0 * slot + spread_window_secs(preset) + RECEIPT_CAP_S).ceil()
        as u32
}

/// How long an exchange stays in progress after its last frame: the rest
/// of a group's receipts land in the same spread window as the first, and
/// receipts carried from further on follow it. About 30 s at hf-poor.
pub fn exchange_settle_s(preset: Preset) -> u32 {
    spread_window_secs(preset).ceil() as u32
}

/// Longest a due beacon waits for exchanges in progress: one exchange, a
/// receipt's round trip and the rest of its receipts. About 100 s at hf-poor.
pub fn beacon_exchange_wait_s(preset: Preset) -> u32 {
    exchange_wait_s(preset) + exchange_settle_s(preset)
}

/// A mail keeps a due beacon until its acceptance: one keyed between the
/// burst and the receiver's report leaves the sender deaf to it, and the
/// whole mail goes again. At most one exchange past the 10-minute `ceiling`.
pub fn beacon_waits_for_mail(now: u32, ceiling: u32, preset: Preset) -> bool {
    now < ceiling.saturating_add(exchange_wait_s(preset))
}

/// A beacon that fell due at `due` still waits for an exchange in progress.
/// It waits for one at most: back-to-back exchanges on a busy chain would
/// otherwise hold it to the 10-minute ceiling, and every station hears it
/// about every 5 minutes. A warm-up beacon is how neighbours learn this
/// station, so it waits one receipt's round trip at most (QA v0.1.96: a
/// busy station's warm-up gaps reached 270 s).
pub fn beacon_waits_for_exchange(now: u32, due: u32, preset: Preset, warm_up: bool) -> bool {
    let most = if warm_up {
        exchange_wait_s(preset)
    } else {
        beacon_exchange_wait_s(preset)
    };
    now < due.saturating_add(most)
}

/// The exchange a due beacon waits for: the one it chose this round while
/// that is still in progress (`chosen_running`), or before it has chosen,
/// any in progress (`open`). Exchanges that start later do not hold it again.
pub fn beacon_exchange(
    chosen: Option<ExchangeKey>,
    chosen_running: bool,
    open: Option<ExchangeKey>,
) -> Option<ExchangeKey> {
    match chosen {
        Some(k) => chosen_running.then_some(k),
        None => open,
    }
}

/// A beacon still in the air queue is taken back to wait for `exchange` only
/// if it has not waited one out this round, so once at most.
pub fn beacon_taken_back(chosen: Option<ExchangeKey>, exchange: Option<ExchangeKey>) -> bool {
    chosen.is_none() && exchange.is_some()
}

/// What an exchange's frames have in common: the line a receipt answers, or
/// the mail on the air.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExchangeKey {
    Line(MsgId),
    Mail,
}

impl std::fmt::Display for ExchangeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Line(id) => write!(f, "line {} and its receipt", id.hex()),
            Self::Mail => f.write_str("a mail and its acceptance"),
        }
    }
}

/// A chat frame's part in an exchange: `(key, answer)`. A line that asks
/// for a receipt opens its exchange, and a receipt for it answers it.
pub fn exchange_step(env: &Envelope) -> Option<(ExchangeKey, bool)> {
    match env.kind {
        MsgType::Ack => env.acked_id().map(|id| (ExchangeKey::Line(id), true)),
        MsgType::Msg | MsgType::Form | MsgType::Checkin | MsgType::Status
            if env.flags.req_ack() =>
        {
            Some((ExchangeKey::Line(env.msg_id), false))
        }
        _ => None,
    }
}

/// Exchanges this station takes part in: a line keyed or heard here and its
/// receipt, a mail and its acceptance. A beacon keyed in between leaves this
/// station deaf to the answer, or collides with it at a neighbour, so a due
/// beacon waits until none is in progress.
#[derive(Default)]
pub struct Exchanges {
    open: HashMap<ExchangeKey, ExchangeTimes>,
}

#[derive(Clone, Copy, Default)]
struct ExchangeTimes {
    /// When a frame asking for an answer last went or came, 0 for never.
    asked: u32,
    /// When an answer last went or came, 0 for never.
    answered: u32,
}

impl ExchangeTimes {
    fn last(&self) -> u32 {
        self.asked.max(self.answered)
    }

    fn running(&self, now: u32, wait: u32, settle: u32) -> bool {
        (self.answered < self.asked && now < self.asked.saturating_add(wait))
            || now < self.last().saturating_add(settle)
    }
}

impl Exchanges {
    /// A frame of exchange `key` keyed or heard here at `now`.
    pub fn note(&mut self, key: ExchangeKey, answer: bool, now: u32) {
        if self.open.len() > 4096 && !self.open.contains_key(&key) {
            self.open.clear();
        }
        let times = self.open.entry(key).or_default();
        if answer {
            times.answered = times.answered.max(now);
        } else {
            times.asked = times.asked.max(now);
        }
    }

    /// Exchange `key` asked less than `wait` ago and has not been answered
    /// since: its receipt is still to come.
    pub fn awaits_answer(&self, key: ExchangeKey, now: u32, wait: u32) -> bool {
        self.open
            .get(&key)
            .is_some_and(|t| t.answered < t.asked && now < t.asked.saturating_add(wait))
    }

    /// An exchange still in progress at `now`: one that asked less than
    /// `wait` ago and has not been answered since, or whose last frame went
    /// or came less than `settle` ago.
    pub fn in_progress(&mut self, now: u32, wait: u32, settle: u32) -> Option<ExchangeKey> {
        self.open
            .retain(|_, t| now < t.last().saturating_add(wait.max(settle)));
        self.open
            .iter()
            .find(|(_, t)| t.running(now, wait, settle))
            .map(|(k, _)| *k)
    }

    /// Exchange `key` is in progress at `now`, as [`Exchanges::in_progress`].
    pub fn is_open(&self, key: ExchangeKey, now: u32, wait: u32, settle: u32) -> bool {
        self.open
            .get(&key)
            .is_some_and(|t| t.running(now, wait, settle))
    }
}

/// Own lines handed over this soon after start wait until another station
/// has been heard (`rf.startup_hold_s`). A neighbour started at the same time
/// may not be listening yet, and every station's first beacon is on the air
/// within about this long of its start.
pub const STARTUP_HOLD_S: u32 = BEACON_MAX_S as u32;

/// Seconds an own line still waits after start, 0 to go now. `hold` is the
/// configured hold; `heard_other`: a frame from another station has decoded
/// here.
pub fn startup_hold_s(now: u32, started: u32, hold: u32, heard_other: bool) -> u32 {
    if heard_other {
        return 0;
    }
    started.saturating_add(hold).saturating_sub(now)
}

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
    pub sides: Arc<Mutex<AckSides>>,
}

impl Engine {
    pub fn new(store: Arc<Store>, our_call: String) -> Self {
        Self {
            store,
            our_call,
            sides: Arc::new(Mutex::new(AckSides::default())),
        }
    }

    /// A receipt heard on the air for a line that is not ours, from the
    /// originator's side of us. See [`AckSides`].
    fn ack_from_upstream(&self, ack: &Envelope, medium: &str) -> bool {
        if ack.kind != MsgType::Ack || !medium.eq_ignore_ascii_case("rf") {
            return false;
        }
        let Some(line) = ack.acked_id() else {
            return false;
        };
        let ours = self.store.get(&line).ok().flatten();
        let mut sides = self.sides.lock();
        match ack_upstream_by_hops(ack, ours.as_ref().map(|m| &m.env)) {
            Some(true) => {
                sides.mark_upstream(line, ack.origin.as_str());
                true
            }
            Some(false) => false,
            None => sides.from_upstream(line, ack.origin.as_str(), Instant::now()),
        }
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
                let upstream = self.ack_from_upstream(env, medium);
                if may_rebroadcast(&self.our_call, &stored, delivery) && !upstream {
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
        assert!(!duplicate_suppresses(&dm, 5, 4), "the sender's retry");
        assert!(duplicate_suppresses(&dm, 4, 4));
        assert!(duplicate_suppresses(&dm, 3, 4));
    }

    /// QA v0.1.93: A's stuck modem keyed an invite and its first retry back to
    /// back. B took the retry for a relay further on and dropped its own copy,
    /// so the invite went on only after A's next retry.
    #[test]
    fn the_senders_retry_keeps_our_relay_of_a_direct_line() {
        let store = Arc::new(Store::open_memory().unwrap());
        let engine = Engine::new(store.clone(), "M0BBB".into());
        let line = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("G4EEE").unwrap(),
            1,
            b"You are invited to #qa".to_vec(),
            6,
            Flags::new(),
        )
        .unwrap();
        assert_eq!(
            engine.on_rx(&line, "rf", None, None).unwrap().action,
            Action::Accept
        );
        assert_eq!(
            engine.on_rx(&line, "rf", None, None).unwrap().action,
            Action::Suppress
        );
        assert!(
            store
                .hold_due(u32::MAX)
                .unwrap()
                .iter()
                .any(|m| m.env.msg_id == line.msg_id),
            "our relay still goes"
        );
        // A copy with as few hops as ours is a relay beside or beyond us.
        let mut relayed = line.clone();
        relayed.hops_left = 5;
        engine.on_rx(&relayed, "rf", None, None).unwrap();
        assert!(!store
            .hold_due(u32::MAX)
            .unwrap()
            .iter()
            .any(|m| m.env.msg_id == line.msg_id));
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
        assert!(!duplicate_suppresses(&dm, 5, 4));
        assert!(duplicate_suppresses(&dm, 3, 4));
        assert!(!closed_group_walks(&{
            let mut b = group.clone();
            b.dest = Callsign::from_raw("BULLETIN");
            b
        }));
    }

    #[test]
    fn a_relay_keys_on_the_preset_rung_in_whole_frame_slots() {
        assert_eq!(relay_rung(Preset::HfPoor), Rung::Rdm600S);
        assert_eq!(relay_rung(Preset::HfWeak), Rung::Rdm300S);
        assert_eq!(relay_rung(Preset::VhfFm), Rung::OfdmQpskHalf);
        assert_eq!(relay_rung(Preset::HfDeep), Rung::Mfsk32R);
        assert!(relay_rung(Preset::HfPoor).payload_bytes() >= Preset::HfPoor.payload_bytes());
        let slot = frame_slot(Preset::HfPoor);
        assert!(slot > std::time::Duration::from_secs(3));
        for _ in 0..12 {
            let delay = spread_delay(Preset::HfPoor, Priority::Routine);
            let wide = slot.mul_f64(1.25).as_secs_f64();
            let n = (delay.as_secs_f64() / wide).round() as u32;
            assert!(n < spread_slots(Priority::Routine));
            assert!((delay.as_secs_f64() - wide * n as f64).abs() < 0.01);
        }
    }

    #[test]
    fn a_neighbour_is_a_station_heard_without_a_relay() {
        let line = group_line();
        assert!(heard_direct(&line, None));
        let mut relayed = line.clone();
        relayed.hops_left -= 1;
        assert!(!heard_direct(&relayed, None));
        let ack = Envelope::ack_for(&line, Callsign::parse("M0DDD").unwrap(), 1, None);
        let mut ack = ack;
        ack.hops_left = crate::emcomm::BULLETIN_TTL;
        assert!(heard_direct(&ack, Some(&line)));
        assert!(heard_direct(&ack, None));
        ack.hops_left -= 1;
        assert!(!heard_direct(&ack, Some(&line)));
        assert!(!heard_direct(&ack, None));
    }

    #[test]
    fn a_relay_spaces_its_copies_of_a_line() {
        let gap = relay_copy_gap_s(Priority::Routine);
        assert_eq!(gap, 45);
        // A copy handed to the modem at 100 (after its slot): the next one
        // waits until 145, so a stuck modem cannot key the two back to back.
        assert!(!answer_again(130, Some(100), gap));
        assert!(answer_again(145, Some(100), gap));
        // A slot delay puts the last copy in the future: still too soon.
        assert!(!answer_again(100, Some(108), gap));
        // Urgent lines keep their own faster pace.
        assert_eq!(relay_copy_gap_s(Priority::Emergency), 8);
    }

    #[test]
    fn the_end_of_a_chain_does_not_repeat_a_line() {
        let n = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // A at the end hears only B, and B relayed the line to it.
        assert!(neighbours_hold(&n(&["M0BBB"]), &[], true));
        // A heard it from the originator itself: B may not have it.
        assert!(!neighbours_hold(&n(&["M0BBB"]), &[], false));
        // B has heard only the originator A, not C beyond it (every C
        // beacon lost): B cannot tell it is not the end.
        assert!(!neighbours_hold(&n(&["G4AAA"]), &n(&["G4AAA"]), false));
        // B in the middle: A sent it, C has not answered yet.
        let b = n(&["G4AAA", "2E0CCC"]);
        assert!(!neighbours_hold(&b, &n(&["G4AAA"]), false));
        assert!(neighbours_hold(&b, &n(&["G4AAA", "2E0CCC"]), false));
        // Nobody heard yet: keep the old walk.
        assert!(!neighbours_hold(&[], &n(&["G4AAA"]), true));
    }

    #[test]
    fn a_direct_message_retry_passes_on_after_a_further_relay() {
        let mut dm = group_line();
        dm.flags = Flags::new();
        dm.dest = Callsign::parse("G4EEE").unwrap();
        assert!(
            retry_passes_on(&dm, true, false),
            "loss can be past the next hop"
        );
        let line = group_line();
        assert!(!retry_passes_on(&line, true, false));
        assert!(retry_passes_on(&line, true, true), "receipts still missing");
        assert!(retry_passes_on(&line, false, false));
        let ack = Envelope::ack_for(&dm, Callsign::parse("G4EEE").unwrap(), 1, None);
        assert!(
            !retry_passes_on(&ack, true, false),
            "a receipt walk still stops"
        );
    }

    #[test]
    fn an_invite_travels_as_far_as_its_group() {
        let invite = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("G4EEE").unwrap(),
            1,
            crate::slash::invite_notice("#qa", "G4AAA").into_bytes(),
            crate::emcomm::BULLETIN_TTL,
            Flags::new(),
        )
        .unwrap();
        assert!(is_group_invite(&invite));
        assert_eq!(initial_hops(&invite), crate::emcomm::BULLETIN_TTL);
        let mut dm = invite.clone();
        dm.body = b"hello".to_vec();
        assert!(!is_group_invite(&dm));
        assert_eq!(initial_hops(&dm), Priority::Routine.default_ttl());
    }

    #[test]
    fn only_the_originators_neighbours_ack_a_bulletin() {
        let mut bulletin = group_line();
        bulletin.dest = Callsign::from_raw("BULLETIN");
        assert_eq!(initial_hops(&bulletin), crate::emcomm::BULLETIN_TTL);
        assert!(acks_line(&bulletin));
        let mut relayed = bulletin.clone();
        relayed.hops_left -= 1;
        assert!(!acks_line(&relayed));
        let mut group = group_line();
        group.hops_left = 2;
        assert!(acks_line(&group), "every closed-group member answers");
        let dm = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::parse("M0BBB").unwrap(),
            1,
            b"dm".to_vec(),
            Priority::Routine.default_ttl(),
            Flags::new(),
        )
        .unwrap();
        assert!(heard_from_origin(&dm, 3));
        assert!(!heard_from_origin(&dm, 2));
    }

    #[test]
    fn a_receipt_from_the_originators_side_is_not_carried() {
        let line = group_line();
        let mut sides = AckSides::default();
        let t0 = Instant::now();
        // B answered before our relay of the line went out: B is upstream.
        assert!(sides.from_upstream(line.msg_id, "M0BBB", t0));
        sides.relaying(line.msg_id, t0 + std::time::Duration::from_secs(8));
        assert!(sides.from_upstream(line.msg_id, "M0BBB", t0 + std::time::Duration::from_secs(4)));
        // D answered after it: D heard our relay, so D's receipt is ours to carry.
        let later = t0 + std::time::Duration::from_secs(30);
        assert!(!sides.from_upstream(line.msg_id, "M0DDD", later));
        assert!(sides.is_upstream(&line.msg_id, "m0bbb"));
        assert!(sides.from_upstream(line.msg_id, "M0BBB", later));
        assert!(!sides.is_upstream(&line.msg_id, "M0DDD"));
    }

    #[test]
    fn the_engine_relays_only_receipts_from_beyond_us() {
        let store = Arc::new(Store::open_memory().unwrap());
        let engine = Engine::new(store.clone(), "2E0CCC".into());
        // C heard the line one hop out (5 left); B heard it at 6, D at 4.
        let line = group_line();
        let mut at_c = line.clone();
        at_c.hops_left = 5;
        engine.on_rx(&at_c, "rf", None, None).unwrap();
        let ack_by = |call: &str, seq: u32, heard: u8| {
            let mut copy = line.clone();
            copy.hops_left = heard;
            let mut a = Envelope::ack_for(&copy, Callsign::parse(call).unwrap(), seq, None);
            a.hops_left = crate::emcomm::BULLETIN_TTL;
            a
        };
        let hold_of = |id: &MsgId| {
            store
                .hold_due(u32::MAX)
                .unwrap()
                .iter()
                .any(|m| m.env.msg_id == *id)
        };
        let from_b = ack_by("M0BBB", 1, 6);
        engine.on_rx(&from_b, "rf", None, None).unwrap();
        assert!(
            !hold_of(&from_b.msg_id),
            "an upstream receipt is not relayed"
        );
        assert!(engine.sides.lock().is_upstream(&line.msg_id, "M0BBB"));
        let from_d = ack_by("M0DDD", 2, 4);
        engine.on_rx(&from_d, "rf", None, None).unwrap();
        assert!(
            hold_of(&from_d.msg_id),
            "a receipt from beyond us walks back"
        );
        let from_hub = ack_by("M0HUB", 3, 6);
        engine.on_rx(&from_hub, "inet", None, None).unwrap();
        assert!(
            hold_of(&from_hub.msg_id),
            "a gateway still keys a hub receipt"
        );

        // An older receipt has no hop byte: answering before our relay of the
        // line went out marks the sender upstream.
        let old = |call: &str, seq: u32| {
            let mut a = ack_by(call, seq, 5);
            a.body.truncate(crate::proto::ids::MSG_ID_LEN);
            a.msg_id = MsgId::compute(&a.origin, &a.dest, seq, &a.body);
            a
        };
        let early = old("M0EEE", 4);
        engine.on_rx(&early, "rf", None, None).unwrap();
        assert!(!hold_of(&early.msg_id));
        engine.sides.lock().relaying(line.msg_id, Instant::now());
        let late = old("M0FFF", 5);
        engine.on_rx(&late, "rf", None, None).unwrap();
        assert!(hold_of(&late.msg_id));
    }

    #[test]
    fn a_receipt_relay_walks_and_repeats_once_next_to_the_originator() {
        let line = group_line();
        let ack = Envelope::ack_for(&line, Callsign::parse("M0DDD").unwrap(), 1, None);
        assert!(ack_relay_walks(&ack, false, 0, 3, false));
        assert!(ack_relay_walks(&ack, true, 0, 3, false), "one repeat");
        assert!(!ack_relay_walks(&ack, true, 1, 3, false), "and no more");
        assert!(!ack_relay_walks(&ack, true, 0, 0, false));
        assert!(!ack_relay_walks(&ack, false, 0, 3, true));
        assert!(!ack_relay_walks(&ack, false, 3, 3, false));
        assert!(!ack_relay_walks(&line, false, 0, 3, false));
    }

    #[test]
    fn beacons_keep_their_pace_whatever_the_traffic() {
        let start = 10_000;
        assert_eq!(next_beacon_at(start, None, 0, 60), start + 60, "a pace in");
        // A pace after the one before, counted from when it reached the
        // modem, until three have gone: one held back still counts.
        assert_eq!(next_beacon_at(start, Some(start + 75), 1, 60), start + 135);
        assert_eq!(next_beacon_at(start, Some(start + 400), 2, 60), start + 460);
        // Then every five minutes, with the round's jitter. Traffic no
        // longer holds it for ten minutes.
        assert_eq!(
            next_beacon_at(start, Some(start + 500), 3, 60),
            start + 500 + BEACON_IDLE_S
        );
        assert_eq!(
            next_beacon_at(start, Some(start + 500), 3, 75),
            start + 500 + BEACON_IDLE_S + 15
        );
        // QA v0.1.95: a short roll took quiet gaps to 285 s. Jitter only
        // adds to the five minutes.
        assert_eq!(
            next_beacon_at(start, Some(start + 500), 3, 45),
            start + 500 + BEACON_IDLE_S + 15
        );
    }

    /// QA v0.1.96: A's second warm-up beacon waited out its direct message,
    /// and its third was taken back from the air queue for one invite, then
    /// another. A beacon waits out one exchange and is taken back once.
    #[test]
    fn a_beacon_waits_out_one_exchange_and_is_taken_back_once() {
        let dm = ExchangeKey::Line(MsgId([1; 8]));
        let invite = ExchangeKey::Line(MsgId([2; 8]));
        assert_eq!(beacon_exchange(None, false, Some(dm)), Some(dm));
        assert_eq!(beacon_exchange(Some(dm), true, Some(invite)), Some(dm));
        assert_eq!(
            beacon_exchange(Some(dm), false, Some(invite)),
            None,
            "the next invite does not hold it again"
        );
        assert!(beacon_taken_back(None, Some(invite)));
        assert!(!beacon_taken_back(Some(invite), Some(dm)), "only once");
        assert!(!beacon_taken_back(None, None));
    }

    #[test]
    fn a_line_and_its_receipt_make_an_exchange() {
        let line = Envelope::new_msg(
            Callsign::parse("G4AAA").unwrap(),
            Callsign::from_raw("NET"),
            1,
            b"g".to_vec(),
            6,
            Flags::new()
                .with(crate::proto::FLAG_GROUP)
                .with(crate::proto::FLAG_REQ_ACK),
        )
        .unwrap();
        let key = ExchangeKey::Line(line.msg_id);
        assert_eq!(exchange_step(&line), Some((key, false)));
        let ack = Envelope::ack_for(&line, Callsign::parse("M0DDD").unwrap(), 1, None);
        assert_eq!(exchange_step(&ack), Some((key, true)));
        let mut beacon = line.clone();
        beacon.kind = MsgType::Beacon;
        assert_eq!(exchange_step(&beacon), None);
        assert_eq!(exchange_step(&group_line()), None, "asks for no receipt");
    }

    #[test]
    fn a_due_beacon_waits_for_an_exchange_in_progress() {
        let p = Preset::HfPoor;
        let (wait, settle) = (exchange_wait_s(p), exchange_settle_s(p));
        assert!((60..=80).contains(&wait), "a receipt's round trip: {wait}");
        assert!((25..=35).contains(&settle), "a spread window: {settle}");
        let line = ExchangeKey::Line(MsgId::from_bytes([1; 8]));
        let mut x = Exchanges::default();
        let t = 50_000;
        x.note(line, false, t);
        assert_eq!(
            x.in_progress(t + 60, wait, settle),
            Some(line),
            "no receipt yet"
        );
        assert_eq!(x.in_progress(t + wait, wait, settle), None, "it never came");
        x.note(line, false, t + 100);
        x.note(line, true, t + 130);
        assert_eq!(
            x.in_progress(t + 129 + settle, wait, settle),
            Some(line),
            "the rest of the receipts land in the same window"
        );
        assert_eq!(x.in_progress(t + 130 + settle, wait, settle), None);
        x.note(ExchangeKey::Mail, false, t + 500);
        assert_eq!(
            x.in_progress(t + 550, wait, settle),
            Some(ExchangeKey::Mail)
        );
        assert!(x.is_open(ExchangeKey::Mail, t + 550, wait, settle));
        assert!(!x.is_open(line, t + 550, wait, settle));
        x.note(ExchangeKey::Mail, true, t + 560);
        assert_eq!(
            x.in_progress(t + 560 + settle, wait, settle),
            None,
            "accepted"
        );
        // One exchange at most past when it fell due.
        let due = t + 600;
        let most = beacon_exchange_wait_s(p);
        assert_eq!(most, wait + settle);
        assert!(beacon_waits_for_exchange(due + most - 1, due, p, false));
        assert!(!beacon_waits_for_exchange(due + most, due, p, false));
        // A warm-up beacon waits one receipt's round trip at most.
        assert!(beacon_waits_for_exchange(due + wait - 1, due, p, true));
        assert!(!beacon_waits_for_exchange(due + wait, due, p, true));
        // A mail holds it until the 10-minute ceiling and one exchange.
        let ceiling = due + 300;
        assert!(beacon_waits_for_mail(ceiling + wait - 1, ceiling, p));
        assert!(!beacon_waits_for_mail(ceiling + wait, ceiling, p));
    }

    #[test]
    fn own_lines_wait_for_another_station_after_start() {
        let (start, hold) = (10_000, STARTUP_HOLD_S);
        assert_eq!(startup_hold_s(start + 10, start, hold, false), hold - 10);
        assert_eq!(
            startup_hold_s(start + 10, start, hold, true),
            0,
            "someone was heard"
        );
        assert_eq!(startup_hold_s(start + hold, start, hold, false), 0);
        assert_eq!(startup_hold_s(start + 10, start, 0, false), 0, "turned off");
    }

    #[test]
    fn a_receipt_never_drops_the_first_relay_from_the_queue() {
        assert!(
            !queued_repeat_unneeded(0),
            "nothing handed to the queue yet"
        );
        assert!(
            !queued_repeat_unneeded(1),
            "the first relay may still be queued"
        );
        assert!(queued_repeat_unneeded(2), "a repeat is queued");
    }

    #[test]
    fn a_busy_channel_holds_a_beacon_only_while_frames_decode() {
        let start = 10_000;
        assert_eq!(beacon_ceiling(start, None), start + BEACON_MAX_GAP_S);
        let last = start + 400;
        let ceiling = beacon_ceiling(start, Some(last));
        assert_eq!(ceiling, last + BEACON_MAX_GAP_S);
        let due = last + BEACON_IDLE_S;
        assert_eq!(
            beacon_channel_wait(due, ceiling, true, 3),
            7,
            "busy with a frame decoded 3 s ago: wait until it is 10 s old"
        );
        assert_eq!(
            beacon_channel_wait(due, ceiling, true, 3_600),
            0,
            "a noise floor or stuck carrier sense: busy, nothing decoded"
        );
        assert_eq!(beacon_channel_wait(due, ceiling, false, 3), 0, "quiet");
        assert_eq!(
            beacon_channel_wait(ceiling - 2, ceiling, true, 0),
            2,
            "never past ten minutes between beacons"
        );
        assert_eq!(beacon_channel_wait(ceiling, ceiling, true, 0), 0);
    }
}
