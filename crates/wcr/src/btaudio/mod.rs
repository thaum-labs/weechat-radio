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
use std::sync::atomic::{AtomicBool, Ordering};
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
}

impl Shared {
    fn send_to_radio(&self, bytes: Vec<u8>) {
        if let Some(tx) = self.to_radio.lock().as_ref() {
            let _ = tx.send(bytes);
        }
    }
}

static RUNNING: OnceLock<Ports> = OnceLock::new();
/// Last radio link state, put back after the station clears its status.
static LAST_STATUS: Mutex<(String, bool)> = Mutex::new((String::new(), false));

/// Starts the bridge for the radio in `cfg.tnc` once per process; later calls
/// return the same ports. The bridge reconnects to the radio by itself.
pub fn ensure_started(cfg: &crate::config::Config, snap: Arc<SharedStatus>) -> io::Result<Ports> {
    if let Some(p) = RUNNING.get() {
        let (text, ok) = LAST_STATUS.lock().clone();
        let mut s = snap.lock();
        s.tnc = text;
        s.tnc_ok = ok;
        return Ok(*p);
    }
    let tnc = cfg.tnc.clone();
    let snap_c = snap.clone();
    let connector: Connector = Box::new(move || connect_radio(&tnc, &snap_c));
    let ports = start(connector, snap)?;
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
    let audio = TcpListener::bind("127.0.0.1:0")?;
    let rigctl = TcpListener::bind("127.0.0.1:0")?;
    let ports = Ports {
        audio: audio.local_addr()?.port(),
        rigctl: rigctl.local_addr()?.port(),
    };
    let shared = Arc::new(Shared::default());

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
fn radio_manager(mut connector: Connector, sh: &Shared, snap: &SharedStatus) {
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
        let mut decode = sbc::Decode::default();
        let mut up = Resampler::radio_to_modem();
        let mut buf = [0u8; 2048];
        let mut pcm = Vec::new();
        let mut at48 = Vec::new();
        let why = loop {
            match reader.read(&mut buf) {
                Ok(0) if zero_is_eof => break "closed".to_string(),
                Ok(0) => continue,
                Ok(n) => deframer.push(&buf[..n], |m| {
                    if let frame::Message::Audio(bytes) = m {
                        decode.push(&bytes, &mut pcm);
                    }
                }),
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                Err(e) => break e.to_string(),
            }
            if pcm.is_empty() {
                continue;
            }
            let f: Vec<f32> = pcm.drain(..).map(|s| s as f32 / 32768.0).collect();
            up.process(&f, &mut at48);
            let mut q = sh.rx.lock();
            q.extend(at48.drain(..));
            let over = q.len().saturating_sub(RX_QUEUE_MAX);
            q.drain(..over);
        };
        *sh.to_radio.lock() = None;
        tracing::warn!("btaudio: {label} audio lost ({why}), reconnecting");
        set_status(snap, format!("{label} lost, reconnecting…"), false);
        std::thread::sleep(RETRY);
    }
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
        self.down.process(samples, &mut at32);
        let pcm: Vec<i16> = at32.iter().map(|&v| to_i16(v)).collect();
        let sbc = self.encode.push(&pcm);
        if !sbc.is_empty() {
            self.held.push(frame::audio(&sbc));
        }
        if self.since.elapsed() >= TX_PREROLL {
            self.flush(sh);
        }
    }

    fn flush(&mut self, sh: &Shared) {
        for m in self.held.drain(..) {
            sh.send_to_radio(m);
        }
    }

    fn end(&mut self, sh: &Shared) {
        let tail = self.encode.finish();
        if !tail.is_empty() {
            self.held.push(frame::audio(&tail));
        }
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
