//! SPDX-License-Identifier: Apache-2.0
//! VOACAP via hfcast.

use crate::choose::{Cell, REQ_SNR_DIGITAL};
use hfcast::api::{FoF2Model, Ionosphere, Model, Numerics, Request, Site, Task};
use hfcast::listing::{parse_listing, MUF_ROW, MUF_SLOT};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(6 * 3600);

/// One VOACAP run for a path: a cell per hour and frequency, and the
/// path MUF with its hop count for each hour.
#[derive(Debug, Clone)]
pub struct Prediction {
    pub cells: Vec<Cell>,
    /// Path MUF in MHz by UTC hour, 0 where the listing printed none.
    pub muf_by_hour: [f64; 24],
    /// Hops of the mode at the MUF by UTC hour ("2F2" is 2).
    pub muf_hops_by_hour: [Option<u8>; 24],
}

#[derive(Clone)]
struct CacheEntry {
    at: Instant,
    prediction: Arc<Prediction>,
}

pub struct Forecast {
    lock: Mutex<()>,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl Forecast {
    pub fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn predict(
        &self,
        tx: (f64, f64),
        rx: (f64, f64),
        freqs_mhz: &[f64],
        ssn: f64,
        year: i32,
        month: u32,
    ) -> Result<Prediction, String> {
        let key = cache_key(tx, rx, ssn, year, month, freqs_mhz);
        if let Some(hit) = self.cache.lock().get(&key) {
            if hit.at.elapsed() < CACHE_TTL {
                return Ok((*hit.prediction).clone());
            }
        }

        // One forecast at a time on the 2 GB droplet.
        let _gate = self.lock.lock();
        if let Some(hit) = self.cache.lock().get(&key) {
            if hit.at.elapsed() < CACHE_TTL {
                return Ok((*hit.prediction).clone());
            }
        }

        let prediction = run_hfcast(tx, rx, freqs_mhz, ssn, year, month)?;
        self.cache.lock().insert(
            key,
            CacheEntry {
                at: Instant::now(),
                prediction: Arc::new(prediction.clone()),
            },
        );
        Ok(prediction)
    }
}

fn cache_key(
    tx: (f64, f64),
    rx: (f64, f64),
    ssn: f64,
    year: i32,
    month: u32,
    freqs: &[f64],
) -> String {
    let bucket = |v: f64| (v * 2.0).round() / 2.0;
    let ssn_b = (ssn / 5.0).round() * 5.0;
    let mut freqs_s: Vec<String> = freqs.iter().map(|f| format!("{f:.3}")).collect();
    freqs_s.sort();
    format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        bucket(tx.0),
        bucket(tx.1),
        bucket(rx.0),
        bucket(rx.1),
        year,
        month,
        ssn_b,
        freqs_s.join(",")
    )
}

/// The request hfcast's `service::run` builds from its JSON, built here so
/// the listing's MODE row (hops) comes out of the same single run.
fn request(
    tx: (f64, f64),
    rx: (f64, f64),
    freqs_mhz: &[f64],
    ssn: f64,
    year: i32,
    month: u32,
) -> Request {
    Request {
        numerics: Numerics::reference(),
        tx: Site {
            name: String::new(),
            lat_deg: tx.0,
            lon_deg: tx.1,
        },
        rx: Site {
            name: String::new(),
            lat_deg: rx.0,
            lon_deg: rx.1,
        },
        month,
        year: year.max(0) as u32,
        ssn,
        power_watts: 100.0,
        freqs_mhz: freqs_mhz.to_vec(),
        required_snr_db: REQ_SNR_DIGITAL,
        noise_dbw: 145.0,
        fof2: FoF2Model::Ccir,
        // Sporadic E on, as hfcast's service does.
        layer_multipliers: [1.0, 1.0, 1.0, 1.0],
        tx_antennas: Vec::new(),
        rx_antennas: Vec::new(),
        ionosphere: Ionosphere::default(),
        model: Model::Compatible,
    }
}

fn run_hfcast(
    tx: (f64, f64),
    rx: (f64, f64),
    freqs_mhz: &[f64],
    ssn: f64,
    year: i32,
    month: u32,
) -> Result<Prediction, String> {
    let req = request(tx, rx, freqs_mhz, ssn, year, month);
    let text = hfcast::api::listing(&PathBuf::from("<embedded>"), &req, Task::Systems)?;
    Ok(parse_prediction(&text, freqs_mhz))
}

