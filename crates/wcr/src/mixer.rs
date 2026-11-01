//! SPDX-License-Identifier: Apache-2.0
//! Sound card capture gain through the card's own mixer (#241).
//!
//! modem73 owns the capture device and copies samples unscaled, so the only
//! receive gain before the analog-to-digital converter is the card's mixer:
//! on a Digirig (CM108) that is `Mic` capture, 0 to +23.8 dB, plus a switch
//! called `Auto Gain Control`. wcr sets these on Linux at every station
//! start and when the RX slider moves, with no sudo: the mixer is the same
//! `/dev/snd/controlC*` the `audio` group already opens. The value lives in
//! wcr.toml (`[modem] rx_gain_db`), never in `alsactl store`, so nothing
//! outside the station's own config changes.

use serde::Serialize;

/// The capture control wcr found and set, for `/status` and the sliders.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureGain {
    /// The mixer control, e.g. `Mic`.
    pub control: String,
    /// Gain now, dB.
    pub db: f32,
    pub min_db: f32,
    pub max_db: f32,
    /// One step of the control, dB (the Digirig's `Mic` moves 1.49 dB).
    pub step_db: f32,
    /// The card's automatic gain: `Some(false)` off, `None` if it has none.
    pub agc: Option<bool>,
}

impl CaptureGain {
    pub fn at_min(&self) -> bool {
        self.db <= self.min_db + 0.5
    }

    pub fn at_max(&self) -> bool {
        self.db >= self.max_db - 0.5
    }
}

/// The ALSA card for a modem73 capture device such as
/// `plughw:CARD=Device,DEV=0`, `hw:1,0` or `default:CARD=Device`, as the
/// mixer name `hw:Device` / `hw:1`. `None` for the system default or a
/// device name that is not an ALSA card (PulseAudio, network audio).
pub fn card_of(device: &str) -> Option<String> {
    let d = device.trim();
    let (kind, rest) = d.split_once(':')?;
    let kind = kind.to_ascii_lowercase();
    if !matches!(
        kind.as_str(),
        "hw" | "plughw" | "default" | "sysdefault" | "front" | "dsnoop"
    ) {
        return None;
    }
    let mut card = None;
    for (i, part) in rest.split(',').enumerate() {
        let part = part.trim();
        if let Some(v) = part
            .strip_prefix("CARD=")
            .or_else(|| part.strip_prefix("card="))
        {
            card = Some(v.to_string());
        } else if i == 0 && !part.contains('=') && !part.is_empty() {
            card = Some(part.to_string());
        }
    }
    card.filter(|c| !c.is_empty()).map(|c| format!("hw:{c}"))
}

/// Capture controls, best first. A Digirig / CM108 card calls it `Mic`.
const CAPTURE_NAMES: &[&str] = &["Mic", "Capture", "Line", "PCM Capture", "Mic Capture"];
/// The CM108's automatic gain switch.
const AGC_NAMES: &[&str] = &["Auto Gain Control", "AGC", "Mic Boost (AGC)"];

#[cfg(all(target_os = "linux", feature = "setup-probe"))]
mod imp {
    use super::*;
    use alsa::mixer::{MilliBel, Mixer, Selem, SelemChannelId};

    fn open(card: &str) -> Result<Mixer, String> {
        Mixer::new(card, false).map_err(|e| format!("cannot open the mixer of {card}: {e}"))
    }

    fn name(s: &Selem) -> String {
        s.get_id().get_name().unwrap_or("").to_string()
    }

    fn capture<'a>(mixer: &'a Mixer) -> Option<Selem<'a>> {
        let all: Vec<Selem<'a>> = mixer.iter().filter_map(Selem::new).collect();
        for want in CAPTURE_NAMES {
            if let Some(i) = all
                .iter()
                .position(|s| s.has_capture_volume() && name(s).eq_ignore_ascii_case(want))
            {
                return all.into_iter().nth(i);
            }
        }
        all.into_iter().find(|s| s.has_capture_volume())
    }

    fn agc_state(mixer: &Mixer, turn_off: bool) -> Option<bool> {
        for s in mixer.iter().filter_map(Selem::new) {
            let n = name(&s);
            if !AGC_NAMES.iter().any(|a| n.eq_ignore_ascii_case(a)) {
                continue;
            }
            let ch = SelemChannelId::mono();
            if s.has_playback_switch() {
                if turn_off {
                    let _ = s.set_playback_switch_all(0);
                }
                return s.get_playback_switch(ch).ok().map(|v| v != 0);
            }
            if s.has_capture_switch() {
                if turn_off {
                    let _ = s.set_capture_switch_all(0);
                }
                return s.get_capture_switch(ch).ok().map(|v| v != 0);
            }
        }
        None
    }

    fn read_selem(s: &Selem, agc: Option<bool>) -> CaptureGain {
        let (lo, hi) = s.get_capture_db_range();
        let db = s
            .get_capture_vol_db(SelemChannelId::mono())
            .map(|v| v.to_db())
            .unwrap_or(lo.to_db());
        let (vmin, vmax) = s.get_capture_volume_range();
        CaptureGain {
            control: name(s),
            db,
            min_db: lo.to_db(),
            max_db: hi.to_db(),
            step_db: super::step_db(lo.to_db(), hi.to_db(), vmax - vmin),
            agc,
        }
    }

    pub fn read(card: &str) -> Result<CaptureGain, String> {
        let mixer = open(card)?;
        let s = capture(&mixer).ok_or_else(|| format!("{card} has no capture gain control"))?;
        let agc = agc_state(&mixer, false);
        Ok(read_selem(&s, agc))
    }

    pub fn set(card: &str, db: f32) -> Result<CaptureGain, String> {
        let mixer = open(card)?;
        let s = capture(&mixer).ok_or_else(|| format!("{card} has no capture gain control"))?;
        let (lo, hi) = s.get_capture_db_range();
        // A slider value on a step can come back a hair under it in
        // floating point, and Floor then took the step below (#243).
        let want = (db + 0.05).clamp(lo.to_db(), hi.to_db());
        s.set_capture_db_all(MilliBel::from_db(want), alsa::Round::Floor)
            .map_err(|e| format!("cannot set {} on {card}: {e}", name(&s)))?;
        if s.has_capture_switch() {
            let _ = s.set_capture_switch_all(1);
        }
        let agc = agc_state(&mixer, true);
        Ok(read_selem(&s, agc))
    }
}

