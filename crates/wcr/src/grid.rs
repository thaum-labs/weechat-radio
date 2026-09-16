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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io91_londonish() {
        let (lat, lon) = to_lat_lon("IO91wm").unwrap();
        assert!((51.0..52.5).contains(&lat), "lat {lat}");
        assert!((-1.5..0.5).contains(&lon), "lon {lon}");
    }
}
