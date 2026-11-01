//! SPDX-License-Identifier: Apache-2.0
//! Bluetooth audio bridge: modem73 talks to a VR-N76 / VR-N7600 (and the other
//! Benshi radios) over the radio's own Bluetooth audio channel, no cable.
//!
//! ```text
//! modem73 ──tcp: network audio (48 kHz s16)──┐
//!         ──rigctl T 1 / T 0────────────────┐│
//!                                    wcr bridge (this module)
//!                       resample 48k↔32k, SBC, 0x7E framing
//!                                            │
//!                    RFCOMM "BS AOC" channel to the radio
//! ```
//!
//! The radio transmits while audio frames arrive and unkeys on the
//! end-of-audio frame, so modem73's PTT (rigctl) decides when we forward its
//! audio and when we send that end frame. This bridge is the audio clock: it
//! feeds modem73 20 ms of received audio (silence when the radio is quiet)
//! and gets 20 ms of transmit audio back each tick.

pub mod frame;
pub mod resample;
pub mod sbc;

use crate::status::SharedStatus;
use crate::tnc::link::Connection;
use parking_lot::Mutex;
use resample::Resampler;
use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};

/// modem73's sample rate.
const MODEM_RATE: usize = 48_000;
/// One clock tick of audio for modem73.
const TICK: Duration = Duration::from_millis(20);
const TICK_SAMPLES: usize = MODEM_RATE / 50;
/// Received audio waiting for modem73, at most this much (then the oldest goes).
const RX_QUEUE_MAX: usize = MODEM_RATE;
/// Audio held back at key-up so a Bluetooth hiccup cannot drain the radio's
/// playout buffer mid-frame.
const TX_PREROLL: Duration = Duration::from_millis(300);
/// Stuck-PTT guard: unkey the radio after this long whatever modem73 says.
const TX_LIMIT: Duration = Duration::from_secs(120);
const RETRY: Duration = Duration::from_secs(5);
/// How often the receive counters go to the log while linked.
const RX_LOG_EVERY: Duration = Duration::from_secs(10);
/// Allowed range for `[tnc] bt_tx_level_db`. modem73's own audio already
/// peaks at full scale (vhf-fm: peak 0 dBFS, RMS -8 dBFS), so any gain above
/// 0 dB only clips it: +6 dB clipped 24% of a QPSK burst and lowered the far
/// end's SNR (QA #239). The radio's own Bluetooth mic gain sets the
/// deviation.
const TX_LEVEL_MIN_DB: f32 = -20.0;
const TX_LEVEL_MAX_DB: f32 = 0.0;

/// Presets that survive the radio's FM + SBC audio path. 8PSK (`hf-good`)
/// fails over the air (HTCommander's measurements, #202); `vox-safe` is for
/// VOX keying, which this path does not use.
pub fn preset_allowed(p: crate::presets::Preset) -> bool {
    use crate::presets::Preset;
    matches!(
        p,
        Preset::VhfFm | Preset::HfPoor | Preset::HfWeak | Preset::HfDeep
    )
}

/// Where modem73 should connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ports {
    pub audio: u16,
    pub rigctl: u16,
}

impl Ports {
    /// `--input-device` / `--output-device` for modem73.
    pub fn audio_device(&self) -> String {
        format!("tcp:127.0.0.1:{}", self.audio)
    }

    /// `--rigctl` for modem73.
    pub fn rigctl_addr(&self) -> String {
        format!("127.0.0.1:{}", self.rigctl)
    }
}

/// Opens the radio's audio channel, or says why not.
pub type Connector = Box<dyn FnMut() -> Result<Connection, String> + Send>;

#[derive(Default)]
struct Shared {
    ptt: AtomicBool,
    /// 48 kHz audio from the radio, waiting for the next tick.
    rx: Mutex<VecDeque<f32>>,
    /// Framed bytes for the radio; `None` while the radio is not linked.
    to_radio: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    /// Linear gain on modem73's transmit audio (`[tnc] bt_tx_level_db`).
    tx_gain: f32,
}

impl Shared {
    fn send_to_radio(&self, bytes: Vec<u8>) {
        if let Some(tx) = self.to_radio.lock().as_ref() {
            let _ = tx.send(bytes);
        }
    }
}

static RUNNING: OnceLock<Ports> = OnceLock::new();
/// Gain on the radio's received audio (`[tnc] bt_rx_level_db`, #241), as
/// f32 bits. Read for every block, so the RX slider takes effect at once.
static RX_GAIN_BITS: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0

