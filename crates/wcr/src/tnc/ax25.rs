//! SPDX-License-Identifier: Apache-2.0
//! Minimal AX.25 UI framing for radios with a built-in packet TNC.
//!
//! The VR-N76 / UV-PRO TNC was built for APRS, so every frame we hand it is
//! wrapped as a plain AX.25 UI frame: `SRC>WCR: <envelope>`. Other packet
//! stations on the frequency see a normal callsign header; our own decoder
//! strips it and reads the WeeChat Radio envelope inside.

/// UI frame, no poll bit.
pub const CONTROL_UI: u8 = 0x03;
/// No layer 3 protocol.
pub const PID_NONE: u8 = 0xF0;
const ADDR_LEN: usize = 7;
const MAX_ADDRS: usize = 10;

/// AX.25 address: up to six A–Z0–9 characters plus SSID 0–15.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub call: String,
    pub ssid: u8,
}

impl Address {
    /// Best-effort conversion of any station id (`M7TJF`, `2E0ABC/P`, `~ALICE`, `G4ABC-7`).
    pub fn from_station(raw: &str) -> Self {
        let s = raw.trim().to_ascii_uppercase();
        let (base, ssid) = match s.rsplit_once('-') {
            Some((b, tail)) if tail.chars().all(|c| c.is_ascii_digit()) && !tail.is_empty() => {
                (b.to_string(), tail.parse::<u8>().unwrap_or(0).min(15))
            }
            _ => (s.clone(), 0),
        };
        let base = base.split('/').next().unwrap_or("");
        let mut call: String = base
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(6)
            .collect();
        if call.is_empty() {
            call = "NOCALL".into();
        }
        Self { call, ssid }
    }

    fn encode(&self, last: bool, command_bit: bool) -> [u8; ADDR_LEN] {
        let mut out = [b' ' << 1; ADDR_LEN];
        for (i, b) in self.call.bytes().take(6).enumerate() {
            out[i] = b << 1;
        }
        // C R R SSID(4) H: reserved bits set, H = end-of-address.
        let mut ssid = 0x60 | ((self.ssid & 0x0F) << 1);
        if command_bit {
            ssid |= 0x80;
        }
        if last {
            ssid |= 0x01;
        }
        out[6] = ssid;
        out
    }

    fn decode(bytes: &[u8]) -> Option<(Self, bool)> {
        if bytes.len() < ADDR_LEN {
            return None;
        }
        let mut call = String::with_capacity(6);
        for &b in &bytes[..6] {
            let c = b >> 1;
            if !(c.is_ascii_alphanumeric() || c == b' ') {
                return None;
            }
            if c != b' ' {
                call.push(c as char);
            }
        }
        let ssid = (bytes[6] >> 1) & 0x0F;
        let last = bytes[6] & 0x01 == 1;
        Some((Self { call, ssid }, last))
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ssid == 0 {
            f.write_str(&self.call)
        } else {
            write!(f, "{}-{}", self.call, self.ssid)
        }
    }
}

/// Build `src>dest` UI frame carrying `info` (no digipeaters, PID F0).
pub fn wrap_ui(src: &Address, dest: &Address, info: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(info.len() + 2 * ADDR_LEN + 2);
    out.extend_from_slice(&dest.encode(false, true));
    out.extend_from_slice(&src.encode(true, false));
    out.push(CONTROL_UI);
    out.push(PID_NONE);
    out.extend_from_slice(info);
    out
}

/// A decoded UI frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiFrame<'a> {
    pub dest: Address,
    pub src: Address,
    pub digis: Vec<Address>,
    pub pid: u8,
    pub info: &'a [u8],
}

/// Parse an AX.25 UI frame. Returns `None` if the bytes are not a UI frame,
/// so callers can fall back to treating them as a raw payload.
pub fn unwrap_ui(frame: &[u8]) -> Option<UiFrame<'_>> {
    let mut addrs = Vec::with_capacity(2);
    let mut pos = 0;
    loop {
        let (addr, last) = Address::decode(frame.get(pos..pos + ADDR_LEN)?)?;
        addrs.push(addr);
        pos += ADDR_LEN;
        if last {
            break;
        }
        if addrs.len() >= MAX_ADDRS {
            return None;
        }
    }
    if addrs.len() < 2 {
        return None;
    }
    let control = *frame.get(pos)?;
    // UI with or without the P/F bit.
    if control & 0xEF != CONTROL_UI {
        return None;
    }
    let pid = *frame.get(pos + 1)?;
    let info = &frame[pos + 2..];
    let src = addrs.remove(1);
    let dest = addrs.remove(0);
    Some(UiFrame {
        dest,
        src,
        digis: addrs,
        pid,
        info,
    })
}

