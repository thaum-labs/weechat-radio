//! SPDX-License-Identifier: Apache-2.0
//! Range test: a moving station (a car) sends one fixed station a short
//! probe line every so often and logs, for each probe, where it was and
//! whether each direction got through.
//!
//! No change is needed at the far end. The probe is an ordinary private
//! line kept on the radio, and every station already answers a private line
//! with a receipt that carries the SNR it heard the line at. So:
//!
//! - a receipt back = both directions work (their SNR from the receipt,
//!   ours from decoding the receipt);
//! - no receipt, but we decoded some other frame from them in the window =
//!   we hear them, they may not hear us;
//! - nothing = no contact.
//!
//! Probes are not retried: a retry would hide where the first try failed.
//! Each row goes to a CSV file, and a KML file is rewritten next to it, in
//! the received-files folder. The exact GPS position never goes on the air.

use crate::proto::{Envelope, MsgId, MsgType};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Seconds between probes unless the user picks another gap.
pub const DEFAULT_EVERY_S: u32 = 30;
/// A probe and its receipt take a few seconds of air on VHF FM.
pub const MIN_EVERY_S: u32 = 15;
pub const MAX_EVERY_S: u32 = 600;

/// The text of a probe line, as the far station shows it.
pub fn probe_text(n: u32) -> String {
    format!("range test probe {n} (automatic, no reply needed)")
}

/// CSV header. One row per probe.
pub const CSV_HEADER: &str =
    "time_utc,lat,lon,probe,result,snr_they_heard_db,snr_we_heard_db,far_station,far_grid";

/// What one probe found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Their receipt came back: both directions work.
    Both,
    /// No receipt, but we decoded another frame from them in the window.
    HeardThem,
    /// Nothing either way.
    NoContact,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Both => "both",
            Outcome::HeardThem => "heard_them",
            Outcome::NoContact => "none",
        }
    }

    /// One plain phrase for the chat notice.
    fn words(self) -> &'static str {
        match self {
            Outcome::Both => "both ways",
            Outcome::HeardThem => "we hear them, no receipt",
            Outcome::NoContact => "no contact",
        }
    }
}

/// One finished probe.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Unix seconds when the probe was sent.
    pub ts: i64,
    /// Where we were when it was sent; `None` without a GPS fix.
    pub pos: Option<(f64, f64)>,
    pub probe: u32,
    pub outcome: Outcome,
    /// The SNR the far station heard our probe at (from its receipt).
    pub snr_they_heard: Option<f32>,
    /// The SNR we decoded their receipt (or other frame) at.
    pub snr_we_heard: Option<f32>,
    /// The grid square the far station's receipt carried, if any.
    pub far_grid: String,
}

/// A probe on the air, waiting for its receipt.
#[derive(Debug, Clone)]
struct Pending {
    id: MsgId,
    ts: i64,
    pos: Option<(f64, f64)>,
    probe: u32,
    receipt: Option<(Option<f32>, Option<f32>, String)>,
    /// SNR of the last other frame from the far station in this window.
    heard_snr: Option<Option<f32>>,
}

impl Pending {
    fn finish(self) -> Row {
        let (outcome, they, we, grid) = match (self.receipt, self.heard_snr) {
            (Some((they, we, grid)), _) => (Outcome::Both, they, we, grid),
            (None, Some(we)) => (Outcome::HeardThem, None, we, String::new()),
            (None, None) => (Outcome::NoContact, None, None, String::new()),
        };
        Row {
            ts: self.ts,
            pos: self.pos,
            probe: self.probe,
            outcome,
            snr_they_heard: they,
            snr_we_heard: we,
            far_grid: grid,
        }
    }
}

/// A running test.
#[derive(Debug)]
struct Session {
    far: String,
    every_s: u32,
    csv: PathBuf,
    kml: PathBuf,
    next_probe: u32,
    next_due: i64,
    pending: Option<Pending>,
    rows: Vec<Row>,
}

/// Range test state for one station. At most one test runs at a time.
#[derive(Debug, Default)]
pub struct RangeTest {
    session: Option<Session>,
}

