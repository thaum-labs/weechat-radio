//! SPDX-License-Identifier: Apache-2.0
//! The calls the Android app makes into the wcr core. Kotlin sees these as
//! plain functions in the `uniffi.wcr_mobile` package.
//!
//! Kept small on purpose: each call just wraps one existing wcr function, so
//! the phone and the desktop app agree on names, bands and grids.

use wcr::{band, grid, presets, presets::Preset};

uniffi::setup_scaffolding!();

/// The wcr core version built into the app, e.g. "1.0.21".
#[uniffi::export]
pub fn core_version() -> String {
    wcr::update::current_version().to_string()
}

/// One modem preset, as the setup screen lists it.
#[derive(uniffi::Record)]
pub struct PresetInfo {
    /// The name stored in wcr.toml, e.g. "vhf-fm".
    pub name: String,
    pub description: String,
    /// The radio does the modulation itself (afsk-1200); modem73 is not used.
    pub uses_radio_tnc: bool,
}

#[uniffi::export]
pub fn presets() -> Vec<PresetInfo> {
    Preset::all()
        .into_iter()
        .map(|p| PresetInfo {
            name: p.as_str().to_string(),
            description: p.description().to_string(),
            uses_radio_tnc: p.is_radio_tnc(),
        })
        .collect()
}

/// "145.500" becomes "145.500 MHz (2m)". None when it is not a frequency.
#[uniffi::export]
pub fn describe_frequency(mhz: String) -> Option<String> {
    band::parse_mhz(&mhz).map(band::describe)
}

/// A warning for a poor chat frequency (APRS, PMR446, out of band), if any.
#[uniffi::export]
pub fn frequency_warning(mhz: String) -> Option<String> {
    band::parse_mhz(&mhz).and_then(band::dial_warning)
}

/// Seconds on air for a message of `bytes` with this preset, e.g. "vhf-fm".
/// None for an unknown preset name.
#[uniffi::export]
pub fn airtime_seconds(preset: String, bytes: u32) -> Option<f64> {
    Preset::parse(&preset).map(|p| presets::airtime_secs(p, bytes as usize, 0))
}

/// The 6-character Maidenhead locator for a GPS fix, e.g. "IO91wm".
#[uniffi::export]
pub fn grid_from_location(lat: f64, lon: f64) -> Option<String> {
    grid::from_lat_lon(lat, lon, 6).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_reach_the_core() {
        assert!(!core_version().is_empty());
        assert!(presets().iter().any(|p| p.name == "vhf-fm"));
        assert!(presets()
            .iter()
            .any(|p| p.name == "afsk-1200" && p.uses_radio_tnc));
        assert_eq!(
            describe_frequency("145.500".into()).as_deref(),
            Some("145.500 MHz (2m)")
        );
        assert_eq!(describe_frequency("radio".into()), None);
        assert!(airtime_seconds("vhf-fm".into(), 100).unwrap() > 0.0);
        assert_eq!(airtime_seconds("nope".into(), 100), None);
        assert_eq!(
            grid_from_location(51.5, -0.12).as_deref().map(|g| &g[..4]),
            Some("IO91")
        );
    }
}
