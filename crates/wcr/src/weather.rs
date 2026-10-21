//! SPDX-License-Identifier: Apache-2.0
//! `/wx`: a one-line weather report for this station's grid square.
//!
//! Current conditions come from Open-Meteo (free, no key). Only the sending
//! station needs internet; the line itself goes out like any chat line.

use crate::error::{Error, Result};
use serde_json::Value;

const OPEN_METEO: &str = "https://api.open-meteo.com/v1/forecast";

/// Fetches current conditions at the centre of `grid` and returns the line.
pub async fn report(grid: &str) -> Result<String> {
    let grid = crate::grid::normalize(grid)?;
    let (lat, lon) = crate::grid::to_lat_lon(&grid)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let resp = client
        .get(OPEN_METEO)
        .query(&[
            ("latitude", format!("{lat:.3}")),
            ("longitude", format!("{lon:.3}")),
            (
                "current",
                "temperature_2m,weather_code,wind_speed_10m,wind_direction_10m".into(),
            ),
            ("wind_speed_unit", "kmh".into()),
        ])
        .send()
        .await
        .map_err(|e| Error::Net(format!("weather service unreachable: {e}")))?;
    if !resp.status().is_success() {
        return Err(Error::Net(format!(
            "weather service answered {}",
            resp.status()
        )));
    }
    let v: Value = resp.json().await.map_err(|e| Error::Net(e.to_string()))?;
    format_line(&grid, &v)
        .ok_or_else(|| Error::Net("weather service sent no current conditions".into()))
}

/// `WX IO81UF 20:40Z: 12C, light rain, wind SW 15 km/h`
pub fn format_line(grid: &str, v: &Value) -> Option<String> {
    let cur = v.get("current")?;
    let temp = cur.get("temperature_2m")?.as_f64()?;
    let code = cur
        .get("weather_code")
        .and_then(|x| x.as_i64())
        .unwrap_or(-1);
    let wind = cur
        .get("wind_speed_10m")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    let dir = cur
        .get("wind_direction_10m")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0);
    // Open-Meteo reports times in UTC as "2026-10-21T20:45".
    let time = cur
        .get("time")
        .and_then(|x| x.as_str())
        .and_then(|t| t.split_once('T'))
        .map(|(_, hm)| format!(" {hm}Z"))
        .unwrap_or_default();
    Some(format!(
        "WX {grid}{time}: {temp:.0}C, {}, wind {} {wind:.0} km/h",
        conditions(code),
        compass(dir)
    ))
}

/// WMO weather code in a few words.
pub fn conditions(code: i64) -> &'static str {
    match code {
        0 => "clear",
        1 => "mostly clear",
        2 => "partly cloudy",
        3 => "overcast",
        45 | 48 => "fog",
        51 | 53 | 55 => "drizzle",
        56 | 57 => "freezing drizzle",
        61 => "light rain",
        63 => "rain",
        65 => "heavy rain",
        66 | 67 => "freezing rain",
        71 => "light snow",
        73 => "snow",
        75 => "heavy snow",
        77 => "snow grains",
        80 => "light showers",
        81 => "showers",
        82 => "heavy showers",
        85 | 86 => "snow showers",
        95 => "thunderstorm",
        96 | 99 => "thunderstorm with hail",
        _ => "conditions unknown",
    }
}

/// Wind direction (degrees, where it blows from) as one of 8 points.
pub fn compass(deg: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let i = ((deg.rem_euclid(360.0) + 22.5) / 45.0) as usize % 8;
    POINTS[i]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_open_meteo_current() {
        let v = serde_json::json!({
            "current": {
                "time": "2026-10-21T20:45",
                "temperature_2m": 11.6,
                "weather_code": 61,
                "wind_speed_10m": 14.8,
                "wind_direction_10m": 230.0
            }
        });
        assert_eq!(
            format_line("IO81UF", &v).unwrap(),
            "WX IO81UF 20:45Z: 12C, light rain, wind SW 15 km/h"
        );
        assert!(format_line("IO81UF", &serde_json::json!({})).is_none());
    }

    #[test]
    fn compass_points() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(350.0), "N");
        assert_eq!(compass(90.0), "E");
        assert_eq!(compass(225.0), "SW");
        assert_eq!(compass(-45.0), "NW");
    }
}
