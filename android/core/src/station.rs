//! SPDX-License-Identifier: Apache-2.0
//! The real station on the phone: the same wcr node the desktop runs, with
//! modem73 as the modem and the radio's Bluetooth audio channel as its sound
//! card.
//!
//! ```text
//! radio ──Bluetooth "BS AOC"── app (Kotlin, Android Bluetooth API)
//!                                 │ loopback TCP, bytes as they are
//!                              wcr btaudio bridge ── modem73 ── wcr node
//!                                                              │ IRC (loopback)
//!                                                   chat client ── Kotlin UI
//! ```
//!
//! Android has no raw Bluetooth sockets, so the app opens the radio's channel
//! and serves it on a loopback port; this module hands the bridge a connector
//! that dials that port. Everything after that is the desktop code unchanged.

use parking_lot::Mutex;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use wcr::config::Config;
use wcr::tnc::link::{Connection, Shared};
use wcr::web::chat::Chat;

/// The app's answer after it tried the radio: 1 = linked, anything else
/// (or a closed socket) = the radio did not answer.
const LINKED: u8 = 1;
/// How long the app may take to open the radio's channel.
const LINK_WAIT: Duration = Duration::from_secs(20);

/// What the app knows when it starts the station.
#[derive(uniffi::Record)]
pub struct StationSetup {
    /// The app's private folder; wcr keeps config/ and data/ under it.
    pub home: String,
    pub callsign: String,
    pub grid: String,
    /// "internet", "internet-radio", "radio" or "radio-plus".
    pub mode: String,
    /// The dial frequency in kHz, 0 if not set.
    pub frequency_khz: u32,
    /// A Bluetooth-safe preset such as "vhf-fm".
    pub preset: String,
    /// Full path of the bundled modem73 (libmodem73.so in the app's lib dir).
    pub modem73: String,
    /// The loopback port where the app serves the radio's channel.
    pub radio_port: u16,
    /// The radio's name, shown on the RADIO row, e.g. "VR-N76".
    pub radio_name: String,
}

struct Running {
    /// Kept for the life of the process: the node and chat run on it.
    _runtime: tokio::runtime::Runtime,
    chat: Arc<Chat>,
    /// The station's loopback status page, e.g. "http://127.0.0.1:8074/status".
    status_url: String,
}

static RUNNING: OnceLock<Running> = OnceLock::new();
/// Why the node stopped, if it did.
static STOPPED: Mutex<Option<String>> = Mutex::new(None);

/// Writes wcr.toml for this setup and starts the station once per process.
/// Returns an error text, or None when it is running. A second call while it
/// runs does nothing (change settings with [`radio_setting`]).
#[uniffi::export]
pub fn start_station(setup: StationSetup) -> Option<String> {
    if RUNNING.get().is_some() {
        return None;
    }
    match start(setup) {
        Ok(r) => {
            let _ = RUNNING.set(r);
            None
        }
        Err(e) => Some(e),
    }
}

fn start(setup: StationSetup) -> Result<Running, String> {
    std::env::set_var("WCR_HOME", &setup.home);
    wcr::config::ensure_dirs().map_err(|e| e.to_string())?;
    let path = Config::default_path();
    let mut cfg = Config::load(&path).unwrap_or_default();
    apply(&mut cfg, &setup)?;
    cfg.save(&path).map_err(|e| e.to_string())?;
    Config::set_active_path(&path);
    let cfg = Config::load(&path).map_err(|e| e.to_string())?;

    let port = setup.radio_port;
    let name = setup.radio_name.clone();
    wcr::btaudio::use_host_connector(Box::new(move || dial_app(port, &name)));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("wcr")
        .build()
        .map_err(|e| e.to_string())?;
    let node_cfg = cfg.clone();
    runtime.spawn(async move {
        let why = match wcr::node::run_node(node_cfg, false).await {
            Ok(()) => "the station stopped".to_string(),
            Err(e) => e.to_string(),
        };
        *STOPPED.lock() = Some(why);
    });
    let chat = {
        let _in_runtime = runtime.enter();
        Chat::start(
            loopback(&cfg.irc.bind),
            cfg.callsign.clone(),
            wcr::config::default_data_dir().join("phone-chat.json"),
        )
    };
    Ok(Running {
        _runtime: runtime,
        chat,
        status_url: format!("http://{}/status", loopback(&cfg.status.bind)),
    })
}

/// The settings the phone owns. Everything else keeps the file's value.
fn apply(cfg: &mut Config, s: &StationSetup) -> Result<(), String> {
    cfg.callsign = s.callsign.trim().to_ascii_uppercase();
    cfg.grid = s.grid.trim().to_string();
    cfg.mode = s
        .mode
        .parse()
        .map_err(|_| format!("unknown mode {}", s.mode))?;
    cfg.rf.frequency_khz = s.frequency_khz;
    cfg.modem.backend = "modem73".into();
    cfg.modem.manage = true;
    cfg.modem.binary = s.modem73.clone();
    cfg.modem.ptt = "bluetooth".into();
    cfg.modem.preset = s.preset.clone();
    cfg.tnc.bt_name = s.radio_name.clone();
    // A phone station has no desktop window or terminal to show.
    cfg.ui.activity_panel = false;
    Ok(())
}

