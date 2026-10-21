//! SPDX-License-Identifier: Apache-2.0
//! USB GPS receiver: a moving station's position from NMEA sentences.
//!
//! Any receiver that shows up as a serial port and sends standard NMEA text
//! works (u-blox USB dongles, radios with a GPS output). While it has a fix,
//! the station's grid is the 8-character square it is in (about 500 m); when
//! the fix is lost for two minutes the grid from setup comes back.

use crate::config::Config;
use crate::status::SharedStatus;
use parking_lot::Mutex;
use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Grid length sent over the air and shown on the map.
pub const GRID_LEN: usize = 8;
/// A fix older than this no longer counts.
const FIX_STALE: Duration = Duration::from_secs(120);
/// Wait before opening the port again after it failed or was unplugged.
const REOPEN_WAIT: Duration = Duration::from_secs(10);

/// What one NMEA line says about the position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nmea {
    Fix { lat: f64, lon: f64 },
    NoFix,
}

/// Reads an RMC or GGA sentence from any talker (GP, GN, GL…). Other
/// sentences and lines with a bad checksum return `None`.
pub fn parse_nmea(line: &str) -> Option<Nmea> {
    let line = line.trim();
    let body = line.strip_prefix('$')?;
    let (body, sum) = match body.split_once('*') {
        Some((b, s)) => (b, Some(s)),
        None => (body, None),
    };
    if let Some(sum) = sum {
        let want = u8::from_str_radix(sum.get(..2)?, 16).ok()?;
        let got = body.bytes().fold(0u8, |a, b| a ^ b);
        if want != got {
            return None;
        }
    }
    let f: Vec<&str> = body.split(',').collect();
    let kind = f.first()?.get(2..)?;
    let (ok, lat_i) = match kind {
        // $GPRMC,time,status,lat,N,lon,E,...  status A = valid
        "RMC" => (f.get(2)? == &"A", 3),
        // $GPGGA,time,lat,N,lon,E,quality,...  quality 0 = no fix
        "GGA" => (f.get(6).is_some_and(|q| !q.is_empty() && *q != "0"), 2),
        _ => return None,
    };
    if !ok {
        return Some(Nmea::NoFix);
    }
    let lat = degrees(f.get(lat_i)?, f.get(lat_i + 1)?, 2)?;
    let lon = degrees(f.get(lat_i + 2)?, f.get(lat_i + 3)?, 3)?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return Some(Nmea::NoFix);
    }
    Some(Nmea::Fix { lat, lon })
}

/// NMEA `ddmm.mmmm` (or `dddmm.mmmm`) plus hemisphere to signed degrees.
fn degrees(value: &str, hemi: &str, deg_digits: usize) -> Option<f64> {
    if value.len() <= deg_digits {
        return None;
    }
    let d: f64 = value.get(..deg_digits)?.parse().ok()?;
    let m: f64 = value.get(deg_digits..)?.parse().ok()?;
    let v = d + m / 60.0;
    match hemi {
        "N" | "E" => Some(v),
        "S" | "W" => Some(-v),
        _ => None,
    }
}

/// Reads the receiver for as long as the station runs. Keeps `snap.grid` on
/// the current square and `snap.gps` on a short state line.
pub async fn run(cfg: Arc<Mutex<Config>>, snap: Arc<SharedStatus>) {
    let mut last_fix: Option<Instant> = None;
    loop {
        let (port, baud) = {
            let c = cfg.lock();
            (c.gps.port.trim().to_string(), c.gps.baud)
        };
        if port.is_empty() {
            return;
        }
        let file = match tokio::task::spawn_blocking({
            let port = port.clone();
            move || crate::tnc::serial::open_raw(&port, baud)
        })
        .await
        {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => {
                set_state(&snap, format!("GPS: cannot open {port}: {e}"));
                fall_back(&cfg, &snap, &mut last_fix);
                tokio::time::sleep(REOPEN_WAIT).await;
                continue;
            }
            Err(_) => return,
        };
        tracing::info!("GPS: reading {port} at {baud} baud");
        set_state(&snap, "GPS: waiting for a fix".into());
        read_port(file, &cfg, &snap, &mut last_fix).await;
        set_state(&snap, format!("GPS: {port} stopped answering"));
        fall_back(&cfg, &snap, &mut last_fix);
        tokio::time::sleep(REOPEN_WAIT).await;
    }
}

