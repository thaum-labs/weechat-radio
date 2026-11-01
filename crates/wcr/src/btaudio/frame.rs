//! SPDX-License-Identifier: Apache-2.0
//! Framing on the radio's Bluetooth audio channel.
//!
//! Each message is `0x7E type payload 0x7E`, with `0x7D` and `0x7E` inside
//! escaped as `0x7D, byte ^ 0x20`. We send SBC audio as type 0; type 1 ends a
//! transmission (the radio unkeys), type 2 is an acknowledgement. The radio
//! sends what it receives as type 3 (some firmware uses 0) and echoes what it
//! transmits as type 9 (HTCommander's hardware notes, #235).

const FLAG: u8 = 0x7E;
const ESCAPE: u8 = 0x7D;

const TYPE_AUDIO: u8 = 0x00;
const TYPE_END: u8 = 0x01;
/// Received audio from the radio. Dropping this type left the Mac deaf (#235).
const TYPE_RX_AUDIO: u8 = 0x03;

/// The radio stops transmitting when it gets this.
pub const END_OF_AUDIO: [u8; 11] = [
    0x7E, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7E,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// One or more SBC frames the radio received off the air.
    Audio(Vec<u8>),
    /// End of a received burst.
    End,
    /// Anything else (acks and types we do not use).
    Other(u8),
}

/// Frame SBC bytes for the radio to transmit.
pub fn audio(sbc: &[u8]) -> Vec<u8> {
    framed(TYPE_AUDIO, sbc)
}

/// Frame SBC bytes the way the radio sends what it receives (tests).
pub fn rx_audio(sbc: &[u8]) -> Vec<u8> {
    framed(TYPE_RX_AUDIO, sbc)
}

fn framed(kind: u8, sbc: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sbc.len() + 8);
    out.push(FLAG);
    for &b in std::iter::once(&kind).chain(sbc) {
        if b == FLAG || b == ESCAPE {
            out.push(ESCAPE);
            out.push(b ^ 0x20);
        } else {
            out.push(b);
        }
    }
    out.push(FLAG);
    out
}

/// Splits the byte stream from the radio into messages.
#[derive(Default)]
pub struct Deframer {
    buf: Vec<u8>,
    in_frame: bool,
    escaped: bool,
}

impl Deframer {
    pub fn push(&mut self, bytes: &[u8], mut on_message: impl FnMut(Message)) {
        for &b in bytes {
            if b == FLAG {
                // Back-to-back flags are an end and a start; an empty frame is
                // just a start.
                if self.in_frame && !self.buf.is_empty() {
                    on_message(decode(&self.buf));
                }
                self.buf.clear();
                self.in_frame = true;
                self.escaped = false;
            } else if !self.in_frame {
                // Garbage before the first flag.
            } else if self.escaped {
                self.buf.push(b ^ 0x20);
                self.escaped = false;
            } else if b == ESCAPE {
                self.escaped = true;
            } else {
                self.buf.push(b);
            }
        }
    }
}

fn decode(body: &[u8]) -> Message {
    match body[0] {
        TYPE_AUDIO | TYPE_RX_AUDIO => Message::Audio(body[1..].to_vec()),
        TYPE_END => Message::End,
        other => Message::Other(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(bytes: &[u8]) -> Vec<Message> {
        let mut d = Deframer::default();
        let mut out = Vec::new();
        d.push(bytes, |m| out.push(m));
        out
    }

    #[test]
    fn audio_roundtrips_with_escapes() {
        let sbc = vec![0x9C, 0x7E, 0x01, 0x7D, 0x20];
        let wire = audio(&sbc);
        assert_eq!(wire.iter().filter(|&&b| b == FLAG).count(), 2);
        assert_eq!(all(&wire), vec![Message::Audio(sbc)]);
    }

    #[test]
    fn received_audio_type_3_is_audio() {
        // The radio's own RX stream, as HTCommander logs it (#235).
        let wire = rx_audio(&[0x9C, 0x31, 0x12]);
        assert_eq!(wire[1], 0x03);
        assert_eq!(all(&wire), vec![Message::Audio(vec![0x9C, 0x31, 0x12])]);
    }

    #[test]
    fn transmit_echo_is_not_received_audio() {
        assert_eq!(
            all(&[FLAG, 0x09, 0x9C, 0x00, FLAG]),
            vec![Message::Other(0x09)]
        );
    }

    #[test]
    fn end_frame_is_recognised() {
        assert_eq!(all(&END_OF_AUDIO), vec![Message::End]);
    }

    #[test]
    fn split_reads_and_leading_garbage() {
        let mut wire = vec![0x11, 0x22];
        wire.extend(audio(&[1, 2, 3]));
        wire.extend(END_OF_AUDIO);
        let mut d = Deframer::default();
        let mut out = Vec::new();
        for chunk in wire.chunks(3) {
            d.push(chunk, |m| out.push(m));
        }
        assert_eq!(out, vec![Message::Audio(vec![1, 2, 3]), Message::End]);
    }
}