/// Payload for the WeeChat Radio decoder: the UI info field if this is an
/// AX.25 frame, otherwise the bytes unchanged (a raw KISS peer).
pub fn payload_of(frame: &[u8]) -> &[u8] {
    match unwrap_ui(frame) {
        Some(ui) if !ui.info.is_empty() => ui.info,
        _ => frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_from_station_variants() {
        assert_eq!(
            Address::from_station("m7tjf"),
            Address {
                call: "M7TJF".into(),
                ssid: 0
            }
        );
        assert_eq!(
            Address::from_station("G4ABC-7"),
            Address {
                call: "G4ABC".into(),
                ssid: 7
            }
        );
        assert_eq!(Address::from_station("2E0ABC/P").call, "2E0ABC");
        assert_eq!(Address::from_station("~ALICE").call, "ALICE");
        assert_eq!(Address::from_station("VK2ABCDEF").call, "VK2ABC");
        assert_eq!(Address::from_station("").call, "NOCALL");
        assert_eq!(Address::from_station("W1AW-99").ssid, 15);
    }

    #[test]
    fn wrap_then_unwrap_roundtrip() {
        let src = Address::from_station("M7TJF");
        let dest = Address::from_station("WCR");
        let body = b"\x02hello radio \xC0\xDB";
        let frame = wrap_ui(&src, &dest, body);
        assert_eq!(frame.len(), 14 + 2 + body.len());
        assert_eq!(frame[14], CONTROL_UI);
        assert_eq!(frame[15], PID_NONE);
        let ui = unwrap_ui(&frame).expect("ui");
        assert_eq!(ui.src, src);
        assert_eq!(ui.dest, dest);
        assert!(ui.digis.is_empty());
        assert_eq!(ui.pid, PID_NONE);
        assert_eq!(ui.info, body);
        assert_eq!(payload_of(&frame), body);
    }

    #[test]
    fn header_bits_follow_spec() {
        let frame = wrap_ui(
            &Address::from_station("G4ABC-3"),
            &Address::from_station("WCR"),
            b"x",
        );
        // dest: 'W','C','R',' ',' ',' ' shifted, SSID byte C=1, RR=11, ssid 0, H=0.
        assert_eq!(frame[0], b'W' << 1);
        assert_eq!(frame[3], b' ' << 1);
        assert_eq!(frame[6], 0xE0);
        // src: SSID 3 → 0x60 | (3<<1) | end bit.
        assert_eq!(frame[13], 0x67);
    }

    #[test]
    fn aprs_frame_with_digis_parses() {
        // M0ABC>APRS,WIDE1-1: !test
        let mut f = Vec::new();
        f.extend_from_slice(&Address::from_station("APRS").encode(false, true));
        f.extend_from_slice(&Address::from_station("M0ABC-9").encode(false, false));
        f.extend_from_slice(&Address::from_station("WIDE1-1").encode(true, false));
        f.push(CONTROL_UI);
        f.push(PID_NONE);
        f.extend_from_slice(b"!test");
        let ui = unwrap_ui(&f).unwrap();
        assert_eq!(ui.src.to_string(), "M0ABC-9");
        assert_eq!(ui.digis.len(), 1);
        assert_eq!(ui.digis[0].to_string(), "WIDE1-1");
        assert_eq!(ui.info, b"!test");
    }

    #[test]
    fn raw_payload_falls_through() {
        // A WeeChat Radio v2 envelope starts with a small version byte; not AX.25.
        let raw = [0x02u8, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80];
        assert!(unwrap_ui(&raw).is_none());
        assert_eq!(payload_of(&raw), &raw);
        assert!(unwrap_ui(b"").is_none());
        assert!(unwrap_ui(&[0u8; 20]).is_none());
    }
}
