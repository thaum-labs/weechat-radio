//! SPDX-License-Identifier: Apache-2.0
//! Shared live status for TUI, WeeChat bar, telemetry, and HTTP endpoint.

use crate::modes::Mode;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub callsign: String,
    /// Where this station is now: the GPS grid while it has a fix, otherwise
    /// the grid from setup.
    pub grid: String,
    /// GPS receiver state for the status panel. Empty when no GPS is set.
    #[serde(default)]
    pub gps: String,
    pub mode: Mode,
    pub channel: String,
    pub ptt: String,
    pub ptt_on: bool,
    pub preset: String,
    pub frequency: String,
    pub snr: f32,
    pub ber: f32,
    pub audio_label: String,
    #[serde(default = "crate::presets::default_audio_db")]
    pub audio_db: f32,
    /// Capture (radio → PC) level in dBFS.
    #[serde(default = "crate::presets::default_audio_db")]
    pub audio_in_db: f32,
    /// Playback (PC → radio) level in dBFS.
    #[serde(default = "crate::presets::default_audio_db")]
    pub audio_out_db: f32,
    /// True while wcr's own capture meter (audio_meter.rs) has the sound
    /// card open. On Linux modem73 usually holds the card, so this stays
    /// false and the level has to come from modem73 (QA #220).
    #[serde(skip)]
    pub local_in_meter: bool,
    pub queue_out: u64,
    pub queue_hold: u64,
    pub hub_ok: bool,
    pub hub_banner: String,
    pub lan_peers: usize,
    pub update_available: Option<String>,
    pub clock_warn: bool,
    /// Current TX robustness rung (e.g. `RDM-300S`).
    #[serde(default)]
    pub tx_rung: String,
    /// Last ARQ retry count shown in the bar.
    #[serde(default)]
    pub retries: u32,
    /// Channel occupancy 0–100 from modem73 CSMA, when available.
    #[serde(default)]
    pub occupancy_pct: u8,
    /// Frames waiting in the RF air queue.
    #[serde(default)]
    pub queue_air: usize,
    /// True while the air-queue pacer is waiting for a clear channel.
    #[serde(default)]
    pub deferred: bool,
    /// Radio TNC link (Bluetooth/serial KISS), e.g. `VR-N76 linked`. Empty with modem73.
    #[serde(default)]
    pub tnc: String,
    /// True while the KISS link to the radio is up.
    #[serde(default)]
    pub tnc_ok: bool,
    /// Bluetooth audio: bytes read from the radio's audio channel since the
    /// station started. Rises ~11 KB/s while linked, even on a quiet channel.
    #[serde(default)]
    pub bt_rx_bytes: u64,
    /// Bluetooth audio messages from the radio, by frame type.
    #[serde(default)]
    pub bt_rx_frames: BtRxFrames,
    /// Bluetooth audio messages whose SBC audio did not decode whole.
    #[serde(default)]
    pub sbc_decode_errors: u64,
    /// Version of the managed modem73 binary, e.g. `2.4.2`. Empty when unknown.
    #[serde(default)]
    pub modem73: String,
    /// Dial frequency in kHz (0 = unknown).
    #[serde(default)]
    pub freq_khz: u32,
    /// Amateur / licence-free band name (`2m`, `40m`, `PMR446`, …).
    #[serde(default)]
    pub band: String,
    /// `manual`, `rig`, or `none`.
    #[serde(default)]
    pub freq_source: String,
    /// Stations heard recently, with band tags for nicklists.
    #[serde(default)]
    pub heard: Vec<HeardBrief>,
    /// Synced channel default priorities (`#net` → `priority`).
    #[serde(default)]
    pub group_prios: Vec<GroupPrioBrief>,
    /// TUI activity panel (mirrors `[ui] activity_panel`).
    #[serde(default = "default_activity_panel")]
    pub activity_panel: bool,
    /// SNR next to radio lines in chat (mirrors `[ui] show_snr`).
    #[serde(default = "default_activity_panel")]
    pub show_snr: bool,
    /// Running `wcr` build version (from Cargo package version).
    #[serde(default = "default_version")]
    pub version: String,
    /// Unix time the next identity beacon will try to key. 0 = none.
    #[serde(default)]
    pub beacon_due: u32,
    /// Full wait for this beacon cycle (seconds).
    #[serde(default)]
    pub beacon_span: u32,
    /// `vox`, `off`, or empty while a beacon is scheduled. `busy`: due, held
    /// while frames decode on a busy channel. `exchange`: due, waiting for a
    /// line's receipt or a mail's acceptance. `pending`: handed over, not yet
    /// at the modem.
    #[serde(default)]
    pub beacon_note: String,
    /// Unix time of the next relay or ARQ retry hold. 0 = none.
    #[serde(default)]
    pub hold_due: u32,
    /// Full hold when `hold_due` last changed (seconds).
    #[serde(default)]
    pub hold_span: u32,
    /// `relay` or `retry`.
    #[serde(default)]
    pub hold_kind: String,
    /// Public map draws a tower. True only in internet-radio while the switch is on.
    #[serde(default)]
    pub tower: bool,
    /// Frames this process has relayed for another station (not our own sends).
    /// Repeats of the same message each count.
    #[serde(default)]
    pub relayed: u64,
    /// Distinct message ids included in `relayed` this process.
    #[serde(default)]
    pub relayed_unique: u64,
    /// Process id of the station serving this status, so the desktop app can
    /// stop exactly this station and its modem73 (#235). 0 from older nodes.
    #[serde(default)]
    pub pid: u32,
    /// Highest capture sample in the last half second, dBFS (#241). `None`
    /// when modem73 does not report it (older modem73, Windows).
    #[serde(default)]
    pub audio_in_peak_db: Option<f32>,
    /// Capture samples at full scale in the last half second, % (#241).
    #[serde(default)]
    pub audio_in_clip_pct: Option<f32>,
    /// RMS of the last frame modem73 decoded, dBFS.
    #[serde(default)]
    pub audio_in_frame_db: Option<f32>,
    /// The receive level in a word (`good`, `low`, `high`, `clipping`,
    /// `noisy`), empty when nothing measures it. See [`crate::levels`].
    #[serde(default)]
    pub audio_in_word: String,
    /// What to do about the receive level, in plain words.
    #[serde(default)]
    pub audio_in_hint: String,
    /// modem73's busy test: `sync`, `level`, or empty (no modem73).
    #[serde(default)]
    pub csma_mode: String,
    /// The level test's threshold, dBFS, when `csma_mode` is `level`.
    #[serde(default)]
    pub csma_threshold_db: Option<f32>,
    /// The RX gain wcr can set on this station, if any (#241).
    #[serde(default)]
    pub rx_gain: Option<RxGain>,
    /// Why there is no RX gain here, when `rx_gain` is `None`.
    #[serde(default)]
    pub rx_gain_note: String,
    /// Transmit level, dB (-26 to 0, #241).
    #[serde(default)]
    pub tx_level_db: f32,
}

