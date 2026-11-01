//! SPDX-License-Identifier: Apache-2.0
//! Receive level: how loud the radio's audio reaches modem73, judged in
//! words a user can act on (#241).
//!
//! Every radio is different, so the judgement is on what decoding needs,
//! not on one radio's numbers. A test rig with two real modem73s (QA #241,
//! vhf-fm QPSK 1/2, squelch-0 hiss shaped like an FM receiver's) measured:
//!
//! - Decoding stayed perfect with the hiss anywhere from -50 to -4 dBFS
//!   RMS, as long as the signal itself did not clip.
//! - A signal that clips loses SNR: 4 % of its samples at full scale cost
//!   12 dB of SNR (35 -> 23 dB), 12 % cost 18 dB.
//! - Below about -60 dBFS modem73 stops finding frames at all.
//!
//! - modem73 stops finding frames when the whole input during a frame is
//!   under about -58 dBFS RMS (found frames at -58, none at -60).
//!
//! So "good" is wide (hiss -45 to -12 dBFS RMS), "high" is where a loud
//! signal starts to clip, and "clipping" is samples at full scale now.
//! Thresholds live here, not in `presets.rs` (frozen ladder).

/// Idle input below this RMS (dBFS) is too quiet on FM: a signal quiets
/// the hiss, so it arrives quieter than the hiss, and under about -58 dBFS
/// modem73 no longer finds the frame.
pub const LOW_DB: f32 = -45.0;
/// The same for a level-test (HF) station, where band noise stays under a
/// signal: just above modem73's floor.
pub const LOW_DB_LEVEL: f32 = -55.0;
/// Idle input above this RMS (dBFS) leaves too little room: a signal as
/// loud as the hiss would clip its peaks (OFDM peaks sit ~9-12 dB above
/// its RMS).
pub const HIGH_DB: f32 = -12.0;
/// A peak this close to full scale (dBFS) is "high" even when the RMS is fine.
pub const PEAK_HIGH_DB: f32 = -1.0;
/// Share of samples at full scale (%) that counts as clipping. Hiss at
/// -9 dBFS RMS clips about 0.5 %.
pub const CLIP_PCT: f32 = 0.5;
/// Level-test (HF) stations: idle noise must sit this far under the busy
/// threshold, or the station waits for a quiet moment that never comes.
pub const BUSY_MARGIN_DB: f32 = 3.0;

/// What the meter says about the input, worst first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Word {
    Clipping,
    High,
    /// Idle noise above the level busy test's threshold (HF): sends wait.
    Busy,
    Low,
    Good,
}

impl Word {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clipping => "clipping",
            Self::High => "high",
            Self::Busy => "noisy",
            Self::Low => "low",
            Self::Good => "good",
        }
    }
}

/// How the receive level can be changed on this station.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RxControl {
    /// Only the radio's own volume (or the computer's sound settings).
    #[default]
    Radio,
    /// The sound card's capture gain, through wcr (Linux mixer). `at_min`:
    /// already at its lowest setting.
    Mixer { at_min: bool, at_max: bool },
    /// wcr's own gain on Bluetooth audio, which the radio sets.
    Bluetooth { at_min: bool, at_max: bool },
}

/// The receive level as measured now.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Input {
    /// RMS, dBFS.
    pub rms_db: f32,
    /// Highest sample in the last half second, dBFS (newer modem73 only).
    pub peak_db: Option<f32>,
    /// Samples at full scale in the last half second, % (newer modem73 only).
    pub clip_pct: Option<f32>,
}

/// The busy test the station's modem73 runs.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Busy {
    /// `Some(threshold dBFS)` for the level test; `None` for sync (FM).
    pub level_threshold_db: Option<f32>,
}

