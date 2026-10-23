//! SPDX-License-Identifier: Apache-2.0
//! Radios setup knows by name. Pick one and setup fills in the link, PTT,
//! preset and CAT settings; the operator only confirms the port and sound card.

use crate::config::Config;
use crate::modes::Mode;
use crate::presets::Preset;

/// How wcr reaches the radio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// The radio has its own KISS TNC over Bluetooth (no modem73, no cable).
    BluetoothTnc { bt_name: &'static str },
    /// modem73 plays the tones through the radio's sound card and keys the
    /// radio over CAT with its built-in Hamlib. No rigctld to install or start.
    Cat { hamlib_model: u32, baud: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Radio {
    /// Shown in setup.
    pub name: &'static str,
    pub link: Link,
    /// Words in a serial port label that mark the CAT port (lowercase).
    pub port_hints: &'static [&'static str],
    /// Words in a sound card name that mark the radio's audio (lowercase).
    pub audio_hints: &'static [&'static str],
    /// What to set on the radio itself, one line.
    pub radio_side: &'static str,
}

/// Hamlib model numbers (`rigctl -l`).
const HAMLIB_X6100: u32 = 3087;
const HAMLIB_G90: u32 = 3088;
const HAMLIB_FT710: u32 = 1049;

pub const RADIOS: &[Radio] = &[
    Radio {
        name: "VR-N76 / UV-PRO / GA-5WB (Bluetooth)",
        link: Link::BluetoothTnc { bt_name: "VR-N76" },
        port_hints: &[],
        audio_hints: &[],
        radio_side: "General Settings → KISS TNC → Enable, Digital Mode off, then Menu → Pairing.",
    },
    Radio {
        name: "VR-N7600 (Bluetooth)",
        link: Link::BluetoothTnc {
            bt_name: "VR-N7600",
        },
        port_hints: &[],
        audio_hints: &[],
        radio_side: "Turn KISS TNC on in the radio menu, then put it in Bluetooth pairing mode.",
    },
    Radio {
        name: "Xiegu X6100 (USB cable)",
        link: Link::Cat {
            hamlib_model: HAMLIB_X6100,
            baud: 19200,
        },
        port_hints: &[],
        audio_hints: &["usb audio codec"],
        radio_side: "USB-C to the computer. Radio in USB (upper sideband), CI-V 19200 baud.",
    },
    Radio {
        name: "Xiegu G90 (Digirig or DE-19 + sound card)",
        link: Link::Cat {
            hamlib_model: HAMLIB_G90,
            baud: 19200,
        },
        port_hints: &["digirig", "cp210"],
        audio_hints: &["usb pnp sound", "usb audio device"],
        radio_side:
            "CAT cable to the interface, audio to the sound card. Radio in USB, CI-V 19200 baud.",
    },
    Radio {
        name: "Yaesu FT-710 (USB cable)",
        link: Link::Cat {
            hamlib_model: HAMLIB_FT710,
            baud: 38400,
        },
        port_hints: &["enhanced"],
        audio_hints: &["usb audio codec"],
        radio_side: "USB to the computer. Radio in USB, CAT RATE 38400 (the factory default).",
    },
];

impl Radio {
    pub fn is_cat(&self) -> bool {
        matches!(self.link, Link::Cat { .. })
    }

    /// Fills in everything this radio needs. The port and sound cards come
    /// from the operator (or [`Radio::pick_port`] / [`Radio::pick_audio`]).
    pub fn apply(&self, cfg: &mut Config) {
        cfg.mode = Mode::InternetRadio;
        match self.link {
            Link::BluetoothTnc { bt_name } => {
                cfg.modem.backend = "bluetooth".into();
                cfg.modem.manage = false;
                cfg.modem.ptt = "tnc".into();
                cfg.modem.preset = Preset::Afsk1200.as_str().into();
                cfg.tnc.bt_name = bt_name.into();
            }
            Link::Cat { hamlib_model, baud } => {
                cfg.modem.backend = "modem73".into();
                cfg.modem.manage = true;
                cfg.modem.ptt = "hamlib".into();
                // ROBUST RDM-600S: the most forgiving on a fading HF path.
                cfg.modem.preset = Preset::HfPoor.as_str().into();
                cfg.modem.hamlib_model = hamlib_model;
                cfg.modem.hamlib_baud = baud;
                cfg.rig.enabled = true;
            }
        }
    }

