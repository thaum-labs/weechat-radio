//! SPDX-License-Identifier: Apache-2.0
//! Mail gateway Auto (#275): a Radio+ station asks which internet-radio
//! gateways hear it, and picks the one with the best two-way link.
//!
//! 1. The station keys a short signed gateway ask (its callsign and a nonce).
//! 2. Every gateway that decodes it reports to the hub the SNR it heard and
//!    its own grid.
//! 3. The hub waits a short window, then gives the reporting gateways reply
//!    slots in turn, best SNR first, at most [`MAX_REPLIES`], so no replies
//!    overlap.
//! 4. Each reply carries the SNR the gateway heard. The station measures the
//!    reply's SNR, ranks by the weaker of the two, and keeps the winner.
//!
//! Every time here follows the preset's airtime, so the same code works on
//! VHF/UHF FM and on HF (vhf-fm is short, hf-weak and hf-deep much longer).

use super::air::keyed_airtime_secs;
use super::proto::{self, MailMeta, MailOp, MailWire};
use crate::presets::{Preset, Rung};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The hub gives reply slots to this many gateways at most.
pub const MAX_REPLIES: usize = 3;
/// A pick older than this is asked again (HF drifts), by itself and before
/// the next mail.
pub const ASK_EVERY_SECS: i64 = 3600;
/// Mail or Check mail failing this many times in a row on the picked gateway
/// moves to the next one on the list.
pub const FAILS_BEFORE_NEXT: u32 = 2;
/// Two links within this many dB are a tie. Only then does distance count.
pub const TIE_DB: f32 = 0.5;
/// After an ask that keyed nothing (radio not up yet), wait this long before
/// the next try, so a station without a radio does not try every tick.
pub const ASK_RETRY_SECS: i64 = 60;
/// After an ask that heard no gateway, Auto asks again by itself after this
/// long, doubling each time nobody answers, up to [`NONE_HEARD_MAX_SECS`]
/// (hourly). A gateway may have been busy or switched off; a lone station
/// must not key every minute either. A gateway beacon heard asks sooner.
pub const NONE_HEARD_FIRST_SECS: i64 = 120;
pub const NONE_HEARD_MAX_SECS: i64 = ASK_EVERY_SECS;

/// Turnaround the hub allows per frame. It does not know each station's.
const TURNAROUND_ALLOW_MS: u32 = 300;
/// Per slot, on top of the reply's airtime: the gateway's lease waits for a
/// quiet channel (1.5 s plus up to 0.8 s dither) and the hub's answer reaches
/// each gateway a little apart.
const SLOT_MARGIN_SECS: f64 = 4.0;
/// From the end of the ask on the air to the first report at the hub: the
/// last frame decodes, and the gateway's HTTPS call reaches the hub.
const REPORT_ALLOW_SECS: f64 = 5.0;
/// The hub's shortest wait for more reports after the first ("~2 s").
const HUB_WINDOW_MIN_SECS: f64 = 2.0;

/// Airtime of one full frame at the preset's starting rung.
fn frame_secs(preset: Preset) -> f64 {
    let mtu = Rung::from_index(preset.ladder_start()).payload_bytes() as usize;
    keyed_airtime_secs(preset, std::iter::once(mtu), TURNAROUND_ALLOW_MS)
}

/// The frames a gateway reply keys, as the gateway builds them.
pub fn reply_frames(nonce: &str, meta: &MailMeta, payload: &str, preset: Preset) -> Vec<Vec<u8>> {
    let mtu = super::air::mail_rung(preset, 0).payload_bytes() as usize;
    let wire = MailWire {
        op: MailOp::GwReply,
        mail_id: nonce.to_string(),
        idx: 0,
        count: 1,
        meta: meta.clone(),
        payload: payload.to_string(),
    };
    proto::frames_for_mtu(&proto::encode_chunk(&wire), mtu).unwrap_or_default()
}

