//! SPDX-License-Identifier: Apache-2.0
//! SBC as the radio speaks it: 32 kHz mono, 16 blocks, 8 subbands, loudness.

use nanoaudio_core::codec::Encoder;
use nanoaudio_sbc::{SbcAllocation, SbcChannelMode, SbcDecoder, SbcEncoder, SbcParameters};

pub const SAMPLE_RATE: u32 = 32_000;
/// Samples in one SBC frame (16 blocks x 8 subbands).
pub const FRAME_SAMPLES: usize = 128;
/// What we send at: the radio's own bitpool. 40 doubled the Bluetooth data
/// rate and put 440 bytes in each 20 ms message, past the 300 bytes the radio
/// takes in one (#235); HTCommander's hardware-tested modem sends 18 too.
/// For modem audio 18 still keeps ~50 dB SNR (see the test below), far above
/// what the FM path gives. Rx is whatever the radio sends.
const TX_BITPOOL: u8 = 18;
/// Most SBC bytes in one framed message to the radio (HTCommander's limit).
pub const MAX_MESSAGE_BYTES: usize = 300;

/// Bytes in one SBC frame we send.
pub fn tx_frame_bytes() -> usize {
    params(TX_BITPOOL).frame_length()
}

fn params(bitpool: u8) -> SbcParameters {
    SbcParameters {
        sample_rate: SAMPLE_RATE,
        channel_mode: SbcChannelMode::Mono,
        allocation: SbcAllocation::Loudness,
        blocks: 16,
        subbands: 8,
        bitpool,
    }
}

pub struct Encode {
    enc: SbcEncoder,
    pending: Vec<i16>,
}

impl Default for Encode {
    fn default() -> Self {
        Self::new()
    }
}

impl Encode {
    pub fn new() -> Self {
        Self {
            enc: SbcEncoder::new(params(TX_BITPOOL)).expect("valid SBC parameters"),
            pending: Vec::new(),
        }
    }

    /// Queue samples; returns the SBC bytes for every whole frame so far.
    pub fn push(&mut self, pcm: &[i16]) -> Vec<u8> {
        self.pending.extend_from_slice(pcm);
        let frames = self.pending.len() / FRAME_SAMPLES;
        if frames == 0 {
            return Vec::new();
        }
        let mut out = vec![0u8; frames * params(TX_BITPOOL).frame_length()];
        let used = frames * FRAME_SAMPLES;
        let done = self
            .enc
            .encode(&self.pending[..used], &mut out)
            .map(|p| p.produced)
            .unwrap_or(0);
        out.truncate(done);
        self.pending.drain(..used);
        out
    }

    /// Pad the last partial frame with silence and encode it, then start over.
    pub fn finish(&mut self) -> Vec<u8> {
        let partial = self.pending.len() % FRAME_SAMPLES;
        let out = if partial > 0 {
            self.push(&vec![0; FRAME_SAMPLES - partial])
        } else {
            Vec::new()
        };
        self.enc.reset();
        self.pending.clear();
        out
    }
}

#[derive(Default)]
pub struct Decode {
    dec: SbcDecoder,
}

impl Decode {
    /// Decode one or more whole SBC frames; a bad frame resets the decoder
    /// and drops the rest of that message.
    pub fn push(&mut self, sbc: &[u8], out: &mut Vec<i16>) {
        let mut off = 0;
        let mut buf = [0i16; FRAME_SAMPLES * 2];
        while off < sbc.len() {
            match self.dec.decode_into(&sbc[off..], &mut buf) {
                Ok(p) if p.consumed > 0 => {
                    out.extend_from_slice(&buf[..p.produced]);
                    off += p.consumed;
                }
                _ => {
                    self.dec.reset();
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_the_size_the_radio_expects() {
        assert_eq!(params(18).frame_length(), 44);
        assert_eq!(params(18).samples_per_channel(), FRAME_SAMPLES);
    }

    #[test]
    fn a_tone_survives_encode_and_decode() {
        let pcm: Vec<i16> = (0..3200)
            .map(|i| {
                (8000.0 * (2.0 * std::f32::consts::PI * 1500.0 * i as f32 / 32_000.0).sin()) as i16
            })
            .collect();
        let mut enc = Encode::new();
        let mut sbc = enc.push(&pcm[..1000]);
        sbc.extend(enc.push(&pcm[1000..]));
        sbc.extend(enc.finish());
        let mut back = Vec::new();
        Decode::default().push(&sbc, &mut back);
        assert_eq!(back.len(), 3200);
        // SBC delays by 73 samples (measured against the BlueZ reference).
        let (mut s, mut e) = (0f64, 0f64);
        for i in 500..3000 {
            s += (pcm[i] as f64).powi(2);
            e += (pcm[i] as f64 - back[i + 73] as f64).powi(2);
        }
        assert!(10.0 * (s / e).log10() > 30.0);
    }

    #[test]
    fn modem_audio_survives_the_radio_bitpool() {
        // OFDM-like: 40 tones from 500 to 2450 Hz with scattered phases.
        let n = 32_000;
        let mut x = vec![0f64; n];
        let mut seed = 12345u64;
        for k in 0..40 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let ph = (seed >> 33) as f64 / (1u64 << 31) as f64 * std::f64::consts::TAU;
            let f = 500.0 + 50.0 * k as f64;
            for (i, v) in x.iter_mut().enumerate() {
                *v += (std::f64::consts::TAU * f * i as f64 / 32_000.0 + ph).sin();
            }
        }
        let peak = x.iter().fold(0f64, |a, &b| a.max(b.abs()));
        let pcm: Vec<i16> = x
            .iter()
            .map(|v| (v / peak * 0.5 * 32767.0) as i16)
            .collect();
        let mut enc = Encode::new();
        let mut sbc = enc.push(&pcm);
        sbc.extend(enc.finish());
        let mut back = Vec::new();
        Decode::default().push(&sbc, &mut back);
        let (mut s, mut e) = (0f64, 0f64);
        for i in 1000..30_000 {
            s += (pcm[i] as f64).powi(2);
            e += (pcm[i] as f64 - back[i + 73] as f64).powi(2);
        }
        let snr = 10.0 * (s / e).log10();
        assert!(snr > 35.0, "SBC SNR {snr:.1} dB");
        assert!(
            tx_frame_bytes() * 5 <= MAX_MESSAGE_BYTES,
            "a 20 ms tick fits one message"
        );
    }

    #[test]
    fn garbage_is_dropped_not_fatal() {
        let mut out = Vec::new();
        Decode::default().push(&[1, 2, 3, 4, 5], &mut out);
        assert!(out.is_empty());
    }
}
