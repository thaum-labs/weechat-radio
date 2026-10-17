//! SPDX-License-Identifier: Apache-2.0
//! KISS TNC client (RFC 1055) over TCP — modem73 default port 8001.

use crate::error::{Error, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};

pub const FEND: u8 = 0xC0;
pub const FESC: u8 = 0xDB;
pub const TFEND: u8 = 0xDC;
pub const TFESC: u8 = 0xDD;

/// KISS TNC parameter commands (port 0).
pub const CMD_TXDELAY: u8 = 0x01;
pub const CMD_PERSIST: u8 = 0x02;
pub const CMD_SLOTTIME: u8 = 0x03;
pub const CMD_TXTAIL: u8 = 0x04;
pub const CMD_FULLDUPLEX: u8 = 0x05;

/// Encode a KISS data frame (port 0, command 0).
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    encode_with_command(0x00, payload)
}

/// Encode a one-byte KISS parameter command such as TXDELAY (in 10 ms units).
pub fn encode_param(cmd: u8, value: u8) -> Vec<u8> {
    encode_with_command(cmd & 0x0F, &[value])
}

fn encode_with_command(command: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 16);
    out.push(FEND);
    out.push(command);
    for &b in payload {
        match b {
            FEND => {
                out.push(FESC);
                out.push(TFEND);
            }
            FESC => {
                out.push(FESC);
                out.push(TFESC);
            }
            other => out.push(other),
        }
    }
    out.push(FEND);
    out
}

#[derive(Default)]
pub struct KissDecoder {
    buf: Vec<u8>,
    in_frame: bool,
    esc: bool,
}

impl KissDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push bytes, return completed payloads (KISS command byte stripped).
    pub fn push(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for &b in data {
            if !self.in_frame {
                if b == FEND {
                    self.in_frame = true;
                    self.buf.clear();
                    self.esc = false;
                }
                continue;
            }
            if self.esc {
                match b {
                    TFEND => self.buf.push(FEND),
                    TFESC => self.buf.push(FESC),
                    other => self.buf.push(other),
                }
                self.esc = false;
                continue;
            }
            match b {
                FESC => self.esc = true,
                FEND => {
                    if self.buf.len() > 1 {
                        // first byte is command
                        frames.push(self.buf[1..].to_vec());
                    }
                    self.buf.clear();
                    // consecutive FEND is idle
                }
                other => self.buf.push(other),
            }
        }
        frames
    }
}

pub struct KissClient {
    pub tx: mpsc::Sender<Vec<u8>>,
    /// Each frame as it is written to the modem.
    keyed: broadcast::Sender<Vec<u8>>,
}

impl Clone for KissClient {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            keyed: self.keyed.clone(),
        }
    }
}

/// A feed of the frames written to the modem, for [`KissClient::keyed`].
pub fn keyed_feed() -> broadcast::Sender<Vec<u8>> {
    broadcast::channel(256).0
}

impl KissClient {
    /// Wrap an already-running transport (Bluetooth / serial link thread),
    /// whose writer passes each frame it writes to `keyed`.
    pub fn from_sender(tx: mpsc::Sender<Vec<u8>>, keyed: broadcast::Sender<Vec<u8>>) -> Self {
        Self { tx, keyed }
    }

    /// Frames as they go to the modem. The air queue and the receipt path
    /// hand frames over long before that on a busy channel.
    pub fn keyed(&self) -> broadcast::Receiver<Vec<u8>> {
        self.keyed.subscribe()
    }

    pub async fn connect(addr: &str) -> Result<(Self, mpsc::Receiver<Vec<u8>>)> {
        let stream = TcpStream::connect(addr).await.map_err(|e| {
            Error::Modem(format!(
                "cannot connect to modem73 KISS at {addr}: {e}. Is modem73 running?"
            ))
        })?;
        stream.set_nodelay(true)?;
        let (read, write) = stream.into_split();
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>(64);
        let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>(64);
        let keyed = keyed_feed();
        tokio::spawn(writer(write, out_rx, keyed.clone()));
        tokio::spawn(reader(read, in_tx));
        Ok((Self { tx: out_tx, keyed }, in_rx))
    }

    pub async fn send(&self, payload: &[u8]) -> Result<()> {
        self.tx
            .send(payload.to_vec())
            .await
            .map_err(|_| Error::Modem("KISS send channel closed".into()))
    }
}

