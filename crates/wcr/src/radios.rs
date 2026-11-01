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
    /// modem73 hears and keys the radio over its Bluetooth audio channel
    /// (`crate::btaudio`): modem73's modes, still no cable.
    BluetoothAudio { bt_name: &'static str },
    /// modem73 plays the tones through the radio's sound card and keys the
    /// radio over CAT with its built-in Hamlib. No rigctld to install or start.
    Cat { hamlib_model: u32, baud: u32 },
    /// No CAT: a Digirig plays the tones and keys the radio on RTS.
    Digirig,
    /// No CAT and no data port: an audio cable, and the radio's VOX keys it.
    Vox,
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
    /// The interface cable to buy, with its link; empty when none is needed.
    pub cable: &'static str,
    pub preset: Preset,
    /// First dial frequency for setup to suggest (CAT radios report their own).
    pub dial_khz: u32,
    /// Bands the radio transmits on (`band::band_for_khz` names). Setup warns
    /// when the saved frequency is outside them.
    pub bands: &'static [&'static str],
    /// One line under the name in setup, for radios with more than one way in.
    pub note: &'static str,
}

/// HF radios (all also cover 6 m or close to it; this only catches a
/// VHF/UHF or licence-free frequency left over from another radio).
const HF: &[&str] = &[
    "160m", "80m", "60m", "40m", "30m", "20m", "17m", "15m", "12m", "10m", "6m",
];
/// Dual-band FM handhelds and mobiles.
const VU: &[&str] = &["2m", "70cm"];
const TEN_M: &[&str] = &["10m"];

const BT_TNC_NOTE: &str = "Built-in 1200 bd packet TNC (KISS TNC on, no modem73). Slow; talks to other 1200 bd packet stations.";
const BT_AUDIO_NOTE: &str = "modem73 over the radio's Bluetooth audio (KISS TNC off). Faster, steps down to sturdier modes on a poor path. Pick this to reach a modem73 gateway, such as a Pi + Digirig.";

/// Hamlib model numbers (`rigctl -l`).
const HAMLIB_X6100: u32 = 3087;
const HAMLIB_G90: u32 = 3088;
const HAMLIB_FT710: u32 = 1049;
const HAMLIB_X6200: u32 = 3091;
const HAMLIB_X5105: u32 = 3089;
const HAMLIB_QDX: u32 = 2052;
const HAMLIB_TRUSDX: u32 = 2055;

const BT_AUDIO_RADIO_SIDE: &str = "Squelch low (1 or 2), KISS TNC and Digital Mode off, then Menu → Pairing. Close the HT phone app: the radio takes one Bluetooth client at a time.";

const G90_CABLE: &str =
    "Xiegu DE-19, or the Digirig Xiegu G90 Cables Set: https://digirig.net/product/xiegu-g90-cables/";

