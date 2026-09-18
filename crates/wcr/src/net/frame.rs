//! SPDX-License-Identifier: Apache-2.0
//! Length-prefixed envelope framing for LAN and direct peer links.

use crate::error::Result;
use crate::proto::Envelope;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

pub async fn read_loop(mut stream: TcpStream, tx: mpsc::Sender<Envelope>) -> Result<()> {
    loop {
        let mut lenb = [0u8; 4];
        if stream.read_exact(&mut lenb).await.is_err() {
            break;
        }
        let len = u32::from_be_bytes(lenb) as usize;
        if len == 0 || len > 8192 {
            break;
        }
        let mut buf = vec![0u8; len];
        if stream.read_exact(&mut buf).await.is_err() {
            break;
        }
        if let Ok(env) = Envelope::decode(&buf) {
            if tx.send(env).await.is_err() {
                break;
            }
        }
    }
    Ok(())
}

pub async fn write_frame(stream: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    let len = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(bytes).await?;
    Ok(())
}
