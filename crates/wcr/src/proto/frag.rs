//! SPDX-License-Identifier: Apache-2.0
//! Reed-Solomon erasure fragments for group / oversized RF frames.

use crate::error::{Error, Result};
use crate::proto::envelope::{Envelope, MsgType, VERSION};
use crate::proto::flags::Flags;
use crate::proto::ids::MsgId;
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Wire header: group_id u32, idx u8, k u8, m u8, orig_kind u8, orig_len u16.
pub const FRAG_HDR: usize = 4 + 1 + 1 + 1 + 1 + 2;

const ASSEMBLER_TTL: Duration = Duration::from_secs(120);

fn group_id(env: &Envelope) -> u32 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(env.origin.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(env.dest.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(&env.seq.to_le_bytes());
    let h = hasher.finalize();
    u32::from_le_bytes(h.as_bytes()[..4].try_into().unwrap())
}

fn shard_size(orig_len: usize, k: usize) -> usize {
    ((orig_len + k - 1) / k).max(1)
}

/// Split `env` into `k` data + `m` parity Frag envelopes. Any `k` reconstruct the body.
pub fn split(env: &Envelope, k: u8, m: u8) -> Result<Vec<Envelope>> {
    let k = k.max(1) as usize;
    let m = m.max(1) as usize;
    if k + m > 16 {
        return Err(Error::protocol("too many erasure shards"));
    }
    let orig = &env.body;
    let size = shard_size(orig.len(), k);
    let mut data = vec![0u8; size * k];
    data[..orig.len()].copy_from_slice(orig);
    let mut shards: Vec<Vec<u8>> = (0..k)
        .map(|i| data[i * size..(i + 1) * size].to_vec())
        .collect();
    shards.extend((0..m).map(|_| vec![0u8; size]));
    let rs = ReedSolomon::new(k, m).map_err(|e| Error::protocol(format!("rs: {e}")))?;
    rs.encode(&mut shards)
        .map_err(|e| Error::protocol(format!("rs encode: {e}")))?;

    let gid = group_id(env);
    let mut out = Vec::with_capacity(k + m);
    for (idx, shard) in shards.into_iter().enumerate() {
        let mut body = Vec::with_capacity(FRAG_HDR + shard.len());
        body.extend_from_slice(&gid.to_le_bytes());
        body.push(idx as u8);
        body.push(k as u8);
        body.push(m as u8);
        body.push(env.kind as u8);
        body.extend_from_slice(&(orig.len() as u16).to_le_bytes());
        body.extend_from_slice(&shard);
        let mut flags = env.flags;
        flags.set(crate::proto::flags::FLAG_REQ_ACK, false);
        flags.set(crate::proto::flags::FLAG_SIGNED, false);
        let frag = Envelope {
            ver: VERSION,
            kind: MsgType::Frag,
            flags,
            msg_id: MsgId::compute(&env.origin, &env.dest, env.seq, &body),
            origin: env.origin.clone(),
            dest: env.dest.clone(),
            hops_left: env.hops_left,
            ts: env.ts,
            seq: env.seq,
            body,
            signature: None,
        };
        out.push(frag);
    }
    Ok(out)
}

struct Group {
    k: u8,
    m: u8,
    orig_kind: MsgType,
    orig_len: u16,
    origin: crate::proto::Callsign,
    dest: crate::proto::Callsign,
    hops_left: u8,
    ts: u32,
    seq: u32,
    flags: Flags,
    shards: HashMap<u8, Vec<u8>>,
    first_seen: Instant,
}

/// Collects Frag envelopes until any `k` shards reconstruct the original message.
///
/// Wire `group_id` is 32 bits of BLAKE3; collisions are rare. Incoming shards
/// whose header metadata does not match the first shard for that id are rejected
/// rather than mixed into the same group.
pub struct FragAssembler {
    groups: HashMap<u32, Group>,
}

impl Default for FragAssembler {
    fn default() -> Self {
        Self::new()
    }
}

impl FragAssembler {
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }

    pub fn push(&mut self, env: &Envelope) -> Result<Option<Envelope>> {
        self.gc();
        if env.kind != MsgType::Frag || env.body.len() < FRAG_HDR {
            return Ok(None);
        }
        let gid = u32::from_le_bytes(env.body[0..4].try_into().unwrap());
        let idx = env.body[4];
        let k = env.body[5];
        let m = env.body[6];
        let orig_kind = MsgType::from_u8(env.body[7])?;
        let orig_len = u16::from_le_bytes(env.body[8..10].try_into().unwrap());
        let shard = env.body[FRAG_HDR..].to_vec();
        if k == 0 || m == 0 || idx as usize >= (k as usize + m as usize) {
            return Err(Error::protocol("bad frag header"));
        }

        {
            let g = self.groups.entry(gid).or_insert_with(|| Group {
                k,
                m,
                orig_kind,
                orig_len,
                origin: env.origin.clone(),
                dest: env.dest.clone(),
                hops_left: env.hops_left,
                ts: env.ts,
                seq: env.seq,
                flags: env.flags,
                shards: HashMap::new(),
                first_seen: Instant::now(),
            });
            if g.k != k
                || g.m != m
                || g.orig_kind != orig_kind
                || g.orig_len != orig_len
                || g.origin != env.origin
                || g.dest != env.dest
                || g.seq != env.seq
                || g.flags != env.flags
            {
                return Err(Error::protocol("frag group metadata mismatch"));
            }
            g.shards.insert(idx, shard);
            if g.shards.len() < g.k as usize {
                return Ok(None);
            }
        }
        let g = match self.groups.remove(&gid) {
            Some(g) => g,
            None => return Ok(None),
        };
        match reconstruct(&g) {
            Ok(body) => {
                let flags = g.flags;
                let msg_id = MsgId::compute(&g.origin, &g.dest, g.seq, &body);
                Ok(Some(Envelope {
                    ver: VERSION,
                    kind: g.orig_kind,
                    flags,
                    msg_id,
                    origin: g.origin,
                    dest: g.dest,
                    hops_left: g.hops_left,
                    ts: g.ts,
                    seq: g.seq,
                    body,
                    signature: None,
                }))
            }
            Err(_) => {
                self.groups.insert(gid, g);
                Ok(None)
            }
        }
    }

    fn gc(&mut self) {
        let now = Instant::now();
        self.groups
            .retain(|_, g| now.duration_since(g.first_seen) < ASSEMBLER_TTL);
    }
}

