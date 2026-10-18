//! SPDX-License-Identifier: Apache-2.0
//! Borrow the shared modem for one Email burst, then restore chat settings.

use crate::air::{AirQueue, ChannelSense, ChannelState, ModemSense};
use crate::modem::control::ModemStatus;
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
        "wait_clear",
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

/// A receipt that took the radio before the lease restores chat's config
/// when it is done. Wait for that (at most `cap`) before setting mail's.
pub async fn wait_ack_done(ports: &RadioPorts, cap: Duration) {
    let Some(air) = &ports.air else {
        return;
    };
    let start = Instant::now();
    while air.is_paused() && start.elapsed() < cap {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A chat frame taken off the air queue just before the lease may still be
/// waiting out carrier sense. It sets chat's config, carrier sense on, just
/// before it goes to the modem. Wait for that (at most `cap`) before setting
/// mail's, or it lands under the burst.
pub async fn wait_chat_handed(ports: &RadioPorts, cap: Duration) {
    let Some(air) = &ports.air else {
        return;
    };
    let start = Instant::now();
    while air.chat_in_flight() && start.elapsed() < cap {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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

/// The transmitter is keying, or still has frames queued behind the one on air.
pub fn modem_keying(st: &ModemStatus) -> bool {
    st.ptt_on
        || st.channel_state.trim().eq_ignore_ascii_case("tx")
        || st.tx_queue.is_some_and(|n| n > 0)
}

/// Upstream modem73 leaves `tx_frame_count` at 0, so the counter alone never
/// drains and the lease used to end on a fixed cap with frames still queued.
/// Done once the transmitter is idle and the counter reached `target`, or the
/// modem was seen keying and reports its queue, or the keyed airtime is over.
pub fn burst_drained(st: &ModemStatus, target: u64, seen_keying: bool, airtime_over: bool) -> bool {
    !modem_keying(st)
        && (tx_drained(st.tx_frame_count, target)
            || (seen_keying && st.tx_queue.is_some())
            || airtime_over)
}

/// Same patience as chat's `pace_and_send`: three times the airtime, plus grace.
pub fn drain_cap(airtime: Duration) -> Duration {
    airtime * 3 + Duration::from_secs(15)
}

/// Wait until every frame of the burst has left the modem. `airtime` is the
/// keyed estimate; the cap is [`drain_cap`] of it.
pub async fn wait_drained(control: &ControlClient, target: u64, airtime: Duration) {
    let start = Instant::now();
    let cap = drain_cap(airtime);
    let mut seen_keying = false;
    let mut idle_polls = 0u8;
    loop {
        match control.get_status().await {
            Ok(st) => {
                seen_keying |= modem_keying(&st);
                if burst_drained(&st, target, seen_keying, start.elapsed() >= airtime) {
                    idle_polls += 1;
                    if idle_polls >= 2 {
                        return;
                    }
                } else {
                    idle_polls = 0;
                }
            }
            Err(_) => return,
        }
        if start.elapsed() >= cap {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Wait until `tx_frame_count` stops climbing and the transmitter is idle, so
/// a mode change cannot strand a queued frame.
pub async fn wait_tx_stable(control: &ControlClient, cap: Duration) -> u64 {
    let start = Instant::now();
    let mut last = control
        .get_status()
        .await
        .map(|s| s.tx_frame_count)
        .unwrap_or(0);
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let Ok(st) = control.get_status().await else {
            return last;
        };
        if st.tx_frame_count == last && !modem_keying(&st) {
            return last;
        }
        last = st.tx_frame_count;
        if start.elapsed() >= cap {
            return last;
        }
    }
}

/// A frame is being received. The gateway holds its reply rather than keying
/// over a chunk that is still on the air.
pub fn channel_receiving(ports: &RadioPorts) -> bool {
    ports
        .sense
        .as_ref()
        .is_some_and(|s| s.state() == ChannelState::Rx)
}

/// Longest a mail lease waits for an inbound frame to finish before keying.
pub const WAIT_CLEAR_CAP: Duration = Duration::from_secs(12);

/// Let a frame that is being decoded finish before keying. A sender that
/// keys its retry over the gateway's acceptance never hears it. Undecodable
/// noise is not `Rx`, so a noisy floor does not hold the lease.
pub async fn wait_clear(ports: &RadioPorts, cap: Duration) {
    let start = Instant::now();
    while channel_receiving(ports) && start.elapsed() < cap {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upstream(channel: &str, ptt: bool, queue: u64) -> ModemStatus {
        ModemStatus {
            channel_state: channel.into(),
            ptt_on: ptt,
            tx_frame_count: 0,
            tx_queue: Some(queue),
            ..ModemStatus::default()
        }
    }

    #[test]
    fn a_frozen_tx_counter_does_not_end_the_lease_mid_burst() {
        // Upstream modem73: tx_frame_count stays 0 while 48 chunks key.
        let target = drain_target_after(0, 49);
        assert!(!burst_drained(
            &upstream("tx", true, 30),
            target,
            true,
            false
        ));
        assert!(!burst_drained(
            &upstream("idle", false, 2),
            target,
            true,
            true
        ));
        assert!(!burst_drained(
            &upstream("idle", true, 0),
            target,
            true,
            true
        ));
        assert!(burst_drained(
            &upstream("idle", false, 0),
            target,
            true,
            false
        ));
        // Not seen keying yet: the frames may still be on their way to the modem.
        assert!(!burst_drained(
            &upstream("idle", false, 0),
            target,
            false,
            false
        ));
        assert!(burst_drained(
            &upstream("idle", false, 0),
            target,
            false,
            true
        ));
    }

    #[test]
    fn a_counting_modem_still_drains_on_the_counter() {
        let st = ModemStatus {
            tx_frame_count: 12,
            ..ModemStatus::default()
        };
        assert!(burst_drained(&st, 12, false, false));
        assert!(!burst_drained(&st, 13, false, false));
        assert!(burst_drained(&st, 13, false, true));
        assert!(!modem_keying(&st));
        assert!(modem_keying(&upstream("rx", false, 1)));
        assert!(!modem_keying(&upstream("rx", false, 0)));
    }

    #[test]
    fn the_drain_cap_matches_chat_patience() {
        assert_eq!(
            drain_cap(Duration::from_secs(200)),
            Duration::from_secs(615)
        );
        assert!(drain_cap(Duration::from_secs(200)) > Duration::from_secs(90));
    }
}
