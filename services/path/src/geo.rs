//! SPDX-License-Identifier: Apache-2.0
//! Distance, Maidenhead grids, and calling-frequency regions.

use std::f64::consts::PI;

const EARTH_KM: f64 = 6371.0;

/// Great-circle distance in kilometres.
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lon1) = (a.0.to_radians(), a.1.to_radians());
    let (lat2, lon2) = (b.0.to_radians(), b.1.to_radians());
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_KM * h.sqrt().asin().min(1.0)
}

/// Region key used to pick calling frequencies from the published list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    UsCa,
    UkEu,
    Au,
    Nz,
    Other,
}

impl Region {
    pub fn from_country(code: &str) -> Self {
        match code.trim().to_ascii_uppercase().as_str() {
            "US" | "CA" => Self::UsCa,
            "AU" => Self::Au,
            "NZ" => Self::Nz,
            c if is_uk_eu(c) => Self::UkEu,
            _ => Self::Other,
        }
    }

    /// Shared 2m FM calling region, if both ends map to one.
    pub fn shared_vhf(a: Self, b: Self) -> Option<Self> {
        match (a, b) {
            (Self::UsCa, Self::UsCa) => Some(Self::UsCa),
            (Self::UkEu, Self::UkEu) => Some(Self::UkEu),
            (Self::Au, Self::Au) => Some(Self::Au),
            _ => None,
        }
    }

    /// Shared 40m spot region, if both ends agree.
    pub fn shared_40m(a: Self, b: Self) -> Option<&'static str> {
        match (a, b) {
            (Self::UkEu, Self::UkEu) => Some("uk_eu"),
            (Self::UsCa, Self::UsCa)
            | (Self::Au, Self::Au)
            | (Self::Nz, Self::Nz)
            | (Self::UsCa, Self::Au)
            | (Self::Au, Self::UsCa)
            | (Self::UsCa, Self::Nz)
            | (Self::Nz, Self::UsCa)
            | (Self::Au, Self::Nz)
            | (Self::Nz, Self::Au) => Some("us_au_nz"),
            _ => None,
        }
    }
}

fn is_uk_eu(code: &str) -> bool {
    matches!(
        code,
        "GB" | "UK"
            | "IE"
            | "AT"
            | "BE"
            | "BG"
            | "HR"
            | "CY"
            | "CZ"
            | "DK"
            | "EE"
            | "FI"
            | "FR"
            | "DE"
            | "GR"
            | "HU"
            | "IT"
            | "LV"
            | "LT"
            | "LU"
            | "MT"
            | "NL"
            | "PL"
            | "PT"
            | "RO"
            | "SK"
            | "SI"
            | "ES"
            | "SE"
            | "IS"
            | "NO"
            | "CH"
            | "LI"
            | "MC"
            | "AD"
            | "SM"
            | "VA"
            | "AL"
            | "BA"
            | "ME"
            | "MK"
            | "RS"
            | "XK"
            | "UA"
            | "MD"
            | "BY"
    )
}

/// Centroid of a Maidenhead square. Same rules as `wcr` grid helpers.
pub fn grid_to_lat_lon(grid: &str) -> Option<(f64, f64)> {
    let g = grid.trim().to_ascii_uppercase();
    if ![2, 4, 6, 8].contains(&g.len()) {
        return None;
    }
    let b = g.as_bytes();
    if !b[0].is_ascii_uppercase() || !b[1].is_ascii_uppercase() {
        return None;
    }
    if g.len() >= 4 && (!b[2].is_ascii_digit() || !b[3].is_ascii_digit()) {
        return None;
    }
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
        if !(b[4].is_ascii_uppercase() && b[5].is_ascii_uppercase()) {
            return None;
        }
        lon += (b[4] - b'A') as f64 * (2.0 / 24.0);
        lat += (b[5] - b'A') as f64 * (1.0 / 24.0);
        lon_span = 2.0 / 24.0;
        lat_span = 1.0 / 24.0;
    }
    if g.len() >= 8 {
        if !(b[6].is_ascii_digit() && b[7].is_ascii_digit()) {
            return None;
        }
        lon += (b[6] - b'0') as f64 * (2.0 / 24.0 / 10.0);
        lat += (b[7] - b'0') as f64 * (1.0 / 24.0 / 10.0);
        lon_span = 2.0 / 24.0 / 10.0;
        lat_span = 1.0 / 24.0 / 10.0;
    }
    Some((lat + lat_span / 2.0, lon + lon_span / 2.0))
}

/// True when `q` looks like a Maidenhead locator rather than a place name.
pub fn looks_like_grid(q: &str) -> bool {
    let g = q.trim().to_ascii_uppercase();
    if ![2, 4, 6, 8].contains(&g.len()) {
        return false;
    }
    let b = g.as_bytes();
    b[0].is_ascii_uppercase()
        && b[1].is_ascii_uppercase()
        && (g.len() < 4 || (b[2].is_ascii_digit() && b[3].is_ascii_digit()))
        && (g.len() < 6 || (b[4].is_ascii_uppercase() && b[5].is_ascii_uppercase()))
        && (g.len() < 8 || (b[6].is_ascii_digit() && b[7].is_ascii_digit()))
}

/// Intermediate points along the short-path great circle (lon, lat) for the map.
#[allow(dead_code)]
pub fn great_circle_points(a: (f64, f64), b: (f64, f64), n: usize) -> Vec<[f64; 2]> {
    let n = n.max(2);
    let lat1 = a.0.to_radians();
    let lon1 = a.1.to_radians();
    let lat2 = b.0.to_radians();
    let lon2 = b.1.to_radians();
    let d = 2.0
        * (((lat2 - lat1) / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * ((lon2 - lon1) / 2.0).sin().powi(2))
        .sqrt()
        .asin();
    if d < 1e-9 {
        return vec![[a.1, a.0], [b.1, b.0]];
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let f = i as f64 / (n - 1) as f64;
        let a_coef = ((1.0 - f) * d).sin() / d.sin();
        let b_coef = (f * d).sin() / d.sin();
        let x = a_coef * lat1.cos() * lon1.cos() + b_coef * lat2.cos() * lon2.cos();
        let y = a_coef * lat1.cos() * lon1.sin() + b_coef * lat2.cos() * lon2.sin();
        let z = a_coef * lat1.sin() + b_coef * lat2.sin();
        let lat = z.atan2((x * x + y * y).sqrt());
        let lon = y.atan2(x);
        out.push([lon * 180.0 / PI, lat * 180.0 / PI]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn london_new_york_distance() {
        let d = distance_km((51.5, -0.1), (40.7, -74.0));
        assert!((5500.0..5800.0).contains(&d), "got {d}");
    }

    #[test]
    fn close_pair() {
        let d = distance_km((51.50, -0.12), (51.52, -0.10));
        assert!(d < 50.0, "got {d}");
    }

    #[test]
    fn grid_fn31() {
        let (lat, lon) = grid_to_lat_lon("FN31").unwrap();
        assert!((41.0..42.0).contains(&lat));
        assert!((-73.0..-72.0).contains(&lon));
    }

    #[test]
    fn region_uk() {
        assert_eq!(Region::from_country("GB"), Region::UkEu);
        assert_eq!(Region::from_country("DE"), Region::UkEu);
        assert_eq!(Region::from_country("US"), Region::UsCa);
    }
}
