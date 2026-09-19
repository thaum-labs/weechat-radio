//! SPDX-License-Identifier: Apache-2.0
//! Long-lived KISS link to a radio TNC with automatic reconnect.
//!
//! One manager thread owns the connection and reads frames; one writer thread
//! drains the node's outbound queue, wraps each envelope in AX.25, paces the
//! frames for the radio's one-frame-per-PTT TNC, and writes KISS.

use crate::air::{ChannelSense, ModemSense};
use crate::config::Config;
use crate::modem::kiss::{self, KissClient, KissDecoder};
use crate::status::SharedStatus;
use crate::tnc::{ax25, bluetooth, serial};
use parking_lot::Mutex;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const RETRY_BLUETOOTH: Duration = Duration::from_secs(5);
const RETRY_SERIAL: Duration = Duration::from_secs(3);
const RETRY_AFTER_DROP: Duration = Duration::from_secs(2);
/// How long the writer waits for the link to come back before dropping a frame.
const WRITE_WAIT: Duration = Duration::from_secs(8);

/// A connected transport, split for two threads.
pub struct Connection {
    pub reader: Box<dyn Read + Send>,
    pub writer: Box<dyn Write + Send>,
    /// `read()` returning 0 means "closed" (sockets) rather than "timeout" (serial).
    pub zero_is_eof: bool,
    /// Human label shown in the status bar, e.g. `VR-N76`.
    pub label: String,
}

/// Shared read/write handle for socket-style transports where `&T: Read + Write`.
pub struct Shared<T>(pub Arc<T>);

impl<T> Read for Shared<T>
where
    for<'a> &'a T: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buf)
    }
}

impl<T> Write for Shared<T>
where
    for<'a> &'a T: Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.0).flush()
    }
}

struct LinkState {
    writer: Mutex<Option<Box<dyn Write + Send>>>,
    up: AtomicBool,
}

/// Start the radio TNC link. Returns the KISS sender the node writes to and
/// the receiver it reads decoded payloads from. Reconnects forever.
pub fn start_link(
    cfg: &Config,
    snap: Arc<SharedStatus>,
    sense: Arc<ModemSense>,
) -> (KissClient, mpsc::Receiver<Vec<u8>>) {
    let cfg_c = cfg.clone();
    let snap_c = snap.clone();
    start_link_with(cfg, snap, sense, move || connect(&cfg_c, &snap_c))
}

/// Same as [`start_link`] with a caller-supplied connector (tests, other transports).
pub fn start_link_with<F>(
    cfg: &Config,
    snap: Arc<SharedStatus>,
    sense: Arc<ModemSense>,
    connector: F,
) -> (KissClient, mpsc::Receiver<Vec<u8>>)
where
    F: FnMut() -> Result<Connection, (String, Duration)> + Send + 'static,
{
    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>(64);
    let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>(64);
    let state = Arc::new(LinkState {
        writer: Mutex::new(None),
        up: AtomicBool::new(false),
    });

    let cfg_m = cfg.clone();
    let state_m = state.clone();
    let snap_m = snap.clone();
    std::thread::Builder::new()
        .name("tnc-link".into())
        .spawn(move || manager(cfg_m, state_m, snap_m, sense, in_tx, connector))
        .expect("spawn tnc link thread");

    let cfg_w = cfg.clone();
    std::thread::Builder::new()
        .name("tnc-write".into())
        .spawn(move || writer_loop(cfg_w, state, out_rx))
        .expect("spawn tnc writer thread");

    (KissClient::from_sender(out_tx), in_rx)
}

fn set_status(snap: &SharedStatus, text: impl Into<String>, ok: bool) {
    let mut s = snap.lock();
    s.tnc = text.into();
    s.tnc_ok = ok;
    s.ptt = "tnc".into();
    if !ok {
        s.channel = "idle".into();
    }
}

fn connect(cfg: &Config, snap: &SharedStatus) -> Result<Connection, (String, Duration)> {
    if cfg.modem.is_bluetooth() {
        let wanted = if cfg.tnc.bt_name.trim().is_empty() {
            "VR-N76".to_string()
        } else {
            cfg.tnc.bt_name.trim().to_string()
        };
        set_status(snap, format!("searching for {wanted}…"), false);
        let dev = bluetooth::resolve(&cfg.tnc).map_err(|e| (e, RETRY_BLUETOOTH))?;
        set_status(snap, format!("connecting to {}…", dev.label()), false);
        bluetooth::connect(&dev).map_err(|e| {
            (
                format!(
                    "{} not answering: {}. Radio on, Bluetooth on, KISS TNC enabled, HT app closed?",
                    dev.short_name(),
                    short_err(&e)
                ),
                RETRY_BLUETOOTH,
            )
        })
    } else {
        let path = cfg.tnc.serial.trim();
        if path.is_empty() {
            return Err((
                "set tnc.serial in wcr.toml (COM7, /dev/rfcomm0, /dev/cu.VR-N76)".into(),
                RETRY_SERIAL,
            ));
        }
        set_status(snap, format!("opening {path}…"), false);
        serial::open(path).map_err(|e| (format!("{path}: {}", short_err(&e)), RETRY_SERIAL))
    }
}