/// Airtime of the longest reply a gateway keys: long callsigns, a low SNR
/// and an 8-character grid.
pub fn reply_secs(preset: Preset) -> f64 {
    let meta = MailMeta {
        from: "MM0ABC/P".into(),
        dest: "MM0XYZ/P".into(),
        ..MailMeta::default()
    };
    let payload = reply_payload(Some(-21.5), "IO91WM12");
    let frames = reply_frames("0123456789abcdef", &meta, &payload, preset);
    keyed_airtime_secs(preset, frames.iter().map(Vec::len), TURNAROUND_ALLOW_MS)
}

/// One gateway's reply slot: its airtime plus room for the lease.
pub fn slot_secs(preset: Preset) -> f64 {
    reply_secs(preset) + SLOT_MARGIN_SECS
}

/// How long the hub waits after the first report for the others. Gateways
/// decode the same ask at nearly the same moment; on a slow preset their
/// decoders finish further apart.
pub fn hub_window_secs(preset: Preset) -> f64 {
    HUB_WINDOW_MIN_SECS.max(frame_secs(preset) * 0.5)
}

/// How long the asking station listens after its ask is on the air: the
/// reports, the hub's window, every slot and half a slot of grace.
pub fn listen_secs(preset: Preset) -> f64 {
    let slot = slot_secs(preset);
    REPORT_ALLOW_SECS + hub_window_secs(preset) + MAX_REPLIES as f64 * slot + slot * 0.5
}

/// What the station signs, so the hub only coordinates asks it really keyed.
pub fn ask_token(callsign: &str, ts: i64, nonce: &str) -> Vec<u8> {
    format!(
        "wcr-mail-gateway-ask\n{}\n{ts}\n{nonce}",
        callsign.trim().to_ascii_uppercase()
    )
    .into_bytes()
}

/// The ask a Radio+ station keys: its own signature over [`ask_token`].
pub fn signed_ask(
    keys: &crate::proto::IdentityKeys,
    callsign: &str,
    ts: i64,
    nonce: &str,
    grid: &str,
) -> AskPayload {
    AskPayload {
        ts,
        pk: keys.public_hex(),
        sig: hex::encode(keys.sign_bytes(&ask_token(callsign, ts, nonce))),
        g: grid.to_string(),
    }
}

/// What a gateway that heard `ask` posts to the hub. The station's signature
/// is passed on untouched; the gateway signs the whole post with its own key.
pub fn ask_report(
    asker: &str,
    nonce: &str,
    ask: &AskPayload,
    snr: Option<f32>,
    gateway_grid: &str,
    preset: &str,
) -> serde_json::Value {
    serde_json::json!({
        "for": asker,
        "n": nonce,
        "ts": ask.ts,
        "pk": ask.pk,
        "sig": ask.sig,
        "g": ask.g,
        "snr": snr,
        "grid": gateway_grid,
        "preset": preset,
    })
}

/// The ask's payload on the air. Its wire id is the nonce.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AskPayload {
    pub ts: i64,
    pub pk: String,
    pub sig: String,
    /// The asking station's grid, for the distance tie-break.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub g: String,
}

/// `s=<snr> g=<grid>`: the SNR the gateway heard the ask at, and its grid.
pub fn reply_payload(snr: Option<f32>, grid: &str) -> String {
    let mut out = String::new();
    if let Some(s) = snr {
        out.push_str(&format!("s={s:.1}"));
    }
    let g = grid.trim();
    if !g.is_empty() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!("g={g}"));
    }
    out
}

/// `(snr the gateway heard, gateway grid)` from a reply payload.
pub fn parse_reply(payload: &str) -> (Option<f32>, String) {
    let mut snr = None;
    let mut grid = String::new();
    for part in payload.split_whitespace() {
        if let Some(v) = part.strip_prefix("s=") {
            snr = v.parse::<f32>().ok().filter(|x| x.is_finite());
        } else if let Some(v) = part.strip_prefix("g=") {
            grid = crate::grid::position(v).unwrap_or_default();
        }
    }
    (snr, grid)
}

