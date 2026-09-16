//! SPDX-License-Identifier: Apache-2.0
//! Envelope flags and priority.

use serde::{Deserialize, Serialize};

pub const FLAG_INET_OK: u16 = 1 << 0;
pub const FLAG_GROUP: u16 = 1 << 1;
pub const FLAG_SIGNED: u16 = 1 << 2;
pub const FLAG_REQ_ACK: u16 = 1 << 3;
pub const FLAG_THIRD_PARTY: u16 = 1 << 4;
pub const FLAG_NO_INET: u16 = 1 << 7;
pub const FLAG_PRIORITY_SHIFT: u16 = 5;
pub const FLAG_PRIORITY_MASK: u16 = 0b11 << FLAG_PRIORITY_SHIFT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    #[default]
    Routine = 0,
    Priority = 1,
    Emergency = 2,
}

impl Priority {
    pub fn from_bits(bits: u16) -> Self {
        match (bits & FLAG_PRIORITY_MASK) >> FLAG_PRIORITY_SHIFT {
            2 => Self::Emergency,
            1 => Self::Priority,
            _ => Self::Routine,
        }
    }

    pub fn bits(self) -> u16 {
        (self as u16) << FLAG_PRIORITY_SHIFT
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Routine => "routine",
            Self::Priority => "priority",
            Self::Emergency => "emergency",
        }
    }

    /// Default TTL: emergency 5, priority 4, routine 3.
    pub fn default_ttl(self) -> u8 {
        match self {
            Self::Emergency => 5,
            Self::Priority => 4,
            Self::Routine => 3,
        }
    }

    /// Relay jitter: shorter for higher priority (milliseconds).
    pub fn jitter_ms(self) -> std::ops::RangeInclusive<u64> {
        match self {
            Self::Emergency => 50..=150,
            Self::Priority => 150..=400,
            Self::Routine => 400..=1200,
        }
    }

    pub fn parse_prefix(text: &str) -> (Self, &str) {
        let t = text.trim_start();
        if let Some(rest) = t.strip_prefix("!!") {
            (Self::Emergency, rest.trim_start())
        } else if let Some(rest) = t.strip_prefix('!') {
            (Self::Priority, rest.trim_start())
        } else {
            (Self::Routine, t)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags(pub u16);

impl Flags {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn with(mut self, bit: u16) -> Self {
        self.0 |= bit;
        self
    }

    pub fn set(&mut self, bit: u16, on: bool) {
        if on {
            self.0 |= bit;
        } else {
            self.0 &= !bit;
        }
    }

    pub fn has(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    pub fn inet_ok(self) -> bool {
        self.has(FLAG_INET_OK) && !self.has(FLAG_NO_INET)
    }

    pub fn no_inet(self) -> bool {
        self.has(FLAG_NO_INET)
    }

    pub fn group(self) -> bool {
        self.has(FLAG_GROUP)
    }

    pub fn signed(self) -> bool {
        self.has(FLAG_SIGNED)
    }

    pub fn req_ack(self) -> bool {
        self.has(FLAG_REQ_ACK)
    }

    pub fn third_party(self) -> bool {
        self.has(FLAG_THIRD_PARTY)
    }

    pub fn priority(self) -> Priority {
        Priority::from_bits(self.0)
    }

    pub fn set_priority(&mut self, p: Priority) {
        self.0 = (self.0 & !FLAG_PRIORITY_MASK) | p.bits();
    }

    pub fn raw(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_bits_roundtrip() {
        for p in [Priority::Routine, Priority::Priority, Priority::Emergency] {
            let mut f = Flags::new();
            f.set_priority(p);
            assert_eq!(f.priority(), p);
        }
    }

    #[test]
    fn prefix() {
        assert_eq!(Priority::parse_prefix("!!help").0, Priority::Emergency);
        assert_eq!(Priority::parse_prefix("!net").0, Priority::Priority);
        assert_eq!(Priority::parse_prefix("hello").0, Priority::Routine);
    }
}
