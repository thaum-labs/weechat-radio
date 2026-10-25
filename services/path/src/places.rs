//! SPDX-License-Identifier: Apache-2.0
//! Place search and reverse geocode via Photon (Komoot).

use crate::geo::{grid_to_lat_lon, looks_like_grid};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const UA: &str = "WeeChatRadioPath/0.1 (https://weechatradio.com; path helper)";
const CACHE_TTL: Duration = Duration::from_secs(24 * 3600);

#[derive(Debug, Clone, Serialize)]
pub struct Place {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
}

pub struct Places {
    client: reqwest::Client,
    forward: Mutex<HashMap<String, (Instant, Vec<Place>)>>,
    reverse: Mutex<HashMap<String, (Instant, Place)>>,
}

impl Places {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(8))
            .build()
            .expect("reqwest client");
        Self {
            client,
            forward: Mutex::new(HashMap::new()),
            reverse: Mutex::new(HashMap::new()),
        }
    }

    pub async fn search(&self, q: &str) -> Result<Vec<Place>, String> {
        let q = q.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }
        if looks_like_grid(q) {
            if let Some((lat, lon)) = grid_to_lat_lon(q) {
                let country = self
                    .reverse(lat, lon)
                    .await
                    .map(|p| p.country)
                    .unwrap_or_default();
                return Ok(vec![Place {
                    label: q.to_ascii_uppercase(),
                    lat,
                    lon,
                    country,
                }]);
            }
        }
        let key = q.to_ascii_lowercase();
        if let Some((at, hits)) = self.forward.lock().get(&key) {
            if at.elapsed() < CACHE_TTL {
                return Ok(hits.clone());
            }
        }
        let url = format!(
            "https://photon.komoot.io/api/?q={}&limit=6",
            urlencoding_simple(q)
        );
        let text = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let places = parse_photon_features(&text)?;
        self.forward
            .lock()
            .insert(key, (Instant::now(), places.clone()));
        Ok(places)
    }

    pub async fn reverse(&self, lat: f64, lon: f64) -> Result<Place, String> {
        let key = format!("{:.2},{:.2}", lat, lon);
        if let Some((at, p)) = self.reverse.lock().get(&key) {
            if at.elapsed() < CACHE_TTL {
                return Ok(p.clone());
            }
        }
        let url = format!("https://photon.komoot.io/reverse?lat={lat}&lon={lon}");
        let text = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let places = parse_photon_features(&text)?;
        let place = places.into_iter().next().unwrap_or(Place {
            label: format!("{lat:.3}, {lon:.3}"),
            lat,
            lon,
            country: String::new(),
        });
        self.reverse
            .lock()
            .insert(key, (Instant::now(), place.clone()));
        Ok(place)
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn parse_photon_features(text: &str) -> Result<Vec<Place>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let features = v
        .get("features")
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for f in features {
        let coords = f
            .pointer("/geometry/coordinates")
            .and_then(|c| c.as_array())
            .ok_or_else(|| "feature missing coordinates".to_string())?;
        let lon = coords
            .first()
            .and_then(|x| x.as_f64())
            .ok_or_else(|| "bad lon".to_string())?;
        let lat = coords
            .get(1)
            .and_then(|x| x.as_f64())
            .ok_or_else(|| "bad lat".to_string())?;
        let props = f.get("properties").cloned().unwrap_or(Value::Null);
        let name = props
            .get("name")
            .and_then(|x| x.as_str())
            .unwrap_or("place");
        let city = props
            .get("city")
            .or_else(|| props.get("county"))
            .and_then(|x| x.as_str());
        let country = props
            .get("countrycode")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_ascii_uppercase();
        let label = match city {
            Some(c) if !name.eq_ignore_ascii_case(c) => format!("{name}, {c}"),
            _ => name.to_string(),
        };
        out.push(Place {
            label,
            lat,
            lon,
            country,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_photon() {
        let raw = r#"{"features":[{"geometry":{"coordinates":[-0.12,51.5]},"properties":{"name":"London","countrycode":"gb","city":"London"}}]}"#;
        let p = parse_photon_features(raw).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].country, "GB");
        assert!((p[0].lat - 51.5).abs() < 0.01);
    }
}
