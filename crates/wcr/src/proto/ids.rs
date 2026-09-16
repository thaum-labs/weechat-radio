//! SPDX-License-Identifier: Apache-2.0
//! Message identifiers.

use crate::proto::callsign::Callsign;
use serde::{Deserialize, Serialize};

pub const MSG_ID_LEN: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MsgId(pub [u8; MSG_ID_LEN]);

impl MsgId {
    /// Clock-tolerant id: BLAKE3(origin || dest || seq || body)[..8].
    /// `seq` is a per-origin counter so identical bodies still unique.
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
