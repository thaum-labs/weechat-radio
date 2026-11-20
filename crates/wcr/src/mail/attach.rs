//! SPDX-License-Identifier: Apache-2.0
//! Files attached to a mail, carried inside the mail text.
//!
//! Each file goes at the end of the body as a marked block of base64
//! lines; several files are several blocks, one after the other:
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
//! The hub takes the blocks out and sends the files as real attachments;
//! for mail coming in, the hub adds a block and the station saves the
//! file when the mail arrives. Base64 costs a third more airtime.

use crate::error::{Error, Result};
use crate::modes::Mode;
use crate::presets::Preset;
use base64::Engine as _;

/// Largest file the hub brings in over the radio (inbound mail), and the
/// old limit for every preset but vhf-fast.
pub const ATTACH_MAX_BYTES: usize = 8192;
/// Largest total of a mail's files (Tobias, QA #365): about 45 min on the
/// air on vhf-fast, sent in blocks with only the missing chunks sent again.
pub const ATTACH_MAX_FAST_BYTES: usize = 512 * 1024;

/// What a station may attach to its own mail, and why. One place for the
/// limit and its words, so the desktop app, the Pi page and the station
/// say the same thing (QA #365 rc13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// Internet or Internet+Radio: the station hands its mail to the hub
    /// over the internet, whatever the preset (QA #365 rc13: a Pi gateway
    /// was held to the radio limit for its own mail).
    Internet,
    /// Radio+ on vhf-fast: files go over the radio, up to 512 KB in all.
    Fast,
    /// Radio+ on any other preset: files go over the radio on vhf-fast
    /// only (Tobias's card, rc10).
    NoFiles(Preset),
}

impl Limit {
    /// The limit for a station in `mode` on `preset`.
    pub fn for_station(mode: Mode, preset: Preset) -> Self {
        match mode {
            Mode::Internet | Mode::InternetRadio => Self::Internet,
            Mode::RadioPlus | Mode::Radio if preset == Preset::VhfFast => Self::Fast,
            Mode::RadioPlus | Mode::Radio => Self::NoFiles(preset),
        }
    }

    /// Bytes of files one mail may carry, all files together.
    pub fn max(self) -> usize {
        match self {
            Self::Internet | Self::Fast => ATTACH_MAX_FAST_BYTES,
            Self::NoFiles(_) => 0,
        }
    }

    /// One line on what can be attached, for under the Attach button.
    pub fn line(self) -> String {
        match self {
            Self::Internet => {
                "Files up to 512 KB in all; pictures are shrunk to fit. This station sends mail over the internet.".into()
            }
            Self::Fast => "Files up to 512 KB in all on vhf-fast; pictures are shrunk to fit.".into(),
            Self::NoFiles(p) => format!(
                "No files on {}: files go with mail over the radio on vhf-fast only (up to 512 KB).",
                p.as_str()
            ),
        }
    }

    /// Why `name` (`len` bytes) is not attached, when `used` bytes of other
    /// files are attached already. Says what to do about it.
    pub fn refusal(self, name: &str, len: usize, used: usize) -> String {
        let size = crate::proto::file::human_size(len);
        match self {
            Self::NoFiles(p) => format!(
                "{name} can't go on {}: files go with mail over the radio on vhf-fast only. Switch to vhf-fast (2 m or 70 cm, needs a steady link of about 10 dB) to send files up to 512 KB.",
                p.as_str()
            ),
            _ if used == 0 || len > self.max() => {
                format!("{name} is {size}, over the 512 KB limit for a mail.")
            }
            _ => format!(
                "{name} ({size}) would take this mail's files over 512 KB ({} attached already).",
                crate::proto::file::human_size(used)
            ),
        }
    }

    /// Get `name` ready to go with files of `used` bytes already attached:
    /// a picture is shrunk into the room left, any other file must fit.
    /// The `Err` is [`Limit::refusal`] or why a picture would not shrink.
    pub fn attach(
        self,
        name: &str,
        data: Vec<u8>,
        used: usize,
    ) -> std::result::Result<(Attachment, Option<String>), String> {
        let clean = crate::proto::file::clean_name(name);
        let len = data.len();
        let room = self.max().saturating_sub(used);
        // A picture needs some room to shrink into; under 1 KB it is
        // refused like any other file.
        if room == 0 || (len > room && crate::picture::is_picture(&data) && room < 1024) {
            return Err(self.refusal(&clean, len, used));
        }
        match prepare(&clean, data, room) {
            Ok(done) => Ok(done),
            Err(Error::Msg(m)) if m.starts_with(TOO_BIG) => Err(self.refusal(&clean, len, used)),
            Err(e) => Err(e.plain()),
        }
    }
}

/// The attachment limit on `preset` over the radio: 512 KB on vhf-fast,
/// 8 KB elsewhere. Only the hub's inbound path uses the 8 KB.
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
/// How [`prepare`]'s "does not fit" error starts, so [`Limit::attach`] can
/// put its own words on it.
const TOO_BIG: &str = "too big: ";

/// A file carried in a mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
}

/// `text` with `file` added at the end.
pub fn join(text: &str, file: &Attachment) -> String {
    join_all(text, std::slice::from_ref(file))
}

/// `text` with every one of `files` added at the end, in order.
pub fn join_all(text: &str, files: &[Attachment]) -> String {
    let mut out = text.trim_end().to_string();
    for file in files {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&file.data);
        out.reserve(b64.len() + 80);
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
    }
    out
}