/// Returns when the port fails or sends nothing for a minute (unplugged).
async fn read_port(
    mut file: std::fs::File,
    cfg: &Arc<Mutex<Config>>,
    snap: &Arc<SharedStatus>,
    last_fix: &mut Option<Instant>,
) {
    let mut line = Vec::new();
    let mut last_byte = Instant::now();
    loop {
        // Each read waits at most 200 ms, so a stopped station frees the port quickly.
        let res = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 512];
            let r = file.read(&mut buf).map(|n| buf[..n].to_vec());
            (file, r)
        })
        .await;
        let Ok((f, r)) = res else {
            return;
        };
        file = f;
        let bytes = match r {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Vec::new(),
            Err(_) => return,
        };
        if bytes.is_empty() {
            if last_byte.elapsed() > Duration::from_secs(60) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        } else {
            last_byte = Instant::now();
        }
        for b in bytes {
            if b == b'\n' {
                let text = String::from_utf8_lossy(&line).into_owned();
                line.clear();
                match parse_nmea(&text) {
                    Some(Nmea::Fix { lat, lon }) => {
                        if let Ok(g) = crate::grid::from_lat_lon(lat, lon, GRID_LEN) {
                            *last_fix = Some(Instant::now());
                            let mut s = snap.lock();
                            s.grid = g.clone();
                            s.gps = format!("GPS: fix {g}");
                        }
                    }
                    Some(Nmea::NoFix) if last_fix.is_none_or(|t| t.elapsed() > FIX_STALE) => {
                        set_state(snap, "GPS: waiting for a fix".into());
                    }
                    _ => {}
                }
            } else if line.len() < 120 {
                line.push(b);
            }
        }
        if last_fix.is_some_and(|t| t.elapsed() > FIX_STALE) {
            fall_back(cfg, snap, last_fix);
            set_state(snap, "GPS: fix lost, using the grid from setup".into());
        }
    }
}

fn set_state(snap: &SharedStatus, text: String) {
    snap.lock().gps = text;
}

/// Back to the grid typed in setup.
fn fall_back(cfg: &Mutex<Config>, snap: &SharedStatus, last_fix: &mut Option<Instant>) {
    *last_fix = None;
    let grid = cfg.lock().grid.clone();
    snap.lock().grid = grid;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rmc_with_a_fix_gives_the_position() {
        let n = parse_nmea("$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A")
            .unwrap();
        let Nmea::Fix { lat, lon } = n else {
            panic!("expected a fix, got {n:?}");
        };
        assert!((lat - 48.1173).abs() < 0.001, "lat {lat}");
        assert!((lon - 11.5167).abs() < 0.001, "lon {lon}");
    }

    #[test]
    fn gga_from_any_talker_west_and_south() {
        let n = parse_nmea("$GNGGA,092750.000,5321.6802,N,00630.3372,W,1,8,1.03,61.7,M,55.2,M,,")
            .unwrap();
        let Nmea::Fix { lat, lon } = n else {
            panic!("expected a fix, got {n:?}");
        };
        assert!((lat - 53.3613).abs() < 0.001, "lat {lat}");
        assert!((lon + 6.5056).abs() < 0.001, "lon {lon}");
        let n = parse_nmea("$GPGGA,000000,3351.000,S,15112.000,E,1,05,1.0,0,M,0,M,,").unwrap();
        assert!(matches!(n, Nmea::Fix { lat, .. } if lat < -33.0));
    }

    #[test]
    fn no_fix_and_bad_lines() {
        assert_eq!(
            parse_nmea("$GPRMC,123519,V,,,,,,,230394,,,N"),
            Some(Nmea::NoFix)
        );
        assert_eq!(
            parse_nmea("$GPGGA,123519,,,,,0,00,,,M,,M,,"),
            Some(Nmea::NoFix)
        );
        // Bad checksum, other sentence, noise.
        assert_eq!(
            parse_nmea("$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*00"),
            None
        );
        assert_eq!(parse_nmea("$GPGSV,3,1,11,03,03,111,00*74"), None);
        assert_eq!(parse_nmea("garbage"), None);
    }

    #[test]
    fn a_fix_becomes_an_eight_character_grid() {
        let Some(Nmea::Fix { lat, lon }) =
            parse_nmea("$GPGGA,120000,5130.000,N,00007.500,W,1,08,1.0,10,M,47,M,,")
        else {
            panic!("no fix");
        };
        let g = crate::grid::from_lat_lon(lat, lon, GRID_LEN).unwrap();
        assert_eq!(g.len(), 8);
        assert!(g.to_ascii_uppercase().starts_with("IO91WM"), "{g}");
    }
}
