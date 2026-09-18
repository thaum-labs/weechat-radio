//! SPDX-License-Identifier: Apache-2.0
//! Serial, audio, and rigctld discovery for the setup wizard.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LabeledPort {
    pub name: String,
    pub label: String,
}

pub fn serial_ports() -> Vec<LabeledPort> {
    #[cfg(feature = "setup-probe")]
    {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let label = port_label(&p);
                LabeledPort {
                    name: p.port_name,
                    label,
                }
            })
            .collect()
    }
    #[cfg(not(feature = "setup-probe"))]
    {
        Vec::new()
    }
}

#[cfg(feature = "setup-probe")]
fn port_label(p: &serialport::SerialPortInfo) -> String {
    let mut extra = String::new();
    if let serialport::SerialPortType::UsbPort(u) = &p.port_type {
        if let Some(prod) = &u.product {
            extra.push_str(prod);
        }
        if let Some(mfg) = &u.manufacturer {
            if !extra.is_empty() {
                extra.push_str(" · ");
            }
            extra.push_str(mfg);
        }
    }
    if extra.is_empty() {
        p.port_name.clone()
    } else {
        format!("{} — {}", p.port_name, extra)
    }
}

pub fn find_digirig(ports: &[LabeledPort]) -> Option<String> {
    for p in ports {
        let hay = p.label.to_ascii_lowercase();
        if hay.contains("digirig") {
            return Some(p.name.clone());
        }
    }
    None
}

pub fn list_audio_inputs() -> Vec<String> {
    #[cfg(feature = "setup-probe")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let Ok(devs) = host.input_devices() else {
            return Vec::new();
        };
        devs.filter_map(|d| d.name().ok()).collect()
    }
    #[cfg(not(feature = "setup-probe"))]
    {
        Vec::new()
    }
}

const RIGCTLD_CANDIDATES: &[&str] = &["127.0.0.1:4532", "127.0.0.1:4533", "localhost:4532"];

pub fn probe_rigctld_hosts() -> Vec<String> {
    RIGCTLD_CANDIDATES
        .iter()
        .filter(|addr| rigctld_responds(addr))
        .map(|s| (*s).to_string())
        .collect()
}

fn rigctld_responds(addr: &str) -> bool {
    let Ok(mut addrs) = addr.to_socket_addrs() else {
        return false;
    };
    let Some(sock) = addrs.next() else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&sock, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    if stream.write_all(b"f\n").is_err() {
        return false;
    }
    let mut buf = [0u8; 128];
    stream.read(&mut buf).map(|n| n > 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digirig_matches_label() {
        let ports = vec![LabeledPort {
            name: "COM5".into(),
            label: "COM5 — Digirig Mobile".into(),
        }];
        assert_eq!(find_digirig(&ports).as_deref(), Some("COM5"));
    }
}