/// The word and a plain hint for the input.
pub fn judge(input: Input, busy: Busy, control: RxControl) -> (Word, String) {
    let clipping = input.clip_pct.is_some_and(|c| c >= CLIP_PCT);
    let high = input.rms_db > HIGH_DB
        || input.peak_db.is_some_and(|p| p >= PEAK_HIGH_DB)
        || input.clip_pct.is_some_and(|c| c > 0.0);
    let down = turn_down(control);
    if clipping {
        let pct = input.clip_pct.unwrap_or(0.0);
        // On Bluetooth the clip count is taken before wcr's RX gain: the
        // radio's own audio is clipping, and RX gain cannot undo it (#247).
        if matches!(control, RxControl::Bluetooth { .. }) {
            return (
                Word::Clipping,
                format!(
                    "Clipping in the radio ({pct:.1}% of samples at full scale, before RX gain). Turn the radio's volume down until the meter is green; RX gain cannot fix this."
                ),
            );
        }
        return (
            Word::Clipping,
            format!(
                "Clipping ({pct:.1}% of samples at full scale). {down}, until the meter is green."
            ),
        );
    }
    if high {
        return (
            Word::High,
            format!("A little hot: a loud signal may clip. {down} a little."),
        );
    }
    if let Some(th) = busy.level_threshold_db {
        if input.rms_db > th - BUSY_MARGIN_DB {
            return (
                Word::Busy,
                format!(
                    "Idle noise is above the busy threshold ({th:.0} dBFS), so sends wait for a quiet moment. {down} until idle noise reads under {:.0} dBFS.",
                    th - BUSY_MARGIN_DB
                ),
            );
        }
    }
    let low = if busy.level_threshold_db.is_some() {
        LOW_DB_LEVEL
    } else {
        LOW_DB
    };
    if input.rms_db < low {
        return (
            Word::Low,
            format!("Too quiet: weak signals may be lost. {}.", turn_up(control)),
        );
    }
    (Word::Good, "Good: leave it.".into())
}

fn turn_down(control: RxControl) -> &'static str {
    match control {
        RxControl::Mixer { at_min: false, .. } => "Lower RX gain, or turn the radio's volume down",
        RxControl::Mixer { at_min: true, .. } => {
            "RX gain is at its lowest, so the radio's own audio is too loud: turn the radio's volume down"
        }
        RxControl::Bluetooth { at_min: false, .. } => "Lower RX gain",
        RxControl::Bluetooth { at_min: true, .. } => {
            "RX gain is at its lowest: the radio's Bluetooth audio is too loud for wcr to fix"
        }
        RxControl::Radio => "Turn the radio's volume down",
    }
}

fn turn_up(control: RxControl) -> &'static str {
    match control {
        RxControl::Mixer { at_max: false, .. } => "Turn the radio's volume up, or raise RX gain",
        RxControl::Bluetooth { at_max: false, .. } => "Raise RX gain",
        _ => "Turn the radio's volume up",
    }
}

/// Allowed transmit level change, dB: modem73's `tx_drive`. Our patched
/// modem73 takes 0.01..1.0 (-40 dB, #243: the Pi was still overdriven at
/// -26); the Windows port stops at 0.05 (-26 dB).
pub const TX_LEVEL_MIN_DB: f32 = if cfg!(windows) { -26.0 } else { -40.0 };
/// `tx_drive` at [`TX_LEVEL_MIN_DB`].
const TX_DRIVE_MIN: f32 = if cfg!(windows) { 0.05 } else { 0.01 };
pub const TX_LEVEL_MAX_DB: f32 = 0.0;

/// A transmit level kept in range.
pub fn tx_level_db(db: f32) -> f32 {
    if db.is_finite() {
        db.clamp(TX_LEVEL_MIN_DB, TX_LEVEL_MAX_DB)
    } else {
        0.0
    }
}

/// modem73's `tx_drive` for a transmit level in dB.
pub fn tx_drive(db: f32) -> f32 {
    10f32.powf(tx_level_db(db) / 20.0).clamp(TX_DRIVE_MIN, 1.0)
}

/// modem73's `--tx-level` percent for a transmit level in dB.
pub fn tx_level_pct(db: f32) -> u32 {
    (tx_drive(db) * 100.0)
        .round()
        .clamp(TX_DRIVE_MIN * 100.0, 100.0) as u32
}

