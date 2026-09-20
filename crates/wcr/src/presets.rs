//! SPDX-License-Identifier: Apache-2.0
//! Modem presets for every level of ham.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    VhfFm,
    HfGood,
    HfPoor,
    HfWeak,
    VoxSafe,
    HfDeep,
    /// Radio with a built-in 1200 bd AFSK KISS TNC (VR-N76, UV-PRO, GA-5WB).
    Afsk1200,
}

impl Preset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vhf-fm" | "vhf" | "fm" => Some(Self::VhfFm),
            "hf-good" | "hfgood" => Some(Self::HfGood),
            "hf-poor" | "hfpoor" => Some(Self::HfPoor),
            "hf-weak" | "hfweak" => Some(Self::HfWeak),
            "vox-safe" | "vox" => Some(Self::VoxSafe),
            "hf-deep" | "hfdeep" => Some(Self::HfDeep),
            "afsk-1200" | "afsk1200" | "afsk" | "tnc" | "packet" => Some(Self::Afsk1200),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::VhfFm => "vhf-fm",
            Self::HfGood => "hf-good",
            Self::HfPoor => "hf-poor",
            Self::HfWeak => "hf-weak",
            Self::VoxSafe => "vox-safe",
            Self::HfDeep => "hf-deep",
            Self::Afsk1200 => "afsk-1200",
        }
    }

    pub fn all() -> [Preset; 7] {
        [
            Self::VhfFm,
            Self::HfGood,
            Self::HfPoor,
            Self::HfWeak,
            Self::VoxSafe,
            Self::HfDeep,
            Self::Afsk1200,
        ]
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::VhfFm => "VHF/UHF FM, clean local links (OFDM QPSK 1/2)",
            Self::HfGood => "Good HF SSB path (OFDM 8PSK 1/2 + postamble)",
            Self::HfPoor => "Fading HF, NVIS (ROBUST RDM-600S)",
            Self::HfWeak => "Very weak HF (RDM-300S)",
            Self::VoxSafe => {
                "Any radio with VOX: MFSK-32R plus a long lead so the radio is keyed before data"
            }
            Self::HfDeep => "Deep-fade HF backup (MFSK-32R, below the noise floor)",
            Self::Afsk1200 => {
                "Radio's own KISS TNC over Bluetooth (VR-N76, UV-PRO, GA-5WB): 1200 bd AFSK packet"
            }
        }
    }

    /// The radio does the modulation itself; modem73 is not used.
    pub fn is_radio_tnc(self) -> bool {
        matches!(self, Self::Afsk1200)
    }

    /// Approximate payload bitrate in bits per second (after FEC).
    pub fn bitrate_bps(self) -> u32 {
        match self {
            Self::VhfFm => 2400,
            Self::HfGood => 2400,
            Self::HfPoor => 378,
            Self::HfWeak => 194,
            Self::VoxSafe => 99,
            Self::HfDeep => 99,
            Self::Afsk1200 => 1200,
        }
    }

    /// Typical PHY payload bytes per frame.
    pub fn payload_bytes(self) -> u32 {
        match self {
            Self::VhfFm => 512,
            Self::HfGood => 512,
            Self::HfPoor => 170,
            Self::HfWeak => 170,
            Self::VoxSafe => 55,
            Self::HfDeep => 55,
            // AX.25 info field; the built-in TNC is happiest well under 256.
            Self::Afsk1200 => 200,
        }
    }

    /// Preamble + lead-tone overhead in milliseconds.
    pub fn overhead_ms(self) -> u32 {
        match self {
            Self::VoxSafe => 1200,
            // TXDELAY 600 ms + HDLC flags + 18-byte AX.25 header.
            Self::Afsk1200 => 900,
            _ => 400,
        }
    }

    pub fn is_hf(self) -> bool {
        matches!(
            self,
            Self::HfGood | Self::HfPoor | Self::HfWeak | Self::HfDeep | Self::VoxSafe
        )
    }

    /// modem73 `csma_band`: 0 = HF timings, 1 = VHF/UHF.
    pub fn csma_band(self) -> u8 {
        if self.is_hf() {
            0
        } else {
            1
        }
    }

    pub fn csma_band_name(self) -> &'static str {
        if self.is_hf() {
            "hf"
        } else {
            "vhf"
        }
    }

    /// Strip Ed25519 on RF for the slower HF presets so a chat line fits one frame.
    pub fn unsigned_on_rf(self) -> bool {
        matches!(
            self,
            Self::HfPoor | Self::HfWeak | Self::HfDeep | Self::VoxSafe
        )
    }

    /// Minimum VOX lead before MFSK-32R data. A clipped preamble wastes the whole burst.
    pub const VOX_SAFE_LEAD_MS: u32 = 900;
    /// Hold the radio keyed after the last symbols so VOX does not drop the tail.
    pub const VOX_SAFE_TAIL_MS: u32 = 300;

    pub fn vox_lead_ms(self, configured: u32) -> u32 {
        if matches!(self, Self::VoxSafe) {
            configured.max(Self::VOX_SAFE_LEAD_MS)
        } else {
            configured
        }
    }

    pub fn vox_tail_ms(self, configured: u32) -> u32 {
        if matches!(self, Self::VoxSafe) {
            configured.max(Self::VOX_SAFE_TAIL_MS)
        } else {
            configured
        }
    }

    /// Starting index on the robustness ladder (0 = fastest).
    pub fn ladder_start(self) -> usize {
        match self {
            Self::VhfFm | Self::HfGood | Self::Afsk1200 => 0,
            Self::HfPoor => 2,
            Self::HfWeak => 3,
            Self::HfDeep | Self::VoxSafe => 4,
        }
    }

    /// Slow MFSK/RDM is quieter than the default -30 dB CSMA gate, so the
    /// modem keys over an inbound frame after the lead tone. MFSK sync is
    /// not used as DCD.
    pub fn csma_threshold_db(self) -> Option<i16> {
        match self {
            Self::HfPoor | Self::HfWeak | Self::HfDeep | Self::VoxSafe => Some(-50),
            _ => None,
        }
    }

    pub fn modem73_args(self) -> Vec<String> {
        let band = self.csma_band_name();
        let mut args = match self {
            // modem73 has no AFSK mode; if someone forces this preset onto a
            // sound-card modem, fall back to the plain VHF profile.
            Self::VhfFm | Self::Afsk1200 => vec![
                "-m".into(),
                "QPSK".into(),
                "-r".into(),
                "1/2".into(),
                "--csma-band".into(),
                band.into(),
            ],
            Self::HfGood => vec![
                "-m".into(),
                "8PSK".into(),
                "-r".into(),
                "1/2".into(),
                "--csma-band".into(),
                band.into(),
                "--postamble".into(),
            ],
            Self::HfPoor => vec![
                "--robust-mode".into(),
                "RDM-600S".into(),
                "--csma-band".into(),
                band.into(),
            ],
            Self::HfWeak => vec![
                "--robust-mode".into(),
                "RDM-300S".into(),
                "--csma-band".into(),
                band.into(),
            ],
            Self::HfDeep | Self::VoxSafe => vec![
                "--mfsk-mode".into(),
                "MFSK-32R".into(),
                "--csma-band".into(),
                band.into(),
            ],
        };
        if let Some(db) = self.csma_threshold_db() {
            args.extend(["--csma-threshold".into(), db.to_string()]);
        }
        args
    }

    pub fn control_config(self) -> serde_json::Value {
        let mut v = match self {
            Self::VhfFm | Self::Afsk1200 => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 0,
                "modulation": "QPSK",
                "code_rate": "1/2",
                "csma_band": self.csma_band(),
                "csma_enabled": true
            }),
            Self::HfGood => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 0,
                "modulation": "8PSK",
                "code_rate": "1/2",
                "csma_band": self.csma_band(),
                "csma_enabled": true,
                "postamble": true
            }),
            Self::HfPoor => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 2,
                "robust_mode": 6,
                "csma_band": self.csma_band(),
                "csma_enabled": true
            }),
            Self::HfWeak => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 2,
                "robust_mode": 7,
                "csma_band": self.csma_band(),
                "csma_enabled": true
            }),
            Self::HfDeep | Self::VoxSafe => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 1,
                "mfsk_mode": 3,
                "csma_band": self.csma_band(),
                "csma_enabled": true
            }),
        };
        if let Some(db) = self.csma_threshold_db() {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("carrier_threshold_db".into(), serde_json::json!(db));
            }
        }
        v
    }
}

