//! SPDX-License-Identifier: Apache-2.0
//! Live sound-card meters.
//!
//! modem73 2.4 reports RX level only after a complete frame decodes. On
//! macOS/Linux this monitor opens the capture device in shared mode. On
//! Windows a second WASAPI capture (or even IAudioMeterInformation on the
//! capture endpoint) glitches modem73's stream so MFSK CRC dies. We only
//! meter the render endpoint.

use crate::status::SharedStatus;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[cfg(all(feature = "setup-probe", not(windows)))]
pub fn spawn_input_meter(device_name: String, snap: Arc<SharedStatus>, cancel: CancellationToken) {
    let _ = std::thread::Builder::new()
        .name("wcr-audio-meter".into())
        .spawn(move || {
            if let Err(e) = run_input_meter(&device_name, snap, cancel) {
                tracing::warn!("live audio input meter unavailable: {e}");
            }
        });
}

#[cfg(all(feature = "setup-probe", windows))]
pub fn spawn_input_meter(_device_name: String, snap: Arc<SharedStatus>, cancel: CancellationToken) {
    let _ = std::thread::Builder::new()
        .name("wcr-audio-meter".into())
        .spawn(move || {
            if let Err(e) = run_wasapi_peak_meters(snap, cancel) {
                tracing::warn!("WASAPI peak meters unavailable: {e}");
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

#[cfg(all(feature = "setup-probe", windows))]
fn run_wasapi_peak_meters(
    snap: Arc<SharedStatus>,
    cancel: CancellationToken,
) -> Result<(), String> {
    use std::time::Duration;
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        let render = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("render endpoint: {e}"))?;
        let out_meter: IAudioMeterInformation = render
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("render meter: {e}"))?;
        tracing::info!("WASAPI render peak meter (capture left to modem73)");
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(100));
            let out_db = peak_to_db(out_meter.GetPeakValue().unwrap_or(0.0));
            let mut s = snap.lock();
            if s.audio_label.as_str() != "no modem" {
                s.audio_out_db = out_db;
            }
        }
    }
    Ok(())
}

fn peak_to_db(peak: f32) -> f32 {
    let peak = peak.clamp(0.0, 1.0) as f64;
    if peak <= 0.0001 {
        crate::presets::AUDIO_FLOOR_DB
    } else {
        (20.0 * peak.log10()) as f32
    }
}

#[cfg(any(test, all(feature = "setup-probe", not(windows))))]
/// First wait before reopening a meter stream that failed.
const METER_RETRY_MIN_SECS: u64 = 2;
#[cfg(any(test, all(feature = "setup-probe", not(windows))))]
/// Longest wait between reopen attempts.
const METER_RETRY_MAX_SECS: u64 = 60;
#[cfg(all(feature = "setup-probe", not(windows)))]
/// A stream that stayed up this long counts as healthy again, so the
/// next failure starts over at the short wait.
const METER_HEALTHY_SECS: u64 = 30;

#[cfg(any(test, all(feature = "setup-probe", not(windows))))]
/// Next wait after a failed attempt: doubles, capped at the maximum.
fn next_meter_retry(secs: u64) -> u64 {
    (secs * 2).clamp(METER_RETRY_MIN_SECS, METER_RETRY_MAX_SECS)
}