/// Great-circle distance between two grid squares, in km.
pub fn distance_km(a: &str, b: &str) -> Option<f64> {
    let (la1, lo1) = crate::grid::to_lat_lon(crate::grid::position(a)?.as_str()).ok()?;
    let (la2, lo2) = crate::grid::to_lat_lon(crate::grid::position(b)?.as_str()).ok()?;
    let (p1, p2) = (la1.to_radians(), la2.to_radians());
    let dp = (la2 - la1).to_radians();
    let dl = (lo2 - lo1).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    Some(6371.0 * 2.0 * h.sqrt().min(1.0).asin())
}

/// A gateway that answered an ask.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub call: String,
    /// SNR the gateway heard our ask at.
    pub up: Option<f32>,
    /// SNR we heard the gateway's reply at.
    pub down: Option<f32>,
    #[serde(default)]
    pub grid: String,
    /// From our grid to the gateway's, when both are known.
    #[serde(default)]
    pub km: Option<f64>,
}

impl Candidate {
    /// The link works only if both directions do: the weaker of the two.
    pub fn link(&self) -> Option<f32> {
        match (self.up, self.down) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

/// Ties within [`TIE_DB`] go to the nearer station. Distance never beats a
/// better link: on HF a near station can sit in the skip zone.
fn better(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let by_link = match (a.link(), b.link()) {
        (Some(x), Some(y)) if (x - y).abs() < TIE_DB => Ordering::Equal,
        (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    by_link
        .then_with(|| match (a.km, b.km) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| a.call.cmp(&b.call))
}

/// Best first. Fills in each distance from `my_grid`.
pub fn rank(mut list: Vec<Candidate>, my_grid: &str) -> Vec<Candidate> {
    for c in &mut list {
        c.km = distance_km(my_grid, &c.grid);
    }
    let mut seen = std::collections::HashSet::new();
    list.retain(|c| seen.insert(c.call.to_ascii_uppercase()));
    list.sort_by(better);
    list
}

/// One gateway's report of an ask, at the hub.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub gateway: String,
    pub snr: Option<f32>,
    #[serde(default)]
    pub grid: String,
}

/// A reply slot the hub gives a gateway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub gateway: String,
    pub idx: usize,
    /// Seconds after the hub's answer that this gateway keys.
    pub delay_secs: f64,
}

/// Best SNR first (distance to the asking station breaks a tie), at most
/// [`MAX_REPLIES`], each slot one `slot` after the one before.
pub fn assign_slots(reports: &[Report], asker_grid: &str, slot: f64) -> Vec<Slot> {
    let cands = reports
        .iter()
        .map(|r| Candidate {
            call: r.gateway.to_ascii_uppercase(),
            up: r.snr,
            down: None,
            grid: r.grid.clone(),
            km: None,
        })
        .collect();
    rank(cands, asker_grid)
        .into_iter()
        .take(MAX_REPLIES)
        .enumerate()
        .map(|(idx, c)| Slot {
            gateway: c.call,
            idx,
            delay_secs: idx as f64 * slot,
        })
        .collect()
}

struct Round {
    opened: Instant,
    reports: Vec<Report>,
    done: tokio::sync::watch::Sender<Option<Vec<Slot>>>,
}

/// The hub's side: collects the reports of one ask, then answers every
/// waiting gateway with its slot at the same moment.
#[derive(Default)]
pub struct AskBoard {
    rounds: Mutex<HashMap<String, Round>>,
}

/// Rounds are forgotten after this long.
const ROUND_KEEP: Duration = Duration::from_secs(600);

impl AskBoard {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Report that `report.gateway` heard `asker`'s ask `nonce`. Waits until
    /// the hub's window closes and returns this gateway's slot, or `None`
    /// when it is not one of the best [`MAX_REPLIES`] or came too late.
    pub async fn report(
        self: &Arc<Self>,
        asker: &str,
        nonce: &str,
        asker_grid: &str,
        preset: Preset,
        report: Report,
    ) -> Option<Slot> {
        let key = format!("{}:{nonce}", asker.to_ascii_uppercase());
        let me = report.gateway.to_ascii_uppercase();
        let window = Duration::from_secs_f64(hub_window_secs(preset));
        let (mut rx, first) = {
            let mut rounds = self.rounds.lock();
            rounds.retain(|_, r| r.opened.elapsed() < ROUND_KEEP);
            let first = !rounds.contains_key(&key);
            let round = rounds.entry(key.clone()).or_insert_with(|| Round {
                opened: Instant::now(),
                reports: Vec::new(),
                done: tokio::sync::watch::channel(None).0,
            });
            if round.done.borrow().is_some() {
                // The slots are out already. A late gateway keys nothing.
                return None;
            }
            if !round
                .reports
                .iter()
                .any(|r| r.gateway.eq_ignore_ascii_case(&me))
            {
                round.reports.push(report);
            }
            (round.done.subscribe(), first)
        };
        if first {
            let board = Arc::clone(self);
            let key = key.clone();
            let grid = asker_grid.to_string();
            tokio::spawn(async move {
                tokio::time::sleep(window).await;
                let rounds = board.rounds.lock();
                if let Some(r) = rounds.get(&key) {
                    let slots = assign_slots(&r.reports, &grid, slot_secs(preset));
                    let _ = r.done.send(Some(slots));
                }
            });
        }
        let wait = window + Duration::from_secs(5);
        let slots = tokio::time::timeout(wait, async {
            loop {
                if let Some(s) = rx.borrow().clone() {
                    return Some(s);
                }
                if rx.changed().await.is_err() {
                    return None;
                }
            }
        })
        .await
        .ok()
        .flatten()?;
        slots.into_iter().find(|s| s.gateway == me)
    }
}

/// Where Auto stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PickState {
    /// Not asked yet in this run.
    #[default]
    Idle,
    Asking,
    Picked,
    /// Asked, and no gateway replied.
    NoneHeard,
}

/// The current Auto pick: the ranked list and which one is in use.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GatewayPick {
    pub state: PickState,
    pub list: Vec<Candidate>,
    pub current: usize,
    /// Unix seconds of the last ask.
    pub asked_at: i64,
    /// Preset and dial the list was heard on.
    pub preset: String,
    pub freq_khz: u32,
    /// Failures in a row on the current gateway.
    pub fails: u32,
    /// Asks in a row that heard no gateway. Sets the wait before the next.
    #[serde(default)]
    pub misses: u32,
    /// A gateway beacon already started an ask in this wait. One per wait,
    /// so a gateway that cannot hear us does not draw an ask every beacon.
    #[serde(default)]
    pub beacon_spent: bool,
}