/// One rung on the TX robustness ladder. Lower index = faster, needs more SNR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    OfdmQpskHalf,
    Rdm1200S,
    Rdm600S,
    Rdm300S,
    Mfsk32R,
}

impl Rung {
    pub const COUNT: usize = 5;

    pub fn from_index(i: usize) -> Self {
        match i.min(Self::COUNT - 1) {
            0 => Self::OfdmQpskHalf,
            1 => Self::Rdm1200S,
            2 => Self::Rdm600S,
            3 => Self::Rdm300S,
            _ => Self::Mfsk32R,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::OfdmQpskHalf => 0,
            Self::Rdm1200S => 1,
            Self::Rdm600S => 2,
            Self::Rdm300S => 3,
            Self::Mfsk32R => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OfdmQpskHalf => "QPSK 1/2",
            Self::Rdm1200S => "RDM-1200S",
            Self::Rdm600S => "RDM-600S",
            Self::Rdm300S => "RDM-300S",
            Self::Mfsk32R => "MFSK-32R",
        }
    }

    pub fn payload_bytes(self) -> u32 {
        match self {
            Self::OfdmQpskHalf => 512,
            Self::Rdm1200S | Self::Rdm600S | Self::Rdm300S => 170,
            Self::Mfsk32R => 55,
        }
    }