/// Sets the gain on the radio's received audio, dB (kept in range).
pub fn set_rx_level_db(db: f32) {
    let gain = 10f32.powf(crate::levels::bt_rx_level_db(db) / 20.0);
    RX_GAIN_BITS.store(gain.to_bits(), Ordering::Relaxed);
}

fn rx_gain() -> f32 {
    f32::from_bits(RX_GAIN_BITS.load(Ordering::Relaxed))
}

/// Received samples as modem73 hears them: gain applied, full scale kept.
fn rx_samples(pcm: &mut Vec<i16>, gain: f32) -> Vec<f32> {
    pcm.drain(..)
        .map(|s| (s as f32 / 32768.0 * gain).clamp(-1.0, 1.0))
        .collect()
}

/// A sample this close to full scale counts as clipped. SBC decoding
/// rounds, so a clipped sample from the radio rarely lands exactly on it.
const CLIP_AT: i16 = 32_440; // -0.09 dBFS

/// Clipping in the radio's own audio, counted before wcr's RX gain (#243):
/// a lowered RX gain hid clipping inside the radio. Reported every
/// [`ClipCount::WINDOW`] samples, the same half second as modem73's count.
#[derive(Default)]
struct ClipCount {
    seen: u32,
    clipped: u32,
}

impl ClipCount {
    /// Half a second of the radio's 32 kHz audio.
    const WINDOW: u32 = 16_000;

    /// Counts `pcm`; returns the % clipped when a window is full.
    fn push(&mut self, pcm: &[i16]) -> Option<f32> {
        for &s in pcm {
            self.seen += 1;
            if s >= CLIP_AT || s <= -CLIP_AT {
                self.clipped += 1;
            }
        }
        if self.seen < Self::WINDOW {
            return None;
        }
        let pct = self.clipped as f32 * 100.0 / self.seen as f32;
        *self = Self::default();
        Some(pct)
    }
}

/// A link that opens but brings no audio this long after `linked` is
/// closed and opened again (QA #243: the Mac's first start sat 76 s with
/// 0 bytes until a beacon's write failed).
const NO_AUDIO_RECONNECT: Duration = Duration::from_secs(5);

/// The link is dead: linked this long ago and nothing has arrived.
fn no_audio_since_linked(linked_for: Duration, bytes: u64) -> bool {
    bytes == 0 && linked_for >= NO_AUDIO_RECONNECT
}
/// Last radio link state, put back after the station clears its status.
static LAST_STATUS: Mutex<(String, bool)> = Mutex::new((String::new(), false));

/// Starts the bridge for the radio in `cfg.tnc` once per process; later calls
/// return the same ports. The bridge reconnects to the radio by itself.
pub fn ensure_started(cfg: &crate::config::Config, snap: Arc<SharedStatus>) -> io::Result<Ports> {
    set_rx_level_db(cfg.tnc.bt_rx_level_db);
    if let Some(p) = RUNNING.get() {
        let (text, ok) = LAST_STATUS.lock().clone();
        let mut s = snap.lock();
        s.tnc = text;
        s.tnc_ok = ok;
        return Ok(*p);
    }
    let tnc = cfg.tnc.clone();
    let snap_c = snap.clone();
    let level = tnc.bt_tx_level_db;
    let connector: Connector = Box::new(move || connect_radio(&tnc, &snap_c));
    let ports = start_with_level(connector, snap, level)?;
    Ok(*RUNNING.get_or_init(|| ports))
}

fn connect_radio(
    tnc: &crate::config::TncConfig,
    snap: &SharedStatus,
) -> Result<Connection, String> {
    use crate::tnc::bluetooth;
    let wanted = match tnc.bt_name.trim() {
        "" => "VR-N76",
        n => n,
    };
    set_status(snap, format!("searching for {wanted}…"), false);
    let dev = bluetooth::resolve(tnc)?;
    set_status(snap, format!("connecting to {}…", dev.label()), false);
    bluetooth::connect_audio(&dev).map_err(|e| {
        format!(
            "{} audio not answering: {}. Radio on, Bluetooth on, phone app closed?",
            dev.short_name(),
            e.to_string()
                .split(" (os error")
                .next()
                .unwrap_or("")
                .trim()
        )
    })
}

fn set_status(snap: &SharedStatus, text: String, ok: bool) {
    *LAST_STATUS.lock() = (text.clone(), ok);
    let mut s = snap.lock();
    s.tnc = text;
    s.tnc_ok = ok;
}

