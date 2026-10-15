//! SPDX-License-Identifier: Apache-2.0
//! Internet mail on the air (`WCRM` chunks). Independent of chat bodies.

use crate::error::{Error, Result};
use crate::proto::MAX_BODY;
use serde::{Deserialize, Serialize};

pub const MAIL_MAX_BYTES: usize = 4096;
pub const MAIL_DOMAIN: &str = "mail.weechatradio.com";
pub const WCR_COPY_HEADER: &str = "X-WCR-Copy";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailOp {
    Data = 0,
    ListReq = 1,
    ListHdr = 2,
    GetReq = 3,
    HubSync = 4,
    VoxData = 5,
    ChunkAck = 6,
    CompleteAck = 7,
}

impl MailOp {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Data),
            1 => Some(Self::ListReq),
            2 => Some(Self::ListHdr),
            3 => Some(Self::GetReq),
            4 => Some(Self::HubSync),
            5 => Some(Self::VoxData),
            6 => Some(Self::ChunkAck),
            7 => Some(Self::CompleteAck),
            _ => None,
        }
    }

    pub fn wants_ack(self) -> bool {
        matches!(self, Self::VoxData)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailMeta {
    #[serde(default, alias = "f")]
    pub from: String,
    #[serde(default, alias = "t")]
    pub to: String,
    #[serde(default, alias = "s")]
    pub subject: String,
    #[serde(default)]
    pub ids: Vec<String>,
    /// Station callsign that should act on this frame.
    #[serde(default, alias = "d")]
    pub dest: String,
    /// Gateway callsign on the RF hop.
    #[serde(default, alias = "v")]
    pub via: String,
    /// True when the sender is waiting for `CompleteAck`. Radio-plus always sets this.
    #[serde(default, alias = "a")]
    pub ack: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailWire {
    pub op: MailOp,
    pub mail_id: String,
    pub idx: u16,
    pub count: u16,
    pub meta: MailMeta,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Wire(MailWire),
    /// A slice of a WCRM blob that did not fit the rung MTU.
    Slice {
        group: String,
        part: u16,
        parts: u16,
        data: String,
    },
}

pub fn wcr_address(callsign: &str) -> String {
    format!("{}@{}", callsign.trim().to_ascii_uppercase(), MAIL_DOMAIN)
}

pub fn callsign_from_wcr(addr: &str) -> Option<String> {
    let (local, domain) = addr.split_once('@')?;
    if domain.eq_ignore_ascii_case(MAIL_DOMAIN) && !local.is_empty() {
        Some(local.to_ascii_uppercase())
    } else {
        None
    }
}

pub fn validate_internet_addr(addr: &str) -> Result<()> {
    let s = addr.trim();
    if s.is_empty() {
        return Err(Error::config("Enter an internet address in To."));
    }
    if s.contains(char::is_whitespace) {
        return Err(Error::config("To must be a single email address."));
    }
    let Some((local, domain)) = s.split_once('@') else {
        return Err(Error::config("To must look like an email address."));
    };
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(Error::config("To must look like an email address."));
    }
    if domain.eq_ignore_ascii_case(MAIL_DOMAIN) {
        return Err(Error::config(
            "Ham-to-ham stays in Live Chat. Email To is an internet address.",
        ));
    }
    Ok(())
}

pub fn trim_body(body: &str) -> Result<String> {
    let t = body.trim();
    if t.len() > MAIL_MAX_BYTES {
        return Err(Error::config(format!(
            "Mail is {} bytes. The limit is {MAIL_MAX_BYTES} bytes of plain text.",
            t.len()
        )));
    }
    Ok(t.to_string())
}

/// Identity of a mail message. Routing fields (`dest`, `via`) are not part of it.
pub fn compute_mail_id(from: &str, to: &str, subject: &str, body: &str, ack: bool) -> String {
    blake3::hash(format!("{from}\n{to}\n{subject}\n{ack}\n{body}").as_bytes())
        .to_hex()
        .to_string()
}

/// What a station signs so a gateway may pull that station's inbox.
/// `ids` empty means "list". A timestamp stops a heard request being reused later.
pub fn pull_token(callsign: &str, ts: i64, ids: &[String]) -> Vec<u8> {
    let mut ids = ids.to_vec();
    ids.sort();
    format!(
        "wcr-mail-pull\n{}\n{ts}\n{}",
        callsign.trim().to_ascii_uppercase(),
        ids.join("\n")
    )
    .into_bytes()
}

fn wire_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

fn wire_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn meta_json(meta: &MailMeta) -> String {
    let mut m = serde_json::Map::new();
    if !meta.from.is_empty() {
        m.insert("f".into(), serde_json::Value::String(meta.from.clone()));
    }
    if !meta.to.is_empty() {
        m.insert("t".into(), serde_json::Value::String(meta.to.clone()));
    }
    if !meta.subject.is_empty() {
        m.insert("s".into(), serde_json::Value::String(meta.subject.clone()));
    }
    if !meta.dest.is_empty() {
        m.insert("d".into(), serde_json::Value::String(meta.dest.clone()));
    }
    if !meta.via.is_empty() {
        m.insert("v".into(), serde_json::Value::String(meta.via.clone()));
    }
    if meta.ack {
        m.insert("a".into(), serde_json::Value::Bool(true));
    }
    if !meta.ids.is_empty() {
        m.insert(
            "ids".into(),
            serde_json::Value::Array(
                meta.ids
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(m).to_string()
}

/// A chunk header is usable when its index is inside a non-zero total.
pub fn chunk_acceptable(idx: u16, count: u16) -> bool {
    count > 0 && idx < count
}

/// `b:<total>:<hex>` lists the chunk indexes already stored for that total.
/// Bit 0 of the first byte is index 0. A comma list of 46 indexes does not
/// fit one hf-poor frame; the bitmap does.
pub fn have_bitmap(total: u16, indexes: &[u16]) -> String {
    let nbytes = ((total as usize) + 7) / 8;
    let mut bits = vec![0u8; nbytes.max(1)];
    for &idx in indexes {
        if idx >= total {
            continue;
        }
        let byte = (idx / 8) as usize;
        bits[byte] |= 1 << (idx % 8);
    }
    format!("b:{total}:{}", hex::encode(bits))
}

/// `None` when `body` is a single chunk index, not a have-report.
pub fn parse_have_bitmap(body: &str) -> Option<(u16, Vec<u16>)> {
    let rest = body.trim().strip_prefix("b:")?;
    let (total_s, hexpart) = rest.split_once(':')?;
    let total: u16 = total_s.parse().ok()?;
    if total == 0 || hexpart.is_empty() || hexpart.len() > 64 || hexpart.len() % 2 != 0 {
        return None;
    }
    if !hexpart.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let bytes = hex::decode(hexpart).ok()?;
    let mut out = Vec::new();
    for (i, byte) in bytes.iter().enumerate() {
        for bit in 0..8u16 {
            if byte & (1 << bit) == 0 {
                continue;
            }
            let idx = (i as u16).saturating_mul(8).saturating_add(bit);
            if idx < total {
                out.push(idx);
            }
        }
    }
    Some((total, out))
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChunkPut {
    /// `count` is 0 or `idx` is past the total. Do not store it.
    Rejected,
    Pending,
    Ready(Vec<MailWire>),
}

/// One bucket per mail. The index is the key, not the total in the header.
/// A resend that stamps the byte length still fills the hole it belongs in.
/// The smaller total wins an index, so a real chunk count replaces a byte
/// length, and a repeat of the same total does not replace the stored copy.
pub fn put_chunk(slot: &mut std::collections::HashMap<u16, MailWire>, wire: MailWire) -> ChunkPut {
    if !chunk_acceptable(wire.idx, wire.count) {
        return ChunkPut::Rejected;
    }
    match slot.get(&wire.idx) {
        Some(prev) if prev.count <= wire.count => {}
        _ => {
            slot.insert(wire.idx, wire);
        }
    }
    let Some(count) = slot.values().map(|w| w.count).min() else {
        return ChunkPut::Pending;
    };
    if !(0..count).all(|i| slot.contains_key(&i)) {
        return ChunkPut::Pending;
    }
    let mut parts = Vec::with_capacity(count as usize);
    for i in 0..count {
        if let Some(part) = slot.remove(&i) {
            parts.push(part);
        }
    }
    slot.clear();
    ChunkPut::Ready(parts)
}

pub fn encode_chunk(w: &MailWire) -> Vec<u8> {
    let line = format!(
        "WCRM\t{}\t{}\t{}\t{}\t{}\t{}",
        w.op as u8,
        w.mail_id,
        w.idx,
        w.count,
        wire_escape(&meta_json(&w.meta)),
        wire_escape(&w.payload)
    );
    line.into_bytes()
}

pub fn decode_chunk(bytes: &[u8]) -> Result<MailWire> {
    let s = std::str::from_utf8(bytes).map_err(|e| Error::protocol(e.to_string()))?;
    if !s.starts_with("WCRM\t") {
        return Err(Error::protocol("not a mail chunk"));
    }
    let parts: Vec<&str> = s.splitn(7, '\t').collect();
    if parts.len() < 7 {
        return Err(Error::protocol("short mail chunk"));
    }
    let op = parts[1]
        .parse::<u8>()
        .ok()
        .and_then(MailOp::from_u8)
        .ok_or_else(|| Error::protocol("bad mail op"))?;
    let mail_id = parts[2].to_string();
    let idx = parts[3].parse().map_err(|_| Error::protocol("bad idx"))?;
    let count = parts[4].parse().map_err(|_| Error::protocol("bad count"))?;
    let meta: MailMeta = serde_json::from_str(&wire_unescape(parts[5])).unwrap_or_default();
    let payload = wire_unescape(parts[6]);
    Ok(MailWire {
        op,
        mail_id,
        idx,
        count,
        meta,
        payload,
    })
}

pub fn try_decode(payload: &[u8]) -> Option<Incoming> {
    if payload.starts_with(b"WCRM\t") {
        return decode_chunk(payload).ok().map(Incoming::Wire);
    }
    if payload.starts_with(b"WCRP\t") {
        let s = std::str::from_utf8(payload).ok()?;
        let parts: Vec<&str> = s.splitn(5, '\t').collect();
        if parts.len() < 5 {
            return None;
        }
        let part = parts[2].parse().ok()?;
        let parts_n = parts[3].parse().ok()?;
        return Some(Incoming::Slice {
            group: parts[1].to_string(),
            part,
            parts: parts_n,
            data: parts[4].to_string(),
        });
    }
    None
}

/// Split `body` so each encoded WCRM chunk is at most `limit` bytes (and `MAX_BODY`).
pub fn chunk_to_limit(mail_id: &str, meta: &MailMeta, body: &str, limit: usize) -> Vec<MailWire> {
    let limit = limit.clamp(1, MAX_BODY);
    let op = if meta.ack {
        MailOp::VoxData
    } else {
        MailOp::Data
    };
    let empty = MailMeta::default();
    let mut pieces: Vec<String> = Vec::new();
    let mut rest = body;
    let mut first = true;
    if body.is_empty() {
        pieces.push(String::new());
    }
    while !rest.is_empty() || pieces.is_empty() {
        let meta_now = if first { meta.clone() } else { empty.clone() };
        // Reserve three digits for the index and the total. Stamping the real
        // count afterwards used to push the frame past the rung, and each
        // chunk was then sliced into two.
        let (head, leftover) = take_fitting(rest, limit, |p| {
            encode_chunk(&MailWire {
                op,
                mail_id: mail_id.to_string(),
                idx: 999,
                count: 999,
                meta: meta_now.clone(),
                payload: p.to_string(),
            })
            .len()
                <= limit
        });
        pieces.push(head);
        rest = leftover;
        first = false;
        if rest.is_empty() {
            break;
        }
    }
    let n = pieces.len().max(1) as u16;
    pieces
        .into_iter()
        .enumerate()
        .map(|(i, p)| MailWire {
            op,
            mail_id: mail_id.to_string(),
            idx: i as u16,
            count: n,
            meta: if i == 0 { meta.clone() } else { empty.clone() },
            payload: p,
        })
        .collect()
}

fn take_fitting<'a>(
    text: &'a str,
    mut max: usize,
    fits: impl Fn(&str) -> bool,
) -> (String, &'a str) {
    if text.is_empty() {
        return (String::new(), "");
    }
    max = max.min(text.len()).max(1);
    loop {
        let (head, tail) = take_prefix(text, max);
        if fits(&head) || head.is_empty() {
            return (head, tail);
        }
        let next = head.len().saturating_sub(1);
        if next == 0 {
            return (head, tail);
        }
        max = next;
    }
}

fn take_prefix(text: &str, max: usize) -> (String, &str) {
    if text.len() <= max {
        return (text.to_string(), "");
    }
    let mut take = max;
    while take > 0 && !text.is_char_boundary(take) {
        take -= 1;
    }
    if take == 0 {
        take = text.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    if let Some(i) = text[..take].rfind(char::is_whitespace).filter(|&i| i > 0) {
        return (text[..i].to_string(), &text[i..]);
    }
    (text[..take].to_string(), &text[take..])
}

pub fn assemble_chunks(parts: Vec<MailWire>) -> Result<(MailMeta, String, MailOp)> {
    if parts.is_empty() {
        return Err(Error::protocol("no mail chunks"));
    }
    let mail_id = parts[0].mail_id.clone();
    let op = parts[0].op;
    let mut by_idx = std::collections::BTreeMap::new();
    for p in parts {
        if p.mail_id != mail_id || p.op != op {
            return Err(Error::protocol("mail chunk mismatch"));
        }
        by_idx.entry(p.idx).or_insert(p);
    }
    let n = by_idx.len() as u16;
    if n == 0 || (0..n).any(|i| !by_idx.contains_key(&i)) {
        return Err(Error::protocol("incomplete mail"));
    }
    let meta = by_idx.get(&0).map(|p| p.meta.clone()).unwrap_or_default();
    let mut body = String::new();
    for i in 0..n {
        body.push_str(&by_idx.get(&i).expect("index checked").payload);
    }
    Ok((meta, body, op))
}

/// Slice an encoded chunk so every keyed frame is at most `mtu` bytes.
/// A frame that already fits is sent as WCRM. Wider frames become WCRP slices.
pub fn frames_for_mtu(wire: &[u8], mtu: usize) -> Result<Vec<Vec<u8>>> {
    if mtu < 24 {
        return Err(Error::protocol("mail rung MTU is too small"));
    }
    if wire.len() <= mtu {
        return Ok(vec![wire.to_vec()]);
    }
    let text = std::str::from_utf8(wire).map_err(|e| Error::protocol(e.to_string()))?;
    let group = &blake3::hash(wire).to_hex().to_string()[..8];
    let mut slices: Vec<String> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let mut take = (mtu.saturating_sub(24)).min(rest.len()).max(1);
        while take > 0 && !rest.is_char_boundary(take) {
            take -= 1;
        }
        if take == 0 {
            take = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
        let mut piece = rest[..take].to_string();
        loop {
            let parts_guess = slices.len() + 2;
            let frame = format!("WCRP\t{group}\t{}\t{parts_guess}\t{piece}", slices.len());
            if frame.len() <= mtu || piece.chars().count() <= 1 {
                slices.push(piece);
                rest = &rest[take..];
                break;
            }
            piece.pop();
            while !piece.is_empty() && !rest.is_char_boundary(piece.len()) {
                piece.pop();
            }
            take = piece.len().max(1);
            if piece.is_empty() {
                return Err(Error::protocol("cannot fit mail frame to MTU"));
            }
        }
    }
    let n = slices.len() as u16;
    Ok(slices
        .into_iter()
        .enumerate()
        .map(|(i, data)| format!("WCRP\t{group}\t{i}\t{n}\t{data}").into_bytes())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(idx: u16, count: u16, payload: &str) -> MailWire {
        MailWire {
            op: MailOp::Data,
            mail_id: "m1".into(),
            idx,
            count,
            meta: MailMeta::default(),
            payload: payload.into(),
        }
    }

    #[test]
    fn have_bitmap_round_trips_and_fits_one_hf_poor_frame() {
        let indexes: Vec<u16> = (0..46).filter(|i| i % 2 == 0).collect();
        let body = have_bitmap(46, &indexes);
        let (total, got) = parse_have_bitmap(&body).expect("bitmap");
        assert_eq!(total, 46);
        assert_eq!(got, indexes);
        assert!(parse_have_bitmap("12").is_none());
        assert!(parse_have_bitmap("b:46:zz").is_none());
        let frame = encode_chunk(&MailWire {
            op: MailOp::ChunkAck,
            mail_id: "ab".repeat(32),
            idx: 0,
            count: 1,
            meta: MailMeta {
                dest: "QA85A".into(),
                ..MailMeta::default()
            },
            payload: body,
        });
        assert!(frame.len() <= 170, "have report is {} bytes", frame.len());
    }

    #[test]
    fn complete_ack_fits_one_hf_poor_frame() {
        let id = "ab".repeat(32);
        let meta = MailMeta {
            dest: "QA85B".into(),
            ..MailMeta::default()
        };
        let frame = encode_chunk(&MailWire {
            op: MailOp::CompleteAck,
            mail_id: id,
            idx: 0,
            count: 1,
            meta,
            payload: "ok".into(),
        });
        assert!(frame.len() <= 170, "complete ack is {} bytes", frame.len());
        let empty = encode_chunk(&MailWire {
            op: MailOp::ListHdr,
            mail_id: "cd".repeat(32),
            idx: 0,
            count: 1,
            meta: MailMeta {
                dest: "QA85A".into(),
                ..MailMeta::default()
            },
            payload: "[]".into(),
        });
        assert!(empty.len() <= 170, "empty list is {} bytes", empty.len());
    }

    #[test]
    fn a_bad_total_does_not_block_a_consistent_set() {
        let mut slot = std::collections::HashMap::new();
        assert_eq!(put_chunk(&mut slot, wire(6, 6, "x")), ChunkPut::Rejected);
        assert_eq!(put_chunk(&mut slot, wire(0, 0, "x")), ChunkPut::Rejected);
        // A byte-length total shares the mail. The real chunk count replaces it.
        assert!(matches!(
            put_chunk(&mut slot, wire(0, 121, "x")),
            ChunkPut::Pending
        ));
        assert!(matches!(
            put_chunk(&mut slot, wire(2, 121, "nope")),
            ChunkPut::Pending
        ));
        for i in 0..3 {
            assert!(matches!(
                put_chunk(&mut slot, wire(i, 6, "c")),
                ChunkPut::Pending
            ));
        }
        // A later try fills the holes. A repeat of an index already stored stays.
        assert!(matches!(
            put_chunk(&mut slot, wire(1, 6, "other")),
            ChunkPut::Pending
        ));
        assert!(matches!(
            put_chunk(&mut slot, wire(3, 6, "c")),
            ChunkPut::Pending
        ));
        assert!(matches!(
            put_chunk(&mut slot, wire(4, 6, "c")),
            ChunkPut::Pending
        ));
        match put_chunk(&mut slot, wire(5, 6, "c")) {
            ChunkPut::Ready(parts) => {
                let (_meta, body, op) = assemble_chunks(parts).unwrap();
                assert_eq!(op, MailOp::Data);
                assert_eq!(body, "cccccc");
            }
            other => panic!("expected a full set, got {other:?}"),
        }
    }

    #[test]
    fn a_resend_labeled_with_the_byte_length_still_completes() {
        let body = long_body(3900);
        let meta = sample_meta();
        let id = compute_mail_id(&meta.from, &meta.to, &meta.subject, &body, meta.ack);
        let limit = 170;
        let wires = chunk_to_limit(&id, &meta, &body, limit);
        let count = wires.len() as u16;
        assert!(count > 1, "expected a split mail, got {count}");
        assert!(wires.iter().all(|w| w.count == count));
        let mut slot = std::collections::HashMap::new();
        let missing: Vec<u16> = wires.iter().map(|w| w.idx).filter(|i| i % 7 == 0).collect();
        assert!(!missing.is_empty());
        for w in wires.iter().filter(|w| !missing.contains(&w.idx)) {
            assert!(
                matches!(put_chunk(&mut slot, w.clone()), ChunkPut::Pending),
                "partial set must not complete"
            );
        }
        let mut last = ChunkPut::Pending;
        for idx in &missing {
            let mut w = wires[*idx as usize].clone();
            // The bug: a resend stamped the body length instead of the chunk count.
            w.count = body.len() as u16;
            last = put_chunk(&mut slot, w);
        }
        match last {
            ChunkPut::Ready(parts) => {
                let (_meta, text, op) = assemble_chunks(parts).unwrap();
                assert_eq!(op, MailOp::VoxData);
                assert_eq!(text, body);
            }
            other => panic!("resend did not complete the mail: {other:?}"),
        }
    }

    /// Every chunk of a 3.9 KB mail, the last one included, fits the rung it is
    /// keyed on and decodes back to the same wire on each hf-poor try.
    #[test]
    fn every_chunk_of_a_long_mail_round_trips_on_each_rung() {
        let para =
            "Line one of the report, with words — and a tab\there.\nSecond line ends here\n\n";
        let mut body = para.repeat(3900 / para.len() + 1);
        while body.len() > 3900 {
            body.pop();
        }
        let body = trim_body(&body).unwrap();
        let meta = sample_meta();
        let id = compute_mail_id(&meta.from, &meta.to, &meta.subject, &body, meta.ack);
        let wires = chunk_to_limit(&id, &meta, &body, 170);
        assert!(
            wires.len() > 40,
            "expected a long split, got {}",
            wires.len()
        );
        let last = wires.last().unwrap();
        assert_eq!(last.idx + 1, last.count);
        for mtu in [170usize, 55] {
            let mut back = Vec::new();
            for w in &wires {
                let frames = frames_for_mtu(&encode_chunk(w), mtu).unwrap();
                assert!(frames.iter().all(|f| f.len() <= mtu));
                let mut slices = Vec::new();
                for f in &frames {
                    match try_decode(f).expect("mail frame") {
                        Incoming::Wire(got) => back.push(got),
                        Incoming::Slice { part, data, .. } => slices.push((part, data)),
                    }
                }
                if !slices.is_empty() {
                    back.push(decode_chunk(&join_slices(&slices).unwrap()).unwrap());
                }
            }
            assert_eq!(back, wires, "mtu {mtu}");
            let (_, text, _) = assemble_chunks(back).unwrap();
            assert_eq!(text, body, "mtu {mtu}");
        }
    }

    fn long_body(n: usize) -> String {
        let word = "hello mail ";
        let mut body = word.repeat(n / word.len());
        body.push_str(&"x".repeat(n - body.len()));
        assert_eq!(body.len(), n);
        body
    }

    fn sample_meta() -> MailMeta {
        MailMeta {
            from: "M7TJF@mail.weechatradio.com".into(),
            to: "operator@example.com".into(),
            subject: "subject".into(),
            dest: "GATEWAY".into(),
            via: "GATEWAY".into(),
            ack: true,
            ..MailMeta::default()
        }
    }
}

pub fn join_slices(parts: &[(u16, String)]) -> Result<Vec<u8>> {
    if parts.is_empty() {
        return Err(Error::protocol("no slices"));
    }
    let mut ordered = parts.to_vec();
    ordered.sort_by_key(|(i, _)| *i);
    let mut out = String::new();
    for (i, (idx, data)) in ordered.iter().enumerate() {
        if *idx as usize != i {
            return Err(Error::protocol("missing mail slice"));
        }
        out.push_str(data);
    }
    Ok(out.into_bytes())
}
