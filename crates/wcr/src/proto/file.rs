//! SPDX-License-Identifier: Apache-2.0
//! Small file attachments in direct messages, on VHF/UHF only.
//!
//! A file goes out as numbered pieces, each one a [`MsgType::File`] frame
//! addressed to one callsign. Every piece asks for a receipt and is retried
//! like a chat line, so lost pieces are sent again. The receiver holds back
//! the receipt for the LAST piece until it has every piece and has saved the
//! file: the sender's `[ok]` on the file line means the whole file arrived.
//!
//! Piece body: `ver u8, file_id u32, idx u8, count u8, total u16`, then (in
//! piece 0 only) `name_len u8, name`, then the data.
//!
//! [`MsgType::File`]: crate::proto::MsgType::File

use crate::error::{Error, Result};
use crate::presets::Preset;
use crate::proto::envelope::{Envelope, CRC_LEN, HEADER_LEN, MAX_BODY};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Largest file that may be sent: about 30 s on vhf-fm.
pub const FILE_MAX_BYTES: usize = 4096;
/// Longest file name kept on the air.
pub const NAME_MAX: usize = 40;
/// Bytes before the data in every piece.
pub const PIECE_HDR: usize = 9;
const PIECE_VER: u8 = 1;
/// A part-received file is dropped after this long without a new piece.
const PARTIAL_TTL: Duration = Duration::from_secs(30 * 60);
/// Finished files are remembered this long, so a late retry still gets its receipt.
const DONE_TTL: Duration = Duration::from_secs(60 * 60);

/// Presets that may carry files: the VHF/UHF FM ones.
pub fn preset_allows(preset: Preset) -> bool {
    matches!(preset, Preset::VhfFm | Preset::Afsk1200)
}

/// Why this file cannot be sent, in plain words. `None` means it can.
pub fn refusal(preset: Preset, uses_radio: bool, target: &str, len: usize) -> Option<String> {
    if target.starts_with('#') || target.starts_with('&') {
        return Some("files go in direct messages only, not in channels".into());
    }
    if !uses_radio {
        return Some("files go over the radio only: switch to a mode that uses the radio".into());
    }
    if !preset_allows(preset) {
        return Some(format!(
            "files go on VHF/UHF only (vhf-fm or afsk-1200); this station is on {}",
            preset.as_str()
        ));
    }
    if len == 0 {
        return Some("the file is empty".into());
    }
    if len > FILE_MAX_BYTES {
        return Some(format!(
            "the file is {}; the limit is {} (about 30 s on vhf-fm, 1.5 min on afsk-1200)",
            human_size(len),
            human_size(FILE_MAX_BYTES)
        ));
    }
    None
}

/// Largest piece body that fits one frame on `preset`, so no piece is
/// split again into fragments.
pub fn body_limit(preset: Preset) -> usize {
    let mtu = preset.payload_bytes() as usize;
    MAX_BODY
        .min(mtu.saturating_sub(HEADER_LEN + CRC_LEN))
        .max(PIECE_HDR + 1 + NAME_MAX + 16)
}

/// A file name that is safe to save anywhere: the last path part only,
/// letters, digits, `.`, `-` and `_`, at most [`NAME_MAX`] bytes.
pub fn clean_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let mut out: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    while out.starts_with('.') {
        out.remove(0);
    }
    if out.len() > NAME_MAX {
        // Keep the extension when shortening.
        let ext = out
            .rfind('.')
            .map(|i| out[i..].to_string())
            .filter(|e| e.len() <= 8)
            .unwrap_or_default();
        out.truncate(NAME_MAX - ext.len());
        out.push_str(&ext);
    }
    if out.trim_matches(['_', '.']).is_empty() {
        "file".into()
    } else {
        out
    }
}

