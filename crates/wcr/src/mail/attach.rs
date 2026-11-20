//! SPDX-License-Identifier: Apache-2.0
//! One file attached to a mail, carried inside the mail text.
//!
//! The file goes at the end of the body as a marked block of base64 lines:
//!
//! ```text
//! See you Saturday.
//!
//! [WCR file: cat.jpg, 7012 bytes]
//! /9j/4AAQSkZJRgABAQAAAQABAAD...
//! [WCR file end]
//! ```
//!
//! Gateways pass mail text through untouched, so any gateway carries it.
//! The hub takes the block out and sends the file as a real attachment;
//! for mail coming in, the hub adds the block and the station saves the
//! file when the mail arrives. Base64 costs a third more airtime.

use crate::error::{Error, Result};
use base64::Engine as _;

/// Largest attached file (after a picture is shrunk) on every preset but
/// vhf-fast. About 1.5 min on vhf-fast.
pub const ATTACH_MAX_BYTES: usize = 8192;
/// Largest attached file on vhf-fast (Tobias, QA #365): about 45 min on the
/// air, sent in blocks with only the missing chunks sent again.
pub const ATTACH_MAX_FAST_BYTES: usize = 512 * 1024;

/// The attachment limit on `preset`: 512 KB on vhf-fast, 8 KB elsewhere.
pub fn max_bytes(preset: crate::presets::Preset) -> usize {
    if preset == crate::presets::Preset::VhfFast {
        ATTACH_MAX_FAST_BYTES
    } else {
        ATTACH_MAX_BYTES
    }
}
const START: &str = "[WCR file: ";
const END: &str = "[WCR file end]";
const LINE: usize = 76;

/// A file carried in a mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
}

/// `text` with `file` added at the end.
pub fn join(text: &str, file: &Attachment) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(&file.data);
    let mut out = String::with_capacity(text.len() + b64.len() + 80);
    out.push_str(text.trim_end());
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(&format!(
        "{START}{}, {} bytes]\n",
        file.name,
        file.data.len()
    ));
    for chunk in b64.as_bytes().chunks(LINE) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or_default());
        out.push('\n');
    }
    out.push_str(END);
    out
}

/// The mail text without its file, and the file if there is one. A block
/// that does not decode is left in the text, so nothing is lost silently.
pub fn split(body: &str) -> (String, Option<Attachment>) {
    let Some(start) = body.rfind(START) else {
        return (body.to_string(), None);
    };
    let rest = &body[start + START.len()..];
    let Some(head_end) = rest.find(']') else {
        return (body.to_string(), None);
    };
    let head = &rest[..head_end];
    let name = head.rsplit_once(", ").map(|(n, _)| n).unwrap_or(head);
    let after = &rest[head_end + 1..];
    let Some(end) = after.find(END) else {
        return (body.to_string(), None);
    };
    let b64: String = after[..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let Ok(data) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return (body.to_string(), None);
    };
    let name = crate::proto::file::clean_name(name);
    let text = body[..start].trim_end().to_string();
    (text, Some(Attachment { name, data }))
}

/// Get a file ready to attach: a picture over `max` is shrunk and renamed
/// `.jpg`; any other file must already fit. The `String` says what was
/// done, for the person ("cat.jpg shrunk from 2.1 MB to 320×240").
pub fn prepare(name: &str, data: Vec<u8>, max: usize) -> Result<(Attachment, Option<String>)> {
    let name = crate::proto::file::clean_name(name);
    if data.is_empty() {
        return Err(Error::Msg(format!("{name} is empty")));
    }
    if data.len() <= max {
        return Ok((Attachment { name, data }, None));
    }
    if crate::picture::is_picture(&data) {
        let before = data.len();
        let s =
            crate::picture::shrink(&data, max).map_err(|e| Error::Msg(format!("{name}: {e}")))?;
        let name = crate::picture::jpeg_name(&name);
        let note = format!(
            "{name} shrunk from {} to {}×{}",
            crate::proto::file::human_size(before),
            s.width,
            s.height
        );
        return Ok((Attachment { name, data: s.jpeg }, Some(note)));
    }
    Err(Error::Msg(too_big_text(&name, data.len(), max)))
}

