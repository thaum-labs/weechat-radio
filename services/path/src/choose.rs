//! SPDX-License-Identifier: Apache-2.0
//! Pick one calling card from distance and VOACAP cells.

use crate::geo::Region;
use serde::Serialize;

/// Digital chat threshold (dB-Hz), between VOACAP CW (19) and SSB voice (38).
pub const REQ_SNR_DIGITAL: f64 = 27.0;
/// Tougher threshold for `hf-weak` when 27 dB-Hz needs more than 100 W.
pub const REQ_SNR_WEAK: f64 = 22.0;
pub const REF_POWER_W: f64 = 100.0;
pub const CLOSE_KM: f64 = 50.0;

const POWER_STEPS: [u32; 5] = [5, 10, 20, 50, 100];

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
        voice_note: voice_note(spot.band),
        footnote: "Simple wire antenna at each end, average noise, digital rather than voice. Check your band plan before you transmit. This is the usual chance for this month, not a promise for today.".into(),
        distance_km,
        next_utc,
    }
}

pub fn fm_card(region: Region, distance_km: f64) -> Option<Card> {
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
        distance_km.round() as u32,
        None,
    ))
}

struct Candidate {
    spot: Spot,
    power: u32,
    reliability: f64,
    preset: &'static str,
}

fn best_at_hour(cells: &[Cell], hour: u8, req: f64, preset: &'static str) -> Option<Candidate> {
    let mut best: Option<Candidate> = None;
    for c in cells.iter().filter(|c| c.hour == hour) {
        let Some(spot) = spot_for(c.freq_mhz) else {
            continue;
        };
        let Some(power) = power_for_snr(c.snr, req) else {
            continue;
        };
        let cand = Candidate {
            spot,
            power,
            reliability: c.reliability,
            preset,
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

fn next_open_hour(cells: &[Cell], from_hour: u8, req: f64) -> Option<u8> {
    for step in 1..=23u8 {
        let h = (from_hour + step) % 24;
        if best_at_hour(cells, h, req, "hf-poor").is_some() {
            return Some(h);
        }
    }
    None
}

pub fn pick_hf(cells: &[Cell], hour_utc: u8, distance_km: f64) -> Card {
    let dist = distance_km.round() as u32;
    if let Some(c) = best_at_hour(cells, hour_utc, REQ_SNR_DIGITAL, "hf-poor") {
        let conf = if c.reliability >= 0.5 {
            "Likely this hour.".to_string()
        } else {
            format!(
                "Possible this hour (about {:.0}% of days this month).",
                c.reliability * 100.0
            )
        };
        return card_shell(
            "hf",
            c.spot,
            true,
            "USB",
            Some(c.power),
            c.preset,
            conf,
            "SSB on, USB for data.".into(),
            dist,
            None,
        );
    }
    if let Some(c) = best_at_hour(cells, hour_utc, REQ_SNR_WEAK, "hf-weak") {
        return card_shell(
            "hf",
            c.spot,
            true,
            "USB",
            Some(c.power),
            "hf-weak",
            "Faint path - use the slower preset.".into(),
            "SSB on, USB for data.".into(),
            dist,
            None,
        );
    }
    let next = next_open_hour(cells, hour_utc, REQ_SNR_DIGITAL)
        .or_else(|| next_open_hour(cells, hour_utc, REQ_SNR_WEAK));
    let next_s = next.map(|h| format!("{h:02}:00 UTC"));
    let confidence = match &next_s {
        Some(t) => format!("Not now. Try {t}."),
        None => "Nothing under 100 W in the next 24 hours.".into(),
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
        dist,
        next_s,
    )
}

pub fn try_close_fm(tx: Region, rx: Region, distance_km: f64) -> Option<Card> {
    if distance_km > CLOSE_KM {
        return None;
    }
    let region = Region::shared_vhf(tx, rx)?;
    fm_card(region, distance_km)
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
        let card = pick_hf(&cells, 12, 800.0);
        assert_eq!(card.mode, "USB");
        assert!(card.ssb);
        assert_eq!(card.band, "40m");
        assert!(card.voice_note.as_ref().unwrap().contains("LSB"));
        assert_eq!(card.power_w, Some(5));
    }

    #[test]
    fn fm_uk() {
        let c = fm_card(Region::UkEu, 12.0).unwrap();
        assert!(!c.ssb);
        assert_eq!(c.mode, "FM");
        assert_eq!(c.frequency_khz, 144_950);
        assert_eq!(c.power_w, Some(5));
    }

    #[test]
    fn close_needs_shared_region() {
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 10.0).is_some());
        assert!(try_close_fm(Region::UkEu, Region::UsCa, 10.0).is_none());
        assert!(try_close_fm(Region::UkEu, Region::UkEu, 80.0).is_none());
    }

    #[test]
    fn nothing_works() {
        let cells = [Cell {
            hour: 12,
            freq_mhz: 14.07,
            reliability: 0.0,
            snr: -10.0,
        }];
        let card = pick_hf(&cells, 12, 5000.0);
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
}
