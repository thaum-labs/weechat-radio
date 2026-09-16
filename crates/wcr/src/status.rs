//! SPDX-License-Identifier: Apache-2.0
//! Shared live status for TUI, WeeChat bar, telemetry, and HTTP endpoint.

use crate::modes::Mode;
use parking_lot::Mutex;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
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