/// `2.1 KB`, `300 B`.
pub fn human_size(n: usize) -> String {
    if n < 1000 {
        format!("{n} B")
    } else {
        format!("{:.1} KB", n as f64 / 1024.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub file_id: u32,
    pub idx: u8,
    pub count: u8,
    /// Whole file length in bytes.
    pub total: u16,
    /// Piece 0 only.
    pub name: Option<String>,
    pub data: Vec<u8>,
}

impl Piece {
    pub fn is_last(&self) -> bool {
        self.idx + 1 == self.count
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PIECE_HDR + NAME_MAX + 1 + self.data.len());
        out.push(PIECE_VER);
        out.extend_from_slice(&self.file_id.to_le_bytes());
        out.push(self.idx);
        out.push(self.count);
        out.extend_from_slice(&self.total.to_le_bytes());
        if let Some(name) = &self.name {
            out.push(name.len() as u8);
            out.extend_from_slice(name.as_bytes());
        }
        out.extend_from_slice(&self.data);
        out
    }

    pub fn decode(body: &[u8]) -> Result<Self> {
        if body.len() < PIECE_HDR || body[0] != PIECE_VER {
            return Err(Error::protocol("not a file piece"));
        }
        let file_id = u32::from_le_bytes(body[1..5].try_into().unwrap());
        let idx = body[5];
        let count = body[6];
        let total = u16::from_le_bytes(body[7..9].try_into().unwrap());
        if count == 0 || idx >= count || total as usize > FILE_MAX_BYTES {
            return Err(Error::protocol("bad file piece header"));
        }
        let mut rest = &body[PIECE_HDR..];
        let name = if idx == 0 {
            let n = *rest
                .first()
                .ok_or_else(|| Error::protocol("file piece has no name"))?
                as usize;
            if rest.len() < 1 + n {
                return Err(Error::protocol("short file name"));
            }
            let raw = String::from_utf8_lossy(&rest[1..1 + n]).to_string();
            rest = &rest[1 + n..];
            Some(clean_name(&raw))
        } else {
            None
        };
        Ok(Self {
            file_id,
            idx,
            count,
            total,
            name,
            data: rest.to_vec(),
        })
    }
}

/// A short id for one file from one station.
pub fn file_id(origin: &str, dest: &str, name: &str, data: &[u8], seq: u32) -> u32 {
    let mut h = blake3::Hasher::new();
    for part in [origin.as_bytes(), dest.as_bytes(), name.as_bytes(), data] {
        h.update(part);
        h.update(&[0]);
    }
    h.update(&seq.to_le_bytes());
    u32::from_le_bytes(h.finalize().as_bytes()[..4].try_into().unwrap())
}

/// Cut `data` into pieces whose bodies are at most `limit` bytes.
pub fn split(file_id: u32, name: &str, data: &[u8], limit: usize) -> Result<Vec<Piece>> {
    if data.is_empty() || data.len() > FILE_MAX_BYTES {
        return Err(Error::protocol("file size out of range"));
    }
    let name = clean_name(name);
    let first_room = limit
        .checked_sub(PIECE_HDR + 1 + name.len())
        .filter(|n| *n > 0)
        .ok_or_else(|| Error::protocol("file piece too small"))?;
    let room = limit - PIECE_HDR;
    let mut cuts = vec![data.len().min(first_room)];
    let mut at = cuts[0];
    while at < data.len() {
        let n = (data.len() - at).min(room);
        cuts.push(n);
        at += n;
    }
    if cuts.len() > u8::MAX as usize {
        return Err(Error::protocol("too many file pieces"));
    }
    let count = cuts.len() as u8;
    let mut out = Vec::with_capacity(cuts.len());
    let mut at = 0;
    for (i, n) in cuts.into_iter().enumerate() {
        out.push(Piece {
            file_id,
            idx: i as u8,
            count,
            total: data.len() as u16,
            name: (i == 0).then(|| name.clone()),
            data: data[at..at + n].to_vec(),
        });
        at += n;
    }
    Ok(out)
}

/// Rough airtime for sending `len` bytes with this preset: every piece plus
/// its receipt, before any retry.
pub fn airtime_secs(preset: Preset, len: usize) -> f64 {
    let limit = body_limit(preset);
    let pieces = ((len + NAME_MAX).div_ceil(limit - PIECE_HDR)).max(1);
    // Frame header and CRC (~30 B on v2); a receipt is ~40 B.
    let piece_frames = pieces as f64 * crate::presets::airtime_secs(preset, limit + 30, 0);
    let receipts = pieces as f64 * crate::presets::airtime_secs(preset, 40, 0);
    let last_short = (pieces * limit).saturating_sub(len + NAME_MAX + pieces * PIECE_HDR);
    let saved = last_short as f64 * 8.0 / preset.bitrate_bps().max(1) as f64;
    (piece_frames + receipts - saved).max(0.0)
}

/// What [`Assembler::push`] made of a piece.
#[derive(Debug)]
pub enum Push {
    /// More pieces to come. `answer` is false for the last piece: its
    /// receipt waits until the whole file is here.
    Pending { answer: bool },
    /// The file is whole. `held_last` is the last piece if its receipt was
    /// held back and must be sent now.
    Complete {
        name: String,
        data: Vec<u8>,
        held_last: Option<Box<Envelope>>,
    },
}

struct Partial {
    count: u8,
    total: u16,
    name: Option<String>,
    pieces: HashMap<u8, Vec<u8>>,
    held_last: Option<Box<Envelope>>,
    touched: Instant,
}

/// Pieces received so far, by sending station and file id.
#[derive(Default)]
pub struct Assembler {
    partial: HashMap<(String, u32), Partial>,
    done: HashMap<(String, u32), Instant>,
}

impl Assembler {
    pub fn new() -> Self {
        Self::default()
    }

    fn sweep(&mut self, now: Instant) {
        self.partial
            .retain(|_, p| now.duration_since(p.touched) < PARTIAL_TTL);
        self.done.retain(|_, t| now.duration_since(*t) < DONE_TTL);
    }

    /// Store one piece from `env` (a File frame from `origin`).
    pub fn push(&mut self, origin: &str, env: &Envelope, piece: Piece) -> Push {
        let now = Instant::now();
        self.sweep(now);
        let key = (origin.to_ascii_uppercase(), piece.file_id);
        if self.done.contains_key(&key) {
            return Push::Pending { answer: true };
        }
        let p = self.partial.entry(key.clone()).or_insert_with(|| Partial {
            count: piece.count,
            total: piece.total,
            name: None,
            pieces: HashMap::new(),
            held_last: None,
            touched: now,
        });
        // A piece that disagrees with the ones already here starts over.
        if p.count != piece.count || p.total != piece.total {
            *p = Partial {
                count: piece.count,
                total: piece.total,
                name: None,
                pieces: HashMap::new(),
                held_last: None,
                touched: now,
            };
        }
        p.touched = now;
        if piece.name.is_some() {
            p.name = piece.name.clone();
        }
        let last = piece.is_last();
        p.pieces.insert(piece.idx, piece.data);
        if p.pieces.len() < p.count as usize {
            if last {
                p.held_last = Some(Box::new(env.clone()));
            }
            return Push::Pending { answer: !last };
        }
        let p = self.partial.remove(&key).expect("present");
        let mut data = Vec::with_capacity(p.total as usize);
        for i in 0..p.count {
            data.extend_from_slice(&p.pieces[&i]);
        }
        if data.len() != p.total as usize {
            // Corrupt set: forget it; the sender's retries start it again.
            return Push::Pending { answer: !last };
        }
        self.done.insert(key, now);
        let held_last = if last { None } else { p.held_last };
        Push::Complete {
            name: p.name.unwrap_or_else(|| "file".into()),
            data,
            held_last,
        }
    }

    /// May a retry of this piece be answered with a receipt? Not the last
    /// piece of a file that is still missing pieces.
    pub fn may_answer(&self, origin: &str, piece: &Piece) -> bool {
        if !piece.is_last() {
            return true;
        }
        let key = (origin.to_ascii_uppercase(), piece.file_id);
        self.done.contains_key(&key) || !self.partial.contains_key(&key)
    }
}

/// Where a received file is saved: `<dir>/<FROM>/<name>`, with `-1`, `-2`
/// … added before the extension when that name is taken.
pub fn save_path(dir: &Path, from: &str, name: &str) -> PathBuf {
    let from = clean_name(&from.to_ascii_uppercase());
    let folder = dir.join(from);
    let name = clean_name(name);
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i..].to_string()),
        _ => (name.clone(), String::new()),
    };
    let mut path = folder.join(&name);
    let mut n = 1;
    while path.exists() {
        path = folder.join(format!("{stem}-{n}{ext}"));
        n += 1;
    }
    path
}