/// The bridge's connector: dial the app's loopback port and wait for its
/// "linked" byte, so a radio that is off shows as not linked rather than as a
/// link that drops at once.
fn dial_app(port: u16, name: &str) -> Result<Connection, String> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| format!("the app's radio link is not running: {e}"))?;
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(LINK_WAIT)).ok();
    let mut answer = [0u8; 1];
    let linked = matches!((&stream).read(&mut answer), Ok(1)) && answer[0] == LINKED;
    if !linked {
        return Err(format!(
            "{name} audio not answering. Radio on, paired with this phone, HT app closed?"
        ));
    }
    stream.set_read_timeout(None).ok();
    let stream = Arc::new(stream);
    Ok(Connection {
        reader: Box::new(Shared(stream.clone())),
        writer: Box::new(Shared(stream)),
        zero_is_eof: true,
        label: name.to_string(),
    })
}

/// "0.0.0.0:6667" or ":6667" becomes "127.0.0.1:6667".
fn loopback(bind: &str) -> String {
    let port = bind.rsplit(':').next().unwrap_or("");
    format!("127.0.0.1:{port}")
}

/// Everything the chat screen draws, as JSON (the `wcr web` page's "hello").
#[uniffi::export]
pub fn chat_snapshot() -> String {
    match RUNNING.get() {
        Some(r) => r.chat.snapshot().to_string(),
        None => r#"{"type":"hello","me":"","up":false,"convs":[]}"#.into(),
    }
}

/// Send a line to "#channel" or a callsign. Returns an error text, or None.
#[uniffi::export]
pub fn chat_send(to: String, text: String) -> Option<String> {
    match RUNNING.get() {
        Some(r) => r.chat.send(&to, &text).err(),
        None => Some("the station is not running".into()),
    }
}

/// Change "preset", "freq" (MHz), "mode" or a level the way the desktop app
/// does. Returns an error text, or None.
#[uniffi::export]
pub fn radio_setting(what: String, value: String) -> Option<String> {
    match RUNNING.get() {
        Some(r) => r.chat.radio_setting(&what, &value).err(),
        None => Some("the station is not running".into()),
    }
}

/// The station's status as JSON (the desktop app's `/status`), or "" while it
/// is not answering yet.
#[uniffi::export]
pub fn station_status() -> String {
    let Some(r) = RUNNING.get() else {
        return String::new();
    };
    if let Some(why) = STOPPED.lock().clone() {
        return serde_json::json!({ "stopped": why }).to_string();
    }
    get(&r.status_url).unwrap_or_default()
}

/// A plain HTTP/1.0 GET to the station's loopback page; no client library.
fn get(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let (host, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}")))?;
    let mut s = TcpStream::connect(host).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(2))).ok();
    write!(s, "GET {path} HTTP/1.0\r\nHost: {host}\r\n\r\n").ok()?;
    let mut body = String::new();
    s.read_to_string(&mut body).ok()?;
    let (head, body) = body.split_once("\r\n\r\n")?;
    head.starts_with("HTTP/1.").then(|| body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    #[test]
    fn loopback_keeps_the_port() {
        assert_eq!(loopback("0.0.0.0:6667"), "127.0.0.1:6667");
        assert_eq!(loopback("127.0.0.1:8074"), "127.0.0.1:8074");
    }

    #[test]
    fn a_radio_that_does_not_answer_is_not_linked() {
        let app = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = app.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut s, _) = app.accept().unwrap();
            s.write_all(&[0]).unwrap();
        });
        let err = dial_app(port, "VR-N76").err().unwrap();
        assert!(err.contains("VR-N76 audio not answering"), "{err}");
    }

    /// The whole phone path with a fake radio behind the app's port: a line
    /// typed into chat goes through the node, modem73 and the bridge and
    /// reaches the "radio" as framed audio. Needs a modem73 build:
    /// `WCR_TEST_MODEM73=/path/to/modem73 cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_chat_line_reaches_the_radio_as_audio() {
        let Some(modem73) = std::env::var_os("WCR_TEST_MODEM73") else {
            return;
        };
        let home = std::env::temp_dir().join(format!("wcr-phone-{}", std::process::id()));
        let app = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = app.local_addr().unwrap().port();
        let frames = Arc::new(AtomicUsize::new(0));
        let seen = frames.clone();
        std::thread::spawn(move || {
            let (mut s, _) = app.accept().unwrap();
            s.write_all(&[LINKED]).unwrap();
            let mut buf = [0u8; 4096];
            // The radio sends nothing (quiet channel); count 0x7e frame marks.
            while let Ok(n) = s.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let marks = buf[..n].iter().filter(|&&b| b == 0x7e).count();
                seen.fetch_add(marks / 2, Ordering::Relaxed);
            }
        });
        let err = start_station(StationSetup {
            home: home.to_string_lossy().into(),
            callsign: "M7TJF".into(),
            grid: "IO81UF".into(),
            mode: "radio".into(),
            frequency_khz: 433_625,
            preset: "vhf-fm".into(),
            modem73: modem73.to_string_lossy().into(),
            radio_port: port,
            radio_name: "VR-N76".into(),
        });
        assert_eq!(err, None);
        let up = wait(Duration::from_secs(60), || {
            chat_snapshot().contains(r#""up":true"#)
                && station_status().contains(r#""tnc_ok":true"#)
        });
        assert!(
            up,
            "status: {}\nchat: {}",
            station_status(),
            chat_snapshot()
        );
        let before = frames.load(Ordering::Relaxed);
        assert_eq!(
            chat_send("#bulletin".into(), "hello from the phone".into()),
            None
        );
        let sent = wait(Duration::from_secs(90), || {
            frames.load(Ordering::Relaxed) > before + 20
        });
        assert!(
            sent,
            "no audio reached the radio; status: {}",
            station_status()
        );
        let _ = std::fs::remove_dir_all(home);
    }

    fn wait(limit: Duration, mut ok: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < limit {
            if ok() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        false
    }
}