/// modem73's own settings file, which it reads at every launch.
pub fn modem73_settings_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        std::path::PathBuf::from(home)
            .join(".config")
            .join("modem73")
            .join("settings"),
    )
}

/// #247: a TX level from before wcr.toml kept one. When wcr.toml has no
/// `tx_level_db` yet and modem73's settings file holds a `tx_drive` below
/// full, that is the level the station was really sending at: take it over
/// once, in dB. `None` when there is nothing to carry over.
pub fn carried_over_tx_level(cfg_text: &str, modem73_settings: &str) -> Option<f32> {
    if cfg_text
        .lines()
        .any(|l| l.trim_start().starts_with("tx_level_db"))
    {
        return None;
    }
    let drive: f32 = modem73_settings.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "tx_drive").then(|| v.trim().parse().ok())?
    })?;
    if !drive.is_finite() || drive <= 0.0 || drive >= 0.995 {
        return None;
    }
    Some(tx_level_db((20.0 * drive.log10() * 10.0).round() / 10.0))
}

/// Bluetooth receive level range, dB.
pub const BT_RX_MIN_DB: f32 = -30.0;
pub const BT_RX_MAX_DB: f32 = 12.0;

/// A Bluetooth receive level kept in range.
pub fn bt_rx_level_db(db: f32) -> f32 {
    if db.is_finite() {
        db.clamp(BT_RX_MIN_DB, BT_RX_MAX_DB)
    } else {
        0.0
    }
}

/// modem73's own default busy threshold, dBFS (vhf-fm, hf-good).
pub const MODEM73_THRESHOLD_DB: f32 = -30.0;

/// The level test's threshold for this station, dBFS, as modem73 runs it.
pub fn level_threshold_db(m: &crate::config::ModemConfig) -> f32 {
    use crate::presets::Preset;
    if m.is_vox() {
        return Preset::VOX_CSMA_THRESHOLD_DB as f32;
    }
    Preset::parse(&m.preset)
        .and_then(|p| p.csma_threshold_db())
        .map(f32::from)
        .unwrap_or(MODEM73_THRESHOLD_DB)
}

/// At station start (a sound-card or Bluetooth audio radio): sets the
/// card's RX gain from wcr.toml, and puts the busy test, the RX gain and
/// the TX level in the status for the meters and sliders.
pub fn apply_at_start(cfg: &crate::config::Config, snap: &crate::status::SharedStatus) {
    use crate::config::CsmaMode;
    let m = &cfg.modem;
    let (mode, threshold) = match m.csma_mode(cfg.rf.frequency_khz) {
        Some(CsmaMode::Sync) => ("sync", None),
        Some(CsmaMode::Level) => ("level", Some(level_threshold_db(m))),
        // VOX: modem73's level test with the VOX gate.
        None => ("level", Some(level_threshold_db(m))),
    };
    let (rx_gain, note) = if m.is_bt_audio() {
        (
            Some(crate::status::RxGain::bluetooth(bt_rx_level_db(
                cfg.tnc.bt_rx_level_db,
            ))),
            String::new(),
        )
    } else {
        let got = match m.rx_gain_db {
            Some(db) => crate::mixer::set(&m.audio_input, db),
            None => crate::mixer::read(&m.audio_input),
        };
        match got {
            Ok(g) => {
                if m.rx_gain_db.is_some() {
                    tracing::info!(
                        "RX gain: {} {:+.1} dB (range {:+.1} to {:+.1}), automatic gain {}",
                        g.control,
                        g.db,
                        g.min_db,
                        g.max_db,
                        match g.agc {
                            Some(true) => "on",
                            Some(false) => "off",
                            None => "not on this card",
                        }
                    );
                }
                (Some(crate::status::RxGain::from_mixer(&g)), String::new())
            }
            Err(e) => {
                if m.rx_gain_db.is_some() {
                    tracing::warn!("RX gain not set: {e}");
                }
                (None, e)
            }
        }
    };
    let mut s = snap.lock();
    s.csma_mode = mode.into();
    s.csma_threshold_db = threshold;
    s.rx_gain = rx_gain;
    s.rx_gain_note = note;
    s.tx_level_db = tx_level_db(m.tx_level_db);
}