/// Log each beacon and receipt as it goes to the modem, so their pace can be
/// checked against the station log's timestamps. A receipt names the line it
/// is for (the line's msgid), the line's sender and whose receipt it is. Called
/// where frames are written: the air queue hands frames to the writer
/// directly, not through [`KissClient::send`].
pub fn note_keyed(payload: &[u8]) {
    use crate::proto::MsgType;
    let Ok(env) = crate::proto::Envelope::decode(payload) else {
        return;
    };
    match env.kind {
        MsgType::Beacon => tracing::info!("beacon to modem: {}", env.origin),
        MsgType::Ack => tracing::info!(
            "receipt to modem: line {} from {}, receipt by {}, {} hops left",
            env.acked_id().map(|id| id.hex()).unwrap_or_default(),
            env.dest,
            env.origin,
            env.hops_left
        ),
        _ => {}
    }
}

async fn writer(
    mut write: OwnedWriteHalf,
    mut rx: mpsc::Receiver<Vec<u8>>,
    keyed: broadcast::Sender<Vec<u8>>,
) {
    while let Some(payload) = rx.recv().await {
        note_keyed(&payload);
        let frame = encode_frame(&payload);
        if write.write_all(&frame).await.is_err() {
            break;
        }
        let _ = write.flush().await;
        let _ = keyed.send(payload);
    }
}

async fn reader(mut read: OwnedReadHalf, tx: mpsc::Sender<Vec<u8>>) {
    let mut decoder = KissDecoder::new();
    let mut buf = [0u8; 2048];
    loop {
        match read.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                for frame in decoder.push(&buf[..n]) {
                    if tx.send(frame).await.is_err() {
                        return;
                    }
                }
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiss_roundtrip() {
        let payload = vec![0xC0, 0xDB, 1, 2, 3];
        let encoded = encode_frame(&payload);
        assert_eq!(encoded[0], FEND);
        assert_eq!(*encoded.last().unwrap(), FEND);
        let mut dec = KissDecoder::new();
        let frames = dec.push(&encoded);
        assert_eq!(frames, vec![payload]);
    }

    #[test]
    fn param_command_encodes_low_nibble() {
        // TXDELAY 600 ms = 60 units.
        assert_eq!(encode_param(CMD_TXDELAY, 60), vec![FEND, 0x01, 60, FEND]);
        // Values that collide with FEND must still be escaped.
        assert_eq!(
            encode_param(CMD_PERSIST, FEND),
            vec![FEND, 0x02, FESC, TFEND, FEND]
        );
    }

    /// Captures what the station log would show.
    #[derive(Clone, Default)]
    struct LogBuf(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// v0.1.93 logged beacons in `send`, which the air queue never calls, so
    /// no queued beacon was logged. Frames handed to the writer directly, as
    /// the air queue does, and through `send` must both be logged.
    #[tokio::test]
    async fn beacons_and_receipts_are_logged_as_they_go_to_the_modem() {
        use crate::proto::{Callsign, Envelope, Flags, MsgType};
        let logs = LogBuf::default();
        let sink = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_writer(move || sink.clone())
            .finish();
        let _log = tracing::subscriber::set_default(subscriber);

        let modem = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = modem.local_addr().unwrap().to_string();
        let accept = tokio::spawn(async move { modem.accept().await.unwrap().0 });
        let (k, _rx) = KissClient::connect(&addr).await.unwrap();
        let mut modem = accept.await.unwrap();

        let a = Callsign::parse("G4AAA").unwrap();
        let mut beacon = Envelope::new_msg(
            Callsign::parse("M0BBB").unwrap(),
            Callsign::from_raw("BEACON"),
            1,
            b"B|radio|7074".to_vec(),
            1,
            Flags::new(),
        )
        .unwrap();
        beacon.kind = MsgType::Beacon;
        let line = Envelope::new_msg(
            a.clone(),
            Callsign::parse("M0DDD").unwrap(),
            7,
            b"hi".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        let receipt = Envelope::ack_for(&line, Callsign::parse("M0DDD").unwrap(), 2, None);
        k.tx.send(beacon.encode().unwrap()).await.unwrap();
        k.send(&receipt.encode().unwrap()).await.unwrap();

        let mut dec = KissDecoder::new();
        let mut got = Vec::new();
        let mut buf = [0u8; 1024];
        while got.len() < 2 {
            let n = modem.read(&mut buf).await.unwrap();
            assert!(n > 0, "the modem got {} frames", got.len());
            got.extend(dec.push(&buf[..n]));
        }
        let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("beacon to modem: M0BBB"), "{text}");
        let want = format!(
            "receipt to modem: line {} from G4AAA, receipt by M0DDD, ",
            line.msg_id.hex()
        );
        assert!(text.contains(&want), "{text}");
    }

    #[test]
    fn split_feed() {
        let encoded = encode_frame(b"hello");
        let mut dec = KissDecoder::new();
        let mid = encoded.len() / 2;
        let mut frames = dec.push(&encoded[..mid]);
        frames.extend(dec.push(&encoded[mid..]));
        assert_eq!(frames, vec![b"hello".to_vec()]);
    }
}