fn short_err(e: &io::Error) -> String {
    let s = e.to_string();
    match s.split(" (os error").next() {
        Some(head) => head.trim().to_string(),
        None => s,
    }
}

fn manager<F>(
    cfg: Config,
    state: Arc<LinkState>,
    snap: Arc<SharedStatus>,
    sense: Arc<ModemSense>,
    in_tx: mpsc::Sender<Vec<u8>>,
    mut connector: F,
) where
    F: FnMut() -> Result<Connection, (String, Duration)>,
{
    loop {
        if in_tx.is_closed() {
            return;
        }
        let conn = match connector() {
            Ok(c) => c,
            Err((why, wait)) => {
                tracing::warn!("tnc: {why}");
                set_status(&snap, why, false);
                std::thread::sleep(wait);
                continue;
            }
        };
        let Connection {
            mut reader,
            mut writer,
            zero_is_eof,
            label,
        } = conn;

        if let Err(e) = send_params(&mut *writer, &cfg) {
            tracing::warn!("tnc: could not set KISS parameters: {e}");
        }
        *state.writer.lock() = Some(writer);
        state.up.store(true, Ordering::SeqCst);
        set_status(&snap, format!("{label} linked"), true);
        tracing::info!("tnc: {label} linked");

        let mut decoder = KissDecoder::new();
        let mut buf = [0u8; 1024];
        let reason = loop {
            match reader.read(&mut buf) {
                Ok(0) if zero_is_eof => break "closed by radio".to_string(),
                Ok(0) => continue,
                Ok(n) => {
                    for frame in decoder.push(&buf[..n]) {
                        sense.note_rx();
                        {
                            let mut s = snap.lock();
                            s.channel = "rx".into();
                            s.audio_label = "good".into();
                            s.audio_db = -12.0;
                            s.audio_in_db = -12.0;
                        }
                        let payload = if cfg.tnc.ax25 {
                            ax25::payload_of(&frame).to_vec()
                        } else {
                            frame
                        };
                        if in_tx.blocking_send(payload).is_err() {
                            return;
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => continue,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => break short_err(&e),
            }
        };

        state.up.store(false, Ordering::SeqCst);
        *state.writer.lock() = None;
        tracing::warn!("tnc: {label} link lost ({reason}); reconnecting");
        set_status(&snap, format!("{label} link lost — reconnecting…"), false);
        std::thread::sleep(RETRY_AFTER_DROP);
    }
}

fn send_params(w: &mut dyn Write, cfg: &Config) -> io::Result<()> {
    let t = &cfg.tnc;
    let txdelay = (t.txdelay_ms / 10).clamp(1, 255) as u8;
    let slot = (t.slot_ms / 10).clamp(1, 255) as u8;
    let mut out = Vec::with_capacity(24);
    out.extend(kiss::encode_param(kiss::CMD_TXDELAY, txdelay));
    out.extend(kiss::encode_param(kiss::CMD_PERSIST, t.persist));
    out.extend(kiss::encode_param(kiss::CMD_SLOTTIME, slot));
    out.extend(kiss::encode_param(kiss::CMD_TXTAIL, 5));
    out.extend(kiss::encode_param(kiss::CMD_FULLDUPLEX, 0));
    w.write_all(&out)?;
    w.flush()
}

/// Airtime for one frame at 1200 bd plus TXDELAY and the configured gap:
/// the radio keys once per frame, so we must not hand it the next one early.
pub fn frame_hold(cfg: &Config, wire_len: usize) -> Duration {
    let bits = (wire_len as u64 + 8) * 8 + 16 * 8; // flags + FCS + a little slack
    let ms = bits * 1000 / 1200 + cfg.tnc.txdelay_ms as u64 + cfg.tnc.frame_gap_ms as u64;
    Duration::from_millis(ms)
}

fn writer_loop(cfg: Config, state: Arc<LinkState>, mut out_rx: mpsc::Receiver<Vec<u8>>) {
    let src = ax25::Address::from_station(&cfg.callsign);
    let dest = ax25::Address::from_station(&cfg.tnc.ax25_dest);
    while let Some(payload) = out_rx.blocking_recv() {
        let wire = if cfg.tnc.ax25 {
            ax25::wrap_ui(&src, &dest, &payload)
        } else {
            payload
        };
        let encoded = kiss::encode_frame(&wire);

        let deadline = Instant::now() + WRITE_WAIT;
        loop {
            {
                let mut slot = state.writer.lock();
                if let Some(w) = slot.as_mut() {
                    match w.write_all(&encoded).and_then(|_| w.flush()) {
                        Ok(()) => {}
                        Err(e) => {
                            tracing::warn!("tnc: write failed: {e}");
                            *slot = None;
                            state.up.store(false, Ordering::SeqCst);
                        }
                    }
                    break;
                }
            }
            if Instant::now() >= deadline {
                tracing::warn!("tnc: link down, dropping a frame ({} bytes)", wire.len());
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        std::thread::sleep(frame_hold(&cfg, wire.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_hold_covers_txdelay_and_airtime() {
        let cfg = Config::default();
        // 100 bytes ≈ 0.8 s on air + 600 ms TXDELAY + 400 ms gap.
        let h = frame_hold(&cfg, 100);
        assert!(h >= Duration::from_millis(1700), "{h:?}");
        assert!(h <= Duration::from_millis(2200), "{h:?}");
    }

    #[test]
    fn params_encode_in_10ms_units() {
        let cfg = Config::default();
        let mut out = Vec::new();
        send_params(&mut out, &cfg).unwrap();
        // TXDELAY 600 ms → 60
        assert_eq!(&out[..4], &[kiss::FEND, kiss::CMD_TXDELAY, 60, kiss::FEND]);
        assert!(out.ends_with(&[kiss::FEND, kiss::CMD_FULLDUPLEX, 0, kiss::FEND]));
    }

    /// A fake radio on localhost: checks that outbound payloads arrive as
    /// AX.25 UI frames inside KISS, that inbound KISS frames are unwrapped,
    /// and that the link reconnects after the radio drops it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn link_roundtrip_and_reconnect_over_fake_radio() {
        use std::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let radio = std::thread::spawn(move || {
            let mut heard = Vec::new();
            for session in 0..2 {
                let (mut s, _) = listener.accept().unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                // Radio speaks first: an APRS-style UI frame with a payload.
                let ui = ax25::wrap_ui(
                    &ax25::Address::from_station("G4ABC"),
                    &ax25::Address::from_station("WCR"),
                    format!("from-radio-{session}").as_bytes(),
                );
                s.write_all(&kiss::encode_frame(&ui)).unwrap();
                s.flush().unwrap();
                // Then collect KISS from the node until we see a data frame.
                let mut dec = KissDecoder::new();
                let mut buf = [0u8; 512];
                'outer: loop {
                    let n = s.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    for f in dec.push(&buf[..n]) {
                        heard.push(f.clone());
                        if f.len() > 1 && ax25::unwrap_ui(&f).is_some() {
                            break 'outer;
                        }
                    }
                }
                // Drop the link: the node must reconnect.
                drop(s);
            }
            heard
        });

        let mut cfg = Config::default();
        cfg.callsign = "M7TJF".into();
        cfg.tnc.frame_gap_ms = 0;
        cfg.tnc.txdelay_ms = 10;
        let snap = crate::status::new_shared();
        let sense = Arc::new(ModemSense::passive());
        let (kiss_client, mut in_rx) = start_link_with(&cfg, snap.clone(), sense, move || {
            let s =
                TcpStream::connect(addr).map_err(|e| (e.to_string(), Duration::from_millis(50)))?;
            let s = Arc::new(s);
            Ok(Connection {
                reader: Box::new(Shared(s.clone())),
                writer: Box::new(Shared(s)),
                zero_is_eof: true,
                label: "FAKE".into(),
            })
        });

        let first = tokio::time::timeout(Duration::from_secs(5), in_rx.recv())
            .await
            .expect("rx in time")
            .expect("rx open");
        assert_eq!(first, b"from-radio-0");
        assert!(snap.lock().tnc_ok);
        assert_eq!(snap.lock().tnc, "FAKE linked");

        kiss_client.send(b"hello from node").await.unwrap();

        let second = tokio::time::timeout(Duration::from_secs(8), in_rx.recv())
            .await
            .expect("reconnect in time")
            .expect("rx open");
        assert_eq!(second, b"from-radio-1");
        kiss_client.send(b"second session").await.unwrap();

        let heard = radio.join().unwrap();
        // KISS param commands are 1-byte payloads with the command in the
        // stripped command byte; data frames carry our AX.25 UI.
        let data: Vec<_> = heard.iter().filter_map(|f| ax25::unwrap_ui(f)).collect();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].src.to_string(), "M7TJF");
        assert_eq!(data[0].dest.to_string(), "WCR");
        assert_eq!(data[0].info, b"hello from node");
        assert_eq!(data[1].info, b"second session");
    }
}