/// What `/level` changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Knob {
    Rx,
    Tx,
}

/// `/level`, `/level rx <dB>`, `/level tx <dB>`: shows or sets the RX gain
/// and the TX level, live, and saves them in wcr.toml (#241). `control` is
/// modem73's control port while the radio runs.
pub async fn level_cmd(
    cfg: &parking_lot::Mutex<crate::config::Config>,
    snap: &crate::status::SharedStatus,
    control: Option<&crate::modem::ControlClient>,
    which: Option<&str>,
    value: Option<&str>,
) -> Result<String, String> {
    let knob = match which.map(|w| w.to_ascii_lowercase()) {
        None => return Ok(level_summary(&cfg.lock(), snap)),
        Some(w) if w == "rx" => Knob::Rx,
        Some(w) if w == "tx" => Knob::Tx,
        Some(_) => return Err("usage: /level [rx|tx <dB>]".into()),
    };
    let Some(raw) = value else {
        return Ok(level_summary(&cfg.lock(), snap));
    };
    let db: f32 = raw
        .trim()
        .trim_end_matches("dB")
        .trim_end_matches("db")
        .trim()
        .parse()
        .map_err(|_| format!("{raw} is not a number of dB"))?;
    if !db.is_finite() {
        return Err(format!("{raw} is not a number of dB"));
    }
    if cfg.lock().modem.uses_tnc() {
        return Err("this radio uses its own TNC: set levels on the radio".into());
    }
    match knob {
        Knob::Tx => {
            let db = tx_level_db(db);
            if let Some(c) = control {
                c.set_config(serde_json::json!({ "tx_drive": tx_drive(db) }))
                    .await
                    .map_err(|e| format!("modem73 did not take the TX level: {e}"))?;
            }
            cfg.lock().modem.tx_level_db = db;
            snap.lock().tx_level_db = db;
            Ok(format!("TX level {db:+.1} dB"))
        }
        Knob::Rx => {
            let bt = cfg.lock().modem.is_bt_audio();
            let gain = if bt {
                let db = bt_rx_level_db(db);
                crate::btaudio::set_rx_level_db(db);
                cfg.lock().tnc.bt_rx_level_db = db;
                crate::status::RxGain::bluetooth(db)
            } else {
                let device = cfg.lock().modem.audio_input.clone();
                let g = crate::mixer::set(&device, db)?;
                cfg.lock().modem.rx_gain_db = Some(g.db);
                crate::status::RxGain::from_mixer(&g)
            };
            let text = format!("RX gain {:+.1} dB", gain.db);
            let mut s = snap.lock();
            s.rx_gain = Some(gain);
            s.rx_gain_note.clear();
            s.judge_input();
            Ok(text)
        }
    }
}

