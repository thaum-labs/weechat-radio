//! SPDX-License-Identifier: Apache-2.0
//! Radios with a built-in KISS TNC (Vero VR-N76, BTECH UV-PRO, Radioddity GA-5WB…).
//!
//! These handhelds expose their 1200 bd AFSK packet TNC over Bluetooth Classic
//! (SPP / RFCOMM channel 1) once *General Settings → KISS TNC* is enabled.
//! No sound card, no modem73: the node speaks KISS straight to the radio.
//!
//! Field notes that shaped this module:
//! - The radio keys PTT once per KISS frame and cannot decode back-to-back
//!   frames, so we pace transmissions and keep frames well under 256 bytes.
//! - TXDELAY below ~600 ms clips the start of a frame.
//! - Only one Bluetooth client at a time: the HT phone app must be closed.
//! - Windows creates two COM ports on pairing; the higher one ("SPP Dev")
//!   is the TNC. We skip COM ports entirely and open the RFCOMM socket by
//!   device address, which is why the GUI can just say "Find radio".

pub mod ax25;
pub mod bluetooth;
pub mod cli;
pub mod link;
pub mod serial;

pub use bluetooth::{BtDevice, FindOutcome};
pub use link::start_link;

/// Bluetooth names used by the Benshi-platform radios with a KISS TNC.
pub const KNOWN_RADIOS: &[&str] = &["VR-N76", "UV-PRO", "GA-5WB", "VR-N7500", "GMRS-PRO"];

/// Normalise a Bluetooth name for matching: uppercase, no spaces/dashes/underscores.
pub fn norm_name(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, ' ' | '-' | '_'))
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Does this Bluetooth name belong to a radio we know how to drive?
pub fn is_known_radio(name: &str) -> bool {
    let n = norm_name(name);
    KNOWN_RADIOS.iter().any(|k| n.contains(&norm_name(k)))
}

/// Does `name` match what the user configured (`tnc.bt_name`)?
pub fn name_matches(name: &str, wanted: &str) -> bool {
    let w = norm_name(wanted);
    if w.is_empty() {
        return is_known_radio(name);
    }
    norm_name(name).contains(&w)
}

/// Short human label for a device: `VR-N76 (38:D2:00:01:03:49)`.
pub fn label(name: &str, addr: &str) -> String {
    if name.is_empty() {
        addr.to_string()
    } else if addr.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({addr})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_radio_names_match_loosely() {
        assert!(is_known_radio("VR-N76"));
        assert!(is_known_radio("UV Pro"));
        assert!(is_known_radio("uv-pro"));
        assert!(is_known_radio("GA-5WB"));
        assert!(is_known_radio("VR-N7500"));
        assert!(!is_known_radio("JBL Flip"));
        assert!(!is_known_radio(""));
    }

    #[test]
    fn configured_name_matches() {
        assert!(name_matches("VR-N76", "vr n76"));
        assert!(name_matches("UV-PRO", ""));
        assert!(!name_matches("VR-N76", "UV-PRO"));
        assert!(!name_matches("AirPods", ""));
    }
}
