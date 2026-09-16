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
}

impl Preset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vhf-fm" | "vhf" | "fm" => Some(Self::VhfFm),
            "hf-good" | "hfgood" => Some(Self::HfGood),
            "hf-poor" | "hfpoor" => Some(Self::HfPoor),
            "hf-weak" | "hfweak" => Some(Self::HfWeak),
            "vox-safe" | "vox" => Some(Self::VoxSafe),
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
        }
    }

    pub fn all() -> [Preset; 5] {
        [
            Self::VhfFm,
            Self::HfGood,
            Self::HfPoor,
            Self::HfWeak,
            Self::VoxSafe,
        ]
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::VhfFm => "VHF/UHF FM, clean local links (OFDM QPSK 1/2)",
            Self::HfGood => "Good HF SSB path (OFDM 8PSK 1/2)",
            Self::HfPoor => "Fading HF, NVIS (ROBUST RDM-600)",
            Self::HfWeak => "Very weak HF (RDM-300 / MFSK-16)",
            Self::VoxSafe => {
                "Any radio with VOX: extra lead/tail so the first symbols are not clipped"
            }
        }
    }

    /// Approximate payload bitrate in bits per second (after FEC).
    pub fn bitrate_bps(self) -> u32 {
        match self {
            Self::VhfFm => 2400,
            Self::HfGood => 2400,
            Self::HfPoor => 585,
            Self::HfWeak => 296,
            Self::VoxSafe => 1200,
        }
    }

    /// Typical PHY payload bytes per frame.
    pub fn payload_bytes(self) -> u32 {
        match self {
            Self::VhfFm => 512,
            Self::HfGood => 512,
            Self::HfPoor => 510,
            Self::HfWeak => 510,
            Self::VoxSafe => 256,
        }
    }

    /// Preamble + lead-tone overhead in milliseconds.
    pub fn overhead_ms(self) -> u32 {
        match self {
            Self::VoxSafe => 900,
            _ => 400,
        }
    }

    pub fn modem73_args(self) -> Vec<String> {
        match self {
            Self::VhfFm => vec![
                "-m".into(),
                "QPSK".into(),
                "-r".into(),
                "1/2".into(),
                "--csma-band".into(),
                "vhf".into(),
            ],
            Self::HfGood => vec![
                "-m".into(),
                "8PSK".into(),
                "-r".into(),
                "1/2".into(),
                "--csma-band".into(),
                "hf".into(),
            ],
            Self::HfPoor => vec!["--robust-mode".into(), "RDM-600".into()],
            Self::HfWeak => vec!["--robust-mode".into(), "RDM-300".into()],
            Self::VoxSafe => vec![
                "-m".into(),
                "QPSK".into(),
                "-r".into(),
                "1/2".into(),
                "--short".into(),
            ],
        }
    }

    pub fn control_config(self) -> serde_json::Value {
        match self {
            Self::VhfFm => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 0,
                "modulation": "QPSK",
                "code_rate": "1/2",
                "csma_band": 1,
                "csma_enabled": true
            }),
            Self::HfGood => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 0,
                "modulation": "8PSK",
                "code_rate": "1/2",
                "csma_band": 0,
                "csma_enabled": true
            }),
            Self::HfPoor => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 2,
                "robust_mode": 1
            }),
            Self::HfWeak => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 2,
                "robust_mode": 2
            }),
            Self::VoxSafe => serde_json::json!({
                "cmd": "set_config",
                "modem_type": 0,
                "modulation": "QPSK",
                "code_rate": "1/2",
                "short_frame": true
            }),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airtime_positive() {
        let t = airtime_secs(Preset::HfPoor, 142, 0);
        assert!(t > 1.0 && t < 20.0);
    }
}