fn level_summary(cfg: &crate::config::Config, snap: &crate::status::SharedStatus) -> String {
    let s = snap.lock();
    let rx = match &s.rx_gain {
        Some(g) => format!(
            "RX gain {:+.1} dB ({:+.0} to {:+.0}{})",
            g.db,
            g.min_db,
            g.max_db,
            if g.kind == "bluetooth" {
                ", Bluetooth"
            } else {
                ""
            }
        ),
        None if s.rx_gain_note.is_empty() => "RX gain: the radio's volume".into(),
        None => format!("RX gain: {}", s.rx_gain_note),
    };
    let input = if s.audio_in_word.is_empty() {
        "input not measured".to_string()
    } else {
        format!(
            "input {:.0} dBFS{}{} ({}). {}",
            s.audio_in_db,
            s.audio_in_peak_db
                .map(|p| format!(", peak {p:.0}"))
                .unwrap_or_default(),
            s.audio_in_clip_pct
                .filter(|c| *c > 0.0)
                .map(|c| format!(", {c:.1}% clipped"))
                .unwrap_or_default(),
            s.audio_in_word,
            s.audio_in_hint
        )
    };
    format!(
        "{rx} | TX level {:+.1} dB | {input} | /level rx <dB>, /level tx <dB> ({:.0} to 0)",
        tx_level_db(cfg.modem.tx_level_db),
        TX_LEVEL_MIN_DB
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(rms_db: f32, peak_db: f32, clip_pct: f32) -> Input {
        Input {
            rms_db,
            peak_db: Some(peak_db),
            clip_pct: Some(clip_pct),
        }
    }

    #[test]
    fn an_old_tx_level_is_carried_over_once() {
        // modem73 kept 0.126 (-18 dB); wcr.toml has no level yet.
        let db =
            carried_over_tx_level("[modem]\npreset = \"vhf-fm\"\n", "tx_drive=0.126\n").unwrap();
        assert!((db - -18.0).abs() < 0.1, "{db}");
        // wcr.toml already has one: it wins.
        assert_eq!(
            carried_over_tx_level("[modem]\ntx_level_db = -36.0\n", "tx_drive=0.126\n"),
            None
        );
        // Full drive, nothing, or junk: nothing to carry.
        assert_eq!(carried_over_tx_level("", "tx_drive=1.0\n"), None);
        assert_eq!(carried_over_tx_level("", "callsign=M7TJF\n"), None);
        assert_eq!(carried_over_tx_level("", "tx_drive=abc\n"), None);
    }

    const FM: Busy = Busy {
        level_threshold_db: None,
    };

    /// QA #241, Pi + BF-1909 at squelch 0: -1.5 dBFS of hiss is clipping;
    /// -6.3 dBFS (Digirig capture at 0) still clips its peaks.
    #[test]
    fn the_pi_hiss_qa_measured_reads_as_clipping() {
        let (w, hint) = judge(input(-1.5, 0.0, 26.0), FM, RxControl::Radio);
        assert_eq!(w, Word::Clipping);
        assert!(hint.contains("radio's volume down"), "{hint}");
        let (w, _) = judge(input(-6.3, 0.0, 4.6), FM, RxControl::Radio);
        assert_eq!(w, Word::Clipping);
        let (w, _) = judge(input(-10.0, -0.5, 0.2), FM, RxControl::Radio);
        assert_eq!(w, Word::High);
    }

    #[test]
    fn a_wide_middle_is_good() {
        for rms in [-44.0, -30.0, -20.0, -13.0] {
            let (w, _) = judge(input(rms, rms + 11.0, 0.0), FM, RxControl::Radio);
            assert_eq!(w, Word::Good, "{rms}");
        }
        let (w, hint) = judge(input(-53.8, -42.0, 0.0), FM, RxControl::Radio);
        assert_eq!(w, Word::Low);
        assert!(hint.contains("volume up"), "{hint}");
    }

    /// An older modem73 reports RMS only: the RMS bands still work.
    #[test]
    fn rms_alone_is_judged_too() {
        let rms = |db: f32| Input {
            rms_db: db,
            ..Input::default()
        };
        assert_eq!(judge(rms(-6.0), FM, RxControl::Radio).0, Word::High);
        assert_eq!(judge(rms(-25.0), FM, RxControl::Radio).0, Word::Good);
        assert_eq!(judge(rms(-60.0), FM, RxControl::Radio).0, Word::Low);
    }

    /// The hint says what this station can change.
    #[test]
    fn the_hint_names_the_knob_that_works() {
        let hot = input(-3.0, 0.0, 12.0);
        let (_, h) = judge(
            hot,
            FM,
            RxControl::Mixer {
                at_min: false,
                at_max: false,
            },
        );
        assert!(h.contains("Lower RX gain"), "{h}");
        let (_, h) = judge(
            hot,
            FM,
            RxControl::Mixer {
                at_min: true,
                at_max: false,
            },
        );
        assert!(h.contains("radio's own audio is too loud"), "{h}");
        // Bluetooth clipping happens in the radio, before RX gain (#247).
        let bt = RxControl::Bluetooth {
            at_min: false,
            at_max: false,
        };
        let (_, h) = judge(hot, FM, bt);
        assert!(h.contains("radio's volume down"), "{h}");
        assert!(!h.contains("Lower RX gain"), "{h}");
        // Hot but not clipping: RX gain still helps.
        let (_, h) = judge(input(-6.0, -1.0, 0.0), FM, bt);
        assert!(h.contains("Lower RX gain"), "{h}");
        let quiet = input(-55.0, -44.0, 0.0);
        let (_, h) = judge(
            quiet,
            FM,
            RxControl::Bluetooth {
                at_min: false,
                at_max: false,
            },
        );
        assert!(h.contains("Raise RX gain"), "{h}");
    }

    /// HF keeps the level busy test (#241): idle noise over the threshold
    /// holds every send.
    #[test]
    fn hf_noise_above_the_busy_threshold_is_flagged() {
        let hf = Busy {
            level_threshold_db: Some(-50.0),
        };
        let (w, h) = judge(input(-40.0, -29.0, 0.0), hf, RxControl::Radio);
        assert_eq!(w, Word::Busy);
        assert!(h.contains("-53 dBFS"), "{h}");
        assert_eq!(
            judge(input(-54.0, -43.0, 0.0), hf, RxControl::Radio).0,
            Word::Good
        );
        assert_eq!(
            judge(input(-60.0, -49.0, 0.0), hf, RxControl::Radio).0,
            Word::Low
        );
        assert_eq!(
            judge(input(-44.0, -33.0, 0.0), FM, RxControl::Radio).0,
            Word::Good
        );
    }

    /// QA #243: modem73's frame `level_db` is the 200 ms after a decode
    /// (the hiss after the frame), not the frame, so it is not judged: a
    /// quiet idle input reads low whatever the last decode said.
    #[test]
    fn a_quiet_idle_input_is_low_without_any_frame_override() {
        assert_eq!(
            judge(input(-70.0, -60.0, 0.0), FM, RxControl::Radio).0,
            Word::Low
        );
    }

    #[test]
    fn transmit_level_only_turns_down() {
        assert_eq!(tx_level_db(6.0), 0.0);
        assert_eq!(tx_level_db(-60.0), TX_LEVEL_MIN_DB);
        assert_eq!(tx_level_db(-26.0), -26.0);
        assert_eq!(tx_level_db(f32::NAN), 0.0);
        assert_eq!(tx_level_pct(0.0), 100);
        assert_eq!(tx_level_pct(-6.0), 50);
        assert_eq!(tx_level_pct(-26.0), 5);
        if !cfg!(windows) {
            assert_eq!(tx_level_db(-40.0), -40.0);
            assert_eq!(tx_level_pct(-40.0), 1);
        }
        assert!((tx_drive(-20.0) - 0.1).abs() < 0.001);
    }

    #[test]
    fn the_level_threshold_is_the_one_modem73_runs() {
        let m = |ptt: &str, preset: &str| crate::config::ModemConfig {
            ptt: ptt.into(),
            preset: preset.into(),
            ..Default::default()
        };
        assert_eq!(level_threshold_db(&m("hamlib", "hf-poor")), -50.0);
        assert_eq!(level_threshold_db(&m("hamlib", "hf-good")), -30.0);
        assert_eq!(level_threshold_db(&m("vox", "hf-poor")), -30.0);
    }

    #[test]
    fn bluetooth_receive_level_is_kept_in_range() {
        assert_eq!(bt_rx_level_db(20.0), 12.0);
        assert_eq!(bt_rx_level_db(-40.0), -30.0);
        assert_eq!(bt_rx_level_db(f32::INFINITY), 0.0);
        assert_eq!(bt_rx_level_db(-6.0), -6.0);
    }
}
