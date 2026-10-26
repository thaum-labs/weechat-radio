//! SPDX-License-Identifier: Apache-2.0
//! Pick one calling card from distance and VOACAP cells.

use crate::geo::Region;
use serde::{Deserialize, Serialize};

/// Digital chat threshold (dB-Hz), between VOACAP CW (19) and SSB voice (38).
pub const REQ_SNR_DIGITAL: f64 = 27.0;
/// Tougher threshold for `hf-weak` when 27 dB-Hz needs more than 100 W.
pub const REQ_SNR_WEAK: f64 = 22.0;
pub const REF_POWER_W: f64 = 100.0;
/// Antenna height when the page sends none: a 10 m mast at each end.
pub const DEFAULT_ANTENNA_M: f64 = 10.0;

pub const POWER_STEPS: [u32; 5] = [5, 10, 20, 50, 100];

/// VHF radio horizon in km between a TX antenna `tx_m` and an RX antenna
/// `rx_m` high (4/3 earth radius). 10 m at both ends gives about 26 km.
pub fn radio_horizon_km(tx_m: f64, rx_m: f64) -> f64 {
    4.12 * (tx_m.max(0.0).sqrt() + rx_m.max(0.0).sqrt())
}

/// HF antenna at the user's end, picked on the page. The prediction assumes
/// a simple wire (a dipole); each type is a rough dB gain against that, and
/// short or small antennas lose more below 7 MHz (80m and 160m).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HfAntenna {
    #[default]
    Dipole,
    /// End-fed half-wave.
    Efhw,
    /// End-fed random or long wire on a 9:1 unun, often 10-20 m long.
    Endfed,
    Vertical,
    /// Small magnetic loop.
    Loop,
    /// Yagi or other beam, usually 20m and up.
    Beam,
    /// Mobile whip or manpack.
    Whip,
}

impl HfAntenna {
    pub fn gain_db(self, mhz: f64) -> f64 {
        let low = mhz < 7.0;
        match self {
            HfAntenna::Dipole => 0.0,
            HfAntenna::Efhw => -1.0,
            HfAntenna::Endfed if low => -10.0,
            HfAntenna::Endfed => -4.0,
            HfAntenna::Vertical if low => -4.0,
            HfAntenna::Vertical => -2.0,
            HfAntenna::Loop if low => -12.0,
            HfAntenna::Loop => -6.0,
            HfAntenna::Beam if mhz >= 14.0 => 6.0,
            HfAntenna::Beam => 0.0,
            HfAntenna::Whip if low => -20.0,
            HfAntenna::Whip => -12.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HfAntenna::Dipole => "Dipole",
            HfAntenna::Efhw => "End-fed half-wave",
            HfAntenna::Endfed => "End-fed wire",
            HfAntenna::Vertical => "Vertical",
            HfAntenna::Loop => "Magnetic loop",
            HfAntenna::Beam => "Beam",
            HfAntenna::Whip => "Mobile whip",
        }
    }

    /// Apply this antenna to the predicted cells and say so in the footnote.
    pub fn apply(self, cells: &mut [Cell]) {
        for c in cells {
            c.snr += self.gain_db(c.freq_mhz);
        }
    }

