//! SPDX-License-Identifier: Apache-2.0
//! VOACAP via hfcast.

use crate::choose::{Cell, REQ_SNR_DIGITAL};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const CACHE_TTL: Duration = Duration::from_secs(6 * 3600);

#[derive(Clone)]
struct CacheEntry {
    at: Instant,
    cells: Arc<Vec<Cell>>,
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
    ) -> Result<Vec<Cell>, String> {
        let key = cache_key(tx, rx, ssn, year, month, freqs_mhz);
        if let Some(hit) = self.cache.lock().get(&key) {
            if hit.at.elapsed() < CACHE_TTL {
                return Ok((*hit.cells).clone());
            }
        }

        // One forecast at a time on the 2 GB droplet.
        let _gate = self.lock.lock();
        if let Some(hit) = self.cache.lock().get(&key) {
            if hit.at.elapsed() < CACHE_TTL {
                return Ok((*hit.cells).clone());
            }
        }

        let cells = run_hfcast(tx, rx, freqs_mhz, ssn, year, month)?;
        let arc = Arc::new(cells.clone());
        self.cache.lock().insert(
            key,
            CacheEntry {
                at: Instant::now(),
                cells: arc,
            },
        );
        Ok(cells)
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

fn run_hfcast(
    tx: (f64, f64),
    rx: (f64, f64),
    freqs_mhz: &[f64],
    ssn: f64,
    year: i32,
    month: u32,
) -> Result<Vec<Cell>, String> {
    let bands: Vec<Value> = freqs_mhz.iter().map(|f| Value::from(*f)).collect();
    let req = serde_json::json!({
        "fromLat": tx.0,
        "fromLon": tx.1,
        "toLat": rx.0,
        "toLon": rx.1,
        "month": month,
        "year": year,
        "ssn": ssn,
        "watts": 100,
        "bands": bands,
        "requiredSnrDb": REQ_SNR_DIGITAL,
        "noiseDbw": 145,
        "itshfbc": "<embedded>",
    });
    let raw = hfcast::service::run(&req.to_string()).map_err(|e| e.to_string())?;
    parse_cells(&raw)
}

fn parse_cells(raw: &str) -> Result<Vec<Cell>, String> {
    let v: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let arr = v
        .get("cells")
        .and_then(|c| c.as_array())
        .ok_or_else(|| "forecast missing cells".to_string())?;
    let mut out = Vec::with_capacity(arr.len());
    for c in arr {
        let hour = c.get("hour").and_then(|x| x.as_f64()).unwrap_or(0.0) as u8;
        let freq_mhz = c
            .get("freqMhz")
            .and_then(|x| x.as_f64())
            .ok_or_else(|| "cell missing freqMhz".to_string())?;
        let reliability = c.get("reliability").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let snr = c.get("snr").and_then(|x| x.as_f64()).unwrap_or(-999.0);
        out.push(Cell {
            hour,
            freq_mhz,
            reliability,
            snr,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample() {
        let raw = r#"{"cells":[{"freqMhz":14.07,"hour":12,"reliability":0.7,"snr":35}]}"#;
        let cells = parse_cells(raw).unwrap();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].hour, 12);
        assert!((cells[0].snr - 35.0).abs() < 0.01);
    }
}