    /// Highest (most robust) rung whose PHY MTU can carry `frame_len` bytes.
    pub fn max_for_size(frame_len: usize) -> usize {
        if frame_len <= 55 {
            4
        } else if frame_len <= 170 {
            3
        } else {
            0
        }
    }

    pub fn control_config(self) -> serde_json::Value {
        match self {
            Self::OfdmQpskHalf => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 0,
                "modulation": "QPSK",
                "code_rate": "1/2",
                "csma_enabled": true
            }),
            Self::Rdm1200S => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 2,
                "robust_mode": 5,
                "csma_enabled": true
            }),
            Self::Rdm600S => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 2,
                "robust_mode": 6,
                "csma_enabled": true
            }),
            Self::Rdm300S => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 2,
                "robust_mode": 7,
                "csma_enabled": true
            }),
            Self::Mfsk32R => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 1,
                "mfsk_mode": 3,
                "csma_enabled": true
            }),
        }
    }
}

/// Pick a TX rung for this destination: start from the last ACK, then step down per retry.
pub fn rung_for(preset: Preset, stored: usize, retries: u32, frame_len: usize) -> Rung {
    let start = stored.min(Rung::COUNT - 1).max(preset.ladder_start());
    let idx = (start + retries as usize).min(Rung::max_for_size(frame_len));
    Rung::from_index(idx)
}

/// After an ACK the mode already worked. Step up on a strong report; do not
/// step down — retries already walk the ladder when a frame is missing.
pub fn adjust_rung(current: usize, snr_db: f32) -> usize {
    if snr_db >= 8.0 {
        current.saturating_sub(1)
    } else {
        current
    }
}

/// Estimate airtime for a payload, including overhead.
pub fn airtime_secs(preset: Preset, bytes: usize, extra_overhead_ms: u32) -> f64 {
    let bits = (bytes as f64) * 8.0;
    let tx = bits / preset.bitrate_bps() as f64;
    tx + (preset.overhead_ms() + extra_overhead_ms) as f64 / 1000.0
}

pub fn audio_level_label(level_db: f32) -> &'static str {
    if level_db < -35.0 {
        "low"
    } else if level_db > -3.0 {
        "hot"
    } else {
        "good"
    }
}

/// Silent floor for VU meters (dBFS). Matches modem73's level bar.
pub const AUDIO_FLOOR_DB: f32 = -80.0;
/// Nominal TX playback level when PTT is keyed and the modem has no output meter.
pub const AUDIO_TX_NOMINAL_DB: f32 = -8.0;

pub fn default_audio_db() -> f32 {
    AUDIO_FLOOR_DB
}

/// Map dBFS onto 0..=1 for a horizontal meter. `-80` is empty, `0` is full.
pub fn audio_level_frac(level_db: f32) -> f32 {
    ((level_db - AUDIO_FLOOR_DB) / -AUDIO_FLOOR_DB).clamp(0.0, 1.0)
}

/// Fall toward silence each status tick (~250 ms) so a last-frame peak decays.
pub fn decay_audio_db(current: f32) -> f32 {
    let next = current + (AUDIO_FLOOR_DB - current) * 0.45;
    if next < AUDIO_FLOOR_DB + 0.5 {
        AUDIO_FLOOR_DB
    } else {
        next
    }
}

pub fn audio_meter_live(label: &str) -> bool {
    !matches!(label, "" | "—" | "no modem" | "no audio")
}