impl RangeTest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn running(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.far.as_str())
    }

    /// Starts a test against `far`, logging into `dir`. Returns the CSV path.
    /// A test already running is stopped first (its log stays).
    pub fn start(
        &mut self,
        far: &str,
        every_s: u32,
        dir: &Path,
        now: i64,
    ) -> std::io::Result<PathBuf> {
        let _ = self.stop();
        std::fs::create_dir_all(dir)?;
        let far = far.to_ascii_uppercase();
        let stamp = chrono::DateTime::from_timestamp(now, 0)
            .map(|t| t.format("%Y%m%d-%H%M%S").to_string())
            .unwrap_or_else(|| now.to_string());
        let base = format!("range-test-{}-{stamp}", file_safe(&far));
        let csv = dir.join(format!("{base}.csv"));
        let kml = dir.join(format!("{base}.kml"));
        std::fs::write(&csv, format!("{CSV_HEADER}\n"))?;
        std::fs::write(&kml, kml_text(&far, &[]))?;
        self.session = Some(Session {
            far,
            every_s: every_s.clamp(MIN_EVERY_S, MAX_EVERY_S),
            csv: csv.clone(),
            kml,
            next_probe: 1,
            next_due: now,
            pending: None,
            rows: Vec::new(),
        });
        Ok(csv)
    }

    /// Stops the test, logging the probe still waiting. Returns a summary.
    pub fn stop(&mut self) -> Option<String> {
        let mut s = self.session.take()?;
        if let Some(p) = s.pending.take() {
            let row = p.finish();
            write_row(&mut s, row);
        }
        Some(summary(&s))
    }

    /// A short line on the test, for `/rangetest` with no argument.
    pub fn status(&self) -> Option<String> {
        self.session.as_ref().map(summary)
    }

    /// When a probe is due: logs the one before it and returns the far
    /// station, the probe number and the finished row (for a chat notice).
    pub fn due(&mut self, now: i64) -> Option<(String, u32, Option<Row>)> {
        let s = self.session.as_mut()?;
        if now < s.next_due {
            return None;
        }
        s.next_due = now + s.every_s as i64;
        let n = s.next_probe;
        s.next_probe += 1;
        let done = s.pending.take().map(Pending::finish);
        if let Some(row) = &done {
            write_row(s, row.clone());
        }
        Some((s.far.clone(), n, done))
    }

    /// The probe went to the radio as line `id`.
    pub fn probe_sent(&mut self, id: MsgId, probe: u32, ts: i64, pos: Option<(f64, f64)>) {
        if let Some(s) = self.session.as_mut() {
            s.pending = Some(Pending {
                id,
                ts,
                pos,
                probe,
                receipt: None,
                heard_snr: None,
            });
        }
    }

    /// A frame decoded on the radio at `snr`.
    pub fn heard(&mut self, env: &Envelope, snr: Option<f32>) {
        let Some(s) = self.session.as_mut() else {
            return;
        };
        if !env.origin.as_str().eq_ignore_ascii_case(&s.far) {
            return;
        }
        let Some(p) = s.pending.as_mut() else {
            return;
        };
        if env.kind == MsgType::Ack && env.acked_id() == Some(p.id) {
            p.receipt = Some((env.acked_snr(), snr, env.acked_grid().unwrap_or_default()));
        } else {
            p.heard_snr = Some(snr);
        }
    }
}

/// The chat notice for one finished probe.
pub fn row_notice(far: &str, row: &Row) -> String {
    let mut t = format!(
        "range test probe {} to {far}: {}",
        row.probe,
        row.outcome.words()
    );
    if let Some(db) = row.snr_they_heard {
        t.push_str(&format!(", they heard us at {db:.0} dB"));
    }
    if let Some(db) = row.snr_we_heard {
        t.push_str(&format!(", we heard them at {db:.0} dB"));
    }
    if row.pos.is_none() {
        t.push_str(" (no GPS fix, not on the map)");
    }
    t
}

fn summary(s: &Session) -> String {
    let n = |o: Outcome| s.rows.iter().filter(|r| r.outcome == o).count();
    format!(
        "range test to {} every {} s: {} probes logged ({} both ways, {} heard them only, {} no contact). Log: {}",
        s.far,
        s.every_s,
        s.rows.len(),
        n(Outcome::Both),
        n(Outcome::HeardThem),
        n(Outcome::NoContact),
        s.csv.display()
    )
}

fn write_row(s: &mut Session, row: Row) {
    let line = csv_line(&s.far, &row);
    s.rows.push(row);
    let appended = std::fs::OpenOptions::new()
        .append(true)
        .open(&s.csv)
        .and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = appended {
        tracing::warn!("range test: cannot write {}: {e}", s.csv.display());
    }
    if let Err(e) = std::fs::write(&s.kml, kml_text(&s.far, &s.rows)) {
        tracing::warn!("range test: cannot write {}: {e}", s.kml.display());
    }
}