pub const NO_GATEWAY_HEARD: &str = "No mail gateway heard";

impl GatewayPick {
    pub fn current_call(&self) -> Option<&str> {
        if self.state != PickState::Picked {
            return None;
        }
        self.list.get(self.current).map(|c| c.call.as_str())
    }

    /// Why the pick must be asked again before use, or `None` if it is good.
    pub fn stale(&self, now: i64, preset: &str, freq_khz: u32) -> Option<&'static str> {
        match self.state {
            PickState::Idle => Some("not asked yet"),
            PickState::Asking => None,
            _ if !self.preset.eq_ignore_ascii_case(preset) => Some("the preset changed"),
            _ if self.freq_khz != freq_khz => Some("the band changed"),
            _ if now - self.asked_at >= ASK_EVERY_SECS => Some("the pick is an hour old"),
            PickState::NoneHeard => Some("no gateway was heard last time"),
            PickState::Picked => None,
        }
    }

    /// Store the outcome of an ask: the ranked list, best first.
    pub fn settle(&mut self, list: Vec<Candidate>, now: i64, preset: &str, freq_khz: u32) {
        self.state = if list.is_empty() {
            self.misses = self.misses.saturating_add(1);
            PickState::NoneHeard
        } else {
            self.misses = 0;
            self.beacon_spent = false;
            PickState::Picked
        };
        self.list = list;
        self.current = 0;
        self.asked_at = now;
        self.preset = preset.to_string();
        self.freq_khz = freq_khz;
        self.fails = 0;
    }

    /// Move to the next gateway on the list that is not in `skip`. `None`
    /// when the list is used up.
    pub fn next_after(&mut self, skip: &[String]) -> Option<String> {
        let start = self.current + 1;
        for i in start..self.list.len() {
            let call = &self.list[i].call;
            if !skip.iter().any(|s| s.eq_ignore_ascii_case(call)) {
                self.current = i;
                self.fails = 0;
                return Some(call.clone());
            }
        }
        None
    }

    /// One failure on the current gateway. True once it has failed
    /// [`FAILS_BEFORE_NEXT`] times in a row and the next one should be tried.
    pub fn note_fail(&mut self) -> bool {
        self.fails = self.fails.saturating_add(1);
        self.fails >= FAILS_BEFORE_NEXT
    }

    pub fn note_ok(&mut self) {
        self.fails = 0;
    }

    /// True when an ask that heard nobody should be tried again by itself:
    /// [`NONE_HEARD_FIRST_SECS`] after the first miss, doubling per miss.
    pub fn retry_due(&self, now: i64) -> bool {
        if self.state != PickState::NoneHeard {
            return false;
        }
        let shift = self.misses.saturating_sub(1).min(10);
        let wait = (NONE_HEARD_FIRST_SECS << shift).min(NONE_HEARD_MAX_SECS);
        now - self.asked_at >= wait
    }

    /// True when a gateway beacon may start an ask now: none picked, and no
    /// beacon has started one since the last timed ask.
    pub fn beacon_may_ask(&self) -> bool {
        matches!(self.state, PickState::Idle | PickState::NoneHeard) && !self.beacon_spent
    }

    /// We heard `c` answer us outside an ask: a late ask reply, or a gateway
    /// acking our mail or answering Check mail. Real traffic beats an old
    /// "none heard". `proven` (it carried our mail) also makes it the one in
    /// use; a late ask reply only joins the list when one is already picked.
    /// Not for use while asking. True when the pick changed.
    pub fn heard(
        &mut self,
        c: Candidate,
        proven: bool,
        now: i64,
        preset: &str,
        freq_khz: u32,
    ) -> bool {
        if self.state == PickState::Asking {
            return false;
        }
        let at = self
            .list
            .iter()
            .position(|x| x.call.eq_ignore_ascii_case(&c.call));
        if self.state == PickState::Picked {
            match at {
                // Our gateway carried our mail: the pick is as good as a
                // fresh ask, so the hourly ask is not needed (less chatter).
                Some(i) if i == self.current => {
                    self.fails = 0;
                    if proven {
                        self.asked_at = now;
                    }
                    return false;
                }
                Some(i) if proven => {
                    self.current = i;
                    self.fails = 0;
                    self.asked_at = now;
                    return true;
                }
                Some(_) => return false,
                None if proven => {
                    self.list.insert(0, c);
                    self.current = 0;
                    self.fails = 0;
                    self.asked_at = now;
                    return true;
                }
                None => {
                    self.list.push(c);
                    return true;
                }
            }
        }
        // Not asked yet, or asked and none heard: this gateway is the pick.
        self.list = vec![c];
        self.current = 0;
        self.state = PickState::Picked;
        self.asked_at = now;
        self.preset = preset.to_string();
        self.freq_khz = freq_khz;
        self.fails = 0;
        self.misses = 0;
        self.beacon_spent = false;
        true
    }

    /// One line for setup and status.
    pub fn summary(&self) -> String {
        match self.state {
            PickState::Idle => "Auto: not asked yet".into(),
            PickState::Asking => "Auto: asking which gateways hear you…".into(),
            PickState::NoneHeard => NO_GATEWAY_HEARD.into(),
            PickState::Picked => match self.list.get(self.current) {
                Some(c) => {
                    let link = c
                        .link()
                        .map(|l| format!(", link {l:.0} dB"))
                        .unwrap_or_default();
                    let more = self.list.len().saturating_sub(self.current + 1);
                    let next = if more > 0 {
                        format!(", {more} more heard")
                    } else {
                        String::new()
                    };
                    format!("Auto: {}{link}{next}", c.call)
                }
                None => NO_GATEWAY_HEARD.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(call: &str, up: f32, down: f32, grid: &str) -> Candidate {
        Candidate {
            call: call.into(),
            up: Some(up),
            down: Some(down),
            grid: grid.into(),
            km: None,
        }
    }

    #[test]
    fn the_weaker_of_the_two_snrs_ranks() {
        // A hears us at +20 but we hear it at -5. B is +4 both ways.
        let list = rank(
            vec![
                cand("A1AAA", 20.0, -5.0, "IO91"),
                cand("B1BBB", 4.0, 4.0, "IO91"),
            ],
            "IO91WM",
        );
        assert_eq!(list[0].call, "B1BBB");
        assert_eq!(list[0].link(), Some(4.0));
        assert_eq!(list[1].link(), Some(-5.0));
    }

    #[test]
    fn distance_only_breaks_a_tie() {
        // Near is in the skip zone: a worse link loses however close it is.
        let me = "IO91WM";
        let list = rank(
            vec![
                cand("NEAR", -10.0, -12.0, "IO91WN"),
                cand("FAR", 3.0, 2.0, "JN58TD"),
            ],
            me,
        );
        assert_eq!(list[0].call, "FAR");
        // Same link within half a dB: the nearer station wins.
        let list = rank(
            vec![
                cand("FAR", 3.0, 2.0, "JN58TD"),
                cand("NEAR", 2.2, 5.0, "IO91WN"),
            ],
            me,
        );
        assert_eq!(list[0].call, "NEAR");
        assert!(list[0].km.unwrap() < list[1].km.unwrap());
        // A dB apart is not a tie.
        let list = rank(
            vec![
                cand("FAR", 3.0, 3.0, "JN58TD"),
                cand("NEAR", 2.0, 5.0, "IO91WN"),
            ],
            me,
        );
        assert_eq!(list[0].call, "FAR");
    }

    #[test]
    fn a_reply_payload_round_trips() {
        let p = reply_payload(Some(-7.25), "IO91WM");
        assert_eq!(p, "s=-7.2 g=IO91WM");
        assert_eq!(parse_reply(&p), (Some(-7.2), "IO91WM".to_string()));
        assert_eq!(parse_reply(""), (None, String::new()));
        assert_eq!(parse_reply("s=x g=zz"), (None, String::new()));
    }

    #[test]
    fn slots_never_overlap_and_take_the_best_three() {
        for preset in Preset::all() {
            let slot = slot_secs(preset);
            let reports: Vec<Report> = (0..5)
                .map(|i| Report {
                    gateway: format!("G{i}GW"),
                    snr: Some(i as f32 * 2.0),
                    grid: "IO91".into(),
                })
                .collect();
            let slots = assign_slots(&reports, "IO91WM", slot);
            assert_eq!(slots.len(), MAX_REPLIES, "{preset:?}");
            assert_eq!(slots[0].gateway, "G4GW", "best SNR first");
            assert_eq!(slots[1].gateway, "G3GW");
            assert_eq!(slots[2].gateway, "G2GW");
            let reply = reply_secs(preset);
            for w in slots.windows(2) {
                // The next reply starts only after this one's airtime and
                // the lease's quiet wait (at most 2.3 s) are over.
                assert!(
                    w[1].delay_secs >= w[0].delay_secs + reply + 2.3,
                    "{preset:?}: {w:?}"
                );
            }
        }
    }

    #[test]
    fn slot_timing_follows_the_preset() {
        let fm = slot_secs(Preset::VhfFm);
        let deep = slot_secs(Preset::HfDeep);
        // vhf-fm: one short frame. hf-deep: the reply is sliced into 55-byte
        // MFSK-32R frames at 99 bit/s.
        assert!(
            reply_secs(Preset::VhfFm) < 1.5,
            "{}",
            reply_secs(Preset::VhfFm)
        );
        assert!(fm < 6.0, "vhf-fm slot {fm}");
        assert!(
            reply_secs(Preset::HfDeep) > 8.0,
            "{}",
            reply_secs(Preset::HfDeep)
        );
        assert!(deep > 2.0 * fm, "hf-deep slot {deep} vs vhf-fm {fm}");
        assert!(slot_secs(Preset::HfWeak) > fm);
        // Every slot and the hub's window fit inside the asking station's listen.
        for p in [Preset::VhfFm, Preset::HfDeep] {
            let last_end =
                hub_window_secs(p) + (MAX_REPLIES - 1) as f64 * slot_secs(p) + reply_secs(p);
            assert!(listen_secs(p) > last_end + 3.0, "{p:?}");
            assert!(hub_window_secs(p) >= HUB_WINDOW_MIN_SECS);
        }
        assert!(listen_secs(Preset::HfDeep) > listen_secs(Preset::VhfFm) * 2.0);
    }

    #[test]
    fn a_pick_expires_and_falls_back() {
        let mut p = GatewayPick::default();
        assert_eq!(p.stale(0, "hf-poor", 7074), Some("not asked yet"));
        p.settle(
            vec![cand("A1", 5.0, 5.0, ""), cand("B1", 3.0, 3.0, "")],
            1_000,
            "hf-poor",
            7074,
        );
        assert_eq!(p.current_call(), Some("A1"));
        assert_eq!(p.stale(1_000 + 60, "hf-poor", 7074), None);
        assert_eq!(
            p.stale(1_000 + ASK_EVERY_SECS, "hf-poor", 7074),
            Some("the pick is an hour old")
        );
        assert_eq!(p.stale(1_100, "hf-deep", 7074), Some("the preset changed"));
        assert_eq!(p.stale(1_100, "hf-poor", 14_100), Some("the band changed"));
        assert!(!p.note_fail());
        assert!(p.note_fail());
        assert_eq!(p.next_after(&[]), Some("B1".to_string()));
        assert_eq!(p.current_call(), Some("B1"));
        assert_eq!(p.fails, 0);
        assert_eq!(p.next_after(&[]), None, "list used up");
        p.settle(Vec::new(), 2_000, "hf-poor", 7074);
        assert_eq!(p.state, PickState::NoneHeard);
        assert_eq!(p.summary(), NO_GATEWAY_HEARD);
        assert!(p.stale(2_001, "hf-poor", 7074).is_some());
    }

    #[test]
    fn none_heard_asks_again_later_and_less_often() {
        let mut p = GatewayPick::default();
        assert!(
            !p.retry_due(1_000_000),
            "never asked: the start-up ask covers it"
        );
        p.settle(Vec::new(), 1_000, "vhf-fm", 144_950);
        assert!(!p.retry_due(1_000 + NONE_HEARD_FIRST_SECS - 1));
        assert!(p.retry_due(1_000 + NONE_HEARD_FIRST_SECS));
        p.settle(Vec::new(), 2_000, "vhf-fm", 144_950);
        assert!(!p.retry_due(2_000 + NONE_HEARD_FIRST_SECS));
        assert!(p.retry_due(2_000 + 2 * NONE_HEARD_FIRST_SECS));
        for _ in 0..20 {
            p.settle(Vec::new(), 3_000, "vhf-fm", 144_950);
        }
        assert!(p.retry_due(3_000 + NONE_HEARD_MAX_SECS), "capped");
        p.settle(vec![cand("A1", 1.0, 1.0, "")], 4_000, "vhf-fm", 144_950);
        assert_eq!(p.misses, 0);
        assert!(!p.retry_due(1_000_000), "a pick is not re-asked by this");
    }

    #[test]
    fn one_beacon_ask_per_wait() {
        let mut p = GatewayPick::default();
        assert!(p.beacon_may_ask());
        p.beacon_spent = true;
        p.settle(Vec::new(), 1_000, "vhf-fm", 144_950);
        assert!(!p.beacon_may_ask(), "spent until the timed ask");
        p.beacon_spent = false;
        assert!(p.beacon_may_ask());
        p.settle(vec![cand("A1", 1.0, 1.0, "")], 2_000, "vhf-fm", 144_950);
        assert!(!p.beacon_may_ask(), "a gateway is picked");
    }

    #[test]
    fn traffic_from_a_gateway_clears_none_heard() {
        let mut p = GatewayPick::default();
        p.settle(Vec::new(), 1_000, "vhf-fm", 144_950);
        assert_eq!(p.summary(), NO_GATEWAY_HEARD);
        // A late ask reply: the station hears the gateway after all.
        assert!(p.heard(cand("G1", 3.0, 4.0, ""), false, 1_050, "vhf-fm", 144_950));
        assert_eq!(p.current_call(), Some("G1"));
        assert_eq!(p.stale(1_060, "vhf-fm", 144_950), None, "mail can go now");
        assert_eq!(p.misses, 0);
        // Another late reply joins the list but does not take over.
        assert!(p.heard(cand("G2", 9.0, 9.0, ""), false, 1_070, "vhf-fm", 144_950));
        assert_eq!(p.current_call(), Some("G1"));
        assert!(p.summary().contains("1 more heard"));
        // G2 carries our mail: it is the one in use now.
        p.fails = 1;
        assert!(p.heard(cand("G2", 0.0, 0.0, ""), true, 1_080, "vhf-fm", 144_950));
        assert_eq!(p.current_call(), Some("G2"));
        assert_eq!(p.fails, 0);
        // The current one again: nothing to change, but the hourly ask
        // counts from here.
        assert!(!p.heard(cand("G2", 0.0, 0.0, ""), true, 1_090, "vhf-fm", 144_950));
        assert_eq!(p.asked_at, 1_090);
        assert_eq!(p.stale(1_090 + ASK_EVERY_SECS - 1, "vhf-fm", 144_950), None);
        // Never while an ask runs: the ask settles the list.
        p.state = PickState::Asking;
        assert!(!p.heard(cand("G3", 0.0, 0.0, ""), true, 1_100, "vhf-fm", 144_950));
    }

    #[test]
    fn the_fallback_skips_gateways_already_tried() {
        let mut p = GatewayPick::default();
        p.settle(
            vec![
                cand("A1", 9.0, 9.0, ""),
                cand("B1", 5.0, 5.0, ""),
                cand("C1", 1.0, 1.0, ""),
            ],
            0,
            "vhf-fm",
            144_950,
        );
        assert_eq!(p.next_after(&["B1".into()]), Some("C1".to_string()));
    }

    #[tokio::test]
    async fn the_hub_answers_every_gateway_at_once_best_first() {
        let board = AskBoard::new();
        let mut tasks = Vec::new();
        for (call, snr) in [("G1", -3.0f32), ("G2", 6.0), ("G3", 1.0), ("G4", -9.0)] {
            let b = board.clone();
            tasks.push(tokio::spawn(async move {
                let r = Report {
                    gateway: call.into(),
                    snr: Some(snr),
                    grid: String::new(),
                };
                let t = Instant::now();
                let slot = b.report("M7ASK", "n1", "IO91", Preset::VhfFm, r).await;
                (call, slot, t.elapsed())
            }));
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let mut got = Vec::new();
        for t in tasks {
            got.push(t.await.unwrap());
        }
        let slot = slot_secs(Preset::VhfFm);
        let of = |c: &str| got.iter().find(|g| g.0 == c).unwrap().1.clone();
        assert_eq!(of("G2").unwrap().idx, 0);
        assert_eq!(of("G3").unwrap().idx, 1);
        assert_eq!(of("G1").unwrap().idx, 2);
        assert!((of("G1").unwrap().delay_secs - 2.0 * slot).abs() < 1e-9);
        assert!(of("G4").is_none(), "only the best three reply");
        // A report after the window closed keys nothing.
        let late = board
            .report(
                "M7ASK",
                "n1",
                "IO91",
                Preset::VhfFm,
                Report {
                    gateway: "G5".into(),
                    snr: Some(30.0),
                    grid: String::new(),
                },
            )
            .await;
        assert!(late.is_none());
    }
}
