//! SPDX-License-Identifier: Apache-2.0
//! Shared live status for TUI, WeeChat bar, telemetry, and HTTP endpoint.

use crate::modes::Mode;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
            serve_listener(listener, snap).await;
        }
        Err(e) => tracing::warn!("status HTTP {bind}: {e}"),
    }
}

pub async fn serve_listener(listener: tokio::net::TcpListener, snap: Arc<SharedStatus>) {
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
}