/// The mail text without its files, and the files in the order attached.
/// A block that does not decode is left in the text, so nothing is lost
/// silently.
pub fn split(body: &str) -> (String, Vec<Attachment>) {
    let mut text = body.to_string();
    let mut files = Vec::new();
    while let Some((before, file)) = split_last(&text) {
        files.push(file);
        text = before;
    }
    files.reverse();
    (text, files)
}

/// Bytes of all the files in `files`.
pub fn total(files: &[Attachment]) -> usize {
    files.iter().map(|f| f.data.len()).sum()
}

/// The last file block of `body`, and the text before it.
fn split_last(body: &str) -> Option<(String, Attachment)> {
    let start = body.rfind(START)?;
    let rest = &body[start + START.len()..];
    let head_end = rest.find(']')?;
    let head = &rest[..head_end];
    let name = head.rsplit_once(", ").map(|(n, _)| n).unwrap_or(head);
    let after = &rest[head_end + 1..];
    let end = after.find(END)?;
    let b64: String = after[..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let data = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    let name = crate::proto::file::clean_name(name);
    let text = body[..start].trim_end().to_string();
    Some((text, Attachment { name, data }))
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
    Err(Error::Msg(format!(
        "{TOO_BIG}{name} is {}; the limit is {}",
        crate::proto::file::human_size(data.len()),
        crate::proto::file::human_size(max)
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
        assert_eq!(got, vec![f]);
    }

    #[test]
    fn crlf_and_no_text_still_split() {
        let f = file(300);
        let body = join("", &f).replace('\n', "\r\n");
        let (text, got) = split(&body);
        assert_eq!(text, "");
        assert_eq!(got, vec![f]);
    }

    #[test]
    fn plain_mail_has_no_file() {
        let (text, got) = split("just words [WCR file: not closed");
        assert_eq!(text, "just words [WCR file: not closed");
        assert!(got.is_empty());
    }

    #[test]
    fn a_broken_block_stays_in_the_text() {
        let body = "hi\n\n[WCR file: a.txt, 3 bytes]\n!!!not base64!!!\n[WCR file end]";
        let (text, got) = split(body);
        assert_eq!(text, body);
        assert!(got.is_empty());
    }

    #[test]
    fn names_are_made_safe() {
        let mut f = file(10);
        f.name = "x".into();
        let body = join("", &f).replace("x, 10", "../../etc/passwd, 10");
        assert_eq!(split(&body).1[0].name, "passwd");
    }

    #[test]
    fn several_files_come_back_in_order() {
        let mut a = file(5000);
        a.name = "a.csv".into();
        let mut b = file(300);
        b.name = "b.txt".into();
        let body = join_all("Two files.", &[a.clone(), b.clone()]);
        let (text, got) = split(&body);
        assert_eq!(text, "Two files.");
        assert_eq!(got, vec![a.clone(), b.clone()]);
        assert_eq!(total(&got), 5300);
        // Built one at a time, the same as all at once.
        assert_eq!(join(&join("Two files.", &a), &b), body);
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
        assert!(err.plain().contains("8.0 KB"), "{}", err.plain());
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

    /// QA #365 rc13 BLOCKER: a Pi gateway on vhf-fast was held to 8 KB.
    /// Its own mail goes over the internet: 512 KB on any preset.
    #[test]
    fn the_limit_follows_the_mode_and_the_preset() {
        use crate::modes::Mode;
        for p in [Preset::VhfFast, Preset::VhfFm, Preset::HfPoor] {
            assert_eq!(Limit::for_station(Mode::InternetRadio, p), Limit::Internet);
            assert_eq!(Limit::for_station(Mode::Internet, p).max(), 512 * 1024);
        }
        assert_eq!(
            Limit::for_station(Mode::RadioPlus, Preset::VhfFast),
            Limit::Fast
        );
        assert_eq!(
            Limit::for_station(Mode::RadioPlus, Preset::VhfFast).max(),
            512 * 1024
        );
        let slow = Limit::for_station(Mode::RadioPlus, Preset::VhfFm);
        assert_eq!(slow.max(), 0);
        assert!(
            slow.line().contains("No files on vhf-fm"),
            "{}",
            slow.line()
        );
        assert!(Limit::Internet.line().contains("over the internet"));
        // 32 KB goes from the gateway itself and on Radio+ vhf-fast.
        let qa = vec![b'q'; 32 * 1024];
        assert!(Limit::Internet.attach("qa-32k.bin", qa.clone(), 0).is_ok());
        assert!(Limit::Fast.attach("qa-32k.bin", qa.clone(), 0).is_ok());
        // On vhf-fm it says to switch, and never "switch" on vhf-fast.
        let why = slow.attach("qa-32k.bin", qa, 0).unwrap_err();
        assert!(
            why.contains("can't go on vhf-fm") && why.contains("Switch to vhf-fast"),
            "{why}"
        );
        assert!(why.contains("about 10 dB"), "{why}");
        let huge = Limit::Fast
            .attach("disk.iso", vec![0; ATTACH_MAX_FAST_BYTES + 1], 0)
            .unwrap_err();
        assert!(
            huge.contains("over the 512 KB limit") && !huge.contains("Switch"),
            "{huge}"
        );
        // The total counts: a second file that tips it over says so.
        let second = Limit::Fast
            .attach("b.bin", vec![0; 300 * 1024], 300 * 1024)
            .unwrap_err();
        assert!(second.contains("300.0 KB attached already"), "{second}");
    }
}
