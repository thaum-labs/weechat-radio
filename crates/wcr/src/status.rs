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
    pub audio_db: f32,
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
            audio_label: "good".into(),
            audio_db: -20.0,
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
        }
    }
}

pub type SharedStatus = Mutex<StatusSnapshot>;

pub fn new_shared() -> Arc<SharedStatus> {
    Arc::new(Mutex::new(StatusSnapshot::default()))
}

pub async fn serve(bind: String, snap: Arc<SharedStatus>) {
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
    if let Ok(listener) = tokio::net::TcpListener::bind(&bind).await {
        tracing::info!("status HTTP on {bind}");
        let _ = axum::serve(listener, app).await;
    }
}