pub fn audio_level_idle(level_db: f32) -> bool {
    level_db <= AUDIO_FLOOR_DB + 1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airtime_positive() {
        let t = airtime_secs(Preset::HfPoor, 142, 0);
        assert!(t > 1.0 && t < 20.0);
    }

    #[test]
    fn ladder_steps_down_on_retry() {
        let r = rung_for(Preset::HfGood, 0, 2, 40);
        assert_eq!(r, Rung::Rdm600S);
        let r = rung_for(Preset::HfWeak, 3, 1, 40);
        assert_eq!(r, Rung::Mfsk32R);
    }

    #[test]
    fn mfsk_gated_by_frame_size() {
        assert_eq!(Rung::max_for_size(40), 4);
        assert_eq!(Rung::max_for_size(80), 3);
        assert_eq!(Rung::max_for_size(200), 0);
    }

    #[test]
    fn snr_adjusts_rung() {
        assert_eq!(adjust_rung(2, 12.0), 1);
        assert_eq!(adjust_rung(2, 1.0), 2);
        assert_eq!(adjust_rung(2, 5.0), 2);
    }

    #[test]
    fn audio_meter_maps_db() {
        assert_eq!(audio_level_frac(AUDIO_FLOOR_DB), 0.0);
        assert_eq!(audio_level_frac(0.0), 1.0);
        assert!(audio_level_frac(-40.0) > 0.4 && audio_level_frac(-40.0) < 0.6);
        assert!(decay_audio_db(-8.0) < -8.0);
        assert!(!audio_meter_live("no modem"));
        assert!(audio_meter_live("good"));
        assert!(audio_level_idle(AUDIO_FLOOR_DB));
        assert!(!audio_level_idle(-12.0));
    }

    #[test]
    fn hf_deep_parses() {
        assert_eq!(Preset::parse("hf-deep"), Some(Preset::HfDeep));
        assert!(Preset::HfDeep.unsigned_on_rf());
        assert_eq!(Preset::HfPoor.payload_bytes(), 170);
    }

    #[test]
    fn vox_safe_stays_on_mfsk() {
        assert_eq!(Preset::VoxSafe.ladder_start(), 4);
        assert_eq!(rung_for(Preset::VoxSafe, 0, 0, 40), Rung::Mfsk32R);
        assert_eq!(rung_for(Preset::VoxSafe, 0, 3, 40), Rung::Mfsk32R);
        assert!(Preset::VoxSafe.unsigned_on_rf());
        assert!(Preset::VoxSafe.is_hf());
        assert_eq!(Preset::VoxSafe.payload_bytes(), 55);
        assert_eq!(Preset::VoxSafe.vox_lead_ms(500), Preset::VOX_SAFE_LEAD_MS);
        assert_eq!(Preset::VoxSafe.vox_lead_ms(1200), 1200);
        assert_eq!(Preset::VoxSafe.vox_tail_ms(150), Preset::VOX_SAFE_TAIL_MS);
        let v = Preset::VoxSafe.control_config();
        assert_eq!(v.get("modem_type").and_then(|x| x.as_u64()), Some(1));
        assert_eq!(v.get("mfsk_mode").and_then(|x| x.as_u64()), Some(3));
        assert_eq!(
            v.get("carrier_threshold_db").and_then(|x| x.as_i64()),
            Some(-50)
        );
        let args = Preset::VoxSafe.modem73_args();
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--csma-threshold" && w[1] == "-50"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--mfsk-mode" && w[1] == "MFSK-32R"));
        assert!(
            !args
                .windows(2)
                .any(|w| w[0] == "-m" || w[0] == "--modulation"),
            "MFSK-32R is not an OFDM -m value; modem73 exits on that"
        );
        assert!(Preset::HfDeep
            .modem73_args()
            .windows(2)
            .any(|w| { w[0] == "--mfsk-mode" && w[1] == "MFSK-32R" }));
    }

    #[test]
    fn afsk_preset_is_radio_tnc() {
        assert_eq!(Preset::parse("afsk-1200"), Some(Preset::Afsk1200));
        assert_eq!(Preset::parse("tnc"), Some(Preset::Afsk1200));
        assert!(Preset::Afsk1200.is_radio_tnc());
        assert!(!Preset::Afsk1200.is_hf());
        assert!(!Preset::Afsk1200.unsigned_on_rf());
        assert_eq!(Preset::Afsk1200.bitrate_bps(), 1200);
        // 200 bytes at 1200 bd plus 600 ms TXDELAY: a chat line is a couple of seconds.
        let t = airtime_secs(Preset::Afsk1200, 200, 0);
        assert!(t > 2.0 && t < 3.0, "{t}");
    }

    #[test]
    fn all_presets_enable_csma() {
        for p in Preset::all() {
            let v = p.control_config();
            assert_eq!(
                v.get("csma_enabled").and_then(|x| x.as_bool()),
                Some(true),
                "{}",
                p.as_str()
            );
            assert_eq!(
                v.get("csma_band").and_then(|x| x.as_u64()),
                Some(p.csma_band() as u64),
                "{}",
                p.as_str()
            );
            let args = p.modem73_args();
            assert!(
                args.windows(2)
                    .any(|w| w[0] == "--csma-band" && w[1] == p.csma_band_name()),
                "{}",
                p.as_str()
            );
        }
        for i in 0..Rung::COUNT {
            let v = Rung::from_index(i).control_config();
            assert_eq!(
                v.get("csma_enabled").and_then(|x| x.as_bool()),
                Some(true),
                "{}",
                Rung::from_index(i).as_str()
            );
        }
    }
}