/// Starts the bridge with any radio connector (tests use a fake radio).
pub fn start(connector: Connector, snap: Arc<SharedStatus>) -> io::Result<Ports> {
    start_with_level(connector, snap, 0.0)
}

/// The transmit gain for a `[tnc] bt_tx_level_db` setting, kept in range.
pub fn tx_gain(level_db: f32) -> f32 {
    let db = if level_db.is_finite() {
        level_db.clamp(TX_LEVEL_MIN_DB, TX_LEVEL_MAX_DB)
    } else {
        0.0
    };
    10f32.powf(db / 20.0)
}

/// [`start`] with a transmit level change in dB (0 = modem73's own level).
pub fn start_with_level(
    connector: Connector,
    snap: Arc<SharedStatus>,
    tx_level_db: f32,
) -> io::Result<Ports> {
    let audio = TcpListener::bind("127.0.0.1:0")?;
    let rigctl = TcpListener::bind("127.0.0.1:0")?;
    let ports = Ports {
        audio: audio.local_addr()?.port(),
        rigctl: rigctl.local_addr()?.port(),
    };
    let shared = Arc::new(Shared {
        tx_gain: tx_gain(tx_level_db),
        ..Shared::default()
    });
    if tx_level_db > TX_LEVEL_MAX_DB {
        tracing::warn!(
            "btaudio: bt_tx_level_db {tx_level_db:+.1} would clip modem73's audio, which already peaks at full scale; sending at 0 dB. Raise the radio's Bluetooth mic gain instead."
        );
    } else if tx_level_db != 0.0 {
        tracing::info!(
            "btaudio: transmit level {:+.1} dB",
            20.0 * shared.tx_gain.log10()
        );
    }

    let sh = shared.clone();
    std::thread::Builder::new()
        .name("bt-rigctl".into())
        .spawn(move || {
            for conn in rigctl.incoming().flatten() {
                let sh = sh.clone();
                std::thread::spawn(move || serve_rigctl(conn, &sh));
            }
        })?;

    let sh = shared.clone();
    std::thread::Builder::new()
        .name("bt-audio".into())
        .spawn(move || {
            for conn in audio.incoming().flatten() {
                if let Err(e) = run_clock(conn, &sh) {
                    tracing::info!("btaudio: modem73 audio closed: {e}");
                }
            }
        })?;

    std::thread::Builder::new()
        .name("bt-radio".into())
        .spawn(move || radio_manager(connector, &shared, &snap))?;

    Ok(ports)
}

/// Just enough of rigctld for modem73's PTT: `T 1`, `T 0` and `t`.
fn serve_rigctl(conn: TcpStream, sh: &Shared) {
    let Ok(mut out) = conn.try_clone() else {
        return;
    };
    for line in BufReader::new(conn).lines() {
        let Ok(line) = line else { break };
        let reply = match line.trim() {
            "T 1" | "T 0" => {
                sh.ptt.store(line.trim() == "T 1", Ordering::SeqCst);
                "RPRT 0\n"
            }
            "t" => {
                if sh.ptt.load(Ordering::SeqCst) {
                    "1\n"
                } else {
                    "0\n"
                }
            }
            _ => "RPRT -4\n",
        };
        if out.write_all(reply.as_bytes()).is_err() {
            break;
        }
    }
    // modem73 gone: never leave the radio keyed.
    sh.ptt.store(false, Ordering::SeqCst);
}

