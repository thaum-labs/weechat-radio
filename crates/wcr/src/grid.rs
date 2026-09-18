//! SPDX-License-Identifier: Apache-2.0
//! Maidenhead grid squares to lat/lon centroids.

use crate::error::{Error, Result};
use std::collections::HashMap;

pub fn normalize(grid: &str) -> Result<String> {
    let g = grid.trim().to_ascii_uppercase();
    if ![2, 4, 6, 8].contains(&g.len()) {
        return Err(Error::config(
            "grid square must be 2, 4, 6 or 8 characters (e.g. IO91wm)",
        ));
    }
    let b = g.as_bytes();
    if !b[0].is_ascii_uppercase() || !b[1].is_ascii_uppercase() {
        return Err(Error::config("grid square must start with two letters"));
    }
    if g.len() >= 4 && (!b[2].is_ascii_digit() || !b[3].is_ascii_digit()) {
        return Err(Error::config("grid square field must be two digits"));
    }
    Ok(g)
}

/// Centroid of a Maidenhead square. Precision 4 is ~100 km, 6 is ~5 km.
pub fn to_lat_lon(grid: &str) -> Result<(f64, f64)> {
    let g = normalize(grid)?;
    let b = g.as_bytes();
    let mut lon = (b[0] - b'A') as f64 * 20.0 - 180.0;
    let mut lat = (b[1] - b'A') as f64 * 10.0 - 90.0;
    let mut lon_span = 20.0;
    let mut lat_span = 10.0;
    if g.len() >= 4 {
        lon += (b[2] - b'0') as f64 * 2.0;
        lat += (b[3] - b'0') as f64 * 1.0;
        lon_span = 2.0;
        lat_span = 1.0;
    }
    if g.len() >= 6 {
        lon += (b[4] - b'A') as f64 * (2.0 / 24.0);
        lat += (b[5] - b'A') as f64 * (1.0 / 24.0);
        lon_span = 2.0 / 24.0;
        lat_span = 1.0 / 24.0;
    }
    if g.len() >= 8 {
        lon += (b[6] - b'0') as f64 * (2.0 / 24.0 / 10.0);
        lat += (b[7] - b'0') as f64 * (1.0 / 24.0 / 10.0);
        lon_span = 2.0 / 24.0 / 10.0;
        lat_span = 1.0 / 24.0 / 10.0;
    }
    Ok((lat + lat_span / 2.0, lon + lon_span / 2.0))
}

/// Convert WGS84 to a Maidenhead locator. Same algorithm as Ceefax Station
/// (`latlon_to_maidenhead`, default 6 characters).
pub fn from_lat_lon(lat: f64, lon: f64, precision: usize) -> Result<String> {
    let precision = match precision {
        2 | 4 | 6 | 8 => precision,
        _ => {
            return Err(Error::config(
                "Maidenhead precision must be 2, 4, 6 or 8 characters",
            ))
        }
    };
    let mut lon = ((lon + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    let lat = lat.clamp(-90.0, 90.0);
    lon += 180.0;
    let lat = lat + 90.0;
    let field_lon = (lon / 20.0).floor() as i32;
    let field_lat = (lat / 10.0).floor() as i32;
    let mut grid = String::new();
    grid.push(char::from(b'A' + field_lon as u8));
    grid.push(char::from(b'A' + field_lat as u8));
    if precision >= 4 {
        let lon_rem = lon % 20.0;
        let lat_rem = lat % 10.0;
        grid.push(char::from(b'0' + (lon_rem / 2.0).floor() as u8));
        grid.push(char::from(b'0' + lat_rem.floor() as u8));
    }
    if precision >= 6 {
        let lon_rem = lon % 20.0;
        let lat_rem = lat % 10.0;
        let sub_lon = ((lon_rem % 2.0) / 2.0 * 24.0).floor() as u8;
        let sub_lat = ((lat_rem % 1.0) * 24.0).floor() as u8;
        grid.push(char::from(b'A' + sub_lon.min(23)));
        grid.push(char::from(b'A' + sub_lat.min(23)));
    }
    if precision >= 8 {
        let lon_rem = lon % 20.0;
        let lat_rem = lat % 10.0;
        let sub_lon = (lon_rem % 2.0) / 2.0 * 24.0;
        let sub_lat = (lat_rem % 1.0) * 24.0;
        grid.push(char::from(b'0' + ((sub_lon % 1.0) * 10.0).floor() as u8));
        grid.push(char::from(b'0' + ((sub_lat % 1.0) * 10.0).floor() as u8));
    }
    Ok(grid)
}

#[derive(Debug, Clone)]
pub struct DetectedGrid {
    pub grid: String,
    pub label: String,
}

#[derive(Debug, Clone)]
struct GeoHit {
    lat: f64,
    lon: f64,
    label: String,
    /// Lower is trusted more when votes tie.
    rank: u8,
}

/// Look up an approximate Maidenhead square from public IP geolocation.
///
/// Several free providers disagree (UK ISP IPv4 often maps to the wrong city).
/// We query more than one and only auto-fill when they agree on the 4-character
/// square (~100 km). That avoids trusting a single bad database row.
pub fn detect_from_ip() -> Option<DetectedGrid> {
    detect_from_ip_inner().ok().flatten()
}

/// Approximate 6-character Maidenhead square for a specific IP (hub telemetry fallback).
pub fn grid_for_ip(ip: &str) -> Option<String> {
    let ip = ip.trim();
    if ip.is_empty() || ip == "127.0.0.1" || ip == "::1" {
        return None;
    }
    if let Some(rest) = ip.strip_prefix("::ffff:") {
        if rest == "127.0.0.1" {
            return None;
        }
    }
    grid_for_ip_inner(ip).ok().flatten()
}

fn grid_for_ip_inner(ip: &str) -> Result<Option<String>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .user_agent("WeeChatRadio/0.1")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let url = format!("http://ip-api.com/json/{ip}?fields=status,lat,lon");
    let Some(hit) = try_ip_api(&client, &url, 0)? else {
        return Ok(None);
    };
    Ok(from_lat_lon(hit.lat, hit.lon, 6).ok())
}

fn detect_from_ip_inner() -> Result<Option<DetectedGrid>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .user_agent("WeeChatRadio/0.1")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;

    let mut hits = Vec::new();
    // Prefer providers that tend to see the client's real address family.
    if let Some(h) = try_ipwho_is(&client)? {
        hits.push(h);
    }
    if let Some(h) = try_geojs(&client)? {
        hits.push(h);
    }
    if let Some(h) = try_ipapi_co(&client)? {
        hits.push(h);
    }
    // ip-api.com (HTTP) often geolocates UK residential IPv4 to a distant PoP.
    if let Some(h) = try_ip_api(
        &client,
        "http://ip-api.com/json/?fields=status,lat,lon,city,regionName,country",
        3,
    )? {
        hits.push(h);
    }

    Ok(pick_consensus(hits))
}

