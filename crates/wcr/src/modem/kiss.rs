//! SPDX-License-Identifier: Apache-2.0
//! KISS TNC client (RFC 1055) over TCP — modem73 default port 8001.

use crate::error::{Error, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

pub const FEND: u8 = 0xC0;
pub const FESC: u8 = 0xDB;
pub const TFEND: u8 = 0xDC;
pub const TFESC: u8 = 0xDD;

/// Encode a KISS data frame (port 0, command 0).
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 16);
    out.push(FEND);
    out.push(0x00); // port 0, data
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
}

impl Clone for KissClient {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

impl KissClient {
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
        tokio::spawn(writer(write, out_rx));
        tokio::spawn(reader(read, in_tx));
        Ok((Self { tx: out_tx }, in_rx))
    }

    pub async fn send(&self, payload: &[u8]) -> Result<()> {
        self.tx
            .send(payload.to_vec())
            .await
            .map_err(|_| Error::Modem("KISS send channel closed".into()))
    }
}

async fn writer(mut write: OwnedWriteHalf, mut rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(payload) = rx.recv().await {
        let frame = encode_frame(&payload);
        if write.write_all(&frame).await.is_err() {
            break;
        }
        let _ = write.flush().await;
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
    fn split_feed() {
        let encoded = encode_frame(b"hello");
        let mut dec = KissDecoder::new();
        let mid = encoded.len() / 2;
        let mut frames = dec.push(&encoded[..mid]);
        frames.extend(dec.push(&encoded[mid..]));
        assert_eq!(frames, vec![b"hello".to_vec()]);
    }
}
