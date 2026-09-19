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

fn json_bool(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(|x| x.as_bool())
}

fn json_string(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

/// modem73 can emit a negative `rx_frame_count` (sync minus errors). That
/// must not fail the whole object — serde `u64` would drop `audio_connected`.
fn json_u64(v: &Value, key: &str) -> u64 {
    if let Some(n) = v.get(key).and_then(|x| x.as_i64()) {
        return n.max(0) as u64;
    }
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0)
}

fn json_i32(v: &Value, key: &str) -> i32 {
    v.get(key)
        .and_then(|x| x.as_i64())
        .map(|n| n.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
        .unwrap_or(0)
}

impl ModemStatus {
    pub fn from_json(v: Value) -> Self {
        Self {
            channel_state: json_string(&v, "channel_state"),
            ptt_on: json_bool(&v, "ptt_on").unwrap_or(false),
            rx_frame_count: json_u64(&v, "rx_frame_count"),
            tx_frame_count: json_u64(&v, "tx_frame_count"),
            last_snr: json_f32(&v, &["last_snr"]).unwrap_or(0.0),
            last_ber: json_f32(&v, &["last_ber"]).unwrap_or(0.0),
            occupancy_pct: json_i32(&v, "occupancy_pct"),
            // Missing field: keep meters alive (control events have no flag).
            audio_connected: json_bool(&v, "audio_connected").unwrap_or(true),
            audio_in_db: json_f32(
                &v,
                &[
                    "audio_in_db",
                    "input_level_db",
                    "audio_level_db",
                    "capture_level_db",
                ],
            ),
            audio_out_db: json_f32(
                &v,
                &[
                    "audio_out_db",
                    "output_level_db",
                    "playback_level_db",
                    "tx_level_db",
                ],
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
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

fn is_control_event(v: &Value) -> bool {
    v.get("event").and_then(|e| e.as_str()).is_some()
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

async fn write_request(
    write: &mut WriteHalf<TcpStream>,
    json: Value,
    reply: oneshot::Sender<Result<Value>>,
) -> Option<oneshot::Sender<Result<Value>>> {
    let payload = match serde_json::to_vec(&json) {
        Ok(p) => p,
        Err(e) => {
            let _ = reply.send(Err(Error::from(e)));
            return None;
        }
    };
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    if write.write_all(&frame).await.is_err() {
        let _ = reply.send(Err(Error::Modem("control write failed".into())));
        return None;
    }
    Some(reply)
}

async fn control_loop(
    mut read: ReadHalf<TcpStream>,
    mut write: WriteHalf<TcpStream>,
    mut cmds: mpsc::Receiver<ControlCmd>,
    events: mpsc::Sender<RxFrameEvent>,
) {
    let mut pending: Option<oneshot::Sender<Result<Value>>> = None;
    let mut queued: std::collections::VecDeque<(Value, oneshot::Sender<Result<Value>>)> =
        std::collections::VecDeque::new();
    let mut incoming = Vec::new();
    loop {
        tokio::select! {
            cmd = cmds.recv() => {
                let Some(cmd) = cmd else { break };
                let ControlCmd::Request { json, reply } = cmd;
                if pending.is_some() {
                    queued.push_back((json, reply));
                    continue;
                }
                match write_request(&mut write, json, reply).await {
                    Some(r) => pending = Some(r),
                    None => break,
                }
            }
            res = read_frame(&mut read, &mut incoming) => {
                match res {
                    Ok(None) => break,
                    Ok(Some(v)) => {
                        if is_control_event(&v) {
                            if v.get("event").and_then(|e| e.as_str()) == Some("rx_frame") {
                                if let Ok(ev) = serde_json::from_value::<RxFrameEvent>(v) {
                                    let _ = events.send(ev).await;
                                }
                            }
                            continue;
                        }
                        if let Some(reply) = pending.take() {
                            let _ = reply.send(Ok(v));
                        }
                        if let Some((json, reply)) = queued.pop_front() {
                            match write_request(&mut write, json, reply).await {
                                Some(r) => pending = Some(r),
                                None => break,
                            }
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
        assert!(st.audio_connected);
    }

    #[test]
    fn negative_rx_count_keeps_audio_and_ptt() {
        let st = ModemStatus::from_json(serde_json::json!({
            "channel_state": "tx",
            "ptt_on": true,
            "tx_queue": 0,
            "rx_frame_count": -1,
            "tx_frame_count": 0,
            "last_snr": 0,
            "last_ber": -1,
            "audio_connected": true,
            "occupancy_pct": 9,
            "ok": true
        }));
        assert!(st.audio_connected);
        assert!(st.ptt_on);
        assert_eq!(st.channel_state, "tx");
        assert_eq!(st.rx_frame_count, 0);
        assert_eq!(st.occupancy_pct, 9);
    }

    #[test]
    fn control_events_are_not_status_replies() {
        let ev = serde_json::json!({"event": "config_changed"});
        assert!(is_control_event(&ev));
        assert!(!is_control_event(&serde_json::json!({
            "ok": true,
            "audio_connected": true
        })));
        let frame = serde_json::from_value::<RxFrameEvent>(serde_json::json!({
            "event": "rx_frame",
            "snr": 8.5,
            "level_db": -14.0,
            "modem": "mfsk",
            "mode": "MFSK-32R"
        }))
        .unwrap();
        assert!((frame.snr - 8.5).abs() < 0.01);
        assert!((frame.level_db - -14.0).abs() < 0.01);
    }
}
