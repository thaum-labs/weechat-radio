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

/// Largest attached file (after a picture is shrunk). About 1.5 min on vhf-fast.
pub const ATTACH_MAX_BYTES: usize = 8192;
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

/// Get a file ready to attach: a picture over the limit is shrunk and
/// renamed `.jpg`; any other file must already fit. The `String` says what
/// was done, for the person ("cat.jpg shrunk from 2.1 MB to 320×240").
pub fn prepare(name: &str, data: Vec<u8>) -> Result<(Attachment, Option<String>)> {
    let name = crate::proto::file::clean_name(name);
    if data.is_empty() {
        return Err(Error::protocol("the file is empty"));
    }
    if data.len() <= ATTACH_MAX_BYTES {
        return Ok((Attachment { name, data }, None));
    }
    if crate::picture::is_picture(&data) {
        let before = data.len();
        let s = crate::picture::shrink(&data, ATTACH_MAX_BYTES).map_err(Error::protocol)?;
        let name = crate::picture::jpeg_name(&name);
        let note = format!(
            "{name} shrunk from {} to {}×{}",
            crate::proto::file::human_size(before),
            s.width,
            s.height
        );
        return Ok((Attachment { name, data: s.jpeg }, Some(note)));
    }
    Err(Error::protocol(format!(
        "{name} is {}; files up to {} can go with a mail (pictures are shrunk to fit)",
        crate::proto::file::human_size(data.len()),
        crate::proto::file::human_size(ATTACH_MAX_BYTES)
    )))
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
        assert!(prepare("data.csv", vec![b'1'; ATTACH_MAX_BYTES]).is_ok());
        let err = prepare("data.csv", vec![b'1'; ATTACH_MAX_BYTES + 1]).unwrap_err();
        assert!(
            err.plain().contains("files up to 8.0 KB"),
            "{}",
            err.plain()
        );
        let img = image::RgbImage::from_fn(1200, 900, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x * y) % 251) as u8])
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let (a, note) = prepare("photo.png", png).unwrap();
        assert_eq!(a.name, "photo.jpg");
        assert!(a.data.len() <= ATTACH_MAX_BYTES);
        assert!(note.unwrap().contains("shrunk"));
    }
}
