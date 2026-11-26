//! SPDX-License-Identifier: Apache-2.0
//! QA sim (#402): two VR-N76-style Bluetooth audio radios on one channel.
//!
//! Each radio listens on a TCP port that speaks the radio's audio-channel
//! framing (0x7E messages, SBC 32 kHz mono), so a station started with
//! `WCR_TEST_BT_RADIO=127.0.0.1:PORT` runs its real Bluetooth bridge and a
//! real modem73 against it. What one radio is sent to transmit, the other
//! hears; with nothing on the air each radio streams faint hiss, as a radio
//! at squelch 0 does.
//!
//! `--stale NAME` makes that radio behave like the Mac in QA #402: after a
//! station's link to it goes away, the next link opens but brings no audio
//! until the link is reset (a connection to PORT+1, which the station makes
//! for `bluetooth::reset_link`).
//!
//! ```text
//! cargo run --example sim_bt_radio -- A=47001 B=47011 [--stale A]
//! ```

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wcr::btaudio::{frame, sbc};

/// 20 ms of the radio's 32 kHz audio.
const TICK_SAMPLES: usize = 640;
const TICK: Duration = Duration::from_millis(20);
/// Hiss level with no carrier: about -26 dBFS RMS.
const HISS_AMP: f32 = 0.05;

struct Radio {
    name: String,
    port: u16,
    client: Mutex<Option<TcpStream>>,
    /// Audio this radio is transmitting, waiting to reach the other radio.
    tx: Mutex<VecDeque<i16>>,
    keyed: AtomicBool,
    links: AtomicU32,
    /// Opens bring no audio until a link reset (QA #402's Mac).
    stale: AtomicBool,
    stale_mode: bool,
    reset_seen: AtomicBool,
}

fn log(msg: &str) {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let secs = t as u64 % 86_400;
    println!(
        "{:02}:{:02}:{:02}.{:03} sim: {msg}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60,
        ((t.fract()) * 1000.0) as u32
    );
}

fn main() {
    let mut radios = Vec::new();
    let mut stale = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--stale" {
            stale.push(args.next().expect("--stale NAME"));
        } else if let Some((name, port)) = a.split_once('=') {
            radios.push((name.to_string(), port.parse::<u16>().expect("PORT")));
        }
    }
    assert!(
        radios.len() == 2,
        "usage: sim_bt_radio A=PORT B=PORT [--stale A]"
    );
    let radios: Vec<Arc<Radio>> = radios
        .into_iter()
        .map(|(name, port)| {
            Arc::new(Radio {
                stale_mode: stale.contains(&name),
                name,
                port,
                client: Mutex::new(None),
                tx: Mutex::new(VecDeque::new()),
                keyed: AtomicBool::new(false),
                links: AtomicU32::new(0),
                stale: AtomicBool::new(false),
                reset_seen: AtomicBool::new(false),
            })
        })
        .collect();
    for r in &radios {
        accept(r.clone());
        accept_reset(r.clone());
    }
    log(&format!(
        "radios {} on {} and {} on {} (stale: {:?})",
        radios[0].name, radios[0].port, radios[1].name, radios[1].port, stale
    ));
    clock(&radios[0], &radios[1]);
}

fn accept(r: Arc<Radio>) {
    let listener = TcpListener::bind(("127.0.0.1", r.port)).expect("bind radio port");
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let n = r.links.fetch_add(1, Ordering::SeqCst) + 1;
            let reset = r.reset_seen.swap(false, Ordering::SeqCst);
            let goes_stale = r.stale_mode && n > 1 && !reset;
            r.stale.store(goes_stale, Ordering::SeqCst);
            log(&format!(
                "{}: link {n} opened{}",
                r.name,
                if goes_stale {
                    " (STALE: no audio until the link is reset)"
                } else if reset {
                    " (after a link reset: audio flows)"
                } else {
                    ""
                }
            ));
            let reader = conn.try_clone().expect("clone");
            if let Some(old) = r.client.lock().unwrap().replace(conn) {
                let _ = old.shutdown(std::net::Shutdown::Both);
            }
            let r2 = r.clone();
            std::thread::spawn(move || read_station(r2, reader, n));
        }
    });
}

fn accept_reset(r: Arc<Radio>) {
    let listener = TcpListener::bind(("127.0.0.1", r.port + 1)).expect("bind reset port");
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            drop(conn);
            r.stale.store(false, Ordering::SeqCst);
            r.reset_seen.store(true, Ordering::SeqCst);
            log(&format!("{}: Bluetooth link reset by the station", r.name));
        }
    });
}

/// What the station sends the radio: SBC audio to transmit, and the end
/// frame that unkeys it.
fn read_station(r: Arc<Radio>, mut conn: TcpStream, link: u32) {
    let mut deframer = frame::Deframer::default();
    let mut dec = sbc::Decode::default();
    let mut buf = [0u8; 4096];
    let mut keyed_at: Option<Instant> = None;
    let mut samples = 0usize;
    loop {
        let n = match conn.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut pcm = Vec::new();
        let mut ended = false;
        deframer.push(&buf[..n], |m| match m {
            frame::Message::Audio(b) => dec.push(&b, &mut pcm),
            frame::Message::End => ended = true,
            frame::Message::Other(_) => {}
        });
        if !pcm.is_empty() {
            if keyed_at.is_none() {
                keyed_at = Some(Instant::now());
                samples = 0;
                r.keyed.store(true, Ordering::SeqCst);
                log(&format!("{}: KEYED (PTT on)", r.name));
            }
            samples += pcm.len();
            r.tx.lock().unwrap().extend(pcm);
        }
        if ended {
            if let Some(at) = keyed_at.take() {
                r.keyed.store(false, Ordering::SeqCst);
                log(&format!(
                    "{}: unkeyed after {} ms, {:.1} s of audio sent",
                    r.name,
                    at.elapsed().as_millis(),
                    samples as f32 / 32_000.0
                ));
            }
        }
    }
    r.keyed.store(false, Ordering::SeqCst);
    log(&format!("{}: link {link} closed by the station", r.name));
}

/// Every 20 ms each radio sends its station what it hears: the other
/// radio's transmission, else hiss. A stale link sends nothing.
fn clock(a: &Arc<Radio>, b: &Arc<Radio>) {
    let mut enc = [sbc::Encode::new(), sbc::Encode::new()];
    let mut noise = 0x1234_5678u32;
    let mut next = Instant::now();
    loop {
        next += TICK;
        for (i, (me, other)) in [(a, b), (b, a)].into_iter().enumerate() {
            let mut heard: Vec<i16> = {
                let mut q = other.tx.lock().unwrap();
                let n = q.len().min(TICK_SAMPLES);
                q.drain(..n).collect()
            };
            let carrier = !heard.is_empty() || other.keyed.load(Ordering::SeqCst);
            if !carrier {
                heard.clear();
            }
            while heard.len() < TICK_SAMPLES {
                let s = if carrier {
                    0
                } else {
                    noise ^= noise << 13;
                    noise ^= noise >> 17;
                    noise ^= noise << 5;
                    let u = (noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
                    (u * HISS_AMP * 32767.0) as i16
                };
                heard.push(s);
            }
            let bytes = enc[i].push(&heard);
            if me.stale.load(Ordering::SeqCst) || bytes.is_empty() {
                continue;
            }
            let msg = frame::rx_audio(&bytes);
            let mut slot = me.client.lock().unwrap();
            if let Some(c) = slot.as_mut() {
                if c.write_all(&msg).is_err() {
                    *slot = None;
                }
            }
        }
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    }
}
