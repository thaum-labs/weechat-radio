//! SPDX-License-Identifier: Apache-2.0
//! Envelope types and codec (v1 on the wire for legacy, v2 by default).

use crate::error::{Error, Result};
use crate::proto::callsign::{Callsign, PACKED_LEN};
use crate::proto::compress;
use crate::proto::flags::{Flags, Priority, FLAG_COMPRESSED, FLAG_GROUP_IDX, FLAG_SIGNED};
use crate::proto::ids::{MsgId, MSG_ID_LEN};
use serde::{Deserialize, Serialize};

/// Current on-air / hub version. Decode still accepts [`VERSION_V1`].
pub const VERSION: u8 = 2;
pub const VERSION_V1: u8 = 1;
/// Bytes before body in a v1 frame.
pub const HEADER_LEN: usize = 1 + 1 + 2 + MSG_ID_LEN + PACKED_LEN + PACKED_LEN + 1 + 4 + 4 + 2;
pub const CRC_LEN: usize = 2;
pub const SIG_LEN: usize = 64;
pub const MAX_BODY: usize = 300;

/// Split chat so each piece fits [`MAX_BODY`] (and the radio MTU).
/// Prefers whitespace; a single over-long token is hard-cut on a char boundary.
pub fn split_body_chunks(text: &str, max: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let max = max.max(1);
    if text.len() <= max {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if rest.len() <= max {
            out.push(rest.to_string());
            break;
        }
        let mut take = max;
        while take > 0 && !rest.is_char_boundary(take) {
            take -= 1;
        }
        if take == 0 {
            take = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
        let slice = &rest[..take];
        let cut = slice
            .rfind(char::is_whitespace)
            .filter(|&i| i > 0)
            .unwrap_or(take);
        let chunk = slice[..cut].trim_end();
        if chunk.is_empty() {
            out.push(slice.to_string());
            rest = rest[take..].trim_start();
        } else {
            out.push(chunk.to_string());
            rest = rest[cut..].trim_start();
        }
    }
    out
}

const CRC: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_IBM_3740);

/// Well-known group destinations packed as a single index byte on v2.
const GROUP_TABLE: &[&str] = &["BULLETIN", "BEACON"];

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
    Frag = 10,
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
            10 => Ok(Self::Frag),
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
            Self::Frag => "frag",
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

    pub fn ack_for(original: &Envelope, from: Callsign, seq: u32, snr_db: Option<f32>) -> Self {
        let mut flags = Flags::new();
        if original.flags.inet_ok() {
            flags = flags.with(crate::proto::flags::FLAG_INET_OK);
        }
        if original.flags.no_inet() {
            flags = flags.with(crate::proto::flags::FLAG_NO_INET);
        }
        let mut body = original.msg_id.0.to_vec();
        if let Some(snr) = snr_db {
            let q = snr.round().clamp(-32.0, 31.0) as i8;
            body.push(q as u8);
        }
        Self {
            ver: VERSION,
            kind: MsgType::Ack,
            flags,
            msg_id: MsgId::compute(&from, &original.origin, seq, &body),
            origin: from,
            dest: original.origin.clone(),
            hops_left: original.flags.priority().default_ttl(),
            ts: now_ts(),
            seq,
            body,
            signature: None,
        }
    }

    pub fn acked_id(&self) -> Option<MsgId> {
        if self.kind != MsgType::Ack || self.body.len() < MSG_ID_LEN {
            return None;
        }
        let mut id = [0u8; MSG_ID_LEN];
        id.copy_from_slice(&self.body[..MSG_ID_LEN]);
        Some(MsgId(id))
    }

    /// Receiver SNR in dB, if the ACK carried the extra byte.
    pub fn acked_snr(&self) -> Option<f32> {
        if self.kind != MsgType::Ack || self.body.len() < MSG_ID_LEN + 1 {
            return None;
        }
        Some(self.body[MSG_ID_LEN] as i8 as f32)
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Unsigned copy used on slow HF so the PHY frame stays small.
    pub fn without_signature(&self) -> Self {
        let mut c = self.clone();
        c.signature = None;
        c.flags.set(FLAG_SIGNED, false);
        c
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.ver == VERSION_V1 {
            self.encode_v1(true)
        } else {
            self.encode_v2(true)
        }
    }

    /// Canonical bytes that the Ed25519 signature covers (no signature trailer).
    pub fn signed_payload(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.signature = None;
        copy.flags.set(FLAG_SIGNED, true);
        if copy.ver == VERSION_V1 {
            copy.encode_v1(false)
        } else {
            copy.encode_v2(false)
        }
    }

    fn encode_v1(&self, require_sig: bool) -> Result<Vec<u8>> {
        if self.body.len() > MAX_BODY {
            return Err(Error::protocol("body too large"));
        }
        let mut buf = Vec::with_capacity(HEADER_LEN + self.body.len() + CRC_LEN + SIG_LEN);
        buf.push(VERSION_V1);
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
        self.append_sig(&mut buf, require_sig)?;
        Ok(buf)
    }

    fn encode_v2(&self, require_sig: bool) -> Result<Vec<u8>> {
        if self.body.len() > MAX_BODY {
            return Err(Error::protocol("body too large"));
        }
        let (wire_body, compressed) = match compress::compress(&self.body) {
            Some(c) => (c, true),
            None => (self.body.clone(), false),
        };
        let mut flags = self.flags;
        flags.set(FLAG_COMPRESSED, compressed);
        let group_idx = group_index(self.dest.as_str());
        let use_idx = (self.flags.group() || self.kind == MsgType::Beacon) && group_idx.is_some();
        flags.set(FLAG_GROUP_IDX, use_idx);

        let packed = ((self.kind as u16) << 12) | (flags.raw() & 0x0FFF);
        let mut buf = Vec::with_capacity(24 + wire_body.len() + CRC_LEN + SIG_LEN);
        buf.push(VERSION);
        buf.extend_from_slice(&packed.to_le_bytes());
        buf.extend_from_slice(&self.origin.pack());
        if use_idx {
            buf.push(group_idx.unwrap());
        } else {
            buf.extend_from_slice(&self.dest.pack());
        }
        buf.push(self.hops_left);
        buf.extend_from_slice(&pack_minutes(self.ts).to_le_bytes());
        put_varint(&mut buf, self.seq);
        put_varint(&mut buf, wire_body.len() as u32);
        buf.extend_from_slice(&wire_body);
        let crc = CRC.checksum(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        self.append_sig(&mut buf, require_sig)?;
        Ok(buf)
    }

    fn append_sig(&self, buf: &mut Vec<u8>, require_sig: bool) -> Result<()> {
        if let Some(sig) = self.signature {
            buf.extend_from_slice(&sig);
        } else if self.flags.signed() && require_sig {
            return Err(Error::protocol("SIGNED flag set but no signature"));
        }
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(Error::protocol("frame too short"));
        }
        match bytes[0] {
            VERSION_V1 => decode_v1(bytes),
            VERSION => decode_v2(bytes),
            ver => Err(Error::protocol(format!(
                "unsupported protocol version {ver}"
            ))),
        }
    }

    pub fn priority(&self) -> Priority {
        self.flags.priority()
    }
}