/// An RX gain wcr can set: the sound card's mixer (`mixer`) or its own gain
/// on Bluetooth audio (`bluetooth`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RxGain {
    pub kind: String,
    pub db: f32,
    pub min_db: f32,
    pub max_db: f32,
    /// The mixer control (`Mic`), empty for Bluetooth.
    #[serde(default)]
    pub control: String,
    /// The card's automatic gain, if it has one.
    #[serde(default)]
    pub agc: Option<bool>,
}

impl RxGain {
    pub fn from_mixer(g: &crate::mixer::CaptureGain) -> Self {
        Self {
            kind: "mixer".into(),
            db: g.db,
            min_db: g.min_db,
            max_db: g.max_db,
            control: g.control.clone(),
            agc: g.agc,
        }
    }

    pub fn bluetooth(db: f32) -> Self {
        Self {
            kind: "bluetooth".into(),
            db,
            min_db: crate::levels::BT_RX_MIN_DB,
            max_db: crate::levels::BT_RX_MAX_DB,
            control: String::new(),
            agc: None,
        }
    }

    fn control(&self) -> crate::levels::RxControl {
        let at_min = self.db <= self.min_db + 0.5;
        let at_max = self.db >= self.max_db - 0.5;
        if self.kind == "bluetooth" {
            crate::levels::RxControl::Bluetooth { at_min, at_max }
        } else {
            crate::levels::RxControl::Mixer { at_min, at_max }
        }
    }
}

fn default_version() -> String {
    crate::update::current_version().to_string()
}

