//! SPDX-License-Identifier: Apache-2.0
//! Borrow the shared modem for one Email burst, then restore chat settings.

use crate::air::{AirQueue, ChannelSense, ChannelState, ModemSense};
use crate::modem::{ControlClient, KissClient};
use crate::presets::{Preset, Rung};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// Order of a mail burst. Pause first so queued chat cannot take the modem
/// while Email is waiting for the current transmission to finish.
pub fn lease_sequence() -> &'static [&'static str] {
    &[
        "pause",
        "wait_idle",
        "drain_before",
        "snapshot",
        "set_mail_config",
        "vox_lead",
        "send_frames",
        "drain_after",
        "restore_snapshot",
        "resume",
    ]
}

pub fn tx_drained(count: u64, target: u64) -> bool {
    count >= target
}

pub fn drain_target_after(before: u64, frames: u64) -> u64 {
    before.saturating_add(frames)
}

#[derive(Clone)]
pub struct RadioPorts {
    pub kiss: Option<KissClient>,
    pub control: Option<ControlClient>,
    pub air: Option<AirQueue>,
    pub sense: Option<Arc<ModemSense>>,
}

/// Mail TX config. CSMA is off only for the lease. Chat's `control_config` is not modified.
pub fn mail_tx_config(preset: Preset, rung: Rung) -> Value {
    let snap = preset.control_config();
    let mut v = rung.control_config();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("csma_enabled".into(), serde_json::json!(false));
        if let Some(band) = snap.get("csma_band").cloned() {
            obj.insert("csma_band".into(), band);
        }
        if let Some(th) = snap.get("carrier_threshold_db").cloned() {
            obj.insert("carrier_threshold_db".into(), th);
        }
    }
    v
}

pub fn restore_config(preset: Preset) -> Value {
    preset.control_config()
}

pub async fn wait_idle(ports: &RadioPorts, cap: Duration) -> bool {
    let start = Instant::now();
    loop {
        let tx = ports
            .sense
            .as_ref()
            .map(|s| s.state() == ChannelState::Tx)
            .unwrap_or(false);
        if !tx {
            return true;
        }
        if start.elapsed() >= cap {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn wait_tx_count(control: &ControlClient, target: u64, cap: Duration) {
    let start = Instant::now();
    loop {
        match control.get_status().await {
            Ok(st) if tx_drained(st.tx_frame_count, target) => return,
            Err(_) => return,
            _ => {}
        }
        if start.elapsed() >= cap {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Wait until `tx_frame_count` stops climbing, so a mode change cannot strand a queued frame.
pub async fn wait_tx_stable(control: &ControlClient, cap: Duration) -> u64 {
    let start = Instant::now();
    let mut last = control
        .get_status()
        .await
        .map(|s| s.tx_frame_count)
        .unwrap_or(0);
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let now = control
            .get_status()
            .await
            .map(|s| s.tx_frame_count)
            .unwrap_or(last);
        if now == last {
            return now;
        }
        last = now;
        if start.elapsed() >= cap {
            return last;
        }
    }
}
