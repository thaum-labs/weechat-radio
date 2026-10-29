//! SPDX-License-Identifier: Apache-2.0
//! Streaming rational resampler: the radio's Bluetooth audio runs at 32 kHz,
//! modem73 at 48 kHz.
//!
//! Upsample by `up`, low-pass with a windowed sinc, keep every `down`th
//! sample, computed polyphase so only the kept samples are worked out.

use std::collections::VecDeque;

/// Taps per polyphase branch. 32 gives > 60 dB stopband, far more than the
/// SBC link needs, and costs nothing at audio rates.
const TAPS_PER_PHASE: usize = 32;

pub struct Resampler {
    up: usize,
    down: usize,
    /// `phases[p][k]`: tap `k` of branch `p`.
    phases: Vec<Vec<f32>>,
    history: VecDeque<f32>,
    /// Position of the next output on the upsampled grid, relative to the
    /// newest input sample's slot.
    phase: usize,
}

impl Resampler {
    /// 32 kHz from the radio to 48 kHz for modem73.
    pub fn radio_to_modem() -> Self {
        Self::new(3, 2)
    }

    /// 48 kHz from modem73 to 32 kHz for the radio.
    pub fn modem_to_radio() -> Self {
        Self::new(2, 3)
    }

    pub fn new(up: usize, down: usize) -> Self {
        let n = up * TAPS_PER_PHASE;
        // Cut off at 90 % of the lower Nyquist, on the upsampled grid.
        let cutoff = 0.45 / up.max(down) as f64;
        let mid = (n - 1) as f64 / 2.0;
        let mut proto = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f64 - mid;
            let sinc = if t == 0.0 {
                2.0 * cutoff
            } else {
                (2.0 * std::f64::consts::PI * cutoff * t).sin() / (std::f64::consts::PI * t)
            };
            // Blackman window.
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos();
            // Gain `up` restores the level lost to zero stuffing.
            proto.push(sinc * w * up as f64);
        }
        let phases = (0..up)
            .map(|p| {
                (0..TAPS_PER_PHASE)
                    .map(|k| proto[p + k * up] as f32)
                    .collect()
            })
            .collect();
        Self {
            up,
            down,
            phases,
            history: std::iter::repeat_n(0.0, TAPS_PER_PHASE).collect(),
            phase: 0,
        }
    }

    /// Feed input samples, append the resampled output to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for &x in input {
            self.history.pop_front();
            self.history.push_back(x);
            while self.phase < self.up {
                let taps = &self.phases[self.phase];
                let mut acc = 0.0f32;
                // history.back() is the newest sample and meets tap 0.
                for (k, h) in self.history.iter().rev().enumerate() {
                    acc += taps[k] * h;
                }
                out.push(acc);
                self.phase += self.down;
            }
            self.phase -= self.up;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, rate: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate).sin())
            .collect()
    }

    /// SNR of `got` against an ideal tone, at the best (fractional) delay.
    fn snr_db(hz: f32, rate: f32, got: &[f32]) -> f32 {
        (0..1200)
            .map(|step| {
                let lag = step as f64 * 0.05;
                let (mut s, mut e) = (0.0f64, 0.0f64);
                for (i, &g) in got.iter().enumerate().skip(400) {
                    let t = (i as f64 - lag) / rate as f64;
                    let want = 0.5 * (2.0 * std::f64::consts::PI * hz as f64 * t).sin();
                    s += want * want;
                    e += (want - g as f64).powi(2);
                }
                (10.0 * (s / e.max(1e-12)).log10()) as f32
            })
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn rates_come_out_right() {
        let mut up = Resampler::radio_to_modem();
        let mut out = Vec::new();
        up.process(&vec![0.0; 3200], &mut out);
        assert_eq!(out.len(), 4800);
        let mut down = Resampler::modem_to_radio();
        let mut back = Vec::new();
        down.process(&out, &mut back);
        assert_eq!(back.len(), 3200);
    }

    #[test]
    fn modem_band_tones_survive_both_ways() {
        for hz in [500.0, 1500.0, 2500.0] {
            let at32 = tone(hz, 32_000.0, 3200);
            let mut at48 = Vec::new();
            Resampler::radio_to_modem().process(&at32, &mut at48);
            assert!(snr_db(hz, 48_000.0, &at48) > 40.0, "{hz} Hz up");
            let mut back = Vec::new();
            Resampler::modem_to_radio().process(&at48, &mut back);
            assert!(snr_db(hz, 32_000.0, &back) > 40.0, "{hz} Hz round trip");
        }
    }

    #[test]
    fn chunking_does_not_change_the_output() {
        let x = tone(1000.0, 48_000.0, 4800);
        let mut whole = Vec::new();
        Resampler::modem_to_radio().process(&x, &mut whole);
        let mut r = Resampler::modem_to_radio();
        let mut parts = Vec::new();
        for c in x.chunks(37) {
            r.process(c, &mut parts);
        }
        assert_eq!(whole, parts);
    }
}
