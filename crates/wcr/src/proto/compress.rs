//! SPDX-License-Identifier: Apache-2.0
//! Smaz-style short-string codec for chat bodies. Body is always stored
//! uncompressed in [`Envelope`]; compression is a wire-only transform.

use crate::error::{Error, Result};

/// Codebook tuned for English chat plus a handful of ham abbreviations.
/// Index is the encoded byte. Keep this list stable: changing it is a
/// protocol break for compressed frames.
const CODEBOOK: &[&[u8]] = &[
    b" ",
    b"the ",
    b"e ",
    b"t ",
    b"a ",
    b"of ",
    b"o ",
    b"and ",
    b"i ",
    b"n ",
    b"s ",
    b"e",
    b"r ",
    b"in ",
    b"d ",
    b"h ",
    b"to ",
    b"l ",
    b"a",
    b"is ",
    b"u ",
    b"m ",
    b"c ",
    b"w ",
    b"that ",
    b"for ",
    b"on ",
    b"with ",
    b"as ",
    b"at ",
    b"be ",
    b"this ",
    b"have ",
    b"from ",
    b"by ",
    b"not ",
    b"are ",
    b"was ",
    b"but ",
    b"they ",
    b"you ",
    b"he ",
    b"she ",
    b"we ",
    b"it ",
    b"or ",
    b"an ",
    b"en ",
    b"ing ",
    b"ion ",
    b"tion",
    b"ment",
    b"the",
    b"and",
    b"ing",
    b"er",
    b"ed ",
    b"es ",
    b"ly ",
    b"al ",
    b"re ",
    b"th",
    b"in",
    b"er ",
    b"on",
    b"an",
    b"en",
    b"at",
    b"es",
    b"or",
    b"re",
    b"st",
    b"to",
    b"it",
    b"is",
    b"ou",
    b"ar",
    b"as",
    b"ha",
    b"le",
    b"se",
    b"ng",
    b"he",
    b"ve",
    b"nd",
    b"hi",
    b"me",
    b"ne",
    b"wh",
    b"if ",
    b"so ",
    b"no ",
    b"yes ",
    b"ok ",
    b"OK ",
    b"please ",
    b"thanks ",
    b"thank ",
    b"hello",
    b"Hello",
    b"radio",
    b"copy ",
    b"need ",
    b"help ",
    b"over ",
    b"roger ",
    b"cq ",
    b"CQ ",
    b"de ",
    b"ur ",
    b"rpt ",
    b"qth ",
    b"QTH ",
    b"73 ",
    b"88 ",
    b"msg ",
    b"ack ",
    b"net ",
    b"qsy ",
    b"qrm ",
    b"qrn ",
    b"qsb ",
    b"qrs ",
    b"qrp ",
    b"kw ",
    b"hz ",
    b"mhz ",
    b"khz ",
    b"usb ",
    b"lsb ",
    b"fm ",
    b"ssb ",
    b"emergency",
    b"priority",
    b"bulletin",
    b"checkin",
    b"status",
    b"http://",
    b"https://",
    b"www.",
    b".com",
    b".org",
    b"http",
    b"www",
    b"0",
    b"1",
    b"2",
    b"3",
    b"4",
    b"5",
    b"6",
    b"7",
    b"8",
    b"9",
    b".",
    b",",
    b"!",
    b"?",
    b":",
    b";",
    b"-",
    b"/",
    b"'",
    b"\"",
    b"(",
    b")",
    b"\n",
    b"T",
    b"I",
    b"A",
    b"S",
    b"O",
    b"W",
    b"H",
    b"C",
    b"B",
    b"N",
    b"R",
    b"D",
    b"L",
    b"M",
    b"P",
    b"F",
    b"G",
    b"E",
    b"U",
    b"Y",
    b"K",
    b"V",
    b"J",
    b"X",
    b"Q",
    b"Z",
    b"ll",
    b"ss",
    b"tt",
    b"ff",
    b"pp",
    b"rr",
    b"mm",
    b"nn",
    b"oo",
    b"ee",
    b"aa",
    b"ch",
    b"sh",
    b"th ",
    b"the",
    b"ing",
    b"ion",
];

const LITERAL: u8 = 0xFF;

/// Compress `src`. Returns `None` unless the result is strictly smaller.
pub fn compress(src: &[u8]) -> Option<Vec<u8>> {
    if src.is_empty() || CODEBOOK.len() > 254 {
        return None;
    }
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        let mut best: Option<(usize, usize)> = None; // (code, len)
        for (code, word) in CODEBOOK.iter().enumerate() {
            if word.is_empty() || i + word.len() > src.len() {
                continue;
            }
            if &src[i..i + word.len()] == *word {
                if best.map(|(_, l)| word.len() > l).unwrap_or(true) {
                    best = Some((code, word.len()));
                }
            }
        }
        if let Some((code, len)) = best {
            out.push(code as u8);
            i += len;
        } else {
            out.push(LITERAL);
            out.push(src[i]);
            i += 1;
        }
        if out.len() >= src.len() {
            return None;
        }
    }
    if out.len() < src.len() {
        Some(out)
    } else {
        None
    }
}

pub fn decompress(src: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(src.len() * 2);
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        i += 1;
        if b == LITERAL {
            if i >= src.len() {
                return Err(Error::protocol("truncated compressed body"));
            }
            out.push(src[i]);
            i += 1;
        } else {
            let idx = b as usize;
            if idx >= CODEBOOK.len() {
                return Err(Error::protocol("bad compression code"));
            }
            out.extend_from_slice(CODEBOOK[idx]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_english() {
        let msg = b"hello radio please copy need help over";
        let c = compress(msg).expect("should shrink");
        assert!(c.len() < msg.len());
        assert_eq!(decompress(&c).unwrap(), msg);
    }

    #[test]
    fn incompressible_stays_none() {
        let msg = &[0x00, 0x01, 0x02, 0x03, 0x04];
        assert!(compress(msg).is_none());
    }

    #[test]
    fn codebook_fits_in_a_byte() {
        assert!(CODEBOOK.len() <= 254, "codebook is {}", CODEBOOK.len());
    }
}
