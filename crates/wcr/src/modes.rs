//! SPDX-License-Identifier: Apache-2.0
//! Operating modes for a WeeChat Radio node.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Four operating modes. Colour codes match the TUI, WeeChat bar, and map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Hub/peers only. Radio idle.
    Internet,
    /// Radio primary, internet fills gaps. This node is a gateway.
    #[default]
    InternetRadio,
    /// Pure RF. Frames carry NO_INET so gateways never forward them.
    Radio,
    /// RF locally; frames flagged INET_OK so a gateway may carry them onward.
    RadioPlus,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Internet => "internet",
            Self::InternetRadio => "internet-radio",
            Self::Radio => "radio",
            Self::RadioPlus => "radio-plus",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Internet => "Internet",
            Self::InternetRadio => "Internet+Radio",
            Self::Radio => "Radio",
            Self::RadioPlus => "Radio+",
        }
    }

    /// Hex colour used by TUI, WeeChat, and the public map.
    pub fn color_hex(self) -> &'static str {
        match self {
            Self::Internet => "#00e5ff",
            Self::InternetRadio => "#39ff14",
            Self::Radio => "#ffbf00",
            Self::RadioPlus => "#ff4dff",
        }
    }

    pub fn uses_radio(self) -> bool {
        !matches!(self, Self::Internet)
    }

    pub fn uses_internet(self) -> bool {
        matches!(self, Self::Internet | Self::InternetRadio)
    }

    /// Outgoing RF frames from this mode should allow internet forwarding.
    pub fn inet_ok_on_tx(self) -> bool {
        matches!(self, Self::InternetRadio | Self::RadioPlus | Self::Internet)
    }

    /// Outgoing RF frames from this mode must never be forwarded to the internet.
    pub fn no_inet_on_tx(self) -> bool {
        matches!(self, Self::Radio)
    }

    /// Gateway: may put internet-originated traffic on the air.
    pub fn is_gateway(self) -> bool {
        matches!(self, Self::InternetRadio)
    }

    /// Start the radio path when switching `from` → `to`.
    pub fn start_radio(from: Self, to: Self) -> bool {
        !from.uses_radio() && to.uses_radio()
    }

    /// Stop the radio path when switching `from` → `to`.
    pub fn stop_radio(from: Self, to: Self) -> bool {
        from.uses_radio() && !to.uses_radio()
    }

    /// Connect hub/peers when switching `from` → `to`.
    pub fn start_hub(from: Self, to: Self) -> bool {
        !from.uses_internet() && to.uses_internet()
    }

    /// Drop hub/peers when switching `from` → `to`.
    pub fn stop_hub(from: Self, to: Self) -> bool {
        from.uses_internet() && !to.uses_internet()
    }

    pub fn all() -> [Mode; 4] {
        [
            Self::Internet,
            Self::InternetRadio,
            Self::Radio,
            Self::RadioPlus,
        ]
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "internet" | "inet" => Ok(Self::Internet),
            "internet-radio" | "internetradio" | "inet-radio" | "gateway" => {
                Ok(Self::InternetRadio)
            }
            "radio" | "rf" => Ok(Self::Radio),
            "radio-plus" | "radio+" | "radioplus" => Ok(Self::RadioPlus),
            other => Err(format!(
                "unknown mode '{other}'. Use internet, internet-radio, radio, or radio-plus."
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_aliases() {
        assert_eq!("radio+".parse::<Mode>().unwrap(), Mode::RadioPlus);
        assert_eq!("gateway".parse::<Mode>().unwrap(), Mode::InternetRadio);
        assert!(Mode::Radio.no_inet_on_tx());
        assert!(Mode::InternetRadio.is_gateway());
        assert!(!Mode::Radio.uses_internet());
        assert!(!Mode::RadioPlus.uses_internet());
        assert!(Mode::Internet.uses_internet());
        assert!(Mode::InternetRadio.uses_internet());
    }

    #[test]
    fn mode_switch_starts_and_stops_the_right_paths() {
        use Mode::*;
        let cases = [
            (Internet, InternetRadio, true, false, false, false),
            (Internet, Radio, true, false, false, true),
            (Internet, RadioPlus, true, false, false, true),
            (InternetRadio, Internet, false, true, false, false),
            (InternetRadio, Radio, false, false, false, true),
            (InternetRadio, RadioPlus, false, false, false, true),
            (Radio, Internet, false, true, true, false),
            (Radio, InternetRadio, false, false, true, false),
            (Radio, RadioPlus, false, false, false, false),
            (RadioPlus, Internet, false, true, true, false),
            (RadioPlus, InternetRadio, false, false, true, false),
            (RadioPlus, Radio, false, false, false, false),
        ];
        for (from, to, start_rf, stop_rf, start_net, stop_net) in cases {
            assert_eq!(
                Mode::start_radio(from, to),
                start_rf,
                "{from} → {to} start radio"
            );
            assert_eq!(
                Mode::stop_radio(from, to),
                stop_rf,
                "{from} → {to} stop radio"
            );
            assert_eq!(
                Mode::start_hub(from, to),
                start_net,
                "{from} → {to} start hub"
            );
            assert_eq!(Mode::stop_hub(from, to), stop_net, "{from} → {to} stop hub");
        }
        for m in Mode::all() {
            assert!(!Mode::start_radio(m, m));
            assert!(!Mode::stop_radio(m, m));
            assert!(!Mode::start_hub(m, m));
            assert!(!Mode::stop_hub(m, m));
        }
    }
}
