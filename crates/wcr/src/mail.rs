//! SPDX-License-Identifier: Apache-2.0
//! Internet mail payloads for RF/hub (`MsgType::Mail`).

use crate::error::{Error, Result};
use crate::proto::callsign::is_plausible_callsign;
use crate::proto::MAX_BODY;
use serde::{Deserialize, Serialize};

/// Plain-text body cap after HTML strip (v1).
pub const MAIL_MAX_BYTES: usize = 4096;
pub const MAIL_DOMAIN: &str = "mail.weechatradio.com";
pub const WCR_COPY_HEADER: &str = "X-WCR-Copy";
/// RF check-mail session limits (v1).
pub const CHECK_MAIL_MAX_MSGS: usize = 5;
pub const CHECK_MAIL_MAX_BYTES: usize = 20 * 1024;

/// Metadata bytes reserved in each 300-byte envelope chunk (after the `WCRM` prefix).
pub const MAIL_CHUNK_META: usize = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailOp {
    /// Outbound or inbound body chunk.
    Data = 0,
    /// Field asks hub/gateway for waiting headers.
    ListReq = 1,
    /// Header row in a list response.
    ListHdr = 2,
    /// Field asks for one or more bodies by id.
    GetReq = 3,
    /// Hub→node sync push (internet modes only).
    HubSync = 4,
    /// VOX-only data: receiver ACKs every chunk and final assembly.
    VoxData = 5,
    /// VOX receiver accepted this chunk index.
    ChunkAck = 6,
    /// VOX receiver assembled and accepted the complete mail.
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MailMeta {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MailWire {
    pub op: MailOp,
    pub mail_id: String,
    pub idx: u16,
    pub count: u16,
    pub meta: MailMeta,
    pub payload: String,
}

pub fn wcr_address(callsign: &str) -> String {
    format!("{}@{}", callsign.trim().to_ascii_lowercase(), MAIL_DOMAIN)
}

pub fn validate_internet_addr(addr: &str) -> Result<()> {
    let s = addr.trim();
    if s.is_empty() {
        return Err(Error::config("empty To: address"));
    }
    if s.contains(char::is_whitespace) {
        return Err(Error::config("To: must be a single email address"));
    }
    let Some((local, domain)) = s.split_once('@') else {
        return Err(Error::config("To: must look like an email address"));
    };
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(Error::config("To: must look like an email address"));
    }
    if s.to_ascii_uppercase()
        .ends_with(&format!("@{}", MAIL_DOMAIN.to_ascii_uppercase()))
    {
        return Err(Error::config(
            "ham mail uses Live Chat; Email To: is for internet addresses only",
        ));
    }
    Ok(())
}

pub fn trim_body(body: &str) -> Result<String> {
    let t = body.trim();
    if t.len() > MAIL_MAX_BYTES {
        return Err(Error::config(format!(
            "mail body is {} bytes; max is {} bytes plain text",
            t.len(),
            MAIL_MAX_BYTES
        )));
    }
    Ok(t.to_string())
}

pub fn is_third_party_to(addr: &str) -> bool {
    let local = addr.split('@').next().unwrap_or("");
    !is_plausible_callsign(&local.to_ascii_uppercase())
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

pub fn encode_chunk(w: &MailWire) -> Vec<u8> {
    let meta = serde_json::to_string(&w.meta).unwrap_or_else(|_| "{}".into());
    let line = format!(
        "WCRM\t{}\t{}\t{}\t{}\t{}\t{}",
        w.op as u8,
        w.mail_id,
        w.idx,
        w.count,
        wire_escape(&meta),
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

/// Split a full mail body into on-air chunk payloads that each fit [`MAX_BODY`].
/// Full metadata goes on chunk 0 only; later chunks carry `{}` so more bytes stay for text.
pub fn chunk_payloads(mail_id: &str, meta: &MailMeta, body: &str) -> Vec<MailWire> {
    let empty = MailMeta::default();
    let probe = 999u16;
    let oh_first = encode_chunk(&MailWire {
        op: MailOp::Data,
        mail_id: mail_id.to_string(),
        idx: 0,
        count: probe,
        meta: meta.clone(),
        payload: String::new(),
    })
    .len();
    let oh_rest = encode_chunk(&MailWire {
        op: MailOp::Data,
        mail_id: mail_id.to_string(),
        idx: probe,
        count: probe,
        meta: empty.clone(),
        payload: String::new(),
    })
    .len();
    let max_first = MAX_BODY.saturating_sub(oh_first).max(24);
    let max_rest = MAX_BODY.saturating_sub(oh_rest).max(24);

    let mut pieces: Vec<String> = Vec::new();
    let (head, mut rest) = take_fitting_payload(body, max_first, |p| {
        encode_chunk(&MailWire {
            op: MailOp::Data,
            mail_id: mail_id.to_string(),
            idx: 0,
            count: probe,
            meta: meta.clone(),
            payload: p.to_string(),
        })
        .len()
            <= MAX_BODY
    });
    pieces.push(head);
    while !rest.is_empty() {
        let (next, leftover) = take_fitting_payload(rest, max_rest, |p| {
            encode_chunk(&MailWire {
                op: MailOp::Data,
                mail_id: mail_id.to_string(),
                idx: probe,
                count: probe,
                meta: empty.clone(),
                payload: p.to_string(),
            })
            .len()
                <= MAX_BODY
        });
        pieces.push(next);
        rest = leftover;
    }

    let n = pieces.len().max(1) as u16;
    pieces
        .into_iter()
        .enumerate()
        .map(|(i, p)| MailWire {
            op: MailOp::Data,
            mail_id: mail_id.to_string(),
            idx: i as u16,
            count: n,
            meta: if i == 0 { meta.clone() } else { empty.clone() },
            payload: p,
        })
        .collect()
}

/// True when every encoded chunk fits the on-air body limit.
pub fn chunks_fit_max_body(chunks: &[MailWire]) -> bool {
    chunks.iter().all(|c| encode_chunk(c).len() <= MAX_BODY)
}

fn take_fitting_payload<'a>(
    text: &'a str,
    mut max: usize,
    fits: impl Fn(&str) -> bool,
) -> (String, &'a str) {
    if text.is_empty() {
        return (String::new(), "");
    }
    max = max.min(text.len()).max(1);
    loop {
        let (head, tail) = take_body_prefix(text, max);
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

fn take_body_prefix(text: &str, max: usize) -> (String, &str) {
    if text.is_empty() {
        return (String::new(), "");
    }
    let max = max.max(1);
    if text.len() <= max {
        return (text.to_string(), "");
    }
    let mut take = max;
    while take > 0 && !text.is_char_boundary(take) {
        take -= 1;
    }
    if take == 0 {
        take = text.chars().next().map(|c| c.len_utf8()).unwrap_or(0);
    }
    if let Some(i) = text[..take].rfind(char::is_whitespace).filter(|&i| i > 0) {
        // Keep the whitespace with the remainder so reassembly stays lossless.
        return (text[..i].to_string(), &text[i..]);
    }
    (text[..take].to_string(), &text[take..])
}

pub fn assemble_chunks(parts: Vec<MailWire>) -> Result<(MailMeta, String)> {
    if parts.is_empty() {
        return Err(Error::protocol("no mail chunks"));
    }
    let mail_id = parts[0].mail_id.clone();
    let count = parts[0].count;
    if count == 0 {
        return Err(Error::protocol("bad mail chunk count"));
    }
    let mut by_idx: std::collections::BTreeMap<u16, MailWire> = std::collections::BTreeMap::new();
    for p in parts {
        if p.mail_id != mail_id {
            return Err(Error::protocol("mail_id mismatch"));
        }
        if p.count != count {
            return Err(Error::protocol("mail chunk count mismatch"));
        }
        by_idx.entry(p.idx).or_insert(p);
    }
    if by_idx.len() != count as usize {
        return Err(Error::protocol("incomplete mail chunks"));
    }
    for i in 0..count {
        if !by_idx.contains_key(&i) {
            return Err(Error::protocol("missing mail chunk"));
        }
    }
    let meta = by_idx.get(&0).map(|p| p.meta.clone()).unwrap_or_default();
    let mut body = String::new();
    for i in 0..count {
        body.push_str(&by_idx[&i].payload);
    }
    Ok((meta, body))
}

/// True when `parts` has every index `0..count` exactly once (duplicates ignored).
pub fn chunks_complete(parts: &[MailWire]) -> bool {
    if parts.is_empty() {
        return false;
    }
    let count = parts[0].count;
    if count == 0 {
        return false;
    }
    let mut seen = vec![false; count as usize];
    for p in parts {
        if p.count != count || p.mail_id != parts[0].mail_id {
            return false;
        }
        let i = p.idx as usize;
        if i >= seen.len() {
            return false;
        }
        seen[i] = true;
    }
    seen.iter().all(|v| *v)
}

/// Strip simple HTML tags for inbound hub processing.
pub fn strip_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        if c == '<' {
            in_tag = true;
            continue;
        }
        if c == '>' {
            in_tag = false;
            continue;
        }
        if !in_tag {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_chunk() {
        let w = MailWire {
            op: MailOp::Data,
            mail_id: "abc".into(),
            idx: 0,
            count: 1,
            meta: MailMeta {
                from: "a@b".into(),
                to: "c@d".into(),
                subject: "hi".into(),
                ids: vec![],
            },
            payload: "hello".into(),
        };
        let back = decode_chunk(&encode_chunk(&w)).unwrap();
        assert_eq!(back.payload, "hello");
    }

    #[test]
    fn vox_reliability_ops_roundtrip() {
        for op in [MailOp::VoxData, MailOp::ChunkAck, MailOp::CompleteAck] {
            let wire = MailWire {
                op,
                mail_id: "reliable-1".into(),
                idx: 2,
                count: 4,
                meta: MailMeta::default(),
                payload: String::new(),
            };
            let decoded = decode_chunk(&encode_chunk(&wire)).unwrap();
            assert_eq!(decoded.op, op);
            assert_eq!(decoded.mail_id, "reliable-1");
            assert_eq!(decoded.idx, 2);
            assert_eq!(decoded.count, 4);
        }
    }

    #[test]
    fn two_kb_mail_chunks_fit_max_body() {
        let body = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(40);
        assert!(body.len() >= 2000);
        let meta = MailMeta {
            from: "m7tjf@mail.weechatradio.com".into(),
            to: "alex@example.com".into(),
            subject: "WCR mail test ~2KB".into(),
            ids: vec![],
        };
        let id = "db341c98ecc41599dc78a730633bb6389e9fd8e9f0a3c593e965ed07634e2a71";
        let chunks = chunk_payloads(id, &meta, &body);
        assert!(chunks_fit_max_body(&chunks), "chunk over MAX_BODY");
        let round: Vec<_> = chunks
            .iter()
            .map(|c| decode_chunk(&encode_chunk(c)).expect("decode"))
            .collect();
        let (m, assembled) = assemble_chunks(round).unwrap();
        assert_eq!(m.from, meta.from);
        assert_eq!(assembled, body);
    }

    #[test]
    fn newlines_survive_the_wire() {
        let w = MailWire {
            op: MailOp::Data,
            mail_id: "n1".into(),
            idx: 0,
            count: 1,
            meta: MailMeta {
                from: "a@b.c".into(),
                to: "d@e.f".into(),
                subject: "line\nbreak".into(),
                ids: vec![],
            },
            payload: "hello\nworld\r\n\ttab\\slash".into(),
        };
        let back = decode_chunk(&encode_chunk(&w)).unwrap();
        assert_eq!(back.payload, w.payload);
        assert_eq!(back.meta.subject, "line\nbreak");
    }

    #[test]
    fn assemble_ignores_duplicate_idx() {
        let mk = |idx, payload: &str| MailWire {
            op: MailOp::Data,
            mail_id: "x".into(),
            idx,
            count: 2,
            meta: MailMeta {
                from: "a@b.c".into(),
                ..MailMeta::default()
            },
            payload: payload.into(),
        };
        let dup = vec![mk(0, "A"), mk(0, "A"), mk(1, "B")];
        assert!(chunks_complete(&dup));
        let (_, body) = assemble_chunks(dup).unwrap();
        assert_eq!(body, "AB");
        let missing = vec![mk(0, "A"), mk(0, "A")];
        assert!(!chunks_complete(&missing));
        assert!(assemble_chunks(missing).is_err());
    }
}