fn try_ip_api(client: &reqwest::blocking::Client, url: &str, rank: u8) -> Result<Option<GeoHit>> {
    let v: serde_json::Value = match client.get(url).send() {
        Ok(r) => r.json().unwrap_or(serde_json::Value::Null),
        Err(_) => return Ok(None),
    };
    if v.get("status").and_then(|s| s.as_str()) != Some("success") {
        return Ok(None);
    }
    Ok(geo_hit(
        json_f64(v.get("lat")),
        json_f64(v.get("lon")),
        city_label(&v, &["city", "regionName", "country"]),
        rank,
    ))
}

fn try_ipapi_co(client: &reqwest::blocking::Client) -> Result<Option<GeoHit>> {
    let v: serde_json::Value = match client.get("https://ipapi.co/json/").send() {
        Ok(r) => r.json().unwrap_or(serde_json::Value::Null),
        Err(_) => return Ok(None),
    };
    if v.get("error").and_then(|e| e.as_bool()) == Some(true) {
        return Ok(None);
    }
    Ok(geo_hit(
        json_f64(v.get("latitude")),
        json_f64(v.get("longitude")),
        city_label(&v, &["city", "region", "country_name"]),
        2,
    ))
}

fn try_ipwho_is(client: &reqwest::blocking::Client) -> Result<Option<GeoHit>> {
    let v: serde_json::Value = match client.get("https://ipwho.is/").send() {
        Ok(r) => r.json().unwrap_or(serde_json::Value::Null),
        Err(_) => return Ok(None),
    };
    if v.get("success").and_then(|s| s.as_bool()) == Some(false) {
        return Ok(None);
    }
    Ok(geo_hit(
        json_f64(v.get("latitude")),
        json_f64(v.get("longitude")),
        city_label(&v, &["city", "region", "country"]),
        0,
    ))
}

fn try_geojs(client: &reqwest::blocking::Client) -> Result<Option<GeoHit>> {
    let v: serde_json::Value = match client.get("https://get.geojs.io/v1/ip/geo.json").send() {
        Ok(r) => r.json().unwrap_or(serde_json::Value::Null),
        Err(_) => return Ok(None),
    };
    Ok(geo_hit(
        json_f64(v.get("latitude")),
        json_f64(v.get("longitude")),
        city_label(&v, &["city", "region", "country"]),
        1,
    ))
}

fn geo_hit(lat: Option<f64>, lon: Option<f64>, label: String, rank: u8) -> Option<GeoHit> {
    let (Some(lat), Some(lon)) = (lat, lon) else {
        return None;
    };
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    if lat.abs() < 0.01 && lon.abs() < 0.01 {
        return None;
    }
    Some(GeoHit {
        lat,
        lon,
        label,
        rank,
    })
}