fn file_safe(call: &str) -> String {
    call.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn db(v: Option<f32>) -> String {
    v.map(|d| format!("{d:.0}")).unwrap_or_default()
}

fn iso(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// One CSV line (no newline).
pub fn csv_line(far: &str, r: &Row) -> String {
    let (lat, lon) = r
        .pos
        .map(|(a, o)| (format!("{a:.6}"), format!("{o:.6}")))
        .unwrap_or_default();
    format!(
        "{},{lat},{lon},{},{},{},{},{far},{}",
        iso(r.ts),
        r.probe,
        r.outcome.as_str(),
        db(r.snr_they_heard),
        db(r.snr_we_heard),
        r.far_grid
    )
}

/// KML for Google Earth or My Maps: one coloured pin per probe with a fix.
pub fn kml_text(far: &str, rows: &[Row]) -> String {
    let mut k = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document>\n",
    );
    k.push_str(&format!("<name>Range test to {far}</name>\n"));
    // KML colours are aabbggrr.
    for (id, color) in [
        ("both", "ff3fbf4f"),
        ("heard_them", "ff1fa8f5"),
        ("none", "ff3c3ce0"),
    ] {
        k.push_str(&format!(
            "<Style id=\"{id}\"><IconStyle><color>{color}</color><scale>0.7</scale><Icon><href>http://maps.google.com/mapfiles/kml/shapes/shaded_dot.png</href></Icon></IconStyle></Style>\n"
        ));
    }
    for r in rows {
        let Some((lat, lon)) = r.pos else {
            continue;
        };
        let mut desc = r.outcome.words().to_string();
        if let Some(d) = r.snr_they_heard {
            desc.push_str(&format!(", they heard us at {d:.0} dB"));
        }
        if let Some(d) = r.snr_we_heard {
            desc.push_str(&format!(", we heard them at {d:.0} dB"));
        }
        k.push_str(&format!(
            "<Placemark><name>{}</name><description>{} {desc}</description><styleUrl>#{}</styleUrl><Point><coordinates>{lon:.6},{lat:.6}</coordinates></Point></Placemark>\n",
            r.probe,
            iso(r.ts),
            r.outcome.as_str()
        ));
    }
    k.push_str("</Document></kml>\n");
    k
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Callsign, Flags};

    fn probe_env(seq: u32) -> Envelope {
        Envelope::new_msg(
            Callsign::parse("M0CAR").unwrap(),
            Callsign::parse("G0HOM").unwrap(),
            seq,
            probe_text(seq).into_bytes(),
            3,
            Flags::new(),
        )
        .unwrap()
    }

    fn receipt(for_line: &Envelope, snr: f32) -> Envelope {
        let mut a = Envelope::ack_for(for_line, Callsign::parse("G0HOM").unwrap(), 9, Some(snr));
        a.add_ack_grid("IO91WM12");
        a
    }

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "wcr-range-{}-{}",
            std::process::id(),
            rand::random::<u32>()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_receipt_means_both_ways_with_both_snrs() {
        let dir = tmp();
        let mut t = RangeTest::new();
        let csv = t.start("g0hom", 30, &dir, 1_700_000_000).unwrap();
        let (far, n, done) = t.due(1_700_000_000).unwrap();
        assert_eq!((far.as_str(), n, done), ("G0HOM", 1, None));
        let line = probe_env(1);
        t.probe_sent(line.msg_id, 1, 1_700_000_000, Some((51.5, -0.12)));
        assert!(t.due(1_700_000_010).is_none(), "not due before the gap");
        t.heard(&receipt(&line, -4.0), Some(7.0));
        let (_, n, done) = t.due(1_700_000_030).unwrap();
        assert_eq!(n, 2);
        let row = done.unwrap();
        assert_eq!(row.outcome, Outcome::Both);
        assert_eq!(row.snr_they_heard, Some(-4.0));
        assert_eq!(row.snr_we_heard, Some(7.0));
        assert_eq!(row.far_grid, "IO91WM12");
        let text = std::fs::read_to_string(&csv).unwrap();
        assert_eq!(
            text.lines().nth(1).unwrap(),
            "2023-11-14T22:13:20Z,51.500000,-0.120000,1,both,-4,7,G0HOM,IO91WM12"
        );
        let kml = std::fs::read_to_string(csv.with_extension("kml")).unwrap();
        assert!(
            kml.contains("<coordinates>-0.120000,51.500000</coordinates>"),
            "{kml}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn another_frame_from_them_means_we_hear_them_only() {
        let dir = tmp();
        let mut t = RangeTest::new();
        t.start("G0HOM", 30, &dir, 100).unwrap();
        t.due(100).unwrap();
        let line = probe_env(1);
        t.probe_sent(line.msg_id, 1, 100, None);
        // A receipt for some other line, and a frame from a stranger.
        t.heard(&receipt(&probe_env(77), 3.0), Some(-2.0));
        let mut stranger = receipt(&line, 9.0);
        stranger.origin = Callsign::parse("M0XYZ").unwrap();
        t.heard(&stranger, Some(12.0));
        let row = t.due(130).unwrap().2.unwrap();
        assert_eq!(row.outcome, Outcome::HeardThem);
        assert_eq!((row.snr_they_heard, row.snr_we_heard), (None, Some(-2.0)));
        assert!(row_notice("G0HOM", &row).contains("no GPS fix"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn silence_is_no_contact_and_stop_logs_the_last_probe() {
        let dir = tmp();
        let mut t = RangeTest::new();
        let csv = t.start("G0HOM", 5, &dir, 100).unwrap();
        t.due(100).unwrap();
        t.probe_sent(probe_env(1).msg_id, 1, 100, Some((1.0, 2.0)));
        // The gap is clamped to the minimum.
        assert!(t.due(110).is_none());
        let sum = t.stop().unwrap();
        assert!(
            sum.contains("1 probes logged (0 both ways, 0 heard them only, 1 no contact)"),
            "{sum}"
        );
        assert!(t.running().is_none());
        let text = std::fs::read_to_string(&csv).unwrap();
        assert!(text.lines().nth(1).unwrap().contains(",1,none,,,G0HOM,"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_probe_is_not_a_signal_report_request() {
        assert!(!crate::signal_report::is_request(&probe_text(3)));
    }
}