fn default_activity_panel() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupPrioBrief {
    pub channel: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeardBrief {
    pub callsign: String,
    #[serde(default)]
    pub band: String,
    #[serde(default)]
    pub freq_khz: u32,
    #[serde(default)]
    pub medium: String,
    #[serde(default)]
    pub last_heard: u32,
    #[serde(default)]
    pub snr: Option<f32>,
    #[serde(default)]
    pub gateway: bool,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub welfare: String,
    /// The grid this station last sent over the air, if any.
    #[serde(default)]
    pub grid: String,
    /// The mode it runs, when it sent a grid (`radio`, `radio-plus`, …).
    #[serde(default)]
    pub mode: String,
}

impl HeardBrief {
    pub fn band_tag(&self) -> &str {
        if !self.band.is_empty() {
            &self.band
        } else if self.medium == "inet" || self.medium == "lan" || self.callsign.starts_with('~') {
            "inet"
        } else {
            "?"
        }
    }
}

impl StatusSnapshot {
    pub fn set_freq(&mut self, khz: u32, source: &str) {
        self.freq_khz = khz;
        self.band = crate::band::band_for_khz(khz).unwrap_or("").into();
        self.freq_source = if khz == 0 {
            "none".into()
        } else {
            source.into()
        };
        self.frequency = if khz == 0 {
            String::new()
        } else {
            format!("{} MHz", crate::band::fmt_mhz(khz))
        };
    }

    pub fn apply_modem_audio(&mut self, st: &crate::modem::control::ModemStatus) {
        if !st.audio_connected {
            self.audio_in_db = crate::presets::AUDIO_FLOOR_DB;
            self.audio_out_db = crate::presets::AUDIO_FLOOR_DB;
            self.audio_db = crate::presets::AUDIO_FLOOR_DB;
            self.audio_label = "no audio".into();
            return;
        }
        if let Some(db) = st.audio_in_db {
            self.audio_in_db = db.max(crate::presets::AUDIO_FLOOR_DB);
        } else if cfg!(windows) && !st.channel_state.eq_ignore_ascii_case("rx") {
            // No live capture meter on Windows (that glitched MFSK). Fall
            // toward empty so the last decode does not leave IN stuck.
            self.audio_in_db = crate::presets::decay_audio_db(self.audio_in_db);
        }
        if let Some(db) = st.audio_out_db {
            self.audio_out_db = db;
        } else if st.ptt_on || st.channel_state.eq_ignore_ascii_case("tx") {
            self.audio_out_db = crate::presets::AUDIO_TX_NOMINAL_DB;
        } else if !cfg!(windows) {
            // Windows owns OUT via the WASAPI endpoint peak (no second stream).
            self.audio_out_db = crate::presets::decay_audio_db(self.audio_out_db);
        }
        self.audio_db = self.audio_in_db;
        // Nothing measures the capture live (old modem73, and wcr's meter
        // could not open the card): say so instead of showing -80 dB as "low".
        let measured = st.audio_in_db.is_some() || self.local_in_meter || cfg!(windows);
        self.audio_label = if measured {
            crate::presets::audio_level_label(self.audio_in_db).into()
        } else {
            "—".into()
        };
        self.audio_in_peak_db = st.audio_in_peak_db;
        self.audio_in_clip_pct = st.audio_in_clip_pct;
        // The words need a live level: on Windows IN only moves on a decode,
        // so an idle Windows station would always read "low".
        let live = st.audio_in_db.is_some() || self.local_in_meter;
        if !live {
            self.audio_in_word.clear();
            self.audio_in_hint.clear();
        } else if !(st.ptt_on || st.channel_state.eq_ignore_ascii_case("tx")) {
            // While we send, the input is our own transmission (or nothing):
            // keep the last idle judgement.
            self.judge_input();
        }
    }

    /// Sets `audio_in_word` and `audio_in_hint` from the levels now.
    pub fn judge_input(&mut self) {
        let input = crate::levels::Input {
            rms_db: self.audio_in_db,
            peak_db: self.audio_in_peak_db,
            clip_pct: self.audio_in_clip_pct,
            frame_db: self.audio_in_frame_db,
        };
        let busy = crate::levels::Busy {
            level_threshold_db: if self.csma_mode == "level" {
                self.csma_threshold_db
            } else {
                None
            },
        };
        let control = self
            .rx_gain
            .as_ref()
            .map(|g| g.control())
            .unwrap_or_default();
        let (word, hint) = crate::levels::judge(input, busy, control);
        self.audio_in_word = word.as_str().into();
        self.audio_in_hint = hint;
    }

    pub fn prio_for_channel(&self, channel: &str) -> &str {
        let key = channel.trim_start_matches('#').to_ascii_lowercase();
        self.group_prios
            .iter()
            .find(|g| g.channel.trim_start_matches('#').eq_ignore_ascii_case(&key))
            .map(|g| g.priority.as_str())
            .unwrap_or("routine")
    }

    pub fn set_hold_due(&mut self, due: u32, kind: &str, now: u32) {
        if due == 0 {
            self.hold_due = 0;
            self.hold_span = 0;
            self.hold_kind.clear();
            return;
        }
        if self.hold_due != due {
            self.hold_span = due.saturating_sub(now).max(1);
            self.hold_due = due;
            self.hold_kind = kind.to_string();
        }
    }

    pub fn has_live_countdown(&self) -> bool {
        self.beacon_due > 0 || self.hold_due > 0
    }

    pub fn beacon_lane(&self, now: f64) -> DueLane {
        match self.beacon_note.as_str() {
            "vox" => DueLane::idle("off · vox"),
            "off" => DueLane::idle("off"),
            _ if self.beacon_due == 0 => DueLane::idle("—"),
            "pending" => DueLane::live("pending".into(), 0.0),
            "exchange" => DueLane::live("after exchange".into(), 0.0),
            "busy" => DueLane::live(
                format!("busy {}", countdown_label(self.beacon_due, now)),
                remaining_frac(self.beacon_due, self.beacon_span, now),
            ),
            _ => DueLane::live(
                countdown_label(self.beacon_due, now),
                remaining_frac(self.beacon_due, self.beacon_span, now),
            ),
        }
    }

    pub fn hold_lane(&self, now: f64) -> DueLane {
        if self.hold_due == 0 {
            return DueLane::idle("—");
        }
        let kind = if self.hold_kind.is_empty() {
            "hold"
        } else {
            self.hold_kind.as_str()
        };
        DueLane::live(
            format!("{kind} {}", countdown_label(self.hold_due, now)),
            remaining_frac(self.hold_due, self.hold_span, now),
        )
    }
}

/// One countdown row (beacon or hold) for GUI / TUI.
#[derive(Debug, Clone, PartialEq)]
pub struct DueLane {
    pub label: String,
    pub frac: f32,
    pub live: bool,
}

impl DueLane {
    fn idle(label: &str) -> Self {
        Self {
            label: label.into(),
            frac: 0.0,
            live: false,
        }
    }

    fn live(label: String, frac: f32) -> Self {
        Self {
            label,
            frac,
            live: true,
        }
    }
}

pub fn unix_now_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn remaining_secs_f(due: u32, now: f64) -> f64 {
    if due == 0 {
        0.0
    } else {
        (due as f64 - now).max(0.0)
    }
}

pub fn remaining_frac(due: u32, span: u32, now: f64) -> f32 {
    if due == 0 || span == 0 {
        return 0.0;
    }
    (remaining_secs_f(due, now) / span as f64).clamp(0.0, 1.0) as f32
}

pub fn fmt_mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

pub fn countdown_label(due: u32, now: f64) -> String {
    let rem = remaining_secs_f(due, now);
    if rem < 0.5 {
        "now".into()
    } else {
        fmt_mmss(rem.ceil() as u32)
    }
}

impl Default for StatusSnapshot {
    fn default() -> Self {
        Self {
            callsign: String::new(),
            grid: String::new(),
            gps: String::new(),
            mode: Mode::InternetRadio,
            channel: "idle".into(),
            ptt: "none".into(),
            ptt_on: false,
            preset: "vhf-fm".into(),
            frequency: String::new(),
            snr: 0.0,
            ber: 0.0,
            audio_label: "—".into(),
            audio_db: crate::presets::AUDIO_FLOOR_DB,
            audio_in_db: crate::presets::AUDIO_FLOOR_DB,
            audio_out_db: crate::presets::AUDIO_FLOOR_DB,
            local_in_meter: false,
            queue_out: 0,
            queue_hold: 0,
            hub_ok: false,
            hub_banner: String::new(),
            lan_peers: 0,
            update_available: None,
            clock_warn: false,
            tx_rung: String::new(),
            retries: 0,
            occupancy_pct: 0,
            queue_air: 0,
            deferred: false,
            tnc: String::new(),
            tnc_ok: false,
            bt_rx_bytes: 0,
            bt_rx_frames: BtRxFrames::default(),
            sbc_decode_errors: 0,
            modem73: String::new(),
            freq_khz: 0,
            band: String::new(),
            freq_source: "none".into(),
            heard: Vec::new(),
            group_prios: Vec::new(),
            activity_panel: true,
            show_snr: true,
            version: default_version(),
            beacon_due: 0,
            beacon_span: 0,
            beacon_note: String::new(),
            hold_due: 0,
            hold_span: 0,
            hold_kind: String::new(),
            tower: false,
            relayed: 0,
            relayed_unique: 0,
            pid: std::process::id(),
            audio_in_peak_db: None,
            audio_in_clip_pct: None,
            audio_in_frame_db: None,
            audio_in_word: String::new(),
            audio_in_hint: String::new(),
            csma_mode: String::new(),
            csma_threshold_db: None,
            rx_gain: None,
            rx_gain_note: String::new(),
            tx_level_db: 0.0,
        }
    }
}

/// `group_prios` as shown on `/radio status`: `#net=routine,#ops=emergency`.
pub fn group_prio_summary(prios: &[GroupPrioBrief]) -> String {
    if prios.is_empty() {
        return "—".into();
    }
    prios
        .iter()
        .map(|g| format!("{}={}", g.channel.trim_start_matches('#'), g.priority))
        .collect::<Vec<_>>()
        .join(",")
}

/// Bluetooth audio messages received from the radio, by frame type (QA #237):
/// 0x00 / 0x03 are received audio, 0x01 end of audio, 0x02 ack, 0x09 the
/// radio's echo of our own transmit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BtRxFrames {
    #[serde(rename = "0x00")]
    pub t00: u64,
    #[serde(rename = "0x01")]
    pub t01: u64,
    #[serde(rename = "0x02")]
    pub t02: u64,
    #[serde(rename = "0x03")]
    pub t03: u64,
    #[serde(rename = "0x09")]
    pub t09: u64,
    pub other: u64,
}