pub const RADIOS: &[Radio] = &[
    Radio {
        name: "VR-N76 / UV-PRO / GA-5WB (Bluetooth)",
        link: Link::BluetoothTnc { bt_name: "VR-N76" },
        port_hints: &[],
        audio_hints: &[],
        radio_side: "General Settings → KISS TNC → Enable, Digital Mode off, then Menu → Pairing.",
        cable: "",
        preset: Preset::Afsk1200,
        dial_khz: 144950,
        bands: VU,
        note: BT_TNC_NOTE,
    },
    Radio {
        name: "VR-N7600 (Bluetooth)",
        link: Link::BluetoothTnc {
            bt_name: "VR-N7600",
        },
        port_hints: &[],
        audio_hints: &[],
        radio_side: "Turn KISS TNC on in the radio menu, then put it in Bluetooth pairing mode.",
        cable: "",
        preset: Preset::Afsk1200,
        dial_khz: 144950,
        bands: VU,
        note: BT_TNC_NOTE,
    },
    Radio {
        name: "VR-N76 / UV-PRO / GA-5WB (Bluetooth audio, modem73)",
        link: Link::BluetoothAudio { bt_name: "VR-N76" },
        port_hints: &[],
        audio_hints: &[],
        radio_side: BT_AUDIO_RADIO_SIDE,
        cable: "",
        preset: Preset::VhfFm,
        dial_khz: 144950,
        bands: VU,
        note: BT_AUDIO_NOTE,
    },
    Radio {
        name: "VR-N7600 (Bluetooth audio, modem73)",
        link: Link::BluetoothAudio {
            bt_name: "VR-N7600",
        },
        port_hints: &[],
        audio_hints: &[],
        radio_side: BT_AUDIO_RADIO_SIDE,
        cable: "",
        preset: Preset::VhfFm,
        dial_khz: 144950,
        bands: VU,
        note: BT_AUDIO_NOTE,
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
        cable: "",
        // ROBUST RDM-600S: the most forgiving on a fading HF path.
        preset: Preset::HfPoor,
        dial_khz: 7045,
        bands: HF,
        note: "",
    },
    Radio {
        name: "Xiegu G90 (DE-19 or Digirig)",
        link: Link::Cat {
            hamlib_model: HAMLIB_G90,
            baud: 19200,
        },
        // DE-19: CH340 serial; Digirig: CP210x.
        port_hints: &["ch340", "ch341", "digirig", "cp210"],
        audio_hints: &["usb pnp sound", "usb audio device"],
        radio_side: "DE-19 (one USB cable) or both Digirig cables. Radio in USB, CI-V 19200 baud.",
        cable: G90_CABLE,
        // ROBUST RDM-600S: the most forgiving on a fading HF path.
        preset: Preset::HfPoor,
        dial_khz: 7045,
        bands: HF,
        note: "",
    },
    Radio {
        name: "Xiegu X6200 (USB cable)",
        link: Link::Cat {
            hamlib_model: HAMLIB_X6200,
            baud: 19200,
        },
        port_hints: &[],
        audio_hints: &["usb audio codec"],
        radio_side: "USB-C to the computer. Radio in USB (upper sideband), CI-V 19200 baud.",
        cable: "",
        preset: Preset::HfPoor,
        dial_khz: 7045,
        bands: HF,
        note: "",
    },
    Radio {
        name: "Xiegu G106 (DE-19 or Digirig)",
        // The G106 answers the G90's CI-V commands.
        link: Link::Cat {
            hamlib_model: HAMLIB_G90,
            baud: 19200,
        },
        port_hints: &["ch340", "ch341", "digirig", "cp210"],
        audio_hints: &["usb pnp sound", "usb audio device"],
        radio_side: "DE-19 (one USB cable) or both Digirig cables. Radio in USB, CI-V 19200 baud.",
        cable: G90_CABLE,
        preset: Preset::HfPoor,
        dial_khz: 7045,
        bands: HF,
        note: "",
    },
    Radio {
        name: "Xiegu X5105 (DE-19 or Digirig)",
        link: Link::Cat {
            hamlib_model: HAMLIB_X5105,
            baud: 19200,
        },
        port_hints: &["ch340", "ch341", "digirig", "cp210"],
        audio_hints: &["usb pnp sound", "usb audio device"],
        radio_side: "DE-19 (one USB cable) or both Digirig cables. Radio in USB, CI-V 19200 baud.",
        cable: G90_CABLE,
        preset: Preset::HfPoor,
        dial_khz: 7045,
        bands: HF,
        note: "",
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
        cable: "",
        // ROBUST RDM-600S: the most forgiving on a fading HF path.
        preset: Preset::HfPoor,
        dial_khz: 7045,
        bands: HF,
        note: "",
    },
    Radio {
        name: "QRP Labs QDX (USB cable)",
        link: Link::Cat {
            hamlib_model: HAMLIB_QDX,
            baud: 9600,
        },
        port_hints: &["qdx"],
        audio_hints: &["qdx"],
        radio_side: "USB to the computer: the QDX has its own sound card and CAT port.",
        cable: "",
        // The QDX sends one tone at a time: only MFSK gets through it.
        preset: Preset::HfDeep,
        dial_khz: 7045,
        bands: HF,
        note: "",
    },
    Radio {
        name: "uSDX / (tr)uSDX (CAT + sound card)",
        link: Link::Cat {
            hamlib_model: HAMLIB_TRUSDX,
            baud: 38400,
        },
        port_hints: &["ch340", "ch341"],
        audio_hints: &["usb pnp sound", "usb audio device"],
        radio_side: "USB for CAT (38400 baud), a USB sound card or Digirig on the audio jacks.",
        cable: "",
        // The uSDX sends one tone at a time well; MFSK is the safe choice.
        preset: Preset::HfDeep,
        dial_khz: 7045,
        bands: HF,
        note: "",
    },
    Radio {
        name: "AnyTone AT-5555N (Digirig, 10 m)",
        link: Link::Digirig,
        port_hints: &["digirig", "cp210"],
        audio_hints: &["usb pnp sound", "usb audio device"],
        radio_side: "Digirig cable in the mic socket. Radio in USB on 10 m.",
        cable:
            "Digirig AnyTone AT-778UV cable: https://digirig.net/product/anytone-at-778uv-cable/",
        preset: Preset::HfPoor,
        dial_khz: 28120,
        bands: TEN_M,
        note: "",
    },
    Radio {
        name: "Retevis RT95 (Digirig)",
        link: Link::Digirig,
        port_hints: &["digirig", "cp210"],
        audio_hints: &["usb pnp sound", "usb audio device"],
        radio_side: "Digirig cable in the mic socket. Radio on FM.",
        cable:
            "Digirig AnyTone AT-778UV cable: https://digirig.net/product/anytone-at-778uv-cable/",
        preset: Preset::VhfFm,
        dial_khz: 144950,
        bands: VU,
        note: "",
    },
    Radio {
        name: "IC-980 Pro (audio cable, VOX)",
        link: Link::Vox,
        port_hints: &[],
        audio_hints: &[],
        radio_side: "Turn VOX on and set its delay long. Audio cable to the mic and speaker jacks.",
        cable: "",
        preset: Preset::VoxSafe,
        dial_khz: 144950,
        bands: VU,
        note: "",
    },
];