    pub fn footnote(self, card: &mut Card) {
        if self != HfAntenna::Dipole {
            card.footnote = card.footnote.replacen(
                "Simple wire antenna at each end",
                &format!("{} at your end, simple wire at the other", self.label()),
                1,
            );
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub ok: bool,
    pub kind: &'static str,
    pub frequency_mhz: String,
    pub frequency_khz: u32,
    pub band: &'static str,
    pub ssb: bool,
    pub mode: &'static str,
    pub power_w: Option<u32>,
    pub preset: &'static str,
    pub qsy: String,
    pub preset_cmd: String,
    pub confidence: String,
    pub note: String,
    /// One or two plain sentences on why this band was picked.
    pub why: String,
    pub voice_note: Option<String>,
    pub footnote: String,
    pub distance_km: u32,
    pub next_utc: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub hour: u8,
    pub freq_mhz: f64,
    pub reliability: f64,
    pub snr: f64,
}

#[derive(Debug, Clone, Copy)]
struct Spot {
    khz: u32,
    band: &'static str,
    label: &'static str,
}

const WW_SPOTS: &[Spot] = &[
    Spot {
        khz: 3_580,
        band: "80m",
        label: "3.580",
    },
    Spot {
        khz: 5_357,
        band: "60m",
        label: "5.357",
    },
    Spot {
        khz: 10_140,
        band: "30m",
        label: "10.140",
    },
    Spot {
        khz: 14_070,
        band: "20m",
        label: "14.070",
    },
    Spot {
        khz: 18_100,
        band: "17m",
        label: "18.100",
    },
    Spot {
        khz: 21_070,
        band: "15m",
        label: "21.070",
    },
    Spot {
        khz: 24_920,
        band: "12m",
        label: "24.920",
    },
    Spot {
        khz: 28_120,
        band: "10m",
        label: "28.120",
    },
];

pub fn hf_freqs_mhz(tx: Region, rx: Region) -> Vec<f64> {
    let mut out: Vec<f64> = WW_SPOTS.iter().map(|s| s.khz as f64 / 1000.0).collect();
    match Region::shared_40m(tx, rx) {
        Some("uk_eu") => out.insert(1, 7.045),
        Some("us_au_nz") => out.insert(1, 7.090),
        _ => {}
    }
    out
}

fn spot_for(freq_mhz: f64) -> Option<Spot> {
    let khz = (freq_mhz * 1000.0).round() as u32;
    WW_SPOTS
        .iter()
        .copied()
        .find(|s| s.khz == khz)
        .or_else(|| match khz {
            7_045 => Some(Spot {
                khz: 7_045,
                band: "40m",
                label: "7.045",
            }),
            7_090 => Some(Spot {
                khz: 7_090,
                band: "40m",
                label: "7.090",
            }),
            _ => None,
        })
}

pub fn round_power_up(watts: f64) -> Option<u32> {
    if !watts.is_finite() || watts <= 0.0 {
        return None;
    }
    POWER_STEPS.into_iter().find(|&p| (p as f64) >= watts)
}

/// Power at `REF_POWER_W` that yields `snr`, scaled to meet `req`.
pub fn power_for_snr(snr: f64, req: f64) -> Option<u32> {
    let needed = REF_POWER_W * 10f64.powf((req - snr) / 10.0);
    round_power_up(needed)
}

fn voice_note(band: &str) -> Option<String> {
    match band {
        "160m" | "80m" | "40m" => Some(format!("Voice on {band} is LSB. Data stays on USB.")),
        _ => None,
    }
}

fn card_shell(
    kind: &'static str,
    spot: Spot,
    ssb: bool,
    mode: &'static str,
    power_w: Option<u32>,
    preset: &'static str,
    confidence: String,
    note: String,
    why: String,
    distance_km: u32,
    next_utc: Option<String>,
) -> Card {
    Card {
        ok: power_w.is_some() || kind == "fm",
        kind,
        frequency_mhz: spot.label.to_string(),
        frequency_khz: spot.khz,
        band: spot.band,
        ssb,
        mode,
        power_w,
        preset,
        qsy: format!("/radio qsy {}", spot.label),
        preset_cmd: format!("/radio preset {preset}"),
        confidence,
        note,
        why,
        voice_note: voice_note(spot.band),
        footnote: "Simple wire antenna at each end, average noise, digital rather than voice. Check your band plan before you transmit. This is the usual chance for this month, not a promise for today.".into(),
        distance_km,
        next_utc,
    }
}

pub fn fm_card(region: Region, distance_km: f64, tx_m: f64, rx_m: f64) -> Option<Card> {
    let spot = match region {
        Region::UkEu => Spot {
            khz: 144_950,
            band: "2m",
            label: "144.950",
        },
        Region::UsCa => Spot {
            khz: 145_010,
            band: "2m",
            label: "145.010",
        },
        Region::Au => Spot {
            khz: 146_550,
            band: "2m",
            label: "146.550",
        },
        _ => return None,
    };
    Some(card_shell(
        "fm",
        spot,
        false,
        "FM",
        Some(5),
        "vhf-fm",
        "Close path - line of sight on 2m FM.".into(),
        "A hill can still block a short path. SSB stays off.".into(),
        fm_why(distance_km, tx_m, rx_m),
        distance_km.round() as u32,
        None,
    ))
}

fn fm_why(distance_km: f64, tx_m: f64, rx_m: f64) -> String {
    let horizon = radio_horizon_km(tx_m, rx_m);
    let ants = antennas_label(tx_m, rx_m);
    if distance_km <= horizon {
        format!("2m FM: {distance_km:.0} km is inside the {horizon:.0} km radio horizon for {ants} antennas.")
    } else {
        format!("2m FM: {distance_km:.0} km is past the {horizon:.0} km smooth-earth radio horizon for {ants} antennas, but the ground along this path leaves a radio line.")
    }
}

fn antenna_label(antenna_m: f64) -> String {
    if (antenna_m - antenna_m.round()).abs() < 0.05 {
        format!("{:.0} m", antenna_m)
    } else {
        format!("{:.1} m", antenna_m)
    }
}

/// "10 m" when both ends match, "20 m and 1.5 m" (TX first) when not.
fn antennas_label(tx_m: f64, rx_m: f64) -> String {
    let (tx, rx) = (antenna_label(tx_m), antenna_label(rx_m));
    if tx == rx {
        tx
    } else {
        format!("{tx} and {rx}")
    }
}

struct Candidate {
    spot: Spot,
    power: u32,
    reliability: f64,
}

/// Lowest-power spot that works at `hour` within `max_w`, most reliable on a tie.
fn best_at_hour(cells: &[Cell], hour: u8, req: f64, max_w: u32) -> Option<Candidate> {
    let mut best: Option<Candidate> = None;
    for c in cells.iter().filter(|c| c.hour == hour) {
        let Some(spot) = spot_for(c.freq_mhz) else {
            continue;
        };
        let Some(power) = power_for_snr(c.snr, req).filter(|&p| p <= max_w) else {
            continue;
        };
        let cand = Candidate {
            spot,
            power,
            reliability: c.reliability,
        };
        let replace = match &best {
            None => true,
            Some(b) if cand.power < b.power => true,
            Some(b) if cand.power == b.power && cand.reliability > b.reliability => true,
            _ => false,
        };
        if replace {
            best = Some(cand);
        }
    }
    best
}

fn next_open_hour(cells: &[Cell], from_hour: u8, req: f64, max_w: u32) -> Option<u8> {
    (1..=23u8)
        .map(|step| (from_hour + step) % 24)
        .find(|&h| best_at_hour(cells, h, req, max_w).is_some())
}

/// Does `khz` still work at `hour` within `max_w`?
fn spot_open(cells: &[Cell], khz: u32, hour: u8, req: f64, max_w: u32) -> bool {
    cells.iter().any(|c| {
        c.hour == hour
            && (c.freq_mhz * 1000.0).round() as u32 == khz
            && power_for_snr(c.snr, req).is_some_and(|p| p <= max_w)
    })
}

/// First hour after `hour` when `khz` stops working, or None if it stays open all day.
fn closes_at(cells: &[Cell], khz: u32, hour: u8, req: f64, max_w: u32) -> Option<u8> {
    (1..=23u8)
        .map(|step| (hour + step) % 24)
        .find(|&h| !spot_open(cells, khz, h, req, max_w))
}

/// Best opening in the 12 hours from `hour`, as (hour, candidate).
fn best_next_12h(cells: &[Cell], hour: u8, req: f64, max_w: u32) -> Option<(u8, Candidate)> {
    let mut best: Option<(u8, Candidate)> = None;
    for step in 0..12u8 {
        let h = (hour + step) % 24;
        let Some(c) = best_at_hour(cells, h, req, max_w) else {
            continue;
        };
        let better = match &best {
            None => true,
            Some((_, b)) => {
                c.power < b.power || (c.power == b.power && c.reliability > b.reliability + 0.05)
            }
        };
        if better {
            best = Some((h, c));
        }
    }
    best
}

fn hh(hour: u8) -> String {
    format!("{hour:02}:00 UTC")
}

/// Pick the HF card for `hour_utc`. `now` says whether that hour is the
/// current one, which only changes the wording ("this hour" or "at 14:00 UTC").
pub fn pick_hf(cells: &[Cell], hour_utc: u8, now: bool, distance_km: f64, max_w: u32) -> Card {
    let dist = distance_km.round() as u32;
    let when = if now {
        "this hour".to_string()
    } else {
        format!("at {}", hh(hour_utc))
    };
    let open_when = if now { "now".to_string() } else { when.clone() };
    let later = best_next_12h(cells, hour_utc, REQ_SNR_DIGITAL, max_w);

    if let Some(c) = best_at_hour(cells, hour_utc, REQ_SNR_DIGITAL, max_w) {
        let conf = if c.reliability >= 0.5 {
            format!("Likely {when}.")
        } else {
            format!(
                "Possible {when} (about {:.0}% of days this month).",
                c.reliability * 100.0
            )
        };
        let mut why = format!(
            "{}: open {open_when} on this path, {} W is enough.",
            c.spot.band, c.power
        );
        match closes_at(cells, c.spot.khz, hour_utc, REQ_SNR_DIGITAL, max_w) {
            Some(h) => why.push_str(&format!(" It closes about {}.", hh(h))),
            None => why.push_str(" It stays open all day."),
        }
        if let Some((h, b)) = later.filter(|(h, b)| *h != hour_utc && b.power < c.power) {
            why.push_str(&format!(
                " Best in the next 12 h: {} at {}, {} W.",
                b.spot.band,
                hh(h),
                b.power
            ));
        }
        return card_shell(
            "hf",
            c.spot,
            true,
            "USB",
            Some(c.power),
            "hf-poor",
            conf,
            "SSB on, USB for data.".into(),
            why,
            dist,
            None,
        );
    }
    if let Some(c) = best_at_hour(cells, hour_utc, REQ_SNR_WEAK, max_w) {
        let mut why = format!(
            "{}: too faint {when} for the normal preset at {max_w} W, so use the slower one.",
            c.spot.band
        );
        if let Some((h, b)) = later {
            why.push_str(&format!(
                " Best in the next 12 h: {} at {}, {} W.",
                b.spot.band,
                hh(h),
                b.power
            ));
        }
        return card_shell(
            "hf",
            c.spot,
            true,
            "USB",
            Some(c.power),
            "hf-weak",
            "Faint path - use the slower preset.".into(),
            "SSB on, USB for data.".into(),
            why,
            dist,
            None,
        );
    }
    let next = next_open_hour(cells, hour_utc, REQ_SNR_DIGITAL, max_w)
        .or_else(|| next_open_hour(cells, hour_utc, REQ_SNR_WEAK, max_w));
    let next_s = next.map(hh);
    let confidence = match &next_s {
        Some(t) => format!("Not {open_when}. Try {t}."),
        None => format!("Nothing at {max_w} W in the next 24 hours."),
    };
    let why = match next {
        Some(h) => {
            let band = best_at_hour(cells, h, REQ_SNR_DIGITAL, max_w)
                .or_else(|| best_at_hour(cells, h, REQ_SNR_WEAK, max_w))
                .map(|c| c.spot.band)
                .unwrap_or("HF");
            format!(
                "No band reaches this far {when} at {max_w} W. The first opening is {band} at {}.",
                hh(h)
            )
        }
        None => format!("No band reaches this far at {max_w} W on any hour this month."),
    };
    // Still show the least-bad frequency at this hour for context.
    let fallback = cells
        .iter()
        .filter(|c| c.hour == hour_utc)
        .max_by(|a, b| {
            a.snr
                .partial_cmp(&b.snr)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .and_then(|c| spot_for(c.freq_mhz))
        .unwrap_or(Spot {
            khz: 14_070,
            band: "20m",
            label: "14.070",
        });
    card_shell(
        "hf",
        fallback,
        true,
        "USB",
        None,
        "hf-poor",
        confidence,
        "SSB on, USB for data. Wait for a better hour or use a gateway.".into(),
        why,
        dist,
        next_s,
    )
}

/// Pick the HF card for one band the user chose by hand. When that band is
/// closed, the card says so about that band alone, not about all of HF.
pub fn pick_hf_band(
    cells: &[Cell],
    band: &str,
    hour_utc: u8,
    now: bool,
    distance_km: f64,
    max_w: u32,
) -> Card {
    let mut card = pick_hf(cells, hour_utc, now, distance_km, max_w);
    if card.ok {
        return card;
    }
    let when = if now {
        "this hour".to_string()
    } else {
        format!("at {}", hh(hour_utc))
    };
    card.confidence = format!("{band} doesn't work on this path {when}.");
    card.why = match &card.next_utc {
        Some(t) => format!("{band} is closed {when} at {max_w} W. It opens about {t}."),
        None => format!("{band} doesn't open on this path at {max_w} W on any hour this month."),
    };
    card.note = "SSB on, USB for data.".into();
    card
}

/// Whether one HF band works on the path at an hour, and at what power.
#[derive(Debug, Clone, Serialize)]
pub struct BandWorks {
    pub band: &'static str,
    pub ok: bool,
    pub power_w: Option<u32>,
}

/// Every HF band on the path, judged on its own at `hour_utc` within
/// `max_w`. The same list goes out whichever band the user picked.
pub fn hf_bands(cells: &[Cell], freqs: &[f64], hour_utc: u8, max_w: u32) -> Vec<BandWorks> {
    freqs
        .iter()
        .filter_map(|&f| {
            let band = band_of(f)?;
            let only: Vec<Cell> = cells
                .iter()
                .copied()
                .filter(|c| band_of(c.freq_mhz) == Some(band))
                .collect();
            let power = best_at_hour(&only, hour_utc, REQ_SNR_DIGITAL, max_w)
                .or_else(|| best_at_hour(&only, hour_utc, REQ_SNR_WEAK, max_w))
                .map(|c| c.power);
            Some(BandWorks {
                band,
                ok: power.is_some(),
                power_w: power,
            })
        })
        .collect()
}

/// The HF band name ("40m") a predicted frequency belongs to.
pub fn band_of(freq_mhz: f64) -> Option<&'static str> {
    spot_for(freq_mhz).map(|s| s.band)
}

/// 2m FM card for a path past the smooth-earth radio horizon. The page uses
/// it only when the real terrain still leaves a radio line (marginal, not
/// blocked); a 2m path is never longer than this.
pub const FM_TERRAIN_MAX_KM: f64 = 150.0;

pub fn far_fm(tx: Region, rx: Region, distance_km: f64, tx_m: f64, rx_m: f64) -> Option<Card> {
    if distance_km <= radio_horizon_km(tx_m, rx_m) || distance_km > FM_TERRAIN_MAX_KM {
        return None;
    }
    let region = Region::shared_vhf(tx, rx)?;
    fm_card(region, distance_km, tx_m, rx_m)
}

pub fn try_close_fm(
    tx: Region,
    rx: Region,
    distance_km: f64,
    tx_m: f64,
    rx_m: f64,
) -> Option<Card> {
    if distance_km > radio_horizon_km(tx_m, rx_m) {
        return None;
    }
    let region = Region::shared_vhf(tx, rx)?;
    fm_card(region, distance_km, tx_m, rx_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_rounds_up() {
        assert_eq!(round_power_up(1.0), Some(5));
        assert_eq!(round_power_up(5.0), Some(5));
        assert_eq!(round_power_up(11.0), Some(20));
        assert_eq!(round_power_up(100.0), Some(100));
        assert_eq!(round_power_up(101.0), None);
    }

    #[test]
    fn snr_margin_drops_power() {
        // 5 dB over at 100 W → ~31.6 W → 50 W
        assert_eq!(power_for_snr(32.0, 27.0), Some(50));
        // Exact match
        assert_eq!(power_for_snr(27.0, 27.0), Some(100));
        // 10 dB over → 10 W
        assert_eq!(power_for_snr(37.0, 27.0), Some(10));
    }

    #[test]
    fn usb_on_40m() {
        let cells = [Cell {
            hour: 12,
            freq_mhz: 7.045,
            reliability: 0.8,
            snr: 41.0,
        }];
        let card = pick_hf(&cells, 12, true, 800.0, 100);
        assert_eq!(card.mode, "USB");
        assert!(card.ssb);
        assert_eq!(card.band, "40m");
        assert!(card.voice_note.as_ref().unwrap().contains("LSB"));
        assert_eq!(card.power_w, Some(5));
    }

    #[test]
    fn fm_uk() {
        let c = fm_card(Region::UkEu, 12.0, DEFAULT_ANTENNA_M, DEFAULT_ANTENNA_M).unwrap();
        assert!(!c.ssb);
        assert_eq!(c.mode, "FM");
        assert_eq!(c.frequency_khz, 144_950);
        assert_eq!(c.power_w, Some(5));
    }

    #[test]
    fn close_needs_shared_region() {
        let m = DEFAULT_ANTENNA_M;
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 10.0, m, m).is_some());
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 20.0, m, m).is_some());
        assert!(try_close_fm(Region::UkEu, Region::UsCa, 10.0, m, m).is_none());
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 30.0, m, m).is_none());
    }

    #[test]
    fn nothing_works() {
        let cells = [Cell {
            hour: 12,
            freq_mhz: 14.07,
            reliability: 0.0,
            snr: -10.0,
        }];
        let card = pick_hf(&cells, 12, true, 5000.0, 100);
        assert!(card.power_w.is_none());
        assert!(card.confidence.contains("Nothing") || card.confidence.contains("Not now"));
    }

    #[test]
    fn hf_freq_list_includes_40m() {
        let f = hf_freqs_mhz(Region::UkEu, Region::UkEu);
        assert!(f.contains(&7.045));
        let f2 = hf_freqs_mhz(Region::UsCa, Region::Au);
        assert!(f2.contains(&7.090));
    }

    #[test]
    fn handheld_horizon_is_shorter() {
        assert!((radio_horizon_km(10.0, 10.0) - 26.0).abs() < 0.5);
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 12.0, 1.5, 1.5).is_none());
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 8.0, 1.5, 1.5).is_some());
        let c = try_close_fm(Region::UkEu, Region::UkEu, 8.0, 1.5, 1.5).unwrap();
        assert!(c.why.contains("1.5 m antennas"), "{}", c.why);
    }

    #[test]
    fn a_weaker_antenna_needs_more_power() {
        // 30m needs 20 W with a dipole; an end-fed wire (-4 dB) needs 50 W
        // and a mobile whip (-12 dB) needs more than that.
        let base = vec![cell(12, 10.14, REQ_SNR_DIGITAL + 7.5)];
        let mut endfed = base.clone();
        HfAntenna::Endfed.apply(&mut endfed);
        let mut whip = base.clone();
        HfAntenna::Whip.apply(&mut whip);
        assert_eq!(pick_hf(&base, 12, true, 1000.0, 100).power_w, Some(20));
        assert_eq!(pick_hf(&endfed, 12, true, 1000.0, 100).power_w, Some(50));
        assert!(pick_hf(&whip, 12, true, 1000.0, 100).power_w.unwrap_or(999) > 50);
        let mut card = pick_hf(&endfed, 12, true, 1000.0, 100);
        HfAntenna::Endfed.footnote(&mut card);
        assert!(
            card.footnote.starts_with("End-fed wire at your end"),
            "{}",
            card.footnote
        );
        assert!(HfAntenna::Endfed.gain_db(3.58) < HfAntenna::Endfed.gain_db(7.045));
    }

    #[test]
    fn each_end_has_its_own_height() {
        // 20 m tower to a 1.5 m handheld: 4.12 * (4.47 + 1.22) = about 23 km.
        assert!((radio_horizon_km(20.0, 1.5) - 23.5).abs() < 0.5);
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 20.0, 20.0, 1.5).is_some());
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 20.0, 1.5, 1.5).is_none());
        let c = try_close_fm(Region::UkEu, Region::UkEu, 20.0, 20.0, 1.5).unwrap();
        assert!(c.why.contains("20 m and 1.5 m antennas"), "{}", c.why);
    }

    #[test]
    fn a_path_past_the_horizon_still_offers_fm_for_the_terrain_check() {
        // Two handhelds 20 km apart: past the 10 km horizon, so HF is the
        // pick, but the page can keep FM if the terrain leaves a line.
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 20.0, 1.5, 1.5).is_none());
        let c = far_fm(Region::UkEu, Region::UkEu, 20.0, 1.5, 1.5).unwrap();
        assert_eq!(c.band, "2m");
        assert!(c.why.contains("past the 10 km"), "{}", c.why);
        assert!(far_fm(Region::UkEu, Region::UkEu, 8.0, 1.5, 1.5).is_none());
        assert!(far_fm(Region::UkEu, Region::UkEu, 400.0, 10.0, 10.0).is_none());
        assert!(far_fm(Region::UkEu, Region::UsCa, 20.0, 1.5, 1.5).is_none());
    }

    #[test]
    fn band_names_follow_the_spots() {
        assert_eq!(band_of(7.045), Some("40m"));
        assert_eq!(band_of(10.14), Some("30m"));
        assert_eq!(band_of(9.0), None);
    }

    fn cell(hour: u8, freq_mhz: f64, snr: f64) -> Cell {
        Cell {
            hour,
            freq_mhz,
            reliability: 0.8,
            snr,
        }
    }

    #[test]
    fn why_names_the_band_and_when_it_closes() {
        // 30m open 10..=17, 20m only at 14 and stronger.
        let mut cells = Vec::new();
        for h in 0..24u8 {
            cells.push(cell(
                h,
                10.14,
                if (10..=17).contains(&h) { 30.0 } else { 0.0 },
            ));
            cells.push(cell(h, 14.07, if h == 14 { 40.0 } else { 0.0 }));
        }
        let card = pick_hf(&cells, 12, true, 1500.0, 100);
        assert_eq!(card.band, "30m");
        assert!(card.why.starts_with("30m: open now"), "{}", card.why);
        assert!(card.why.contains("closes about 18:00 UTC"), "{}", card.why);
        assert!(
            card.why.contains("Best in the next 12 h: 20m at 14:00 UTC"),
            "{}",
            card.why
        );
    }

    #[test]
    fn a_later_hour_changes_the_pick_and_the_wording() {
        let mut cells = Vec::new();
        for h in 0..24u8 {
            cells.push(cell(h, 10.14, if h < 12 { 30.0 } else { 0.0 }));
            cells.push(cell(h, 3.58, if h >= 18 { 30.0 } else { 0.0 }));
        }
        let card = pick_hf(&cells, 20, false, 600.0, 100);
        assert_eq!(card.band, "80m");
        assert!(
            card.confidence.contains("at 20:00 UTC"),
            "{}",
            card.confidence
        );
    }

    #[test]
    fn low_power_rules_out_a_band_that_needs_more() {
        // 32 dB-Hz needs 50 W for 27 dB-Hz; at 10 W only the faint preset fits.
        let cells = [cell(12, 10.14, 32.0)];
        assert_eq!(pick_hf(&cells, 12, true, 1500.0, 100).power_w, Some(50));
        let low = pick_hf(&cells, 12, true, 1500.0, 10);
        assert_eq!(low.preset, "hf-weak");
        assert_eq!(low.power_w, Some(10));
        let none = pick_hf(&cells, 12, true, 1500.0, 5);
        assert!(none.power_w.is_none());
        assert!(none.why.contains("5 W"), "{}", none.why);
    }

    #[test]
    fn a_closed_band_picked_by_hand_speaks_for_that_band_only() {
        // 20m opens at 15:00; at 12:00 it is closed.
        let cells: Vec<Cell> = (0..24u8)
            .map(|h| cell(h, 14.07, if h >= 15 { 40.0 } else { 0.0 }))
            .collect();
        let card = pick_hf_band(&cells, "20m", 12, true, 20.0, 20);
        assert!(!card.ok);
        assert_eq!(card.band, "20m");
        assert_eq!(card.confidence, "20m doesn't work on this path this hour.");
        assert!(
            card.why.contains("It opens about 15:00 UTC"),
            "{}",
            card.why
        );
        assert!(!card.why.contains("No band"), "{}", card.why);
        let never: Vec<Cell> = (0..24u8).map(|h| cell(h, 14.07, 0.0)).collect();
        let card = pick_hf_band(&never, "20m", 12, true, 20.0, 20);
        assert!(card.why.contains("any hour this month"), "{}", card.why);
    }

    #[test]
    fn the_band_list_judges_each_band_on_its_own() {
        let cells = [
            cell(12, 7.045, 45.0),
            cell(12, 14.07, 0.0),
            cell(12, 3.58, 30.0),
        ];
        let bands = hf_bands(&cells, &[3.58, 7.045, 14.07], 12, 20);
        let ok: Vec<_> = bands.iter().filter(|b| b.ok).map(|b| b.band).collect();
        assert_eq!(ok, ["80m", "40m"]);
        assert_eq!(bands[1].power_w, Some(5));
        assert!(bands[2].power_w.is_none());
    }
}