#[cfg(not(all(target_os = "linux", feature = "setup-probe")))]
mod imp {
    use super::*;

    pub fn read(_card: &str) -> Result<CaptureGain, String> {
        Err("capture gain is set on Linux only".into())
    }

    pub fn set(_card: &str, _db: f32) -> Result<CaptureGain, String> {
        Err("capture gain is set on Linux only".into())
    }
}

/// One step of a control with `steps` steps over `min_db..max_db`, dB. A
/// control that does not say falls back to 0.5 dB.
pub fn step_db(min_db: f32, max_db: f32, steps: i64) -> f32 {
    if steps > 0 && max_db > min_db {
        (max_db - min_db) / steps as f32
    } else {
        0.5
    }
}

/// The capture gain of the card behind `device`, if wcr can reach it.
pub fn read(device: &str) -> Result<CaptureGain, String> {
    let card = card_of(device).ok_or_else(|| not_a_card(device))?;
    imp::read(&card)
}

/// Sets the capture gain of the card behind `device` (clamped to the
/// card's range) and turns its automatic gain off.
pub fn set(device: &str, db: f32) -> Result<CaptureGain, String> {
    if !db.is_finite() {
        return Err("RX gain must be a number of dB".into());
    }
    let card = card_of(device).ok_or_else(|| not_a_card(device))?;
    imp::set(&card, db)
}

fn not_a_card(device: &str) -> String {
    if device.trim().is_empty() {
        "no sound card is named in [modem] audio_input, so RX gain is the radio's volume".into()
    } else {
        format!("{device} is not an ALSA sound card, so RX gain is the radio's volume")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_comes_from_the_modem73_device_name() {
        assert_eq!(
            card_of("plughw:CARD=Device,DEV=0").as_deref(),
            Some("hw:Device")
        );
        assert_eq!(
            card_of("hw:CARD=Device,DEV=0").as_deref(),
            Some("hw:Device")
        );
        assert_eq!(card_of("default:CARD=Device").as_deref(), Some("hw:Device"));
        assert_eq!(card_of("plughw:1,0").as_deref(), Some("hw:1"));
        assert_eq!(card_of("hw:2").as_deref(), Some("hw:2"));
        assert_eq!(card_of(""), None);
        assert_eq!(card_of("default"), None);
        assert_eq!(card_of("tcp:127.0.0.1:5000"), None);
        assert_eq!(card_of("USB PnP Sound Device"), None);
        assert_eq!(card_of("pulse"), None);
    }

    #[test]
    fn a_device_that_is_not_a_card_says_why() {
        assert!(set("", -3.0).unwrap_err().contains("audio_input"));
        assert!(set("tcp:127.0.0.1:5000", -3.0)
            .unwrap_err()
            .contains("not an ALSA sound card"));
        assert!(set("plughw:CARD=Device,DEV=0", f32::NAN).is_err());
    }

    #[test]
    fn ends_of_the_range_are_known() {
        let g = CaptureGain {
            control: "Mic".into(),
            db: 0.0,
            min_db: 0.0,
            max_db: 23.81,
            step_db: step_db(0.0, 23.81, 16),
            agc: Some(false),
        };
        assert!(g.at_min());
        assert!(!g.at_max());
        // The Digirig's Mic: 16 steps of 1.49 dB (QA #243 saw 5.5 -> 5.95).
        assert!((g.step_db - 1.488).abs() < 0.01);
        assert_eq!(step_db(0.0, 10.0, 0), 0.5);
    }
}