    /// The serial port that looks like this radio's CAT port.
    pub fn pick_port(&self, ports: &[crate::discover::LabeledPort]) -> Option<String> {
        ports
            .iter()
            .find(|p| matches_any(&p.label, self.port_hints))
            .map(|p| p.name.clone())
    }

    /// The sound card that looks like this radio's.
    pub fn pick_audio(&self, devices: &[String]) -> Option<String> {
        devices
            .iter()
            .find(|d| matches_any(d, self.audio_hints))
            .cloned()
    }
}

fn matches_any(text: &str, hints: &[&str]) -> bool {
    let text = text.to_ascii_lowercase();
    hints.iter().any(|h| text.contains(h))
}

/// The supported radio a saved config was set up for, if any.
pub fn from_config(cfg: &Config) -> Option<&'static Radio> {
    if cfg.mode == Mode::Internet {
        return None;
    }
    if cfg.modem.ptt == "hamlib" {
        return RADIOS.iter().find(|r| {
            matches!(r.link, Link::Cat { hamlib_model, .. } if hamlib_model == cfg.modem.hamlib_model)
        });
    }
    if cfg.modem.is_bluetooth() {
        let name = crate::tnc::norm_name(&cfg.tnc.bt_name);
        // Longest name first, so VR-N7600 is not taken for VR-N76.
        let mut bt: Vec<_> = RADIOS
            .iter()
            .filter_map(|r| match r.link {
                Link::BluetoothTnc { bt_name } => Some((r, crate::tnc::norm_name(bt_name))),
                _ => None,
            })
            .collect();
        bt.sort_by_key(|(_, n)| std::cmp::Reverse(n.len()));
        return bt
            .into_iter()
            .find(|(_, n)| name.contains(n.as_str()))
            .map(|(r, _)| r)
            .or_else(|| RADIOS.first());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::LabeledPort;

    fn radio(name: &str) -> &'static Radio {
        RADIOS.iter().find(|r| r.name.starts_with(name)).unwrap()
    }

    #[test]
    fn cat_radio_writes_hamlib_settings() {
        let mut cfg = Config::default();
        radio("Xiegu X6100").apply(&mut cfg);
        assert_eq!(cfg.mode, Mode::InternetRadio);
        assert_eq!(cfg.modem.backend, "modem73");
        assert_eq!(cfg.modem.ptt, "hamlib");
        assert_eq!(cfg.modem.hamlib_model, 3087);
        assert_eq!(cfg.modem.hamlib_baud, 19200);
        assert_eq!(cfg.modem.preset, "hf-poor");
        assert!(cfg.rig.enabled);
        assert_eq!(from_config(&cfg), Some(radio("Xiegu X6100")));
    }

    #[test]
    fn bluetooth_radio_uses_the_radio_tnc() {
        let mut cfg = Config::default();
        radio("VR-N7600").apply(&mut cfg);
        assert_eq!(cfg.modem.backend, "bluetooth");
        assert_eq!(cfg.modem.ptt, "tnc");
        assert_eq!(cfg.tnc.bt_name, "VR-N7600");
        assert_eq!(from_config(&cfg), Some(radio("VR-N7600")));
        cfg.tnc.bt_name = "UV-PRO".into();
        assert_eq!(from_config(&cfg), Some(radio("VR-N76 ")));
    }

    #[test]
    fn other_setups_are_not_a_supported_radio() {
        let mut cfg = Config::default();
        cfg.mode = Mode::InternetRadio;
        cfg.modem.ptt = "vox".into();
        assert_eq!(from_config(&cfg), None);
    }

    #[test]
    fn ft710_picks_the_enhanced_port_and_codec() {
        let ports = vec![
            LabeledPort {
                name: "COM4".into(),
                label: "COM4 — Silicon Labs Dual CP2105 Standard COM Port".into(),
            },
            LabeledPort {
                name: "COM3".into(),
                label: "COM3 — Silicon Labs Dual CP2105 Enhanced COM Port".into(),
            },
        ];
        let ft710 = radio("Yaesu FT-710");
        assert_eq!(ft710.pick_port(&ports).as_deref(), Some("COM3"));
        let audio = vec![
            "Speakers (Realtek)".to_string(),
            "Speakers (USB Audio CODEC )".into(),
        ];
        assert_eq!(
            ft710.pick_audio(&audio).as_deref(),
            Some("Speakers (USB Audio CODEC )")
        );
        assert_eq!(radio("Xiegu X6100").pick_port(&ports), None);
    }
}