/// Why a file is not attached, with its size and the limit. Below the
/// vhf-fast limit it says where the big one is (QA #365 rc12: otherwise
/// hidden).
pub fn too_big_text(name: &str, len: usize, max: usize) -> String {
    let tip = if max < ATTACH_MAX_FAST_BYTES && len <= ATTACH_MAX_FAST_BYTES {
        ". Switch to vhf-fast (2 m or 70 cm, needs a steady link of about 10 dB) to send attachments up to 512 KB"
    } else {
        ""
    };
    format!(
        "{name} is {}; files up to {} can go with a mail here (pictures are shrunk to fit){tip}",
        crate::proto::file::human_size(len),
        crate::proto::file::human_size(max)
    )
}

/// A one-line stand-in for the block, for lists and previews.
pub fn summary(file: &Attachment) -> String {
    format!(
        "📎 {} ({})",
        file.name,
        crate::proto::file::human_size(file.data.len())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(n: usize) -> Attachment {
        Attachment {
            name: "log.csv".into(),
            data: (0..n).map(|i| (i % 251) as u8).collect(),
        }
    }

    #[test]
    fn a_file_comes_back_whole() {
        let f = file(5000);
        let body = join("Hello,\nthe log.\n", &f);
        assert!(body.starts_with("Hello,\nthe log.\n\n[WCR file: log.csv, 5000 bytes]\n"));
        assert!(body.lines().all(|l| l.len() <= LINE || l.starts_with('[')));
        let (text, got) = split(&body);
        assert_eq!(text, "Hello,\nthe log.");
        assert_eq!(got.unwrap(), f);
    }

    #[test]
    fn crlf_and_no_text_still_split() {
        let f = file(300);
        let body = join("", &f).replace('\n', "\r\n");
        let (text, got) = split(&body);
        assert_eq!(text, "");
        assert_eq!(got.unwrap(), f);
    }

    #[test]
    fn plain_mail_has_no_file() {
        let (text, got) = split("just words [WCR file: not closed");
        assert_eq!(text, "just words [WCR file: not closed");
        assert!(got.is_none());
    }

    #[test]
    fn a_broken_block_stays_in_the_text() {
        let body = "hi\n\n[WCR file: a.txt, 3 bytes]\n!!!not base64!!!\n[WCR file end]";
        let (text, got) = split(body);
        assert_eq!(text, body);
        assert!(got.is_none());
    }

    #[test]
    fn names_are_made_safe() {
        let mut f = file(10);
        f.name = "x".into();
        let body = join("", &f).replace("x, 10", "../../etc/passwd, 10");
        assert_eq!(split(&body).1.unwrap().name, "passwd");
    }

    #[test]
    fn big_files_are_refused_and_big_pictures_shrink() {
        assert!(prepare("data.csv", vec![b'1'; ATTACH_MAX_BYTES], ATTACH_MAX_BYTES).is_ok());
        let err = prepare(
            "data.csv",
            vec![b'1'; ATTACH_MAX_BYTES + 1],
            ATTACH_MAX_BYTES,
        )
        .unwrap_err();
        assert!(
            err.plain().contains("files up to 8.0 KB"),
            "{}",
            err.plain()
        );
        assert!(
            err.plain().contains("Switch to vhf-fast") && err.plain().contains("about 10 dB"),
            "{}",
            err.plain()
        );
        let huge = too_big_text("disk.iso", ATTACH_MAX_FAST_BYTES + 1, ATTACH_MAX_BYTES);
        assert!(!huge.contains("vhf-fast"), "{huge}");
        let img = image::RgbImage::from_fn(1200, 900, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 251) as u8])
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let (a, note) = prepare("photo.png", png, ATTACH_MAX_BYTES).unwrap();
        assert_eq!(a.name, "photo.jpg");
        assert!(a.data.len() <= ATTACH_MAX_BYTES);
        assert!(note.unwrap().contains("shrunk"));
    }

    #[test]
    fn vhf_fast_takes_512_kb_and_others_8_kb() {
        use crate::presets::Preset;
        assert_eq!(max_bytes(Preset::VhfFast), 512 * 1024);
        for p in [Preset::VhfFm, Preset::Afsk1200, Preset::HfPoor] {
            assert_eq!(max_bytes(p), 8192);
        }
        let big = vec![b'x'; 200 * 1024];
        assert!(prepare("log.csv", big.clone(), max_bytes(Preset::VhfFast)).is_ok());
        let err = prepare("log.csv", big, max_bytes(Preset::VhfFm)).unwrap_err();
        assert!(err.plain().contains("200.0 KB"), "{}", err.plain());
        assert!(err.plain().contains("8.0 KB"), "{}", err.plain());
    }
}