impl Radio {
    /// The setup path this radio uses (see `setup.rs`): 1 Digirig, 2 VOX,
    /// 4 Bluetooth TNC, 6 CAT, 7 Bluetooth audio.
    pub fn setup_path(&self) -> usize {
        match self.link {
            Link::Digirig => 1,
            Link::Vox => 2,
            Link::BluetoothTnc { .. } => 4,
            Link::Cat { .. } => 6,
            Link::BluetoothAudio { .. } => 7,
        }
    }

    /// True when `khz` is on a band this radio transmits on.
    pub fn covers(&self, khz: u32) -> bool {
        crate::band::band_for_khz(khz).is_some_and(|b| self.bands.contains(&b))
    }

    /// The dial setup should store: the saved one when this radio can use it,
    /// else the radio's default. The note says why it changed (#232, #233).
    pub fn dial_for(&self, saved_khz: u32) -> (u32, Option<String>) {
        if saved_khz != 0 && self.covers(saved_khz) {
            return (saved_khz, None);
        }
        let note = (saved_khz != 0).then(|| {
            format!(
                "{} is not a band the {} transmits on, so the dial is now {}. Set the radio's real frequency with FREQ (or /radio freq) so the map and status match.",
                crate::band::describe(saved_khz),
                self.short_name(),
                crate::band::describe(self.dial_khz),
            )
        });
        (self.dial_khz, note)
    }