/// Folder received files are saved in: `files` beside the station's store
/// (the data folder, unless the config moves the store).
pub fn received_dir(store_path: &Path) -> PathBuf {
    store_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(crate::config::default_data_dir)
        .join("files")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Callsign, Flags, MsgType};

    fn env_for(p: &Piece, seq: u32) -> Envelope {
        let mut e = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            seq,
            p.encode(),
            3,
            Flags::new(),
        )
        .unwrap();
        e.kind = MsgType::File;
        e
    }

    fn data(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 7 % 251) as u8).collect()
    }

    #[test]
    fn only_vhf_uhf_presets_and_direct_messages_may_send() {
        assert!(refusal(Preset::VhfFm, true, "M0XYZ", 100).is_none());
        assert!(refusal(Preset::Afsk1200, true, "M0XYZ", 4096).is_none());
        assert!(refusal(Preset::HfPoor, true, "M0XYZ", 100).is_some());
        assert!(refusal(Preset::VoxSafe, true, "M0XYZ", 100).is_some());
        assert!(refusal(Preset::VhfFm, true, "#bulletin", 100).is_some());
        assert!(refusal(Preset::VhfFm, false, "M0XYZ", 100).is_some());
        assert!(refusal(Preset::VhfFm, true, "M0XYZ", 4097).is_some());
        assert!(refusal(Preset::VhfFm, true, "M0XYZ", 0).is_some());
    }

    #[test]
    fn every_piece_fits_one_frame() {
        for preset in [Preset::VhfFm, Preset::Afsk1200] {
            let limit = body_limit(preset);
            let pieces = split(7, "notes.txt", &data(FILE_MAX_BYTES), limit).unwrap();
            for p in &pieces {
                assert!(p.encode().len() <= limit);
                let env = env_for(p, 1);
                assert!(env.encode().unwrap().len() <= preset.payload_bytes() as usize);
            }
        }
    }

    #[test]
    fn a_file_comes_back_whole_in_any_order() {
        let body = data(1000);
        let mut pieces = split(42, "map.gpx", &body, body_limit(Preset::Afsk1200)).unwrap();
        assert!(pieces.len() > 2);
        pieces.reverse();
        let mut a = Assembler::new();
        let n = pieces.len();
        let mut out = None;
        for (i, p) in pieces.into_iter().enumerate() {
            let back = Piece::decode(&p.encode()).unwrap();
            let env = env_for(&back, i as u32);
            match a.push("G4ABC", &env, back) {
                Push::Pending { answer } => {
                    // The last piece came first: its receipt is held.
                    assert_eq!(answer, i != 0);
                }
                Push::Complete {
                    name,
                    data,
                    held_last,
                } => {
                    assert_eq!(i, n - 1);
                    assert!(held_last.is_some());
                    out = Some((name, data));
                }
            }
        }
        let (name, got) = out.unwrap();
        assert_eq!(name, "map.gpx");
        assert_eq!(got, body);
    }

    #[test]
    fn the_last_pieces_receipt_waits_for_the_whole_file() {
        let pieces = split(9, "a.txt", &data(700), body_limit(Preset::VhfFm)).unwrap();
        assert_eq!(pieces.len(), 3);
        let mut a = Assembler::new();
        let last = pieces[2].clone();
        let env = env_for(&last, 3);
        assert!(matches!(
            a.push("G4ABC", &env, last.clone()),
            Push::Pending { answer: false }
        ));
        assert!(!a.may_answer("G4ABC", &last));
        let _ = a.push("G4ABC", &env_for(&pieces[0], 1), pieces[0].clone());
        let done = a.push("G4ABC", &env_for(&pieces[1], 2), pieces[1].clone());
        assert!(matches!(
            done,
            Push::Complete {
                held_last: Some(_),
                ..
            }
        ));
        assert!(a.may_answer("G4ABC", &last));
        // A late retry of a finished file is answered, not assembled again.
        assert!(matches!(
            a.push("G4ABC", &env, last),
            Push::Pending { answer: true }
        ));
    }

    #[test]
    fn names_are_made_safe() {
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name("C:\\Users\\me\\my photo.jpg"), "my_photo.jpg");
        assert_eq!(clean_name(".hidden"), "hidden");
        assert_eq!(clean_name(""), "file");
        assert_eq!(clean_name(".."), "file");
        let long = format!("{}.txt", "a".repeat(80));
        let c = clean_name(&long);
        assert!(c.len() <= NAME_MAX && c.ends_with(".txt"));
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let first = save_path(dir.path(), "g4abc", "a.txt");
        assert!(first.ends_with("G4ABC/a.txt"));
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        std::fs::write(&first, b"x").unwrap();
        assert!(save_path(dir.path(), "G4ABC", "a.txt").ends_with("G4ABC/a-1.txt"));
    }

    #[test]
    fn four_kb_is_about_half_a_minute_on_vhf_fm() {
        let s = airtime_secs(Preset::VhfFm, FILE_MAX_BYTES);
        assert!((20.0..45.0).contains(&s), "{s}");
        let a = airtime_secs(Preset::Afsk1200, FILE_MAX_BYTES);
        assert!(a > s);
    }

    #[test]
    fn junk_is_not_a_piece() {
        assert!(Piece::decode(b"hello").is_err());
        assert!(Piece::decode(&[1, 0, 0, 0, 0, 3, 2, 1, 0]).is_err());
    }
}
