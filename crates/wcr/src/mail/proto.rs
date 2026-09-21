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
    /// True when the sender is waiting for `CompleteAck` (VOX).
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
    serde_json::json!({
        "f": meta.from,
        "t": meta.to,
        "s": meta.subject,
        "d": meta.dest,
        "v": meta.via,
        "a": meta.ack,
        "ids": meta.ids,
    })
    .to_string()
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
        let (head, leftover) = take_fitting(rest, limit, |p| {
            encode_chunk(&MailWire {
                op,
                mail_id: mail_id.to_string(),
                idx: 0,
                count: 1,
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
    let count = parts[0].count;
    let op = parts[0].op;
    if count == 0 {
        return Err(Error::protocol("bad mail chunk count"));
    }
    let mut by_idx = std::collections::BTreeMap::new();
    for p in parts {
        if p.mail_id != mail_id || p.count != count || p.op != op {
            return Err(Error::protocol("mail chunk mismatch"));
        }
        by_idx.entry(p.idx).or_insert(p);
    }
    if by_idx.len() != count as usize {
        return Err(Error::protocol("incomplete mail"));
    }
    let meta = by_idx.get(&0).map(|p| p.meta.clone()).unwrap_or_default();
    let mut body = String::new();
    for i in 0..count {
        body.push_str(
            &by_idx
                .get(&i)
                .ok_or_else(|| Error::protocol("missing chunk"))?
                .payload,
        );
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
