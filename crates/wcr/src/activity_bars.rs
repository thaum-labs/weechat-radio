//! SPDX-License-Identifier: Apache-2.0
//! The ACTIVITY graph of the modem73 desktop layout: receive level in dB
//! over the last 6 seconds, drawn as modem73's BARS style (graph style 0,
//! `draw_signal_bars` in modem73's `tnc_ui.hh`). Pure logic, no drawing, so
//! the scale is unit-tested and a terminal UI can reuse it.
//!
//! Fixed linear scale, 0 dB at the top, -80 dB at the bottom, 6 rows.
//! 60 samples, one every 100 ms, oldest on the left.

use std::collections::VecDeque;

/// Top of the scale, dB.
pub const MAX_DB: f32 = 0.0;
/// Bottom of the scale, dB.
pub const MIN_DB: f32 = -80.0;
/// Character rows in the graph.
pub const ROWS: usize = 6;
/// Samples kept: 60 × 100 ms = 6 seconds.
pub const HISTORY: usize = 60;
/// Time between samples.
pub const SAMPLE_MS: u64 = 100;
/// A sample counts as "tone" (our own TX) for this long after PTT drops.
pub const TONE_HOLD_MS: u64 = 450;
/// Threshold line when the station reports none. modem73's hero GIF was
/// recorded at -4 dB, which puts the gold line on the top row.
pub const DEFAULT_THRESHOLD_DB: f32 = -4.0;
/// Level of an empty slot: below the scale, so it draws blank.
pub const EMPTY_DB: f32 = -100.0;

/// The glyphs of a part-filled cell, by quarter.
pub const PARTIALS: [char; 5] = [' ', '.', ':', '|', '#'];

/// One 100 ms sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub level_db: f32,
    /// Modem sync held on a real frame (modem73's magenta). WCR has no
    /// reliable DCD bit yet, so callers pass false.
    pub dcd: bool,
    /// This station is transmitting (modem73's red).
    pub tone: bool,
}

impl Sample {
    pub const EMPTY: Sample = Sample {
        level_db: EMPTY_DB,
        dcd: false,
        tone: false,
    };
}

/// Colour role of a column, in modem73's order of precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    /// Our own transmission (pair 2, red).
    Tone,
    /// Confirmed sync on a real frame (pair 6, magenta).
    Dcd,
    /// Level above the threshold (pair 4, cyan).
    Above,
    /// Everything else (pair 1, green).
    Quiet,
}

/// What one character cell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// A full bold picket (`▮`).
    Full(Tint),
    /// A part-filled cell: one of [`PARTIALS`].
    Partial(char, Tint),
    /// The dim gold threshold line (`─`).
    Threshold,
    Blank,
}

/// The last 6 seconds of samples, oldest first.
#[derive(Clone, Debug)]
pub struct History {
    samples: VecDeque<Sample>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            samples: vec![Sample::EMPTY; HISTORY].into(),
        }
    }
}

impl History {
    pub fn push(&mut self, s: Sample) {
        self.samples.push_back(s);
        while self.samples.len() > HISTORY {
            self.samples.pop_front();
        }
    }

    pub fn samples(&self) -> &VecDeque<Sample> {
        &self.samples
    }

    /// Every cell, `[row][col]`, row 0 at the top.
    pub fn grid(&self, width: usize, threshold_db: f32) -> Vec<Vec<Cell>> {
        let thresh = thresh_row(threshold_db);
        (0..ROWS)
            .map(|row| {
                (0..width)
                    .map(|col| {
                        let s = self.samples[hist_idx(col, width, self.samples.len())];
                        cell(s.level_db, row, thresh, tint(&s, threshold_db))
                    })
                    .collect()
            })
            .collect()
    }
}

/// `(row_min, row_max)` in dB: each row covers 80/6 ≈ 13.33 dB.
pub fn row_band(row: usize) -> (f32, f32) {
    let span = MAX_DB - MIN_DB;
    let row_max = MAX_DB - span * row as f32 / ROWS as f32;
    let row_min = MAX_DB - span * (row + 1) as f32 / ROWS as f32;
    (row_min, row_max)
}

/// The sample a column shows: the 60 samples spread over any width.
pub fn hist_idx(col: usize, width: usize, len: usize) -> usize {
    if width == 0 || len == 0 {
        return 0;
    }
    (col * HISTORY / width).min(len - 1)
}

/// The row the threshold line sits on.
pub fn thresh_row(threshold_db: f32) -> usize {
    (((MAX_DB - threshold_db) / (MAX_DB - MIN_DB) * ROWS as f32) as i32).clamp(0, ROWS as i32 - 1)
        as usize
}

/// Partial glyph index for a level inside a row: `(frac * 4)`, 0..=4.
pub fn partial_idx(level_db: f32, row_min: f32, row_max: f32) -> usize {
    let frac = (level_db - row_min) / (row_max - row_min);
    ((frac * 4.0) as i32).clamp(0, 4) as usize
}