    /// The name without the "(…)" link part: "VR-N76 / UV-PRO / GA-5WB".
    pub fn short_name(&self) -> &'static str {
        self.name.split(" (").next().unwrap_or(self.name)
    }

    /// The Bluetooth name to look for, for radios reached over Bluetooth.
    pub fn bt_name(&self) -> Option<&'static str> {
        match self.link {
            Link::BluetoothTnc { bt_name } | Link::BluetoothAudio { bt_name } => Some(bt_name),
            _ => None,
        }
    }

    /// Fills in everything this radio needs. The port and sound cards come
    /// from the operator (or [`Radio::pick_port`] / [`Radio::pick_audio`]).
    pub fn apply(&self, cfg: &mut Config) {
        cfg.mode = Mode::InternetRadio;
        cfg.modem.radio = self.name.into();
        cfg.modem.preset = self.preset.as_str().into();
        match self.link {
            Link::BluetoothTnc { bt_name } => {
                cfg.modem.backend = "bluetooth".into();
                cfg.modem.manage = false;
                cfg.modem.ptt = "tnc".into();
                cfg.tnc.bt_name = bt_name.into();
            }
            Link::BluetoothAudio { bt_name } => {
                cfg.modem.backend = "modem73".into();
                cfg.modem.manage = true;
                cfg.modem.ptt = "bluetooth".into();
                cfg.tnc.bt_name = bt_name.into();
            }
            Link::Cat { hamlib_model, baud } => {
                cfg.modem.backend = "modem73".into();
                cfg.modem.manage = true;
                cfg.modem.ptt = "hamlib".into();
                cfg.modem.hamlib_model = hamlib_model;
                cfg.modem.hamlib_baud = baud;
                cfg.rig.enabled = true;
            }
            Link::Digirig => {
                cfg.modem.backend = "modem73".into();
                cfg.modem.manage = true;
                cfg.modem.ptt = "digirig".into();
                cfg.modem.com_line = "rts".into();
            }
            Link::Vox => {
                cfg.modem.backend = "modem73".into();
                cfg.modem.manage = true;
                cfg.modem.ptt = "vox".into();
                cfg.modem.vox_lead_ms = Preset::VOX_SAFE_LEAD_MS;
                cfg.modem.vox_tail_ms = Preset::VOX_SAFE_TAIL_MS;
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
    RADIOS.iter().find(|r| r.name == cfg.modem.radio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::LabeledPort;

    fn radio(name: &str) -> &'static Radio {
        RADIOS.iter().find(|r| r.name.starts_with(name)).unwrap()
    }

    #[test]
    fn dial_keeps_a_frequency_the_radio_can_use() {
        let r = radio("VR-N76 / UV-PRO / GA-5WB (Bluetooth audio, modem73)");
        assert_eq!(r.dial_for(433625), (433625, None));
        assert_eq!(r.dial_for(0), (144950, None));
        // A PMR446 dial left over from another radio is replaced, with a note.
        let (khz, note) = r.dial_for(446006);
        assert_eq!(khz, 144950);
        assert!(note.unwrap().contains("PMR446"));
        let g90 = radio("Xiegu G90 (DE-19 or Digirig)");
        assert_eq!(g90.dial_for(14105).0, 14105);
        assert_eq!(g90.dial_for(144950).0, 7045);
    }

    #[test]
    fn only_the_two_way_radios_carry_a_note() {
        for r in RADIOS {
            assert_eq!(!r.note.is_empty(), r.bt_name().is_some(), "{}", r.name);
            assert!(!r.bands.is_empty(), "{}", r.name);
            assert!(r.covers(r.dial_khz), "{}", r.name);
        }
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
    fn bluetooth_audio_radio_runs_modem73_over_bluetooth() {
        let mut cfg = Config::default();
        radio("VR-N7600 (Bluetooth audio").apply(&mut cfg);
        assert_eq!(cfg.modem.backend, "modem73");
        assert_eq!(cfg.modem.ptt, "bluetooth");
        assert!(cfg.modem.manage);
        assert!(cfg.modem.is_bt_audio());
        assert_eq!(cfg.modem.preset, "vhf-fm");
        assert_eq!(cfg.tnc.bt_name, "VR-N7600");
        assert_eq!(radio("VR-N7600 (Bluetooth audio").setup_path(), 7);
        assert_eq!(from_config(&cfg), Some(radio("VR-N7600 (Bluetooth audio")));
    }

    #[test]
    fn bluetooth_audio_keeps_to_presets_that_survive_the_link() {
        let mut cfg = Config::default();
        radio("VR-N76 / UV-PRO / GA-5WB (Bluetooth audio").apply(&mut cfg);
        for (asked, kept) in [
            ("hf-poor", "hf-poor"),
            ("hf-weak", "hf-weak"),
            ("hf-deep", "hf-deep"),
            ("hf-good", "vhf-fm"),
            ("vox-safe", "vhf-fm"),
            ("afsk-1200", "vhf-fm"),
        ] {
            cfg.modem.preset = asked.into();
            cfg.normalize();
            assert_eq!(cfg.modem.preset, kept, "{asked}");
        }
    }

    #[test]
    fn bluetooth_radio_uses_the_radio_tnc() {
        let mut cfg = Config::default();
        radio("VR-N7600").apply(&mut cfg);
        assert_eq!(cfg.modem.backend, "bluetooth");
        assert_eq!(cfg.modem.ptt, "tnc");
        assert_eq!(cfg.tnc.bt_name, "VR-N7600");
        assert_eq!(from_config(&cfg), Some(radio("VR-N7600")));
    }

    #[test]
    fn radios_without_cat_use_digirig_or_vox() {
        let mut cfg = Config::default();
        radio("Retevis RT95").apply(&mut cfg);
        assert_eq!(cfg.modem.ptt, "digirig");
        assert_eq!(cfg.modem.com_line, "rts");
        assert_eq!(cfg.modem.preset, "vhf-fm");
        radio("AnyTone AT-5555N").apply(&mut cfg);
        assert_eq!(cfg.modem.preset, "hf-poor");
        assert_eq!(from_config(&cfg), Some(radio("AnyTone AT-5555N")));
        let mut cfg = Config::default();
        radio("IC-980 Pro").apply(&mut cfg);
        assert_eq!(cfg.modem.ptt, "vox");
        assert_eq!(cfg.modem.preset, "vox-safe");
        assert_eq!(radio("IC-980 Pro").setup_path(), 2);
    }

    #[test]
    fn digirig_radios_name_their_cable() {
        for r in RADIOS.iter().filter(|r| r.port_hints.contains(&"digirig")) {
            assert!(
                r.cable.contains("https://digirig.net/product/"),
                "{}",
                r.name
            );
        }
        assert!(radio("Retevis RT95")
            .cable
            .contains("anytone-at-778uv-cable"));
    }

    #[test]
    fn single_tone_radios_use_mfsk() {
        for name in ["QRP Labs QDX", "uSDX"] {
            let mut cfg = Config::default();
            radio(name).apply(&mut cfg);
            assert_eq!(cfg.modem.preset, "hf-deep", "{name}");
            assert_eq!(cfg.modem.ptt, "hamlib", "{name}");
        }
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
                usb_id: None,
            },
            LabeledPort {
                name: "COM3".into(),
                label: "COM3 — Silicon Labs Dual CP2105 Enhanced COM Port".into(),
                usb_id: None,
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