fn decode_v1(bytes: &[u8]) -> Result<Envelope> {
    if bytes.len() < HEADER_LEN + CRC_LEN {
        return Err(Error::protocol("frame too short"));
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
    finish_decode(
        VERSION_V1,
        kind,
        flags,
        Some(MsgId(msg_id)),
        o,
        Callsign::unpack(&d)?,
        hops_left,
        ts,
        seq,
        body_len,
        bytes,
        i,
    )
}

fn decode_v2(bytes: &[u8]) -> Result<Envelope> {
    // ver(1) + packed(2) + origin(6) + dest(1 or 6) + hops(1) + ts(2) + seq(>=1) + len(>=1)
    if bytes.len() < 1 + 2 + PACKED_LEN + 1 + 1 + 2 + 1 + 1 + CRC_LEN {
        return Err(Error::protocol("frame too short"));
    }
    let packed = u16::from_le_bytes([bytes[1], bytes[2]]);
    let kind = MsgType::from_u8((packed >> 12) as u8)?;
    let mut flags = Flags(packed & 0x0FFF);
    let mut i = 3;
    let mut o = [0u8; PACKED_LEN];
    if i + PACKED_LEN > bytes.len() {
        return Err(Error::protocol("truncated origin"));
    }
    o.copy_from_slice(&bytes[i..i + PACKED_LEN]);
    i += PACKED_LEN;
    let dest = if flags.group_idx() {
        if i >= bytes.len() {
            return Err(Error::protocol("truncated group dest"));
        }
        let idx = bytes[i];
        i += 1;
        lookup_group(idx)?
    } else {
        if i + PACKED_LEN > bytes.len() {
            return Err(Error::protocol("truncated dest"));
        }
        let mut d = [0u8; PACKED_LEN];
        d.copy_from_slice(&bytes[i..i + PACKED_LEN]);
        i += PACKED_LEN;
        Callsign::unpack(&d)?
    };
    if i + 1 + 2 > bytes.len() {
        return Err(Error::protocol("truncated header"));
    }
    let hops_left = bytes[i];
    i += 1;
    let minutes = u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap());
    i += 2;
    let ts = unpack_minutes(minutes, now_ts());
    let seq = get_varint(bytes, &mut i)?;
    let body_len = get_varint(bytes, &mut i)? as usize;
    let env = finish_decode(
        VERSION, kind, flags, None, o, dest, hops_left, ts, seq, body_len, bytes, i,
    )?;
    flags.set(FLAG_GROUP_IDX, false);
    flags.set(FLAG_COMPRESSED, false);
    let mut env = env;
    env.flags.set(FLAG_GROUP_IDX, false);
    env.flags.set(FLAG_COMPRESSED, false);
    Ok(env)
}