fn city_label(v: &serde_json::Value, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_f64(v: Option<&serde_json::Value>) -> Option<f64> {
    let v = v?;
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_u64().map(|i| i as f64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Keep hits that share the most common 4-character square; average their
/// coordinates. Require agreement when more than one source answered.
fn pick_consensus(hits: Vec<GeoHit>) -> Option<DetectedGrid> {
    if hits.is_empty() {
        return None;
    }

    let mut with_square: Vec<(String, GeoHit)> = Vec::new();
    for h in hits {
        let Ok(g) = from_lat_lon(h.lat, h.lon, 6) else {
            continue;
        };
        let square = g.chars().take(4).collect::<String>();
        with_square.push((square, h));
    }
    if with_square.is_empty() {
        return None;
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (sq, _) in &with_square {
        *counts.entry(sq.clone()).or_default() += 1;
    }
    let (best_sq, best_n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    let total = with_square.len();

    // Several sources and they disagree on the ~100 km square → do not guess.
    if total > 1 && best_n < 2 {
        return None;
    }

    let mut winners: Vec<&GeoHit> = with_square
        .iter()
        .filter(|(sq, _)| *sq == best_sq)
        .map(|(_, h)| h)
        .collect();
    winners.sort_by_key(|h| h.rank);

    let n = winners.len() as f64;
    let lat = winners.iter().map(|h| h.lat).sum::<f64>() / n;
    let lon = winners.iter().map(|h| h.lon).sum::<f64>() / n;
    let grid = from_lat_lon(lat, lon, 6).ok()?;

    let mut cities: Vec<String> = winners
        .iter()
        .filter_map(|h| h.label.split(',').next().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    cities.sort();
    cities.dedup();
    let label = if cities.is_empty() {
        winners[0].label.clone()
    } else {
        cities.join(" / ")
    };

    Some(DetectedGrid { grid, label })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io91_londonish() {
        let (lat, lon) = to_lat_lon("IO91wm").unwrap();
        assert!((51.0..52.5).contains(&lat), "lat {lat}");
        assert!((-1.5..0.5).contains(&lon), "lon {lon}");
    }

    #[test]
    fn london_encodes_io91wm() {
        let grid = from_lat_lon(51.5074, -0.1278, 6).unwrap();
        assert_eq!(grid, "IO91WM");
    }

    #[test]
    fn frome_encodes_io81uf() {
        let grid = from_lat_lon(51.2283, -2.3221, 6).unwrap();
        assert_eq!(&grid[..4], "IO81");
        assert_eq!(grid, "IO81UF");
    }

    #[test]
    fn roundtrip_centroid_stays_in_square() {
        let g = "IO81UF";
        let (lat, lon) = to_lat_lon(g).unwrap();
        assert_eq!(from_lat_lon(lat, lon, 6).unwrap(), g);
    }

    #[test]
    fn consensus_prefers_majority_over_bad_ipv4_pop() {
        // Real-world Frome failure mode: ip-api → Bedford, others → Frome.
        let hits = vec![
            GeoHit {
                lat: 52.1073,
                lon: -0.4649,
                label: "Bedford, England, United Kingdom".into(),
                rank: 3,
            },
            GeoHit {
                lat: 51.50853,
                lon: -0.12574,
                label: "London, England, United Kingdom".into(),
                rank: 2,
            },
            GeoHit {
                lat: 51.228343,
                lon: -2.3221094,
                label: "Frome, England, United Kingdom".into(),
                rank: 0,
            },
            GeoHit {
                lat: 51.2276,
                lon: -2.3278,
                label: "Frome, England, United Kingdom".into(),
                rank: 1,
            },
        ];
        let hit = pick_consensus(hits).expect("majority Frome");
        assert_eq!(&hit.grid[..4], "IO81");
        assert!(hit.label.contains("Frome"), "{}", hit.label);
    }

    #[test]
    fn consensus_refuses_total_disagreement() {
        let hits = vec![
            GeoHit {
                lat: 52.1073,
                lon: -0.4649,
                label: "Bedford".into(),
                rank: 0,
            },
            GeoHit {
                lat: 51.50853,
                lon: -0.12574,
                label: "London".into(),
                rank: 1,
            },
            GeoHit {
                lat: 51.228343,
                lon: -2.3221094,
                label: "Frome".into(),
                rank: 2,
            },
        ];
        assert!(pick_consensus(hits).is_none());
    }

    #[test]
    fn consensus_accepts_single_source() {
        let hits = vec![GeoHit {
            lat: 51.228343,
            lon: -2.3221094,
            label: "Frome".into(),
            rank: 0,
        }];
        let hit = pick_consensus(hits).unwrap();
        assert_eq!(hit.grid, "IO81UF");
    }
}
