//! SPDX-License-Identifier: Apache-2.0
//! Envelope types and codec.

use crate::error::{Error, Result};
use crate::proto::callsign::{Callsign, PACKED_LEN};
use crate::proto::flags::{Flags, Priority, FLAG_SIGNED};
use crate::proto::ids::{MsgId, MSG_ID_LEN};
use serde::{Deserialize, Serialize};

pub const VERSION: u8 = 1;
/// Bytes before body: ver, type, flags, msg_id, origin, dest, hops, ts, seq, body_len.
pub const HEADER_LEN: usize = 1 + 1 + 2 + MSG_ID_LEN + PACKED_LEN + PACKED_LEN + 1 + 4 + 4 + 2;
pub const CRC_LEN: usize = 2;
pub const SIG_LEN: usize = 64;
pub const MAX_BODY: usize = 300;

const CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_IBM_3740);

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MsgType {
    Msg = 0,
    Ack = 1,
    Beacon = 2,
    Have = 3,
    Want = 4,
    Ping = 5,
    Checkin = 6,
    Status = 7,
    Form = 8,
    File = 9, // reserved
}

impl MsgType {
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Msg),
            1 => Ok(Self::Ack),
            2 => Ok(Self::Beacon),
            3 => Ok(Self::Have),
            4 => Ok(Self::Want),
            5 => Ok(Self::Ping),
            6 => Ok(Self::Checkin),
            7 => Ok(Self::Status),
            8 => Ok(Self::Form),
            9 => Ok(Self::File),
            _ => Err(Error::protocol(format!("unknown message type {v}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Msg => "msg",
            Self::Ack => "ack",
            Self::Beacon => "beacon",
            Self::Have => "have",
            Self::Want => "want",
            Self::Ping => "ping",
            Self::Checkin => "checkin",
            Self::Status => "status",
            Self::Form => "form",
            Self::File => "file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub ver: u8,
    pub kind: MsgType,
    pub flags: Flags,
    pub msg_id: MsgId,
    pub origin: Callsign,
    pub dest: Callsign,
    pub hops_left: u8,
    /// Unix seconds. Informational; not part of identity.
    pub ts: u32,
    /// Per-origin sequence. Used in msg_id so clocks may drift.
    pub seq: u32,
    pub body: Vec<u8>,
    pub signature: Option<[u8; SIG_LEN]>,
}

impl Envelope {
    pub fn new_msg(
        origin: Callsign,
        dest: Callsign,
        seq: u32,
        body: impl Into<Vec<u8>>,
        hops: u8,
        flags: Flags,
    ) -> Result<Self> {
        let body = body.into();
        if body.len() > MAX_BODY {
            return Err(Error::protocol(format!(
                "message is {} bytes; max is {MAX_BODY}. Shorten it and try again.",
                body.len()
            )));
        }
        let ts = now_ts();
        let msg_id = MsgId::compute(&origin, &dest, seq, &body);
        Ok(Self {
            ver: VERSION,
            kind: MsgType::Msg,
            flags,
            msg_id,
            origin,
            dest,
            hops_left: hops,
            ts,
            seq,
            body,
            signature: None,
        })
    }

    pub fn ack_for(original: &Envelope, from: Callsign, seq: u32) -> Self {
        let mut flags = Flags::new();
        if original.flags.inet_ok() {
            flags = flags.with(crate::proto::flags::FLAG_INET_OK);
        }
        if original.flags.no_inet() {
            flags = flags.with(crate::proto::flags::FLAG_NO_INET);
        }
        Self {
            ver: VERSION,
            kind: MsgType::Ack,
            flags,
            msg_id: MsgId::compute(&from, &original.origin, seq, &original.msg_id.0),
            origin: from,
            dest: original.origin.clone(),
            hops_left: original.flags.priority().default_ttl(),
            ts: now_ts(),
            seq,
            body: original.msg_id.0.to_vec(),
            signature: None,
        }
    }

    pub fn acked_id(&self) -> Option<MsgId> {
        if self.kind != MsgType::Ack || self.body.len() != MSG_ID_LEN {
            return None;
        }
        let mut id = [0u8; MSG_ID_LEN];
        id.copy_from_slice(&self.body);
        Some(MsgId(id))
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.body.len() > MAX_BODY {
            return Err(Error::protocol("body too large"));
        }
        let mut buf = Vec::with_capacity(HEADER_LEN + self.body.len() + CRC_LEN + SIG_LEN);
        buf.push(self.ver);
        buf.push(self.kind as u8);
        buf.extend_from_slice(&self.flags.raw().to_le_bytes());
        buf.extend_from_slice(&self.msg_id.0);
        buf.extend_from_slice(&self.origin.pack());
        buf.extend_from_slice(&self.dest.pack());
        buf.push(self.hops_left);
        buf.extend_from_slice(&self.ts.to_le_bytes());
        buf.extend_from_slice(&self.seq.to_le_bytes());
        buf.extend_from_slice(&(self.body.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.body);
        let crc = CRC.checksum(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        if let Some(sig) = self.signature {
            buf.extend_from_slice(&sig);
        } else if self.flags.signed() {
            return Err(Error::protocol("SIGNED flag set but no signature"));
        }
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_LEN + CRC_LEN {
            return Err(Error::protocol("frame too short"));
        }
        let ver = bytes[0];
        if ver != VERSION {
            return Err(Error::protocol(format!(
                "unsupported protocol version {ver}"
            )));
        }
        let kind = MsgType::from_u8(bytes[1])?;
        let flags = Flags(u16::from_le_bytes([bytes[2], bytes[3]]));
        let mut msg_id = [0u8; MSG_ID_LEN];
        msg_id.copy_from_slice(&bytes[4..4 + MSG_ID_LEN]);
        let mut o = [0u8; PACKED_LEN];
        let mut d = [0u8; PACKED_LEN];
        let mut i = 4 + MSG_ID_LEN;
        o.copy_from_slice(&bytes[i..i + PACKED_LEN]);
        i += PACKED_LEN;
        d.copy_from_slice(&bytes[i..i + PACKED_LEN]);
        i += PACKED_LEN;
        let hops_left = bytes[i];
        i += 1;
        let ts = u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        i += 4;
        let seq = u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        i += 4;
        let body_len = u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap()) as usize;
        i += 2;
        let crc_at = i + body_len;
        if bytes.len() < crc_at + CRC_LEN {
            return Err(Error::protocol("truncated body"));
        }
        let body = bytes[i..crc_at].to_vec();
        let want_crc = u16::from_le_bytes(bytes[crc_at..crc_at + CRC_LEN].try_into().unwrap());
        let got_crc = CRC.checksum(&bytes[..crc_at]);
        if want_crc != got_crc {
            return Err(Error::protocol("CRC mismatch — frame was corrupted"));
        }
        let mut signature = None;
        let rest = &bytes[crc_at + CRC_LEN..];
        if flags.signed() {
            if rest.len() < SIG_LEN {
                return Err(Error::protocol("missing signature"));
            }
            let mut sig = [0u8; SIG_LEN];
            sig.copy_from_slice(&rest[..SIG_LEN]);
            signature = Some(sig);
        } else if !rest.is_empty() {
            // Tolerate extra trailing bytes from radio padding.
        }
        let origin = Callsign::unpack(&o)?;
        let dest = Callsign::unpack(&d)?;
        Ok(Self {
            ver,
            kind,
            flags,
            msg_id: MsgId(msg_id),
            origin,
            dest,
            hops_left,
            ts,
            seq,
            body,
            signature,
        })
    }

    pub fn signed_payload(&self) -> Result<Vec<u8>> {
        // Sign header+body+crc without the signature bytes, with SIGNED already set.
        let mut copy = self.clone();
        copy.signature = None;
        copy.flags.set(FLAG_SIGNED, true);
        let encoded = copy.encode_without_sig_requirement()?;
        Ok(encoded)
    }

    fn encode_without_sig_requirement(&self) -> Result<Vec<u8>> {
        let mut flags = self.flags;
        flags.set(FLAG_SIGNED, true);
        let mut buf = Vec::with_capacity(HEADER_LEN + self.body.len() + CRC_LEN);
        buf.push(self.ver);
        buf.push(self.kind as u8);
        buf.extend_from_slice(&flags.raw().to_le_bytes());
        buf.extend_from_slice(&self.msg_id.0);
        buf.extend_from_slice(&self.origin.pack());
        buf.extend_from_slice(&self.dest.pack());
        buf.push(self.hops_left);
        buf.extend_from_slice(&self.ts.to_le_bytes());
        buf.extend_from_slice(&self.seq.to_le_bytes());
        buf.extend_from_slice(&(self.body.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.body);
        let crc = CRC.checksum(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        Ok(buf)
    }

    pub fn priority(&self) -> Priority {
        self.flags.priority()
    }
}

pub fn now_ts() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::flags::{FLAG_INET_OK, FLAG_REQ_ACK};

    #[test]
    fn roundtrip_msg() {
        let origin = Callsign::parse("G4ABC").unwrap();
        let dest = Callsign::parse("M0XYZ").unwrap();
        let env = Envelope::new_msg(
            origin,
            dest,
            7,
            b"hello radio".to_vec(),
            3,
            Flags::new().with(FLAG_INET_OK).with(FLAG_REQ_ACK),
        )
        .unwrap();
        let bytes = env.encode().unwrap();
        let back = Envelope::decode(&bytes).unwrap();
        assert_eq!(env, back);
        assert!(bytes.len() < 80);
    }

    #[test]
    fn crc_detects_corruption() {
        let origin = Callsign::parse("W1AW").unwrap();
        let dest = Callsign::parse("K1ABC").unwrap();
        let env = Envelope::new_msg(origin, dest, 1, b"x".to_vec(), 2, Flags::new()).unwrap();
        let mut bytes = env.encode().unwrap();
        bytes[HEADER_LEN] ^= 0xff;
        assert!(Envelope::decode(&bytes).is_err());
    }

    #[test]
    fn ack_carries_id() {
        let a = Callsign::parse("G4ABC").unwrap();
        let b = Callsign::parse("M0XYZ").unwrap();
        let env = Envelope::new_msg(
            a.clone(),
            b,
            1,
            b"hi".to_vec(),
            3,
            Flags::new().with(FLAG_REQ_ACK),
        )
        .unwrap();
        let ack = Envelope::ack_for(&env, Callsign::parse("M0XYZ").unwrap(), 9);
        let bytes = ack.encode().unwrap();
        let back = Envelope::decode(&bytes).unwrap();
        assert_eq!(back.acked_id().unwrap(), env.msg_id);
    }
}
