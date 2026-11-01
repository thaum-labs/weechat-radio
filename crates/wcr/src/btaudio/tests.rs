//! SPDX-License-Identifier: Apache-2.0
//! The bridge against a fake radio (a local socket speaking the radio's
//! framing) and a fake modem73 (a client on the audio and rigctl ports).

use super::*;
use std::net::TcpListener;

/// A fake radio: the bridge connects to it like an RFCOMM channel.
fn fake_radio() -> (Connector, mpsc::Receiver<TcpStream>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let _ = tx.send(c);
        }
    });
    let connector: Connector = Box::new(move || {
        let s = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        let r = s.try_clone().unwrap();
        Ok(Connection {
            reader: Box::new(r),
            writer: Box::new(s),
            zero_is_eof: true,
            label: "FAKE".into(),
        })
    });
    (connector, rx)
}

fn tone(hz: f32, rate: f32, n: usize, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / rate).sin())
        .collect()
}

/// Level of `hz` in `x` (Goertzel), as an amplitude.
fn level(x: &[f32], hz: f32, rate: f32) -> f32 {
    let w = 2.0 * std::f32::consts::PI * hz / rate;
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s = v + 2.0 * w.cos() * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    let p = s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2;
    2.0 * p.sqrt() / x.len() as f32
}