/// "2F2" is two hops, "1E" one.
fn hops_of(mode: &str) -> Option<u8> {
    let digits: String = mode.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok().filter(|&n| n > 0)
}

/// The listing as cells, the same way hfcast's `service::run` reads it, plus
/// the bad-day spread, take-off angle and hops.
fn parse_prediction(text: &str, freqs: &[f64]) -> Prediction {
    let parsed = parse_listing(text);
    let mut rows: HashMap<(u8, &str, i8), f64> = HashMap::new();
    let mut muf_by_hour = [0.0; 24];
    for s in &parsed.numeric {
        if s.row == MUF_ROW && s.slot == MUF_SLOT {
            muf_by_hour[s.hour as usize % 24] = s.value;
            continue;
        }
        rows.insert((s.hour, s.row.as_str(), s.slot), s.value);
    }
    let mut hops: HashMap<(u8, i8), u8> = HashMap::new();
    let mut muf_hops_by_hour = [None; 24];
    for m in &parsed.modes {
        let Some(n) = hops_of(&m.mode) else { continue };
        if m.slot == MUF_SLOT {
            muf_hops_by_hour[m.hour as usize % 24] = Some(n);
        } else {
            hops.insert((m.hour, m.slot), n);
        }
    }
    let mut cells = Vec::new();
    for hour in 0..24u8 {
        for (slot, &freq_mhz) in freqs.iter().enumerate() {
            let slot = slot as i8;
            let at = |row: &str| rows.get(&(hour, row, slot)).copied();
            // No cell past the last slot with a mode, as in hfcast.
            let (Some(rel), Some(snr)) = (at("REL"), at("SNR")) else {
                continue;
            };
            cells.push(Cell {
                hour,
                freq_mhz,
                reliability: rel.clamp(0.0, 1.0),
                snr,
                snr_low: at("SNR LW"),
                takeoff_deg: at("TANGLE"),
                hops: hops.get(&(hour, slot)).copied(),
            });
        }
    }
    Prediction {
        cells,
        muf_by_hour,
        muf_hops_by_hour,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn hops_from_mode() {
        assert_eq!(hops_of("2F2"), Some(2));
        assert_eq!(hops_of("1E"), Some(1));
        assert_eq!(hops_of("-"), None);
    }

    // The single listing run here must answer exactly what hfcast's own
    // service answered before, for every cell.
    #[test]
    fn same_cells_as_hfcast_service() {
        let (tx, rx) = ((51.38, -2.36), (40.42, -3.70));
        let freqs = [3.58, 7.045, 10.14, 14.07, 21.07, 28.12];
        let ours = run_hfcast(tx, rx, &freqs, 120.0, 2026, 10).unwrap();
        let req = serde_json::json!({
            "fromLat": tx.0, "fromLon": tx.1, "toLat": rx.0, "toLon": rx.1,
            "month": 10, "year": 2026, "ssn": 120.0, "watts": 100,
            "bands": freqs, "requiredSnrDb": REQ_SNR_DIGITAL, "noiseDbw": 145,
            "itshfbc": "<embedded>",
        });
        let raw: Value =
            serde_json::from_str(&hfcast::service::run(&req.to_string()).unwrap()).unwrap();
        let theirs = raw["cells"].as_array().unwrap();
        assert_eq!(ours.cells.len(), theirs.len());
        for (a, b) in ours.cells.iter().zip(theirs) {
            assert_eq!(a.hour as f64, b["hour"].as_f64().unwrap());
            assert_eq!(a.freq_mhz, b["freqMhz"].as_f64().unwrap());
            assert!((a.snr - b["snr"].as_f64().unwrap()).abs() < 1e-9);
            assert!((a.reliability - b["reliability"].as_f64().unwrap()).abs() < 1e-9);
            assert_eq!(a.snr_low, b["snrLowDecile"].as_f64());
            assert_eq!(a.takeoff_deg, b["takeoffAngleDeg"].as_f64());
        }
        let muf: Vec<f64> = raw["mufByHour"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        assert_eq!(ours.muf_by_hour.to_vec(), muf);
        assert!(ours.cells.iter().all(|c| c.hops.is_some()));
        assert!(ours.muf_hops_by_hour.iter().all(|h| h.is_some()));
    }
}