fn reconstruct(g: &Group) -> Result<Vec<u8>> {
    let k = g.k as usize;
    let m = g.m as usize;
    let rs = ReedSolomon::new(k, m).map_err(|e| Error::protocol(format!("rs: {e}")))?;
    let size = g
        .shards
        .values()
        .next()
        .map(|s| s.len())
        .ok_or_else(|| Error::protocol("empty frag group"))?;
    let mut shards: Vec<Option<Vec<u8>>> = vec![None; k + m];
    for (&i, s) in &g.shards {
        if (i as usize) < k + m && s.len() == size {
            shards[i as usize] = Some(s.clone());
        }
    }
    rs.reconstruct(&mut shards)
        .map_err(|e| Error::protocol(format!("rs reconstruct: {e}")))?;
    let mut body = Vec::with_capacity(g.orig_len as usize);
    for shard in shards.into_iter().take(k) {
        let s = shard.ok_or_else(|| Error::protocol("missing data shard after reconstruct"))?;
        body.extend_from_slice(&s);
    }
    body.truncate(g.orig_len as usize);
    Ok(body)
}

/// True when this envelope should go out as erasure-coded fragments on RF.
pub fn should_fragment(env: &Envelope, group_on_hf: bool, encoded_len: usize, mtu: u32) -> bool {
    if matches!(
        env.kind,
        MsgType::Ack | MsgType::Beacon | MsgType::Frag | MsgType::Ping
    ) {
        return false;
    }
    if encoded_len > mtu as usize {
        return true;
    }
    group_on_hf && env.flags.group() && !env.body.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Callsign, Flags, FLAG_GROUP};

    fn sample() -> Envelope {
        Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::from_raw("BULLETIN"),
            7,
            b"weak signal bulletin text that must survive a fade".to_vec(),
            3,
            Flags::new().with(FLAG_GROUP),
        )
        .unwrap()
    }

    #[test]
    fn reconstruct_with_any_k_of_k_plus_m() {
        let env = sample();
        let frags = split(&env, 2, 1).unwrap();
        assert_eq!(frags.len(), 3);
        for drop_idx in 0..3 {
            let mut a = FragAssembler::new();
            let mut got = None;
            for (i, f) in frags.iter().enumerate() {
                if i == drop_idx {
                    continue;
                }
                got = a.push(f).unwrap();
            }
            let back = got.expect("reconstructed");
            assert_eq!(back.body, env.body);
            assert_eq!(back.origin, env.origin);
            assert_eq!(back.dest, env.dest);
            assert_eq!(back.kind, MsgType::Msg);
        }
    }

    #[test]
    fn two_shards_not_enough_when_k_is_2_and_one_is_parity_only_wait() {
        let env = sample();
        let frags = split(&env, 2, 1).unwrap();
        let mut a = FragAssembler::new();
        assert!(a.push(&frags[0]).unwrap().is_none());
        let got = a.push(&frags[1]).unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().body, env.body);
    }

    #[test]
    fn mismatching_metadata_on_same_gid_is_rejected() {
        let env = sample();
        let frags = split(&env, 2, 1).unwrap();
        let mut a = FragAssembler::new();
        assert!(a.push(&frags[0]).unwrap().is_none());
        let mut bad = frags[1].clone();
        bad.body[5] = 3;
        assert!(a.push(&bad).is_err());
        let got = a.push(&frags[1]).unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().body, env.body);
    }
}
