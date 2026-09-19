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
}
