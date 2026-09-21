//! SPDX-License-Identifier: Apache-2.0
//! Internet mail payloads for RF/hub (`MsgType::Mail`).

use crate::error::{Error, Result};
use crate::proto::callsign::is_plausible_callsign;
use crate::proto::split_body_chunks;
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
}

impl MailOp {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Data),
            1 => Some(Self::ListReq),
            2 => Some(Self::ListHdr),
            3 => Some(Self::GetReq),
            4 => Some(Self::HubSync),
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

pub fn encode_chunk(w: &MailWire) -> Vec<u8> {
    let meta = serde_json::to_string(&w.meta).unwrap_or_else(|_| "{}".into());
    let line = format!(
        "WCRM\t{}\t{}\t{}\t{}\t{}\t{}",
        w.op as u8,
        w.mail_id,
        w.idx,
        w.count,
        meta.replace('\t', " "),
        w.payload.replace('\n', " ")
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
    let meta: MailMeta = serde_json::from_str(parts[5]).unwrap_or_default();
    let payload = parts[6].to_string();
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
    let (head, mut rest) = take_body_prefix(body, max_first);
    pieces.push(head);
    while !rest.is_empty() {
        let (next, leftover) = take_body_prefix(rest, max_rest);
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

pub fn assemble_chunks(mut parts: Vec<MailWire>) -> Result<(MailMeta, String)> {
    if parts.is_empty() {
        return Err(Error::protocol("no mail chunks"));
    }
    let mail_id = parts[0].mail_id.clone();
    parts.sort_by_key(|p| p.idx);
    let meta = parts[0].meta.clone();
    let mut body = String::new();
    for p in parts {
        if p.mail_id != mail_id {
            return Err(Error::protocol("mail_id mismatch"));
        }
        body.push_str(&p.payload);
    }
    Ok((meta, body))
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
        // encode_chunk maps newlines to spaces on the wire
        let expect: String = body
            .chars()
            .map(|c| if c == '\n' { ' ' } else { c })
            .collect();
        assert_eq!(assembled, expect);
    }
}