fn rigctl(port: u16, cmd: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(format!("{cmd}\n").as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    line
}

#[test]
fn rigctl_answers_ptt_commands() {
    let (connector, _radio) = fake_radio();
    let ports = start(connector, crate::status::new_shared()).unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", ports.rigctl)).unwrap();
    let mut r = BufReader::new(s.try_clone().unwrap());
    let mut ask = |cmd: &str| {
        s.write_all(format!("{cmd}\n").as_bytes()).unwrap();
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        line
    };
    assert_eq!(ask("T 1"), "RPRT 0\n");
    assert_eq!(ask("t"), "1\n");
    assert_eq!(ask("T 0"), "RPRT 0\n");
    assert_eq!(ask("t"), "0\n");
    assert_eq!(ask("f"), "RPRT -4\n");
}

#[test]
fn radio_audio_reaches_modem73_and_modem73_audio_keys_the_radio() {
    let snap = crate::status::new_shared();
    let (connector, radio_rx) = fake_radio();
    let ports = start(connector, snap.clone()).unwrap();
    let mut radio = radio_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    for _ in 0..50 {
        if snap.lock().tnc_ok {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(snap.lock().tnc, "FAKE linked");

    // The radio hears a 1 kHz tone and streams it to us as SBC, framed as
    // type 3 like the real radio (#235).
    let heard: Vec<i16> = tone(1000.0, 32_000.0, 32_000, 0.4)
        .iter()
        .map(|&v| to_i16(v))
        .collect();
    let mut enc = sbc::Encode::new();
    for chunk in heard.chunks(640) {
        radio.write_all(&frame::rx_audio(&enc.push(chunk))).unwrap();
    }
    radio.write_all(&frame::END_OF_AUDIO).unwrap();

    // modem73 connects and reads a second of audio.
    let mut modem = TcpStream::connect(("127.0.0.1", ports.audio)).unwrap();
    let silence = vec![0u8; TICK_SAMPLES * 2];
    let mut got = Vec::new();
    let mut block = vec![0u8; TICK_SAMPLES * 2];
    for _ in 0..50 {
        modem.read_exact(&mut block).unwrap();
        got.extend(samples(&block));
        modem.write_all(&silence).unwrap();
    }
    let at48 = &got[4800..43_200];
    assert!(
        level(at48, 1000.0, 48_000.0) > 0.3,
        "radio audio reached modem73"
    );
    assert!(level(at48, 1500.0, 48_000.0) < 0.02);

    // modem73 keys up and plays 1.5 kHz for half a second, then unkeys.
    let mut c = rigctl_conn(ports.rigctl);
    c.write_all(b"T 1\n").unwrap();
    let play = tone(1500.0, 48_000.0, 24_000, 0.4);
    for chunk in play.chunks(TICK_SAMPLES) {
        modem.read_exact(&mut block).unwrap();
        let le: Vec<u8> = chunk
            .iter()
            .flat_map(|&v| to_i16(v).to_le_bytes())
            .collect();
        modem.write_all(&le).unwrap();
    }
    c.write_all(b"T 0\n").unwrap();
    for _ in 0..10 {
        modem.read_exact(&mut block).unwrap();
        modem.write_all(&silence).unwrap();
    }

    // The radio got that audio, then the end-of-audio frame.
    radio
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut wire = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n) = radio.read(&mut buf) {
        if n == 0 {
            break;
        }
        wire.extend_from_slice(&buf[..n]);
    }
    let mut msgs = Vec::new();
    frame::Deframer::default().push(&wire, |m| msgs.push(m));
    assert_eq!(
        msgs.last(),
        Some(&frame::Message::End),
        "unkeyed at the end"
    );
    assert_eq!(&wire[..2], &[0x7E, 0x00], "sent as type 0 audio");
    let mut pcm = Vec::new();
    let mut dec = sbc::Decode::default();
    for m in &msgs {
        if let frame::Message::Audio(b) = m {
            assert!(
                b.len() <= sbc::MAX_MESSAGE_BYTES,
                "{} bytes in one message",
                b.len()
            );
            dec.push(b, &mut pcm);
        }
    }
    let sent: Vec<f32> = pcm.iter().map(|&s| s as f32 / 32768.0).collect();
    assert!(
        sent.len() >= 15_000,
        "about half a second at 32 kHz: {}",
        sent.len()
    );
    assert!(level(&sent[1000..15_000], 1500.0, 32_000.0) > 0.3);
}

/// A rigctl connection that stays open (modem73 keeps one).
fn rigctl_conn(port: u16) -> TcpStream {
    TcpStream::connect(("127.0.0.1", port)).unwrap()
}

#[test]
fn dropping_rigctl_unkeys() {
    let (connector, _radio) = fake_radio();
    let ports = start(connector, crate::status::new_shared()).unwrap();
    let mut c = rigctl_conn(ports.rigctl);
    c.write_all(b"T 1\n").unwrap();
    let mut line = String::new();
    BufReader::new(c.try_clone().unwrap())
        .read_line(&mut line)
        .unwrap();
    drop(c);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(rigctl(ports.rigctl, "t"), "0\n");
}

/// Two real modem73s (the network-audio build), each behind a bridge and a
/// fake radio; the fake radios pass audio between them only while keyed,
/// re-encoding it at the radio's own bitpool 18, so every frame crosses SBC
/// twice like it does on air. A KISS frame into one modem73 must come out of
/// the other. Run with `WCR_TEST_MODEM73=/path/to/modem73 cargo test -- --ignored`.
#[test]
#[ignore = "needs a modem73 with network audio (WCR_TEST_MODEM73)"]
fn two_modem73s_talk_through_two_bridges() {
    let Ok(binary) = std::env::var("WCR_TEST_MODEM73") else {
        return;
    };
    let args: Vec<String> = std::env::var("WCR_TEST_MODEM73_ARGS")
        .unwrap_or_default()
        .split_whitespace()
        .map(String::from)
        .collect();
    let (conn_a, radio_a) = fake_radio();
    let (conn_b, radio_b) = fake_radio();
    let ports_a = start(conn_a, crate::status::new_shared()).unwrap();
    let ports_b = start(conn_b, crate::status::new_shared()).unwrap();
    let radio_a = radio_a.recv_timeout(Duration::from_secs(5)).unwrap();
    let radio_b = radio_b.recv_timeout(Duration::from_secs(5)).unwrap();
    for (from, to) in [(&radio_a, &radio_b), (&radio_b, &radio_a)] {
        let mut from = from.try_clone().unwrap();
        let mut to = to.try_clone().unwrap();
        std::thread::spawn(move || air(&mut from, &mut to));
    }

    let dir = tempfile::tempdir().unwrap();
    let spawn = |ports: Ports, kiss: u16, call: &str| {
        let home = dir.path().join(call);
        std::fs::create_dir_all(&home).unwrap();
        std::process::Command::new(&binary)
            .args(["--headless", "-p", &kiss.to_string(), "--control-port", "0"])
            .args([
                "-d",
                &ports.audio_device(),
                "--rigctl",
                &ports.rigctl_addr(),
            ])
            .args(["--callsign", call])
            .args(&args)
            .env("HOME", &home)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap()
    };
    let kiss_a = free_port();
    let kiss_b = free_port();
    let mut a = spawn(ports_a, kiss_a, "TEST1A");
    let mut b = spawn(ports_b, kiss_b, "TEST1B");
    std::thread::sleep(Duration::from_secs(3));

    // Short enough for one MFSK-32R frame (55 bytes).
    let payload: Vec<u8> = b"two bridges, SBC twice "
        .iter()
        .copied()
        .chain(0..16u8)
        .collect();
    let mut ka = TcpStream::connect(("127.0.0.1", kiss_a)).unwrap();
    let mut kb = TcpStream::connect(("127.0.0.1", kiss_b)).unwrap();
    ka.write_all(&crate::modem::kiss::encode_frame(&payload))
        .unwrap();
    kb.set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut got = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(40);
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline && !contains(&got, &payload) {
        if let Ok(n) = kb.read(&mut buf) {
            got.extend_from_slice(&buf[..n]);
        }
    }
    for m in [&mut a, &mut b] {
        let _ = m.kill();
        let _ = m.wait();
    }
    assert!(contains(&got, &payload), "frame crossed the fake air");
}

/// The fake air: what one radio is sent to transmit, the other hears, as the
/// radio's own bitpool-18 SBC stream.
fn air(from: &mut TcpStream, to: &mut TcpStream) {
    let mut deframer = frame::Deframer::default();
    let mut dec = sbc::Decode::default();
    let mut buf = [0u8; 4096];
    let mut pcm = Vec::new();
    let mut enc = nanoaudio_sbc::SbcEncoder::new(nanoaudio_sbc::SbcParameters {
        sample_rate: 32_000,
        channel_mode: nanoaudio_sbc::SbcChannelMode::Mono,
        allocation: nanoaudio_sbc::SbcAllocation::Loudness,
        blocks: 16,
        subbands: 8,
        bitpool: 18,
    })
    .unwrap();
    while let Ok(n) = from.read(&mut buf) {
        if n == 0 {
            return;
        }
        let mut out = Vec::new();
        deframer.push(&buf[..n], |m| match m {
            frame::Message::Audio(b) => dec.push(&b, &mut pcm),
            frame::Message::End => out.extend_from_slice(&frame::END_OF_AUDIO),
            frame::Message::Other(_) => {}
        });
        let whole = pcm.len() / sbc::FRAME_SAMPLES * sbc::FRAME_SAMPLES;
        if whole > 0 {
            use nanoaudio_core::codec::Encoder;
            let mut sbc_bytes = vec![0u8; whole];
            let p = enc.encode(&pcm[..whole], &mut sbc_bytes).unwrap();
            pcm.drain(..whole);
            let mut framed = frame::rx_audio(&sbc_bytes[..p.produced]);
            framed.extend(out);
            out = framed;
        }
        if !out.is_empty() && to.write_all(&out).is_err() {
            return;
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn receive_counters_tell_no_bytes_from_bytes_that_do_not_decode() {
    let snap = crate::status::new_shared();
    let (connector, radio_rx) = fake_radio();
    let _ports = start(connector, snap.clone()).unwrap();
    let mut radio = radio_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(snap.lock().bt_rx_bytes, 0);

    let pcm = vec![0i16; 640];
    let mut enc = sbc::Encode::new();
    let mut sent = 0;
    for kind in [0x00u8, 0x03] {
        // Type 0 the way the radio sends it to macOS, type 3 as on Windows.
        let mut wire = frame::rx_audio(&enc.push(&pcm));
        wire[1] = kind;
        sent += wire.len();
        radio.write_all(&wire).unwrap();
    }
    // A message that is not SBC, the radio's echo, an ack and the end frame.
    let junk = frame::rx_audio(&[0x9C, 0x71, 0x12, 1, 2, 3]);
    let echo = [0x7E, 0x09, 0x9C, 0x00, 0x7E];
    let ack = [0x7E, 0x02, 0x00, 0x7E];
    for part in [&junk[..], &echo, &ack, &frame::END_OF_AUDIO] {
        sent += part.len();
        radio.write_all(part).unwrap();
    }
    radio.flush().unwrap();

    let until = Instant::now() + Duration::from_secs(5);
    while snap.lock().bt_rx_bytes < sent as u64 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    let s = snap.lock();
    assert_eq!(s.bt_rx_bytes, sent as u64);
    let f = s.bt_rx_frames;
    assert_eq!(
        (f.t00, f.t01, f.t02, f.t03, f.t09, f.other),
        (1, 1, 1, 2, 1, 0)
    );
    assert_eq!(s.sbc_decode_errors, 1, "only the junk message fails: {s:?}");
}

#[test]
fn sbc_frame_maths_match_the_radio() {
    // 32 kHz mono, 16 blocks, 8 subbands, loudness, bitpool 18: 44 bytes,
    // 128 samples a frame.
    let frame = [0x9C, 0x71, 0x12];
    let mut two = frame.to_vec();
    two.resize(88, 0);
    assert_eq!(expected_samples(&two), 256);
    // A cut-off frame is still expected, so its loss counts as an error.
    assert_eq!(expected_samples(&two[..60]), 256);
    assert_eq!(expected_samples(&[]), 0);
    assert_eq!(expected_samples(&[1, 2, 3]), usize::MAX);
}

#[test]
fn transmit_level_is_kept_in_range() {
    assert_eq!(tx_gain(0.0), 1.0);
    assert!((tx_gain(6.0) - 1.995).abs() < 0.01);
    assert_eq!(tx_gain(40.0), tx_gain(12.0));
    assert_eq!(tx_gain(-90.0), tx_gain(-20.0));
    assert_eq!(tx_gain(f32::NAN), 1.0);
}
