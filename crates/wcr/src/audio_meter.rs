//! SPDX-License-Identifier: Apache-2.0
//! Live sound-card capture meter.
//!
//! modem73 2.4 reports RX level only after a complete frame decodes. This
//! monitor opens the same capture device in shared mode so ordinary audio is
//! visible while an operator sets the radio/computer input gain.

use crate::status::SharedStatus;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[cfg(feature = "setup-probe")]
pub fn spawn_input_meter(device_name: String, snap: Arc<SharedStatus>, cancel: CancellationToken) {
    let _ = std::thread::Builder::new()
        .name("wcr-audio-meter".into())
        .spawn(move || {
            if let Err(e) = run_input_meter(&device_name, snap, cancel) {
                tracing::warn!("live audio input meter unavailable: {e}");
            }
        });
}

#[cfg(not(feature = "setup-probe"))]
pub fn spawn_input_meter(
    _device_name: String,
    _snap: Arc<SharedStatus>,
    _cancel: CancellationToken,
) {
}

#[cfg(feature = "setup-probe")]
fn run_input_meter(
    device_name: &str,
    snap: Arc<SharedStatus>,
    cancel: CancellationToken,
) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::time::Duration;

    let host = cpal::default_host();
    let device = if device_name.trim().is_empty() {
        host.default_input_device()
            .ok_or_else(|| "no default input device".to_string())?
    } else {
        host.input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| {
                d.name()
                    .map(|n| n.eq_ignore_ascii_case(device_name))
                    .unwrap_or(false)
            })
            .ok_or_else(|| format!("input device {device_name:?} was not found"))?
    };
    let selected_name = device.name().unwrap_or_else(|_| device_name.to_string());
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let sample_format = supported.sample_format();
    let config = supported.config();
    let peak_millidb = Arc::new(AtomicI32::new(-80_000));
    let on_error_name = selected_name.clone();
    let error_callback = move |e| {
        tracing::warn!("audio input meter {on_error_name:?}: {e}");
    };

    let stream = match sample_format {
        SampleFormat::F32 => {
            let peak = peak_millidb.clone();
            device.build_input_stream(
                &config,
                move |data: &[f32], _| note_f32(data, &peak),
                error_callback,
                None,
            )
        }
        SampleFormat::I16 => {
            let peak = peak_millidb.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _| note_i16(data, &peak),
                error_callback,
                None,
            )
        }
        SampleFormat::U16 => {
            let peak = peak_millidb.clone();
            device.build_input_stream(
                &config,
                move |data: &[u16], _| note_u16(data, &peak),
                error_callback,
                None,
            )
        }
        other => return Err(format!("unsupported input sample format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    tracing::info!("live audio input meter on {selected_name}");

    while !cancel.is_cancelled() {
        std::thread::sleep(Duration::from_millis(100));
        let db = peak_millidb.swap(-80_000, Ordering::Relaxed) as f32 / 1000.0;
        let mut s = snap.lock();
        if !matches!(s.audio_label.as_str(), "no modem" | "no audio") {
            s.audio_in_db = db;
            s.audio_db = db;
            s.audio_label = crate::presets::audio_level_label(db).into();
        }
    }
    Ok(())
}

#[cfg(feature = "setup-probe")]
fn note_f32(data: &[f32], peak: &std::sync::atomic::AtomicI32) {
    note_level(samples_db(data.iter().copied()), peak);
}

#[cfg(feature = "setup-probe")]
fn note_i16(data: &[i16], peak: &std::sync::atomic::AtomicI32) {
    note_level(
        samples_db(data.iter().map(|&v| v as f32 / i16::MAX as f32)),
        peak,
    );
}

#[cfg(feature = "setup-probe")]
fn note_u16(data: &[u16], peak: &std::sync::atomic::AtomicI32) {
    note_level(
        samples_db(data.iter().map(|&v| (v as f32 - 32_768.0) / 32_768.0)),
        peak,
    );
}

#[cfg(feature = "setup-probe")]
fn note_level(db: f32, peak: &std::sync::atomic::AtomicI32) {
    use std::sync::atomic::Ordering;
    peak.fetch_max((db * 1000.0) as i32, Ordering::Relaxed);
}

fn samples_db(samples: impl Iterator<Item = f32>) -> f32 {
    let mut sum = 0.0_f64;
    let mut count = 0_u64;
    for sample in samples {
        let sample = sample.clamp(-1.0, 1.0) as f64;
        sum += sample * sample;
        count += 1;
    }
    if count == 0 {
        return crate::presets::AUDIO_FLOOR_DB;
    }
    let rms = (sum / count as f64).sqrt();
    (20.0 * rms.max(0.0001).log10()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_maps_silence_and_full_scale() {
        assert_eq!(samples_db([0.0; 8].into_iter()), -80.0);
        assert!((samples_db([1.0; 8].into_iter()) - 0.0).abs() < 0.001);
        assert!((samples_db([0.1; 8].into_iter()) - -20.0).abs() < 0.01);
    }
}