fn finish_decode(
    ver: u8,
    kind: MsgType,
    flags: Flags,
    msg_id: Option<MsgId>,
    origin_packed: [u8; PACKED_LEN],
    dest: Callsign,
    hops_left: u8,
    ts: u32,
    seq: u32,
    body_len: usize,
    bytes: &[u8],
    body_at: usize,
) -> Result<Envelope> {
    if body_len > MAX_BODY {
        return Err(Error::protocol("body too large"));
    }
    let crc_at = body_at + body_len;
    if bytes.len() < crc_at + CRC_LEN {
        return Err(Error::protocol("truncated body"));
    }
    let mut body = bytes[body_at..crc_at].to_vec();
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
    }
    if flags.compressed() {
        body = compress::decompress(&body)?;
    }
    let origin = Callsign::unpack(&origin_packed)?;
    let msg_id = msg_id.unwrap_or_else(|| MsgId::compute(&origin, &dest, seq, &body));
    Ok(Envelope {
        ver,
        kind,
        flags: {
            let mut f = flags;
            f.set(FLAG_COMPRESSED, false);
            f.set(FLAG_GROUP_IDX, false);
            f
        },
        msg_id,
        origin,
        dest,
        hops_left,
        ts,
        seq,
        body,
        signature,
    })
}

fn group_index(name: &str) -> Option<u8> {
    let n = name.trim().trim_start_matches('#').trim_start_matches('&');
    GROUP_TABLE
        .iter()
        .position(|g| g.eq_ignore_ascii_case(n))
        .map(|i| i as u8)
}

fn lookup_group(idx: u8) -> Result<Callsign> {
    GROUP_TABLE
        .get(idx as usize)
        .map(|s| Callsign::from_raw((*s).to_string()))
        .ok_or_else(|| Error::protocol(format!("unknown group index {idx}")))
}

fn pack_minutes(ts: u32) -> u16 {
    ((ts / 60) % 65536) as u16
}

fn unpack_minutes(wire: u16, now: u32) -> u32 {
    let now_m = now / 60;
    let base = now_m & !0xFFFFu32;
    let mut cand = base.saturating_add(wire as u32);
    if cand > now_m.saturating_add(32768) {
        cand = cand.saturating_sub(65536);
    } else if now_m > cand.saturating_add(32768) {
        cand = cand.saturating_add(65536);
    }
    cand.saturating_mul(60)
}

fn put_varint(buf: &mut Vec<u8>, mut v: u32) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        buf.push(b);
        if v == 0 {
            break;
        }
    }
}

fn get_varint(bytes: &[u8], i: &mut usize) -> Result<u32> {
    let mut r = 0u32;
    let mut shift = 0;
    loop {
        if *i >= bytes.len() {
            return Err(Error::protocol("truncated varint"));
        }
        let b = bytes[*i];
        *i += 1;
        r |= ((b & 0x7f) as u32) << shift;
        if b & 0x80 == 0 {
            return Ok(r);
        }
        shift += 7;
        if shift > 28 {
            return Err(Error::protocol("varint overflow"));
        }
    }
}

pub fn now_ts() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(0)
}

/// How far a timestamp may be from now before we call it clock skew.
pub const CLOCK_SKEW_SECS: u32 = 300;

fn is_clock_probe(kind: MsgType) -> bool {
    matches!(
        kind,
        MsgType::Beacon | MsgType::Ping | MsgType::Checkin | MsgType::Status
    )
}