impl BtRxFrames {
    pub fn count(&mut self, kind: u8) {
        match kind {
            0x00 => self.t00 += 1,
            0x01 => self.t01 += 1,
            0x02 => self.t02 += 1,
            0x03 => self.t03 += 1,
            0x09 => self.t09 += 1,
            _ => self.other += 1,
        }
    }

    pub fn total(&self) -> u64 {
        self.t00 + self.t01 + self.t02 + self.t03 + self.t09 + self.other
    }
}

pub type SharedStatus = Mutex<StatusSnapshot>;

pub fn new_shared() -> Arc<SharedStatus> {
    Arc::new(Mutex::new(StatusSnapshot::default()))
}

pub async fn serve(bind: String, snap: Arc<SharedStatus>) {
    match tokio::net::TcpListener::bind(&bind).await {
        Ok(listener) => {
            tracing::info!("status HTTP on {bind}");
            serve_listener(listener, snap).await;
        }
        Err(e) => tracing::warn!("status HTTP {bind}: {e}"),
    }
}

pub async fn serve_listener(listener: tokio::net::TcpListener, snap: Arc<SharedStatus>) {
    serve_listener_with(listener, snap, axum::Router::new()).await;
}

pub async fn serve_listener_with(
    listener: tokio::net::TcpListener,
    snap: Arc<SharedStatus>,
    extra: axum::Router,
) {
    use axum::{middleware, routing::get, Json, Router};
    let app = Router::new()
        .route(
            "/status",
            get({
                let snap = snap.clone();
                move || {
                    let snap = snap.clone();
                    async move { Json(snap.lock().clone()) }
                }
            }),
        )
        .merge(extra)
        .layer(middleware::from_fn(
            crate::http_local::require_loopback_host,
        ));
    let _ = axum::serve(listener, app).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modem::control::ModemStatus;
    use crate::presets::{AUDIO_FLOOR_DB, AUDIO_TX_NOMINAL_DB};

    #[test]
    fn modem_audio_ptt_fills_out() {
        let mut s = StatusSnapshot::default();
        s.audio_label = "good".into();
        let st = ModemStatus {
            audio_connected: true,
            ptt_on: true,
            audio_in_db: Some(-16.0),
            ..Default::default()
        };
        s.apply_modem_audio(&st);
        assert_eq!(s.audio_in_db, -16.0);
        assert_eq!(s.audio_out_db, AUDIO_TX_NOMINAL_DB);
        assert_eq!(s.audio_label, "good");
    }

    /// QA #241: the Pi's -1.5 dBFS of squelch-0 hiss with a quarter of the
    /// samples at full scale reads "clipping", with a hint naming the knob.
    #[test]
    fn the_input_is_judged_from_the_live_level() {
        let mut s = StatusSnapshot {
            csma_mode: "sync".into(),
            rx_gain: Some(RxGain {
                kind: "mixer".into(),
                db: 17.9,
                min_db: 0.0,
                max_db: 23.81,
                control: "Mic".into(),
                agc: Some(false),
            }),
            ..StatusSnapshot::default()
        };
        let st = ModemStatus {
            audio_connected: true,
            audio_in_db: Some(-1.5),
            audio_in_peak_db: Some(0.0),
            audio_in_clip_pct: Some(26.0),
            ..Default::default()
        };
        s.apply_modem_audio(&st);
        assert_eq!(s.audio_in_word, "clipping");
        assert!(
            s.audio_in_hint.contains("Lower RX gain"),
            "{}",
            s.audio_in_hint
        );
        assert_eq!(s.audio_in_clip_pct, Some(26.0));
        // While we send, the input is our own audio: the idle word stays.
        let tx = ModemStatus {
            audio_connected: true,
            ptt_on: true,
            audio_in_db: Some(-30.0),
            ..Default::default()
        };
        s.apply_modem_audio(&tx);
        assert_eq!(s.audio_in_word, "clipping");
        // No live level (Windows' modem73): no word, rather than "low"
        // between decodes.
        let mut w = StatusSnapshot::default();
        w.apply_modem_audio(&ModemStatus {
            audio_connected: true,
            ..Default::default()
        });
        assert_eq!(w.audio_in_word, "");
    }

    #[test]
    fn group_prios_summary_and_relay_counter() {
        assert_eq!(group_prio_summary(&[]), "—");
        let rows = vec![
            GroupPrioBrief {
                channel: "#net".into(),
                priority: "routine".into(),
            },
            GroupPrioBrief {
                channel: "#ops".into(),
                priority: "emergency".into(),
            },
        ];
        assert_eq!(group_prio_summary(&rows), "net=routine,ops=emergency");
        let mut s = StatusSnapshot::default();
        assert_eq!(s.relayed, 0);
        assert_eq!(s.relayed_unique, 0);
        s.relayed = s.relayed.saturating_add(1);
        s.relayed_unique = s.relayed_unique.saturating_add(1);
        assert_eq!(s.relayed, 1);
        assert_eq!(s.relayed_unique, 1);
    }

    #[test]
    fn missing_sound_card_clears_meters() {
        let mut s = StatusSnapshot::default();
        s.audio_label = "good".into();
        s.audio_in_db = -12.0;
        let st = ModemStatus {
            audio_connected: false,
            ..Default::default()
        };
        s.apply_modem_audio(&st);
        assert_eq!(s.audio_label, "no audio");
        assert_eq!(s.audio_in_db, AUDIO_FLOOR_DB);
        assert_eq!(s.audio_out_db, AUDIO_FLOOR_DB);
    }

    #[test]
    fn modem73_negative_rx_count_still_fills_out() {
        let mut s = StatusSnapshot::default();
        let st = ModemStatus::from_json(serde_json::json!({
            "channel_state": "tx",
            "ptt_on": true,
            "rx_frame_count": -1,
            "audio_connected": true
        }));
        s.apply_modem_audio(&st);
        assert_eq!(s.audio_out_db, AUDIO_TX_NOMINAL_DB);
        assert_ne!(s.audio_label, "no audio");
    }

    #[test]
    fn reconnect_clears_no_modem_latch() {
        let mut s = StatusSnapshot::default();
        s.audio_label = "no modem".into();
        let st = ModemStatus {
            audio_connected: true,
            ptt_on: true,
            ..Default::default()
        };
        s.apply_modem_audio(&st);
        assert_eq!(s.audio_out_db, AUDIO_TX_NOMINAL_DB);
        assert_ne!(s.audio_label, "no modem");
    }

    #[test]
    fn tx_channel_fills_out_without_ptt_flag() {
        let mut s = StatusSnapshot::default();
        s.apply_modem_audio(&ModemStatus {
            audio_connected: true,
            channel_state: "tx".into(),
            ..Default::default()
        });
        assert_eq!(s.audio_out_db, AUDIO_TX_NOMINAL_DB);
    }

    #[test]
    fn last_rx_in_decays_when_modem_has_no_live_level() {
        let mut s = StatusSnapshot::default();
        s.audio_in_db = -12.0;
        s.audio_label = "low".into();
        s.apply_modem_audio(&ModemStatus {
            audio_connected: true,
            channel_state: "idle".into(),
            audio_in_db: None,
            ..Default::default()
        });
        if cfg!(windows) {
            assert!(s.audio_in_db < -12.0, "{}", s.audio_in_db);
            assert!(s.audio_in_db >= AUDIO_FLOOR_DB);
        } else {
            assert_eq!(s.audio_in_db, -12.0);
        }
    }

    #[test]
    fn unmeasured_capture_is_not_called_low() {
        let mut s = StatusSnapshot::default();
        let idle = ModemStatus {
            audio_connected: true,
            channel_state: "idle".into(),
            ..Default::default()
        };
        s.apply_modem_audio(&idle);
        if cfg!(windows) {
            assert_eq!(s.audio_label, "low");
        } else {
            assert_eq!(s.audio_label, "—");
        }
        // modem73 reports the live level: a real label again.
        s.apply_modem_audio(&ModemStatus {
            audio_in_db: Some(-20.0),
            ..idle.clone()
        });
        assert_eq!(s.audio_label, "good");
        s.apply_modem_audio(&ModemStatus {
            audio_in_db: Some(-100.0),
            ..idle
        });
        assert_eq!(s.audio_in_db, AUDIO_FLOOR_DB);
        assert_eq!(s.audio_label, "low");
    }

    #[test]
    fn in_holds_during_rx_without_modem_level() {
        let mut s = StatusSnapshot::default();
        s.audio_in_db = -12.0;
        s.apply_modem_audio(&ModemStatus {
            audio_connected: true,
            channel_state: "rx".into(),
            audio_in_db: None,
            ..Default::default()
        });
        assert_eq!(s.audio_in_db, -12.0);
    }

    #[test]
    fn countdown_math_is_exact() {
        assert_eq!(fmt_mmss(0), "0:00");
        assert_eq!(fmt_mmss(75), "1:15");
        assert_eq!(countdown_label(100, 100.0), "now");
        assert_eq!(countdown_label(145, 100.2), "0:45");
        assert!((remaining_frac(160, 60, 130.0) - 0.5).abs() < f32::EPSILON);
        assert_eq!(remaining_frac(0, 60, 130.0), 0.0);
        let mut s = StatusSnapshot::default();
        s.beacon_note = "vox".into();
        assert_eq!(s.beacon_lane(0.0).label, "off · vox");
        s.beacon_note.clear();
        s.beacon_due = 200;
        s.beacon_span = 60;
        let lane = s.beacon_lane(170.0);
        assert!(lane.live);
        assert_eq!(lane.label, "0:30");
        s.beacon_note = "busy".into();
        assert_eq!(s.beacon_lane(170.0).label, "busy 0:30");
        s.beacon_note = "exchange".into();
        assert_eq!(s.beacon_lane(210.0).label, "after exchange");
        s.beacon_note = "pending".into();
        assert_eq!(s.beacon_lane(215.0).label, "pending");
        s.beacon_note.clear();
        s.set_hold_due(250, "relay", 200);
        assert_eq!(s.hold_span, 50);
        s.set_hold_due(250, "relay", 220);
        assert_eq!(s.hold_span, 50);
        assert_eq!(s.hold_lane(230.0).label, "relay 0:20");
        s.set_hold_due(0, "", 0);
        s.beacon_due = 0;
        assert!(!s.has_live_countdown());
    }

    #[test]
    fn expired_retry_hold_stays_now_until_the_due_changes() {
        let mut s = StatusSnapshot::default();
        s.set_hold_due(100, "retry", 33);
        assert_eq!(s.hold_lane(100.0).label, "retry now");
        // The ARQ loop used to skip without bump_retry, so the same due
        // was painted again and the countdown never restarted.
        s.set_hold_due(100, "retry", 160);
        assert_eq!(s.hold_due, 100);
        assert_eq!(s.hold_span, 67);
        assert_eq!(s.hold_lane(160.0).label, "retry now");
        s.set_hold_due(227, "retry", 160);
        assert_eq!(s.hold_lane(160.0).label, "retry 1:07");
    }
}