/// Keeps the input meter running. When the sound card or audio server
/// goes away, cpal can call the error callback thousands of times a
/// second. The callback only raises a flag; this loop drops the broken
/// stream, logs once, and tries to reopen it with a growing wait.
#[cfg(all(feature = "setup-probe", not(windows)))]
fn run_input_meter(
    device_name: &str,
    snap: Arc<SharedStatus>,
    cancel: CancellationToken,
) -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
    use std::time::{Duration, Instant};

    let peak_millidb = Arc::new(AtomicI32::new(-80_000));
    let mut retry_secs = METER_RETRY_MIN_SECS;
    let mut ever_opened = false;
    let mut reported_down = false;
    while !cancel.is_cancelled() {
        let broken = Arc::new(AtomicBool::new(false));
        match open_input_stream(device_name, peak_millidb.clone(), broken.clone()) {
            Ok((stream, selected_name)) => {
                if reported_down {
                    tracing::info!("live audio input meter back on {selected_name}");
                } else {
                    tracing::info!("live audio input meter on {selected_name}");
                }
                ever_opened = true;
                let opened_at = Instant::now();
                while !cancel.is_cancelled() && !broken.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                    let db = peak_millidb.swap(-80_000, Ordering::Relaxed) as f32 / 1000.0;
                    set_input_level(&snap, db);
                }
                drop(stream);
                if cancel.is_cancelled() {
                    break;
                }
                set_input_level(&snap, crate::presets::AUDIO_FLOOR_DB);
                if opened_at.elapsed() >= Duration::from_secs(METER_HEALTHY_SECS) {
                    retry_secs = METER_RETRY_MIN_SECS;
                }
                tracing::warn!(
                    "audio input meter {selected_name:?} stopped (device or audio server error); \
                     retrying quietly every {METER_RETRY_MIN_SECS}-{METER_RETRY_MAX_SECS} s"
                );
                reported_down = true;
            }
            Err(e) if !ever_opened && !reported_down => {
                // Same as before: a station with no usable input device just
                // has no live meter. Keep trying quietly in case it appears.
                tracing::warn!("live audio input meter unavailable: {e}");
                reported_down = true;
            }
            Err(e) => tracing::debug!("audio input meter reopen failed: {e}"),
        }
        let wait_until = Instant::now() + Duration::from_secs(retry_secs);
        while !cancel.is_cancelled() && Instant::now() < wait_until {
            std::thread::sleep(Duration::from_millis(200));
        }
        retry_secs = next_meter_retry(retry_secs);
    }
    Ok(())
}

#[cfg(all(feature = "setup-probe", not(windows)))]
fn set_input_level(snap: &SharedStatus, db: f32) {
    let mut s = snap.lock();
    if s.audio_label.as_str() != "no modem" {
        s.audio_in_db = db;
        s.audio_db = db;
        s.audio_label = crate::presets::audio_level_label(db).into();
    }
}

/// Opens the capture stream. The error callback never logs: it only sets
/// `broken`, so a dead device cannot flood the log or spin the CPU.
#[cfg(all(feature = "setup-probe", not(windows)))]
fn open_input_stream(
    device_name: &str,
    peak_millidb: Arc<std::sync::atomic::AtomicI32>,
    broken: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(cpal::Stream, String), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;
    use std::sync::atomic::Ordering;

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
    let error_callback = move |_e| broken.store(true, Ordering::Relaxed);

    let peak = peak_millidb;
    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| note_f32(data, &peak),
            error_callback,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| note_i16(data, &peak),
            error_callback,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| note_u16(data, &peak),
            error_callback,
            None,
        ),
        other => return Err(format!("unsupported input sample format {other:?}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, selected_name))
}

#[cfg(all(feature = "setup-probe", not(windows)))]
fn note_f32(data: &[f32], peak: &std::sync::atomic::AtomicI32) {
    note_level(samples_db(data.iter().copied()), peak);
}

#[cfg(all(feature = "setup-probe", not(windows)))]
fn note_i16(data: &[i16], peak: &std::sync::atomic::AtomicI32) {
    note_level(
        samples_db(data.iter().map(|&v| v as f32 / i16::MAX as f32)),
        peak,
    );
}

#[cfg(all(feature = "setup-probe", not(windows)))]
fn note_u16(data: &[u16], peak: &std::sync::atomic::AtomicI32) {
    note_level(
        samples_db(data.iter().map(|&v| (v as f32 - 32_768.0) / 32_768.0)),
        peak,
    );
}

#[cfg(all(feature = "setup-probe", not(windows)))]
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

    #[test]
    fn meter_retry_doubles_up_to_a_minute() {
        assert_eq!(next_meter_retry(METER_RETRY_MIN_SECS), 4);
        assert_eq!(next_meter_retry(32), 60);
        assert_eq!(next_meter_retry(60), 60);
    }

    #[test]
    fn peak_maps_silence_and_full_scale() {
        assert_eq!(peak_to_db(0.0), -80.0);
        assert!((peak_to_db(1.0) - 0.0).abs() < 0.001);
        assert!((peak_to_db(0.1) - -20.0).abs() < 0.01);
    }
}