/// Update the station clock-skew latch.
///
/// Delayed chat / ACK / form frames are normal on radio (queue, ARQ, store-and-forward)
/// and must not raise or keep the warning. Live probes and timestamps in the future do.
/// A timely frame of any kind clears a previous warning (e.g. after NTP/Dimension 4).
pub fn clock_warn_after(prev: bool, kind: MsgType, ts: u32, now: u32) -> bool {
    let future = ts > now.saturating_add(CLOCK_SKEW_SECS);
    if future {
        return true;
    }
    let late = ts.abs_diff(now) > CLOCK_SKEW_SECS;
    if is_clock_probe(kind) {
        return late;
    }
    if !late {
        return false;
    }
    prev
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::flags::{FLAG_INET_OK, FLAG_REQ_ACK};

    #[test]
    fn long_chat_splits_on_words() {
        let text = "lorem ipsum dolor sit amet consectetur adipiscing elit dignissimos rerum nobis dignissimos ullamco nisi id cillum atque elit omnis vero lorem est quibusdam aut rerum rerum nam est sunt quod vel laborum laborum irure fugiat at provident ullamco id atque dolorum lorem ut laborum aliquip consequatur dolor anim facere incididunt";
        assert!(text.len() > MAX_BODY);
        let parts = split_body_chunks(text, MAX_BODY);
        assert!(parts.len() >= 2);
        assert!(parts.iter().all(|p| p.len() <= MAX_BODY));
        assert_eq!(parts.join(" "), text);
        assert_eq!(
            split_body_chunks("hello radio", MAX_BODY),
            vec!["hello radio"]
        );
        let word = "a".repeat(MAX_BODY + 8);
        let hard = split_body_chunks(&word, MAX_BODY);
        assert_eq!(hard.len(), 2);
        assert!(hard.iter().all(|p| p.len() <= MAX_BODY));
    }

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
        assert_eq!(env.origin, back.origin);
        assert_eq!(env.dest, back.dest);
        assert_eq!(env.body, back.body);
        assert_eq!(env.msg_id, back.msg_id);
        assert_eq!(env.seq, back.seq);
        assert_eq!(env.kind, back.kind);
        assert!(bytes.len() < 80);
        assert!(bytes.len() < HEADER_LEN + env.body.len() + CRC_LEN); // tighter than v1
    }

    #[test]
    fn v1_legacy_roundtrip() {
        let origin = Callsign::parse("G4ABC").unwrap();
        let dest = Callsign::parse("M0XYZ").unwrap();
        let mut env = Envelope::new_msg(
            origin,
            dest,
            3,
            b"legacy".to_vec(),
            2,
            Flags::new().with(FLAG_REQ_ACK),
        )
        .unwrap();
        env.ver = VERSION_V1;
        let bytes = env.encode().unwrap();
        assert_eq!(bytes[0], VERSION_V1);
        let back = Envelope::decode(&bytes).unwrap();
        assert_eq!(back.body, b"legacy");
        assert_eq!(back.msg_id, env.msg_id);
        assert_eq!(back.ver, VERSION_V1);
    }

    #[test]
    fn crc_detects_corruption() {
        let origin = Callsign::parse("W1AW").unwrap();
        let dest = Callsign::parse("K1ABC").unwrap();
        let env = Envelope::new_msg(origin, dest, 1, b"x".to_vec(), 2, Flags::new()).unwrap();
        let mut bytes = env.encode().unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xff;
        assert!(Envelope::decode(&bytes).is_err());
    }

    #[test]
    fn ack_carries_id_and_snr() {
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
        let ack = Envelope::ack_for(&env, Callsign::parse("M0XYZ").unwrap(), 9, Some(12.4));
        let bytes = ack.encode().unwrap();
        let back = Envelope::decode(&bytes).unwrap();
        assert_eq!(back.acked_id().unwrap(), env.msg_id);
        assert_eq!(back.acked_snr().unwrap(), 12.0);
    }

    #[test]
    fn group_dest_is_one_byte() {
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::from_raw("BULLETIN"),
            1,
            b"hi".to_vec(),
            3,
            Flags::new().with(crate::proto::flags::FLAG_GROUP),
        )
        .unwrap();
        let bytes = env.encode().unwrap();
        let back = Envelope::decode(&bytes).unwrap();
        assert_eq!(back.dest.as_str(), "BULLETIN");
        // ver + packed + origin + 1 dest + hops + ts + seq + len + body + crc
        assert!(bytes.len() < 40);
    }

    #[test]
    fn msg_id_recomputed_not_on_wire() {
        let env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            4,
            b"id".to_vec(),
            2,
            Flags::new(),
        )
        .unwrap();
        let bytes = env.encode().unwrap();
        // v2 does not start with a raw msg_id after the type/flags word.
        assert_eq!(bytes[0], VERSION);
        let back = Envelope::decode(&bytes).unwrap();
        assert_eq!(back.msg_id, env.msg_id);
    }

    #[test]
    fn delayed_chat_is_not_clock_skew() {
        let now = 1_000_000;
        let old = now - CLOCK_SKEW_SECS - 60;
        let soon = now + 30;
        let future = now + CLOCK_SKEW_SECS + 10;
        assert!(!clock_warn_after(false, MsgType::Msg, old, now));
        assert!(clock_warn_after(true, MsgType::Msg, old, now));
        assert!(!clock_warn_after(true, MsgType::Msg, soon, now));
        assert!(clock_warn_after(false, MsgType::Msg, future, now));
        assert!(clock_warn_after(false, MsgType::Beacon, old, now));
        assert!(!clock_warn_after(true, MsgType::Beacon, soon, now));
        assert!(!clock_warn_after(true, MsgType::Ack, soon, now));
    }
}