/// Keeps the radio's audio channel open: reads its audio into the RX queue
/// and runs a writer for what the clock sends it.
fn radio_manager(mut connector: Connector, sh: &Shared, snap: &Arc<SharedStatus>) {
    loop {
        let conn = match connector() {
            Ok(c) => c,
            Err(why) => {
                tracing::warn!("btaudio: {why}");
                set_status(snap, why, false);
                std::thread::sleep(RETRY);
                continue;
            }
        };
        let Connection {
            mut reader,
            mut writer,
            zero_is_eof,
            label,
        } = conn;
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        *sh.to_radio.lock() = Some(tx);
        std::thread::spawn(move || {
            for bytes in rx {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        set_status(snap, format!("{label} linked"), true);
        tracing::info!("btaudio: {label} audio linked");

        let mut deframer = frame::Deframer::default();
        let mut types = TypeCounter::default();
        let mut decode = sbc::Decode::default();
        let mut up = Resampler::radio_to_modem();
        let mut buf = [0u8; 2048];
        let mut pcm = Vec::new();
        let mut at48 = Vec::new();
        let linked = Arc::new(AtomicBool::new(true));
        spawn_rx_log(label.clone(), snap.clone(), linked.clone());
        let linked_at = Instant::now();
        let mut got_bytes = 0u64;
        let mut clips = ClipCount::default();
        let why = loop {
            if no_audio_since_linked(linked_at.elapsed(), got_bytes) {
                break format!(
                    "no audio from the radio within {} s of linking",
                    NO_AUDIO_RECONNECT.as_secs()
                );
            }
            match reader.read(&mut buf) {
                Ok(0) if zero_is_eof => break "closed".to_string(),
                Ok(0) => continue,
                Ok(n) => {
                    got_bytes += n as u64;
                    let mut stats = snap.lock();
                    stats.bt_rx_bytes += n as u64;
                    types.push(&buf[..n], &mut stats.bt_rx_frames);
                    deframer.push(&buf[..n], |m| {
                        if let frame::Message::Audio(bytes) = m {
                            let before = pcm.len();
                            decode.push(&bytes, &mut pcm);
                            if pcm.len() - before < expected_samples(&bytes) {
                                stats.sbc_decode_errors += 1;
                            }
                        }
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => break e.to_string(),
            }
            if pcm.is_empty() {
                continue;
            }
            if let Some(pct) = clips.push(&pcm) {
                snap.lock().bt_rx_clip_pct = Some(pct);
            }
            let f = rx_samples(&mut pcm, rx_gain());
            up.process(&f, &mut at48);
            let mut q = sh.rx.lock();
            q.extend(at48.drain(..));
            let over = q.len().saturating_sub(RX_QUEUE_MAX);
            q.drain(..over);
        };
        linked.store(false, Ordering::SeqCst);
        *sh.to_radio.lock() = None;
        tracing::warn!("btaudio: {label} audio lost ({why}), reconnecting");
        set_status(snap, format!("{label} lost, reconnecting…"), false);
        std::thread::sleep(RETRY);
    }
}

const RX_FLAG: u8 = 0x7E;
const RX_ESCAPE: u8 = 0x7D;

/// Counts the radio's messages by frame type: the first byte after a flag.
/// (The deframer folds 0x00 and 0x03 into one kind of audio, and QA needs
/// to see which the radio sends.)
#[derive(Default)]
struct TypeCounter {
    after_flag: bool,
    escaped: bool,
}

impl TypeCounter {
    fn push(&mut self, bytes: &[u8], counts: &mut crate::status::BtRxFrames) {
        for &b in bytes {
            if b == RX_FLAG {
                self.after_flag = true;
                self.escaped = false;
            } else if self.after_flag {
                if b == RX_ESCAPE && !self.escaped {
                    self.escaped = true;
                    continue;
                }
                counts.count(if self.escaped { b ^ 0x20 } else { b });
                self.after_flag = false;
                self.escaped = false;
            }
        }
    }
}

/// PCM samples an audio message should decode to: its SBC frames, sized
/// from the first frame's header, with a cut-off last frame counted whole
/// (it is lost, so it counts as an error). `usize::MAX` when the message
/// does not start with an SBC header at all.
fn expected_samples(sbc: &[u8]) -> usize {
    if sbc.is_empty() {
        return 0;
    }
    if sbc.len() < 3 || sbc[0] != 0x9C {
        return usize::MAX;
    }
    let h = sbc[1];
    let blocks = 4 * (((h >> 4) & 3) as usize + 1);
    let mode = (h >> 2) & 3;
    let subbands = if h & 1 == 1 { 8 } else { 4 };
    let bitpool = sbc[2] as usize;
    let channels = if mode == 0 { 1 } else { 2 };
    let data_bits = match mode {
        0 | 1 => blocks * channels * bitpool,
        2 => blocks * bitpool,
        _ => subbands + blocks * bitpool,
    };
    let frame_len = 4 + (4 * subbands * channels) / 8 + data_bits.div_ceil(8);
    sbc.len().div_ceil(frame_len) * blocks * subbands * channels
}

/// Logs the receive counters every [`RX_LOG_EVERY`] while `linked`, from
/// its own thread so "no bytes at all" shows up too.
fn spawn_rx_log(label: String, snap: Arc<SharedStatus>, linked: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new()
        .name("bt-rx-log".into())
        .spawn(move || {
            let mut last = snap.lock().bt_rx_bytes;
            loop {
                std::thread::sleep(RX_LOG_EVERY);
                if !linked.load(Ordering::SeqCst) {
                    return;
                }
                let s = snap.lock();
                let f = s.bt_rx_frames;
                let rate = s.bt_rx_bytes.saturating_sub(last) / RX_LOG_EVERY.as_secs();
                last = s.bt_rx_bytes;
                tracing::info!(
                    "btaudio: {label} rx {} B ({rate} B/s), frames 0x00={} 0x01={} 0x02={} 0x03={} 0x09={} other={}, sbc errors {}",
                    s.bt_rx_bytes,
                    f.t00,
                    f.t01,
                    f.t02,
                    f.t03,
                    f.t09,
                    f.other,
                    s.sbc_decode_errors
                );
            }
        });
}

/// What to send the radio while modem73's PTT is held.
struct Transmit {
    down: Resampler,
    encode: sbc::Encode,
    since: Instant,
    /// Held back until [`TX_PREROLL`] has passed, then sent in one go.
    held: Vec<Vec<u8>>,
    timed_out: bool,
}

impl Transmit {
    fn new() -> Self {
        Self {
            down: Resampler::modem_to_radio(),
            encode: sbc::Encode::new(),
            since: Instant::now(),
            held: Vec::new(),
            timed_out: false,
        }
    }

    fn push(&mut self, samples: &[f32], sh: &Shared) {
        if self.timed_out {
            return;
        }
        if self.since.elapsed() > TX_LIMIT {
            tracing::warn!(
                "btaudio: PTT held over {}s, unkeying the radio",
                TX_LIMIT.as_secs()
            );
            self.end(sh);
            self.timed_out = true;
            return;
        }
        let mut at32 = Vec::new();
        if sh.tx_gain == 1.0 {
            self.down.process(samples, &mut at32);
        } else {
            let louder: Vec<f32> = samples.iter().map(|v| v * sh.tx_gain).collect();
            self.down.process(&louder, &mut at32);
        }
        let pcm: Vec<i16> = at32.iter().map(|&v| to_i16(v)).collect();
        let sbc = self.encode.push(&pcm);
        self.hold(&sbc);
        if self.since.elapsed() >= TX_PREROLL {
            self.flush(sh);
        }
    }

    /// Frame whole SBC frames, at most [`sbc::MAX_MESSAGE_BYTES`] a message.
    fn hold(&mut self, sbc: &[u8]) {
        let per = sbc::MAX_MESSAGE_BYTES / sbc::tx_frame_bytes() * sbc::tx_frame_bytes();
        for part in sbc.chunks(per) {
            self.held.push(frame::audio(part));
        }
    }

    fn flush(&mut self, sh: &Shared) {
        for m in self.held.drain(..) {
            sh.send_to_radio(m);
        }
    }

    fn end(&mut self, sh: &Shared) {
        let tail = self.encode.finish();
        self.hold(&tail);
        self.flush(sh);
        sh.send_to_radio(frame::END_OF_AUDIO.to_vec());
    }
}

fn to_i16(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0) as i16
}

/// One modem73 connection: every tick, give it received audio and take back
/// what it plays.
fn run_clock(mut conn: TcpStream, sh: &Shared) -> io::Result<()> {
    conn.set_nodelay(true)?;
    conn.set_read_timeout(Some(Duration::from_secs(2)))?;
    tracing::info!("btaudio: modem73 connected for audio");
    let mut tx: Option<Transmit> = None;
    let mut out = vec![0u8; TICK_SAMPLES * 2];
    let mut back = vec![0u8; TICK_SAMPLES * 2];
    let mut next = Instant::now();
    let result = loop {
        {
            let mut q = sh.rx.lock();
            for i in 0..TICK_SAMPLES {
                let v = to_i16(q.pop_front().unwrap_or(0.0));
                out[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
            }
        }
        if let Err(e) = conn
            .write_all(&out)
            .and_then(|_| conn.read_exact(&mut back))
        {
            break Err(e);
        }
        let keyed = sh.ptt.load(Ordering::SeqCst);
        match (&mut tx, keyed) {
            (None, true) => {
                let mut t = Transmit::new();
                t.push(&samples(&back), sh);
                tx = Some(t);
            }
            (Some(t), true) => t.push(&samples(&back), sh),
            (Some(t), false) => {
                if !t.timed_out {
                    t.end(sh);
                }
                tx = None;
            }
            (None, false) => {}
        }
        next += TICK;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(200) {
            next = now;
        }
    };
    if let Some(mut t) = tx {
        if !t.timed_out {
            t.end(sh);
        }
    }
    result
}

fn samples(le: &[u8]) -> Vec<f32> {
    le.chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect()
}

#[cfg(test)]
mod tests;
