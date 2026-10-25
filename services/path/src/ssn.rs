//! SPDX-License-Identifier: Apache-2.0
//! Cached smoothed sunspot number from NOAA SWPC.

use parking_lot::Mutex;
use serde_json::Value;
use std::time::{Duration, Instant};

const UA: &str = "WeeChatRadioPath/0.1 (https://weechatradio.com; path helper)";
const CACHE_TTL: Duration = Duration::from_secs(12 * 3600);
/// Quiet-cycle fallback when NOAA is unreachable.
const FALLBACK_SSN: f64 = 80.0;

pub struct Ssn {
    client: reqwest::Client,
    cached: Mutex<Option<(Instant, f64)>>,
}

impl Ssn {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(8))
            .build()
            .expect("reqwest client");
        Self {
            client,
            cached: Mutex::new(None),
        }
    }

    pub async fn current(&self) -> f64 {
        if let Some((at, v)) = *self.cached.lock() {
            if at.elapsed() < CACHE_TTL {
                return v;
            }
        }
        let v = self.fetch().await.unwrap_or(FALLBACK_SSN);
        *self.cached.lock() = Some((Instant::now(), v));
        v
    }

    async fn fetch(&self) -> Result<f64, String> {
        // Monthly observed indices; last row is the most recent complete month.
        let url =
            "https://services.swpc.noaa.gov/json/solar-cycle/observed-solar-cycle-indices.json";
        let text = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        parse_latest_ssn(&text)
    }
}

fn parse_latest_ssn(text: &str) -> Result<f64, String> {
    let arr: Vec<Value> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    for row in arr.iter().rev() {
        if let Some(v) = row
            .get("ssn")
            .or_else(|| row.get("smoothed_ssn"))
            .or_else(|| row.get("SSN"))
            .and_then(|x| x.as_f64())
        {
            if v.is_finite() && v >= 0.0 {
                return Ok(v);
            }
        }
    }
    Err("no ssn in NOAA payload".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample() {
        let raw = r#"[{"time-tag":"2024-01","ssn":100.0},{"time-tag":"2024-02","ssn":110.5}]"#;
        assert!((parse_latest_ssn(raw).unwrap() - 110.5).abs() < 0.01);
    }
}
