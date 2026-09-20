//! SPDX-License-Identifier: Apache-2.0
//! Packed amateur callsigns and guest nicks.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// 6-bit alphabet: space, A-Z, 0-9, /, -, ~ (guest prefix).
const ALPHABET: &[u8] = b" ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-/~";
const MAX_CHARS: usize = 8;
pub const PACKED_LEN: usize = 6; // 8 chars * 6 bits = 48 bits

/// Station identity: a callsign, or a guest nick stored as `~NAME`.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Callsign(String);

impl Callsign {
    pub fn parse(raw: &str) -> Result<Self> {
        let s = raw.trim().to_ascii_uppercase();
        if s.is_empty() {
            return Err(Error::protocol("empty callsign"));
        }
        if s.len() > MAX_CHARS {
            return Err(Error::protocol(format!(
                "callsign '{s}' is longer than {MAX_CHARS} characters"
            )));
        }
        if !s.is_ascii() {
            return Err(Error::protocol("callsign must be ASCII"));
        }
        if s.starts_with('~') {
            let nick = &s[1..];
            if nick.is_empty() {
                return Err(Error::protocol("guest nick is empty"));
            }
            if !nick.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(Error::protocol(
                    "guest nick must be letters, digits, or '-'",
                ));
            }
            return Ok(Self(s));
        }
        if !is_plausible_callsign(&s) {
            return Err(Error::protocol(format!(
                "'{s}' does not look like an amateur callsign. Guests use a leading ~ (e.g. ~ALICE)."
            )));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_guest(&self) -> bool {
        self.0.starts_with('~')
    }

    pub fn pack(&self) -> [u8; PACKED_LEN] {
        pack_chars(&self.0)
    }

    pub fn unpack(bytes: &[u8; PACKED_LEN]) -> Result<Self> {
        let s = unpack_chars(bytes)?;
        if s.starts_with('~') || is_plausible_callsign(&s) {
            Self::parse(&s)
        } else {
            // Group destinations and other packed names.
            Ok(Self(s))
        }
    }

    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Closed-group destination: `&NAME` packed as a synthetic dest.
    pub fn group(name: &str) -> Result<Self> {
        let n = name.trim().trim_start_matches('&').trim_start_matches('#');
        let s = format!("&{}", n.to_ascii_uppercase());
        if s.len() > MAX_CHARS {
            return Err(Error::protocol("group name too long (max 7 after &)"));
        }
        // Bypass amateur-callsign check: groups use '&' which is not in the alphabet.
        // Encode as G/NAME using alphabet-legal chars? Better: use dest flags GROUP
        // and pack the name without '&'. Callers set GROUP flag.
        let packed_name = n.to_ascii_uppercase();
        if packed_name.is_empty() {
            return Err(Error::protocol("empty group name"));
        }
        if packed_name.len() > MAX_CHARS {
            return Err(Error::protocol("group name too long"));
        }
        for c in packed_name.bytes() {
            if !ALPHABET.contains(&c) {
                return Err(Error::protocol("group name has unsupported characters"));
            }
        }
        Ok(Self(packed_name))
    }
}

impl fmt::Display for Callsign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Callsign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Callsign({})", self.0)
    }
}

impl FromStr for Callsign {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

/// Amateur callsign: 1–2 letter/digit prefix, 1 digit, 1–4 letter suffix.
/// Accepts common forms: G4ABC, M0ABC, W1AW, VK2ABC, 2E0ABC, GB3RS.
pub fn is_plausible_callsign(s: &str) -> bool {
    let s = s.trim().to_ascii_uppercase();
    if s.len() < 3 || s.len() > 8 {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '/') {
        return false;
    }
    // Must contain at least one letter and one digit.
    let has_letter = s.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    has_letter && has_digit
}

fn index_of(c: u8) -> Result<u8> {
    ALPHABET
        .iter()
        .position(|&x| x == c)
        .map(|i| i as u8)
        .ok_or_else(|| Error::protocol(format!("unsupported callsign character '{}'", c as char)))
}

fn pack_chars(s: &str) -> [u8; PACKED_LEN] {
    let mut chars = [b' '; MAX_CHARS];
    for (i, b) in s.as_bytes().iter().take(MAX_CHARS).enumerate() {
        chars[i] = *b;
    }
    let mut acc: u64 = 0;
    for c in chars {
        let idx = index_of(c).unwrap_or(0) as u64;
        acc = (acc << 6) | idx;
    }
    let be = acc.to_be_bytes();
    let mut out = [0u8; PACKED_LEN];
    out.copy_from_slice(&be[2..8]);
    out
}

fn unpack_chars(bytes: &[u8; PACKED_LEN]) -> Result<String> {
    let mut padded = [0u8; 8];
    padded[2..8].copy_from_slice(bytes);
    let mut acc = u64::from_be_bytes(padded);
    let mut chars = [b' '; MAX_CHARS];
    for i in (0..MAX_CHARS).rev() {
        let idx = (acc & 0x3f) as usize;
        acc >>= 6;
        let c = *ALPHABET
            .get(idx)
            .ok_or_else(|| Error::protocol("invalid packed callsign"))?;
        chars[i] = c;
    }
    let s = std::str::from_utf8(&chars).unwrap_or("").trim().to_string();
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_callsigns() {
        for s in ["G4ABC", "M0ABC", "W1AW", "VK2ABC", "2E0ABC", "~ALICE"] {
            let c = Callsign::parse(s).unwrap();
            let packed = c.pack();
            let back = Callsign::unpack(&packed).unwrap();
            assert_eq!(c, back, "roundtrip {s}");
        }
    }

    #[test]
    fn reject_bad() {
        assert!(Callsign::parse("").is_err());
        assert!(Callsign::parse("HELLOHELLO").is_err());
        assert!(Callsign::parse("NODIGIT").is_err());
        assert!(Callsign::parse("~").is_err());
    }

    #[test]
    fn long_group_names_pack_as_eight_chars() {
        let c = Callsign::from_raw("COMPATRIOTS");
        let back = Callsign::unpack(&c.pack()).unwrap();
        assert_eq!(back.as_str(), "COMPATRI");
    }

    #[test]
    fn guests_flag() {
        assert!(Callsign::parse("~BOB").unwrap().is_guest());
        assert!(!Callsign::parse("W1AW").unwrap().is_guest());
    }
}
