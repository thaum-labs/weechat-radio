//! SPDX-License-Identifier: Apache-2.0
//! Maidenhead grid squares to lat/lon centroids.

use crate::error::{Error, Result};

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

/// Look up an approximate Maidenhead square from public IP geolocation,
/// matching Ceefax Station (`ip-api.com`, then `ipapi.co`).
pub fn detect_from_ip() -> Option<DetectedGrid> {
    detect_from_ip_inner().ok().flatten()
}

fn detect_from_ip_inner() -> Result<Option<DetectedGrid>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .user_agent("WeeChatRadio/0.1")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    if let Some(hit) = try_ip_api(
        &client,
        "http://ip-api.com/json/?fields=status,lat,lon,city,regionName,country",
    )? {
        return Ok(Some(hit));
    }
    if let Some(hit) = try_ipapi_co(&client)? {
        return Ok(Some(hit));
    }
    try_ip_api(&client, "https://ip-api.com/json/")
}

fn try_ip_api(client: &reqwest::blocking::Client, url: &str) -> Result<Option<DetectedGrid>> {
    let v: serde_json::Value = match client.get(url).send() {
        Ok(r) => r.json().unwrap_or(serde_json::Value::Null),
        Err(_) => return Ok(None),
    };
    if v.get("status").and_then(|s| s.as_str()) != Some("success") {
        return Ok(None);
    }
    let lat = json_f64(v.get("lat"));
    let lon = json_f64(v.get("lon"));
    pack_detected(lat, lon, city_label(&v, &["city", "regionName", "country"]))
}

fn try_ipapi_co(client: &reqwest::blocking::Client) -> Result<Option<DetectedGrid>> {
    let v: serde_json::Value = match client.get("https://ipapi.co/json/").send() {
        Ok(r) => r.json().unwrap_or(serde_json::Value::Null),
        Err(_) => return Ok(None),
    };
    let lat = json_f64(v.get("latitude"));
    let lon = json_f64(v.get("longitude"));
    pack_detected(
        lat,
        lon,
        city_label(&v, &["city", "region", "country_name"]),
    )
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

fn pack_detected(
    lat: Option<f64>,
    lon: Option<f64>,
    label: String,
) -> Result<Option<DetectedGrid>> {
    let (Some(lat), Some(lon)) = (lat, lon) else {
        return Ok(None);
    };
    let grid = from_lat_lon(lat, lon, 6)?;
    Ok(Some(DetectedGrid { grid, label }))
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
    fn roundtrip_centroid_stays_in_square() {
        let g = "IO81UF";
        let (lat, lon) = to_lat_lon(g).unwrap();
        assert_eq!(from_lat_lon(lat, lon, 6).unwrap(), g);
    }
}
