//! SPDX-License-Identifier: Apache-2.0
//! modem73 JSON control port (4-byte big-endian length prefix, port 8073).

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::split;
use tokio::io::ReadHalf;
use tokio::io::WriteHalf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ModemStatus {
    pub channel_state: String,
    pub ptt_on: bool,
    pub rx_frame_count: u64,
    pub tx_frame_count: u64,
    pub last_snr: f32,
    pub last_ber: f32,
    pub occupancy_pct: i32,
    pub audio_connected: bool,
    /// Capture RMS in dBFS when the modem reports it.
    #[serde(default)]
    pub audio_in_db: Option<f32>,
    /// Playback RMS in dBFS when the modem reports it.
    #[serde(default)]
    pub audio_out_db: Option<f32>,
}

fn json_f32(v: &Value, keys: &[&str]) -> Option<f32> {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(|x| x.as_f64()) {
            return Some(n as f32);
        }
    }
    None
}

impl ModemStatus {
    pub fn from_json(v: Value) -> Self {
        let mut st: Self = serde_json::from_value(v.clone()).unwrap_or_default();
        if st.audio_in_db.is_none() {
            st.audio_in_db = json_f32(
                &v,
                &["input_level_db", "audio_level_db", "capture_level_db"],
            );
        }
        if st.audio_out_db.is_none() {
            st.audio_out_db =
                json_f32(&v, &["output_level_db", "playback_level_db", "tx_level_db"]);
        }
        st
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RxFrameEvent {
    pub seq: u64,
    pub time: f64,
    pub snr: f32,
    pub ber_pct: f32,
    pub level_db: f32,
    pub size: u32,
    pub modem: String,
    pub mode: String,
    pub callsign: Option<String>,
}

pub enum ControlCmd {
    Request {
        json: Value,
        reply: oneshot::Sender<Result<Value>>,
    },
}

#[derive(Clone)]
pub struct ControlClient {
    tx: mpsc::Sender<ControlCmd>,
}

impl ControlClient {
    pub async fn connect(addr: &str) -> Result<(Self, mpsc::Receiver<RxFrameEvent>)> {
        let stream = TcpStream::connect(addr).await.map_err(|e| {
            Error::Modem(format!(
                "cannot connect to modem73 control port at {addr}: {e}. Is modem73 running?"
            ))
        })?;
        stream.set_nodelay(true)?;
        let (read, write) = split(stream);
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (ev_tx, ev_rx) = mpsc::channel(64);
        tokio::spawn(control_loop(read, write, cmd_rx, ev_tx));
        Ok((Self { tx: cmd_tx }, ev_rx))
    }

    pub async fn request(&self, json: Value) -> Result<Value> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(ControlCmd::Request { json, reply })
            .await
            .map_err(|_| Error::Modem("control channel closed".into()))?;
        rx.await
            .map_err(|_| Error::Modem("control reply dropped".into()))?
    }

    pub async fn get_status(&self) -> Result<ModemStatus> {
        let v = self
            .request(serde_json::json!({"cmd": "get_status"}))
            .await?;
        Ok(ModemStatus::from_json(v))
    }

    pub async fn set_config(&self, fields: Value) -> Result<()> {
        let mut obj = serde_json::json!({"cmd": "set_config"});
        if let (Some(map), Some(extra)) = (obj.as_object_mut(), fields.as_object()) {
            for (k, v) in extra {
                if k != "cmd" {
                    map.insert(k.clone(), v.clone());
                }
            }
        }
        let v = self.request(obj).await?;
        if v.get("ok").and_then(|x| x.as_bool()) == Some(false) {
            return Err(Error::Modem("modem73 rejected set_config".into()));
        }
        Ok(())
    }

    pub async fn rigctl(&self, command: &str) -> Result<String> {
        let v = self
            .request(serde_json::json!({"cmd": "rigctl", "command": command}))
            .await?;
        Ok(v.get("response")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string())
    }

    pub async fn apply_ptt(&self, ptt: &str, cfg: &crate::config::ModemConfig) -> Result<()> {
        match ptt {
            "vox" => {
                self.set_config(serde_json::json!({
                    "ptt": "vox",
                    // control port may use different field names; also passed on CLI
                }))
                .await
                .ok();
                Ok(())
            }
            "digirig" | "com" => {
                self.set_config(serde_json::json!({
                    "ptt": "com",
                    "com_port": cfg.com_port,
                    "com_line": cfg.com_line
                }))
                .await
                .ok();
                Ok(())
            }
            "cm108" => {
                self.set_config(serde_json::json!({
                    "ptt": "cm108",
                    "cm108_gpio": cfg.cm108_gpio
                }))
                .await
                .ok();
                Ok(())
            }
            "rigctl" => {
                self.set_config(serde_json::json!({
                    "ptt": "rigctl",
                    "rigctl": cfg.rigctl
                }))
                .await
                .ok();
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

async fn control_loop(
    mut read: ReadHalf<TcpStream>,
    mut write: WriteHalf<TcpStream>,
    mut cmds: mpsc::Receiver<ControlCmd>,
    events: mpsc::Sender<RxFrameEvent>,
) {
    let mut pending: Option<oneshot::Sender<Result<Value>>> = None;
    let mut incoming = Vec::new();
    loop {
        tokio::select! {
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    ControlCmd::Request { json, reply } => {
                        let payload = match serde_json::to_vec(&json) {
                            Ok(p) => p,
                            Err(e) => {
                                let _ = reply.send(Err(Error::from(e)));
                                continue;
                            }
                        };
                        let mut frame = Vec::with_capacity(4 + payload.len());
                        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                        frame.extend_from_slice(&payload);
                        if write.write_all(&frame).await.is_err() {
                            let _ = reply.send(Err(Error::Modem("control write failed".into())));
                            break;
                        }
                        pending = Some(reply);
                    }
                }
            }
            res = read_frame(&mut read, &mut incoming) => {
                match res {
                    Ok(None) => break,
                    Ok(Some(v)) => {
                        if v.get("event").and_then(|e| e.as_str()) == Some("rx_frame") {
                            if let Ok(ev) = serde_json::from_value::<RxFrameEvent>(v.clone()) {
                                let _ = events.send(ev).await;
                            }
                        }
                        if let Some(reply) = pending.take() {
                            let _ = reply.send(Ok(v));
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
}

async fn read_frame(read: &mut ReadHalf<TcpStream>, stash: &mut Vec<u8>) -> Result<Option<Value>> {
    loop {
        if stash.len() >= 4 {
            let len = u32::from_be_bytes(stash[..4].try_into().unwrap()) as usize;
            if stash.len() >= 4 + len {
                let json = stash[4..4 + len].to_vec();
                stash.drain(..4 + len);
                let v: Value = serde_json::from_slice(&json)?;
                return Ok(Some(v));
            }
        }
        let mut buf = [0u8; 2048];
        let n = read.read(&mut buf).await?;
        if n == 0 {
            return Ok(None);
        }
        stash.extend_from_slice(&buf[..n]);
    }
}

pub fn encode_control_frame(json: &Value) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(json)?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_control_frames(buf: &mut Vec<u8>) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    loop {
        if buf.len() < 4 {
            break;
        }
        let len = u32::from_be_bytes(buf[..4].try_into().unwrap()) as usize;
        if buf.len() < 4 + len {
            break;
        }
        let json = buf[4..4 + len].to_vec();
        buf.drain(..4 + len);
        out.push(serde_json::from_slice(&json)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefix_roundtrip() {
        let v = serde_json::json!({"cmd": "get_status"});
        let frame = encode_control_frame(&v).unwrap();
        assert_eq!(
            &frame[..4],
            &(serde_json::to_vec(&v).unwrap().len() as u32).to_be_bytes()
        );
        let mut buf = frame;
        let decoded = decode_control_frames(&mut buf).unwrap();
        assert_eq!(decoded, vec![v]);
        assert!(buf.is_empty());
    }

    #[test]
    fn status_parses_in_out_level_aliases() {
        let st = ModemStatus::from_json(serde_json::json!({
            "channel_state": "idle",
            "audio_connected": true,
            "input_level_db": -18.5,
            "output_level_db": -6.0
        }));
        assert!(st.audio_connected);
        assert_eq!(st.audio_in_db, Some(-18.5));
        assert_eq!(st.audio_out_db, Some(-6.0));
        let st = ModemStatus::from_json(serde_json::json!({
            "audio_level_db": -22.0,
            "playback_level_db": -9.0
        }));
        assert_eq!(st.audio_in_db, Some(-22.0));
        assert_eq!(st.audio_out_db, Some(-9.0));
    }
}
