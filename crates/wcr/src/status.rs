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
    pub grid: String,
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
    /// Running `wcr` build version (from Cargo package version).
    #[serde(default = "default_version")]
    pub version: String,
    /// Unix time the next identity beacon will try to key. 0 = none.
    #[serde(default)]
    pub beacon_due: u32,
    /// Full wait for this beacon cycle (seconds).
    #[serde(default)]
    pub beacon_span: u32,
    /// `vox`, `off`, or empty while a beacon is scheduled.
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
            self.audio_in_db = db;
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
        self.audio_label = crate::presets::audio_level_label(self.audio_in_db).into();
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
            freq_khz: 0,
            band: String::new(),
            freq_source: "none".into(),
            heard: Vec::new(),
            group_prios: Vec::new(),
            activity_panel: true,
            version: default_version(),
            beacon_due: 0,
            beacon_span: 0,
            beacon_note: String::new(),
            hold_due: 0,
            hold_span: 0,
            hold_kind: String::new(),
        }
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
            serve_listener(listener, snap, None).await;
        }
        Err(e) => tracing::warn!("status HTTP {bind}: {e}"),
    }
}

pub async fn serve_listener(
    listener: tokio::net::TcpListener,
    snap: Arc<SharedStatus>,
    mail: Option<crate::mail_api::MailApiState>,
) {
    use axum::{routing::get, Json, Router};
    let app = Router::new().route(
        "/status",
        get({
            let snap = snap.clone();
            move || {
                let snap = snap.clone();
                async move { Json(snap.lock().clone()) }
            }
        }),
    );
    let app = if let Some(m) = mail {
        app.merge(crate::mail_api::router(m))
    } else {
        app
    };
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
        s.set_hold_due(250, "relay", 200);
        assert_eq!(s.hold_span, 50);
        s.set_hold_due(250, "relay", 220);
        assert_eq!(s.hold_span, 50);
        assert_eq!(s.hold_lane(230.0).label, "relay 0:20");
        s.set_hold_due(0, "", 0);
        s.beacon_due = 0;
        assert!(!s.has_live_countdown());
    }
}
