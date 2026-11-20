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

/// Largest ordinary file that may be sent: about 1 min on vhf-fast.
pub const FILE_MAX_BYTES: usize = 4096;
/// Largest picture, on vhf-fast only: about 2 min (Tobias, 2026-11-20).
/// Also the most a station takes in, so older stations refuse bigger.
pub const PICTURE_MAX_BYTES: usize = 8192;
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
    matches!(preset, Preset::VhfFm | Preset::VhfFast | Preset::Afsk1200)
}

/// Biggest picture we read in to shrink (a phone photo is 2-8 MB).
pub const PICTURE_SOURCE_MAX: usize = 30 * 1024 * 1024;

/// Pictures go on vhf-fast only (Tobias, 2026-11-20): even shrunk they
/// are the longest thing a station sends. Other files are unchanged.
pub fn picture_refusal(preset: Preset) -> Option<String> {
    if preset == Preset::VhfFast {
        None
    } else {
        Some(format!(
            "pictures go on vhf-fast only; this station is on {}",
            preset.as_str()
        ))
    }
}

/// Why this file cannot be sent, in plain words. `None` means it can.
/// `max` is [`FILE_MAX_BYTES`], or [`PICTURE_MAX_BYTES`] for a picture.
pub fn refusal(
    preset: Preset,
    uses_radio: bool,
    target: &str,
    len: usize,
    max: usize,
) -> Option<String> {
    if target.starts_with('#') || target.starts_with('&') {
        return Some("files go in direct messages only, not in channels".into());
    }
    if !uses_radio {
        return Some("files go over the radio only: switch to a mode that uses the radio".into());
    }
    if !preset_allows(preset) {
        return Some(format!(
            "files go on VHF/UHF only (vhf-fm, vhf-fast or afsk-1200); this station is on {}",
            preset.as_str()
        ));
    }
    if len == 0 {
        return Some("the file is empty".into());
    }
    if len > max {
        return Some(format!(
            "the file is {}; the limit is {} (about {} s on {})",
            human_size(len),
            human_size(max),
            airtime_secs(preset, max).round() as u64,
            preset.as_str()
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

/// Largest piece body in a burst (QA #365). On vhf-fast a piece that fits
/// modem73's 1 s short QAM16 frame (256 B less its length) carries twice
/// the bytes per second of a 2.6 s normal frame, so a burst cuts to that.
/// Receivers take any piece size, so older ones are not affected.
pub fn burst_body_limit(preset: Preset) -> usize {
    if preset == Preset::VhfFast {
        let short = crate::presets::Rung::OfdmQam16Half.mail_mtu() as usize;
        short
            .saturating_sub(HEADER_LEN + CRC_LEN)
            .max(PIECE_HDR + 1 + NAME_MAX + 16)
    } else {
        body_limit(preset)
    }
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
        if count == 0 || idx >= count || total as usize > PICTURE_MAX_BYTES {
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
    if data.is_empty() || data.len() > PICTURE_MAX_BYTES {
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

/// Pieces mode on the air, per piece, beyond its frame and receipt: carrier
/// sense, the receipt's turnaround and the air queue waiting for it. QA's
/// rc10 run on vhf-fast: 16 pieces with no retry took about 8 min.
pub const PIECE_GAP_SECS: f64 = 25.0;
/// A burst receiver reports this long after the last piece it heard, when
/// that was not the file's last piece (which ends every burst). Long enough
/// to ride out a sender's pauses inside one burst: rc11's Pi 3 keyed 38
/// pieces in groups of four with gaps of 6 to 13 s, and a 6 s wait reported
/// in the middle of the burst (QA #365).
pub const BURST_QUIET_SECS: f64 = 15.0;
/// A burst receiver reports this long after hearing the file's last piece,
/// or a piece of a file it already has whole.
pub const REPORT_SOON_SECS: f64 = 1.0;

/// Pieces a `len`-byte file is cut into for a body limit of `limit`.
pub fn piece_count(len: usize, limit: usize) -> usize {
    ((len + NAME_MAX).div_ceil(limit.saturating_sub(PIECE_HDR).max(1))).max(1)
}

/// Honest time to send `len` bytes on `preset`, the way it will go (QA
/// #365): a burst is every piece back to back, then the far end's one
/// report; pieces mode is a piece, its receipt and the wait between.
pub fn send_secs(preset: Preset, len: usize, burst: bool) -> f64 {
    if !burst {
        let gap = if matches!(preset, Preset::VhfFm | Preset::VhfFast) {
            PIECE_GAP_SECS * piece_count(len, body_limit(preset)) as f64
        } else {
            0.0
        };
        return airtime_secs(preset, len) + gap;
    }
    let limit = burst_body_limit(preset);
    let pieces = piece_count(len, limit) as f64;
    let report = 60;
    let (frame, short) = if matches!(preset, Preset::VhfFm | Preset::VhfFast) {
        let rung = crate::presets::Rung::from_index(preset.ladder_start());
        (rung.frame_secs(limit + 30), rung.frame_secs(report))
    } else {
        (
            crate::presets::airtime_secs(preset, limit + 30, 0),
            crate::presets::airtime_secs(preset, report, 0),
        )
    };
    pieces * frame + REPORT_SOON_SECS + short
}

/// Rough airtime for sending `len` bytes with this preset: every piece plus
/// its receipt, before any retry.
pub fn airtime_secs(preset: Preset, len: usize) -> f64 {
    let limit = body_limit(preset);
    let pieces = ((len + NAME_MAX).div_ceil(limit - PIECE_HDR)).max(1);
    // modem73 OFDM frames take their full time however little they carry,
    // so count frames, not bits: a piece and its receipt each.
    if matches!(preset, Preset::VhfFm | Preset::VhfFast) {
        let rung = crate::presets::Rung::from_index(preset.ladder_start());
        return pieces as f64 * (rung.frame_secs(limit + 30) + rung.frame_secs(40));
    }
    // Frame header and CRC (~30 B on v2); a receipt is ~40 B.
    let piece_frames = pieces as f64 * crate::presets::airtime_secs(preset, limit + 30, 0);
    let receipts = pieces as f64 * crate::presets::airtime_secs(preset, 40, 0);
    let last_short = (pieces * limit).saturating_sub(len + NAME_MAX + pieces * PIECE_HDR);
    let saved = last_short as f64 * 8.0 / preset.bitrate_bps().max(1) as f64;
    (piece_frames + receipts - saved).max(0.0)
}

/// Version byte of a burst report: never a piece, so an older station's
/// [`Piece::decode`] passes it by.
pub const REPORT_VER: u8 = 2;

/// The one answer to a burst (QA #365): which pieces of `file_id` the
/// receiver holds. Body: `ver u8 (2), file_id u32, count u8, bitmap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub file_id: u32,
    pub count: u8,
    pub held: Vec<u8>,
}

impl Report {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![REPORT_VER];
        out.extend_from_slice(&self.file_id.to_le_bytes());
        out.push(self.count);
        let mut bits = vec![0u8; usize::from(self.count).div_ceil(8)];
        for &i in &self.held {
            if i < self.count {
                bits[usize::from(i / 8)] |= 1 << (i % 8);
            }
        }
        out.extend_from_slice(&bits);
        out
    }

    pub fn decode(body: &[u8]) -> Result<Self> {
        if body.len() < 6 || body[0] != REPORT_VER {
            return Err(Error::protocol("not a file report"));
        }
        let file_id = u32::from_le_bytes(body[1..5].try_into().unwrap());
        let count = body[5];
        let bits = &body[6..];
        if count == 0 || bits.len() != usize::from(count).div_ceil(8) {
            return Err(Error::protocol("bad file report"));
        }
        let held = (0..count)
            .filter(|i| bits[usize::from(i / 8)] & (1 << (i % 8)) != 0)
            .collect();
        Ok(Self {
            file_id,
            count,
            held,
        })
    }

    pub fn complete(&self) -> bool {
        self.held.len() == usize::from(self.count)
    }

    /// Pieces the receiver lacks, lowest first.
    pub fn missing(&self) -> Vec<u8> {
        (0..self.count).filter(|i| !self.held.contains(i)).collect()
    }
}

/// What [`Assembler::push`] made of a piece.
#[derive(Debug)]
pub enum Push {
    /// More pieces to come. `answer` is false for the last piece: its
    /// receipt waits until the whole file is here. `progress` is pieces held
    /// out of the count when this piece was new (`None` for a repeat);
    /// `first` is true for the first piece heard of a file.
    Pending {
        answer: bool,
        progress: Option<(u8, u8)>,
        first: bool,
    },
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
            return Push::Pending {
                answer: true,
                progress: None,
                first: false,
            };
        }
        let mut first = !self.partial.contains_key(&key);
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
            first = true;
        }
        p.touched = now;
        if piece.name.is_some() {
            p.name = piece.name.clone();
        }
        let last = piece.is_last();
        let new = p.pieces.insert(piece.idx, piece.data).is_none();
        if p.pieces.len() < p.count as usize {
            if last {
                p.held_last = Some(Box::new(env.clone()));
            }
            return Push::Pending {
                answer: !last,
                progress: new.then_some((p.pieces.len() as u8, p.count)),
                first,
            };
        }
        let p = self.partial.remove(&key).expect("present");
        let mut data = Vec::with_capacity(p.total as usize);
        for i in 0..p.count {
            data.extend_from_slice(&p.pieces[&i]);
        }
        if data.len() != p.total as usize {
            // Corrupt set: forget it; the sender's retries start it again.
            return Push::Pending {
                answer: !last,
                progress: None,
                first: false,
            };
        }
        self.done.insert(key, now);
        let held_last = if last { None } else { p.held_last };
        Push::Complete {
            name: p.name.unwrap_or_else(|| "file".into()),
            data,
            held_last,
        }
    }

    /// What a burst receiver reports for `file_id` from `origin`: every
    /// piece once the file is saved, or the pieces held so far.
    pub fn report(&self, origin: &str, file_id: u32, count: u8) -> Report {
        let key = (origin.to_ascii_uppercase(), file_id);
        let held = if self.done.contains_key(&key) {
            (0..count).collect()
        } else {
            let mut held: Vec<u8> = self
                .partial
                .get(&key)
                .map(|p| p.pieces.keys().copied().collect())
                .unwrap_or_default();
            held.sort_unstable();
            held
        };
        Report {
            file_id,
            count,
            held,
        }
    }

    /// The whole file from `origin` is here and saved.
    pub fn is_done(&self, origin: &str, file_id: u32) -> bool {
        self.done
            .contains_key(&(origin.to_ascii_uppercase(), file_id))
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

/// Width of the progress bar in cells.
const BAR_CELLS: usize = 20;

/// A DOS-style progress bar for `have` of `count` pieces, such as
/// `[████████░░░░░░░░░░░░] 6/15`.
pub fn progress_bar(have: u8, count: u8) -> String {
    let count = count.max(1);
    let have = have.min(count);
    let full = (have as usize * BAR_CELLS) / count as usize;
    format!(
        "[{}{}] {have}/{count}",
        "█".repeat(full),
        "░".repeat(BAR_CELLS - full)
    )
}

/// The `+radio/file` tag value (`have/count`) back to a bar.
pub fn progress_bar_from_tag(tag: &str) -> Option<String> {
    let (have, count) = tag.split_once('/')?;
    Some(progress_bar(
        have.trim().parse().ok()?,
        count.trim().parse().ok()?,
    ))
}

/// A burst's step (`+radio/filestep`, QA #365) in the marker style of
/// grouped lines: `keyed`, `waiting`, `resend:N`, `pieces:N`, `complete`.
pub fn step_text(step: &str) -> Option<String> {
    let (word, n) = step.split_once(':').unwrap_or((step, ""));
    Some(match word {
        "keyed" => "burst keyed".into(),
        "waiting" => "burst keyed, waiting for report".into(),
        "resend" => format!("{n} missing, resending"),
        "pieces" => format!("{n} missing, sending piece by piece"),
        "complete" => "complete".into(),
        _ => return None,
    })
}

/// The bar for a `+radio/file` tag and, when there is one, its step.
pub fn progress_with_step(file: &str, step: Option<&str>) -> Option<String> {
    let bar = progress_bar_from_tag(file)?;
    Some(match step.and_then(step_text) {
        Some(t) => format!("{bar} · {t}"),
        None => bar,
    })
}

/// Files we are sending: which piece receipts have come back, so the file
/// line can show how far along it is.
#[derive(Default)]
pub struct Outgoing {
    /// Piece msgid -> file line id (the last piece's msgid).
    pieces: HashMap<String, String>,
    /// File line id -> (destination, piece count, pieces acknowledged).
    files: HashMap<String, (String, u8, std::collections::HashSet<String>)>,
    order: std::collections::VecDeque<String>,
}

/// Files kept in [`Outgoing`] before the oldest is forgotten.
const OUTGOING_CAP: usize = 32;

impl Outgoing {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start tracking a file sent to `dest` as `pieces` (msgids, in order;
    /// the last one is the file line id).
    pub fn start(&mut self, dest: &str, pieces: &[String]) {
        let Some(line) = pieces.last().cloned() else {
            return;
        };
        for p in pieces {
            self.pieces.insert(p.clone(), line.clone());
        }
        self.files.insert(
            line.clone(),
            (dest.to_string(), pieces.len() as u8, Default::default()),
        );
        self.order.push_back(line);
        while self.order.len() > OUTGOING_CAP {
            if let Some(old) = self.order.pop_front() {
                self.forget(&old);
            }
        }
    }

    /// A receipt for `piece`. Returns (file line id, destination, pieces
    /// acknowledged, piece count) when it moved a file along.
    pub fn acked(&mut self, piece: &str) -> Option<(String, String, u8, u8)> {
        let line = self.pieces.get(piece)?.clone();
        let (dest, count, got) = self.files.get_mut(&line)?;
        if !got.insert(piece.to_string()) {
            return None;
        }
        let out = (line.clone(), dest.clone(), got.len() as u8, *count);
        if out.2 >= out.3 {
            self.forget(&line);
        }
        Some(out)
    }

    /// Stop tracking the file line `line` (Cancel) and return its pieces'
    /// msgids, the line's own last. Empty when it is not being sent.
    pub fn cancel(&mut self, line: &str) -> Vec<String> {
        if !self.files.contains_key(line) {
            return Vec::new();
        }
        let mut ids: Vec<String> = self
            .pieces
            .iter()
            .filter(|(p, l)| l.as_str() == line && p.as_str() != line)
            .map(|(p, _)| p.clone())
            .collect();
        ids.sort();
        ids.push(line.to_string());
        self.forget(line);
        ids
    }

    fn forget(&mut self, line: &str) {
        self.files.remove(line);
        self.pieces.retain(|_, l| l != line);
        self.order.retain(|l| l != line);
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

/// Folder for the station's own file work (`files` beside the store):
/// uploads waiting to be sent, and received files when there is no Desktop.
pub fn data_files_dir(store_path: &Path) -> PathBuf {
    store_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(crate::config::default_data_dir)
        .join("files")
}

/// Folder received files are saved in: `[store] files_dir` when set, else
/// "WeeChat Radio" on the Desktop (Mac, Windows, Linux with a desktop),
/// else `files` beside the store (a headless Pi, for one).
pub fn received_dir(store: &crate::config::StoreConfig) -> PathBuf {
    if !store.files_dir.as_os_str().is_empty() {
        return store.files_dir.clone();
    }
    desktop_dir()
        .map(|d| d.join("WeeChat Radio"))
        .unwrap_or_else(|| data_files_dir(&store.path))
}

/// The user's Desktop folder, when there is one on this machine.
fn desktop_dir() -> Option<PathBuf> {
    let dirs = directories::UserDirs::new()?;
    let d = dirs.desktop_dir()?.to_path_buf();
    d.is_dir().then_some(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QA #365: the report round-trips and says what is missing.
    #[test]
    fn a_burst_report_lists_what_is_held_and_missing() {
        let r = Report {
            file_id: 0xdead_beef,
            count: 39,
            held: (0..39).filter(|i| i % 10 != 3).collect(),
        };
        let back = Report::decode(&r.encode()).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.missing(), vec![3, 13, 23, 33]);
        assert!(!back.complete());
        assert!(r.encode().len() <= 11);
        assert!(Report::decode(&[2, 0, 0, 0, 0, 9, 0]).is_err());
    }

    /// The estimate follows the way the file will go: an 8 KB picture in a
    /// burst on vhf-fast is under a minute; QA's 4.4 KB in pieces was
    /// about 8 minutes, and the pieces estimate now says so.
    #[test]
    fn the_estimate_matches_burst_and_pieces() {
        let burst = send_secs(Preset::VhfFast, 8192, true);
        assert!((30.0..60.0).contains(&burst), "{burst:.0} s");
        let pieces = send_secs(Preset::VhfFast, 4500, false);
        assert!((6.0 * 60.0..10.0 * 60.0).contains(&pieces), "{pieces:.0} s");
        // A burst piece fits the 1 s short frame on vhf-fast.
        assert!(burst_body_limit(Preset::VhfFast) + HEADER_LEN + CRC_LEN <= 254);
        assert_eq!(burst_body_limit(Preset::VhfFm), body_limit(Preset::VhfFm));
    }
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
        assert!(refusal(Preset::VhfFm, true, "M0XYZ", 100, FILE_MAX_BYTES).is_none());
        assert!(refusal(Preset::Afsk1200, true, "M0XYZ", 4096, FILE_MAX_BYTES).is_none());
        assert!(refusal(Preset::HfPoor, true, "M0XYZ", 100, FILE_MAX_BYTES).is_some());
        assert!(refusal(Preset::VoxSafe, true, "M0XYZ", 100, FILE_MAX_BYTES).is_some());
        assert!(refusal(Preset::VhfFm, true, "#bulletin", 100, FILE_MAX_BYTES).is_some());
        assert!(refusal(Preset::VhfFm, false, "M0XYZ", 100, FILE_MAX_BYTES).is_some());
        assert!(refusal(Preset::VhfFm, true, "M0XYZ", 4097, FILE_MAX_BYTES).is_some());
        assert!(refusal(Preset::VhfFm, true, "M0XYZ", 0, FILE_MAX_BYTES).is_some());
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
                Push::Pending { answer, .. } => {
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
            Push::Pending { answer: false, .. }
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
            Push::Pending { answer: true, .. }
        ));
    }

    #[test]
    fn the_progress_bar_looks_like_dos() {
        assert_eq!(progress_bar(0, 15), format!("[{}] 0/15", "░".repeat(20)));
        assert_eq!(progress_bar(15, 15), format!("[{}] 15/15", "█".repeat(20)));
        assert_eq!(
            progress_bar(6, 15),
            format!("[{}{}] 6/15", "█".repeat(8), "░".repeat(12))
        );
        assert_eq!(progress_bar_from_tag("1/2"), Some(progress_bar(1, 2)));
        assert_eq!(progress_bar_from_tag("x"), None);
    }

    #[test]
    fn outgoing_counts_each_receipt_once() {
        let mut o = Outgoing::new();
        let ids: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        o.start("G4ABC", &ids);
        assert_eq!(o.acked("a"), Some(("c".into(), "G4ABC".into(), 1, 3)));
        assert_eq!(o.acked("a"), None);
        assert_eq!(o.acked("b").map(|t| t.2), Some(2));
        assert_eq!(o.acked("c").map(|t| t.2), Some(3));
        // Finished files are forgotten.
        assert_eq!(o.acked("zzz"), None);
        assert!(o.files.is_empty() && o.pieces.is_empty());
    }

    #[test]
    fn the_assembler_reports_progress_on_new_pieces() {
        let pieces = split(9, "a.txt", &data(700), body_limit(Preset::VhfFm)).unwrap();
        let mut a = Assembler::new();
        let env0 = env_for(&pieces[0], 1);
        assert!(matches!(
            a.push("G4ABC", &env0, pieces[0].clone()),
            Push::Pending {
                progress: Some((1, 3)),
                first: true,
                ..
            }
        ));
        assert!(matches!(
            a.push("G4ABC", &env0, pieces[0].clone()),
            Push::Pending {
                progress: None,
                first: false,
                ..
            }
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
    fn four_kb_takes_about_a_minute_on_vhf_fast() {
        // 15 pieces of a 2.6 s frame and a ~1 s short receipt.
        let f = airtime_secs(Preset::VhfFast, FILE_MAX_BYTES);
        assert!((45.0..65.0).contains(&f), "{f}");
        // vhf-fm has no short frame, so every receipt takes 2.6 s too.
        let s = airtime_secs(Preset::VhfFm, FILE_MAX_BYTES);
        assert!((70.0..90.0).contains(&s), "{s}");
        let a = airtime_secs(Preset::Afsk1200, FILE_MAX_BYTES);
        assert!(a > f);
    }

    #[test]
    fn an_8_kb_picture_takes_about_two_minutes_on_vhf_fast() {
        let p = airtime_secs(Preset::VhfFast, PICTURE_MAX_BYTES);
        assert!((90.0..130.0).contains(&p), "{p}");
        // 29 pieces: the u8 piece count has room.
        let pieces = split(
            1,
            "cat.jpg",
            &data(PICTURE_MAX_BYTES),
            body_limit(Preset::VhfFast),
        )
        .unwrap();
        assert!(pieces.len() < 255);
        assert!(refusal(Preset::VhfFast, true, "M0XYZ", 8192, PICTURE_MAX_BYTES).is_none());
        assert!(refusal(Preset::VhfFast, true, "M0XYZ", 8193, PICTURE_MAX_BYTES).is_some());
    }

    #[test]
    fn pictures_go_on_vhf_fast_only() {
        assert!(picture_refusal(Preset::VhfFast).is_none());
        assert!(picture_refusal(Preset::VhfFm)
            .unwrap()
            .contains("vhf-fast only"));
        assert!(picture_refusal(Preset::Afsk1200).is_some());
    }

    #[test]
    fn junk_is_not_a_piece() {
        assert!(Piece::decode(b"hello").is_err());
        // A burst report is never taken for a piece, by old or new code.
        let r = Report {
            file_id: 7,
            count: 3,
            held: vec![0, 2],
        };
        assert!(Piece::decode(&r.encode()).is_err());
        assert!(Piece::decode(&[1, 0, 0, 0, 0, 3, 2, 1, 0]).is_err());
    }
}
