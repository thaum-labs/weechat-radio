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

/// The last fix and when it came, for the range test log.
static LAST_FIX: Mutex<Option<(f64, f64, Instant)>> = Mutex::new(None);

/// Where the receiver last put us, while that fix is fresh.
pub fn position() -> Option<(f64, f64)> {
    LAST_FIX
        .lock()
        .filter(|(_, _, at)| at.elapsed() <= FIX_STALE)
        .map(|(lat, lon, _)| (lat, lon))
}

/// What one NMEA line says about the position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Nmea {
    Fix { lat: f64, lon: f64 },
    NoFix,
}

/// The fields of one NMEA line, after its checksum is checked. `None` for
/// noise and lines with a bad checksum.
fn nmea_fields(line: &str) -> Option<Vec<&str>> {
    let body = line.trim().strip_prefix('$')?;
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
    Some(body.split(',').collect())
}

/// Reads an RMC or GGA sentence from any talker (GP, GN, GL…). Other
/// sentences and lines with a bad checksum return `None`.
pub fn parse_nmea(line: &str) -> Option<Nmea> {
    let f = nmea_fields(line)?;
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

/// What one NMEA line says about satellites.
#[derive(Debug, Clone, PartialEq)]
pub enum SatCount {
    /// GGA: satellites used for the fix.
    InUse(u32),
    /// GSV: satellites one system (talker GP, GL, GA, GB…) can see.
    InView { talker: String, count: u32 },
}

/// Reads the satellite count from a GGA or GSV sentence.
pub fn parse_sats(line: &str) -> Option<SatCount> {
    let f = nmea_fields(line)?;
    let head = f.first()?;
    let talker = head.get(..2)?;
    match head.get(2..)? {
        // $GPGGA,time,lat,N,lon,E,quality,satellites,...
        "GGA" => Some(SatCount::InUse(f.get(7)?.parse().ok()?)),
        // $GPGSV,messages,number,satellites in view,...
        "GSV" => Some(SatCount::InView {
            talker: talker.to_string(),
            count: f.get(3)?.parse().ok()?,
        }),
        _ => None,
    }
}

/// Satellite counts for the status line.
#[derive(Debug, Default)]
struct Sats {
    in_use: Option<u32>,
    /// Satellites in view per system; the receiver reports each one separately.
    in_view: std::collections::BTreeMap<String, u32>,
}

impl Sats {
    fn take(&mut self, c: SatCount) {
        match c {
            SatCount::InUse(n) => self.in_use = Some(n),
            SatCount::InView { talker, count } => {
                self.in_view.insert(talker, count);
            }
        }
    }

    fn seen(&self) -> Option<u32> {
        (!self.in_view.is_empty()).then(|| self.in_view.values().sum())
    }

    /// Text after "waiting for a fix", e.g. ", 3 satellites seen".
    fn waiting_note(&self) -> String {
        match self.seen() {
            Some(n) => format!(", {}", plural(n, "satellite seen", "satellites seen")),
            None => String::new(),
        }
    }

    /// Text after the fix grid, e.g. ", 8 satellites".
    fn fix_note(&self) -> String {
        match self.in_use.filter(|n| *n > 0) {
            Some(n) => format!(", {}", plural(n, "satellite", "satellites")),
            None => String::new(),
        }
    }
}

/// Traffic light for the GPS status line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    /// No satellites heard, or the receiver is not answering.
    Red,
    /// Satellites seen but no fix yet.
    Amber,
    /// Position fix: the range test logs where you are.
    Green,
}

/// Which light a GPS status line shows.
pub fn light(state: &str) -> Light {
    let state = state.trim_start_matches("GPS: ");
    if state.starts_with("fix ") && !state.starts_with("fix lost") {
        return Light::Green;
    }
    let seen = state
        .rsplit_once(", ")
        .and_then(|(_, tail)| tail.split(' ').next()?.parse::<u32>().ok())
        .unwrap_or(0);
    if state.starts_with("waiting for a fix") && seen > 0 {
        Light::Amber
    } else {
        Light::Red
    }
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
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
    let mut sats = Sats::default();
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
                if let Some(c) = parse_sats(&text) {
                    sats.take(c);
                }
                match parse_nmea(&text) {
                    Some(Nmea::Fix { lat, lon }) => {
                        if let Ok(g) = crate::grid::from_lat_lon(lat, lon, GRID_LEN) {
                            *last_fix = Some(Instant::now());
                            *LAST_FIX.lock() = Some((lat, lon, Instant::now()));
                            let mut s = snap.lock();
                            s.grid = g.clone();
                            s.gps = format!("GPS: fix {g}{}", sats.fix_note());
                        }
                    }
                    Some(Nmea::NoFix) if last_fix.is_none_or(|t| t.elapsed() > FIX_STALE) => {
                        set_state(
                            snap,
                            format!("GPS: waiting for a fix{}", sats.waiting_note()),
                        );
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
    fn satellites_in_use_and_in_view() {
        assert_eq!(
            parse_sats("$GNGGA,092750.000,5321.6802,N,00630.3372,W,1,8,1.03,61.7,M,55.2,M,,"),
            Some(SatCount::InUse(8))
        );
        assert_eq!(
            parse_sats("$GPGGA,123519,,,,,0,00,,,M,,M,,"),
            Some(SatCount::InUse(0))
        );
        let mut sats = Sats::default();
        assert_eq!(sats.waiting_note(), "", "nothing heard yet");
        for line in [
            "$GPGSV,3,1,11,03,03,111,00,04,15,270,00,06,01,010,00,13,06,292,00",
            "$GPGSV,3,2,11,14,25,170,00,16,57,208,39,18,67,296,40,19,40,246,00",
            "$GLGSV,1,1,03,65,30,100,20,66,40,200,22,67,10,300,",
            "$GPRMC,123519,V,,,,,,,230394,,,N",
        ] {
            if let Some(c) = parse_sats(line) {
                sats.take(c);
            }
        }
        assert_eq!(sats.waiting_note(), ", 14 satellites seen");
        sats.take(SatCount::InView {
            talker: "GP".into(),
            count: 0,
        });
        sats.take(SatCount::InView {
            talker: "GL".into(),
            count: 1,
        });
        assert_eq!(sats.waiting_note(), ", 1 satellite seen");
        assert_eq!(sats.fix_note(), "");
        sats.take(SatCount::InUse(7));
        assert_eq!(sats.fix_note(), ", 7 satellites");
    }

    #[test]
    fn traffic_light_follows_the_status_line() {
        assert_eq!(light("GPS: waiting for a fix"), Light::Red);
        assert_eq!(
            light("GPS: waiting for a fix, 0 satellites seen"),
            Light::Red
        );
        assert_eq!(
            light("GPS: waiting for a fix, 1 satellite seen"),
            Light::Amber
        );
        assert_eq!(
            light("GPS: waiting for a fix, 9 satellites seen"),
            Light::Amber
        );
        assert_eq!(light("GPS: fix IO81UF12, 8 satellites"), Light::Green);
        assert_eq!(light("GPS: fix IO81UF12"), Light::Green);
        assert_eq!(light("GPS: cannot open /dev/cu.x: busy"), Light::Red);
        assert_eq!(
            light("GPS: fix lost, using the grid from setup"),
            Light::Red
        );
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
