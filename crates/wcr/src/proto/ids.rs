//! SPDX-License-Identifier: Apache-2.0
//! Message identifiers.

use crate::proto::callsign::Callsign;
use serde::{Deserialize, Serialize};

pub const MSG_ID_LEN: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MsgId(pub [u8; MSG_ID_LEN]);

impl MsgId {
    /// Clock-tolerant id: BLAKE3(origin || dest || seq || body)[..8].
    ///
    /// The unix timestamp is not an input. Two messages in the same second
    /// differ when the destination, the body, or the per-origin `seq` differs.
    /// `seq` is what separates a repeat of the same text to the same station.
    pub fn compute(origin: &Callsign, dest: &Callsign, seq: u32, body: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(origin.as_str().as_bytes());
        hasher.update(&[0]);
        hasher.update(dest.as_str().as_bytes());
        hasher.update(&[0]);
        hasher.update(&seq.to_le_bytes());
        hasher.update(body);
        let hash = hasher.finalize();
        let mut id = [0u8; MSG_ID_LEN];
        id.copy_from_slice(&hash.as_bytes()[..MSG_ID_LEN]);
        Self(id)
    }

    pub fn from_bytes(b: [u8; MSG_ID_LEN]) -> Self {
        Self(b)
    }

    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn parse_hex(s: &str) -> Option<Self> {
        let v = hex::decode(s).ok()?;
        if v.len() != MSG_ID_LEN {
            return None;
        }
        let mut id = [0u8; MSG_ID_LEN];
        id.copy_from_slice(&v);
        Some(Self(id))
    }
}

impl std::fmt::Display for MsgId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.hex())
    }
}

impl std::fmt::Debug for MsgId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MsgId({})", self.hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Envelope, Flags};

    #[test]
    fn same_second_different_dest_or_text_are_different_ids() {
        let origin = Callsign::parse("~RFSIMB").unwrap();
        let a = Callsign::parse("~RFSIMA").unwrap();
        let c = Callsign::parse("~RFSIMC").unwrap();
        let to_a = MsgId::compute(&origin, &a, 1, b"alpha");
        let to_c = MsgId::compute(&origin, &c, 1, b"beta");
        let same_dest_other_text = MsgId::compute(&origin, &a, 1, b"other");
        let repeat_next_seq = MsgId::compute(&origin, &a, 2, b"alpha");
        assert_ne!(to_a, to_c, "different recipient and text");
        assert_ne!(to_a, same_dest_other_text, "different text");
        assert_ne!(to_a, repeat_next_seq, "same text, next seq");
        assert_eq!(MsgId::compute(&origin, &a, 1, b"alpha"), to_a);

        let e1 = Envelope::new_msg(
            origin.clone(),
            a.clone(),
            1,
            b"for A".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        let e2 = Envelope::new_msg(origin, c, 1, b"for C".to_vec(), 3, Flags::new()).unwrap();
        assert_ne!(e1.msg_id, e2.msg_id);
        let back_a = Envelope::decode(&e1.encode().unwrap()).unwrap();
        let back_c = Envelope::decode(&e2.encode().unwrap()).unwrap();
        assert_eq!(back_a.msg_id, e1.msg_id);
        assert_eq!(back_c.msg_id, e2.msg_id);
        assert_ne!(back_a.msg_id, back_c.msg_id);
    }
}