pub fn tint(s: &Sample, threshold_db: f32) -> Tint {
    if s.tone {
        Tint::Tone
    } else if s.dcd {
        Tint::Dcd
    } else if s.level_db > threshold_db {
        Tint::Above
    } else {
        Tint::Quiet
    }
}

pub fn cell(level_db: f32, row: usize, thresh_row: usize, tint: Tint) -> Cell {
    let (row_min, row_max) = row_band(row);
    if level_db >= row_max {
        Cell::Full(tint)
    } else if level_db > row_min {
        Cell::Partial(PARTIALS[partial_idx(level_db, row_min, row_max)], tint)
    } else if row == thresh_row {
        Cell::Threshold
    } else {
        Cell::Blank
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn six_rows_of_about_13_db_from_0_to_minus_80() {
        let want = [
            (-13.33, 0.0),
            (-26.67, -13.33),
            (-40.0, -26.67),
            (-53.33, -40.0),
            (-66.67, -53.33),
            (-80.0, -66.67),
        ];
        for (row, (lo, hi)) in want.iter().enumerate() {
            let (min, max) = row_band(row);
            assert!(close(min, *lo) && close(max, *hi), "row {row}: {min} {max}");
        }
    }

    #[test]
    fn the_window_is_60_samples_of_100_ms() {
        assert_eq!(HISTORY as u64 * SAMPLE_MS, 6_000);
        let h = History::default();
        assert_eq!(h.samples().len(), HISTORY);
        assert!(h.samples().iter().all(|s| s.level_db == EMPTY_DB));
    }

    #[test]
    fn columns_spread_the_samples_and_never_overrun() {
        assert_eq!(hist_idx(0, 48, 60), 0);
        assert_eq!(hist_idx(47, 48, 60), 58);
        assert_eq!(hist_idx(10, 60, 60), 10);
        // Wider than the history: each sample spans two columns.
        assert_eq!(hist_idx(119, 120, 60), 59);
        assert_eq!(hist_idx(3, 120, 60), 1);
        // A short history clamps to its last sample.
        assert_eq!(hist_idx(47, 48, 10), 9);
    }

    #[test]
    fn partials_step_in_quarters() {
        let (lo, hi) = row_band(2); // -40 .. -26.67
        assert_eq!(partial_idx(-40.0, lo, hi), 0);
        assert_eq!(partial_idx(-36.0, lo, hi), 1);
        assert_eq!(partial_idx(-33.0, lo, hi), 2);
        assert_eq!(partial_idx(-29.5, lo, hi), 3);
        assert_eq!(partial_idx(-26.7, lo, hi), 3);
        assert_eq!(partial_idx(-20.0, lo, hi), 4);
    }

    #[test]
    fn threshold_row_follows_the_db() {
        assert_eq!(thresh_row(-4.0), 0);
        assert_eq!(thresh_row(-30.0), 2);
        assert_eq!(thresh_row(-79.0), 5);
        assert_eq!(thresh_row(-200.0), 5);
        assert_eq!(thresh_row(10.0), 0);
    }

    #[test]
    fn colour_precedence_is_tone_dcd_above_quiet() {
        let s = |level_db, dcd, tone| Sample {
            level_db,
            dcd,
            tone,
        };
        assert_eq!(tint(&s(-50.0, true, true), -30.0), Tint::Tone);
        assert_eq!(tint(&s(-10.0, true, false), -30.0), Tint::Dcd);
        assert_eq!(tint(&s(-10.0, false, false), -30.0), Tint::Above);
        assert_eq!(tint(&s(-30.0, false, false), -30.0), Tint::Quiet);
    }

    #[test]
    fn a_minus_35_db_carrier_draws_three_full_rows_under_a_partial() {
        // The hero look: gold line on row 0, blank row 1, partial on row 2,
        // full bars on rows 3-5.
        let mut h = History::default();
        for _ in 0..HISTORY {
            h.push(Sample {
                level_db: -35.0,
                dcd: false,
                tone: false,
            });
        }
        let g = h.grid(48, DEFAULT_THRESHOLD_DB);
        assert_eq!(g.len(), ROWS);
        assert_eq!(g[0][0], Cell::Threshold);
        assert_eq!(g[1][0], Cell::Blank);
        assert_eq!(g[2][0], Cell::Partial('.', Tint::Quiet));
        for row in 3..ROWS {
            assert_eq!(g[row][47], Cell::Full(Tint::Quiet));
        }
    }

    #[test]
    fn empty_slots_draw_blank_except_the_threshold() {
        let g = History::default().grid(20, -30.0);
        assert!(g[2].iter().all(|c| *c == Cell::Threshold));
        assert!(g[5].iter().all(|c| *c == Cell::Blank));
    }

    #[test]
    fn new_samples_enter_on_the_right() {
        let mut h = History::default();
        h.push(Sample {
            level_db: -10.0,
            dcd: false,
            tone: true,
        });
        let g = h.grid(60, DEFAULT_THRESHOLD_DB);
        assert_eq!(g[5][59], Cell::Full(Tint::Tone));
        assert_eq!(g[5][0], Cell::Blank);
    }
}
