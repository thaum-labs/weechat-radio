//! SPDX-License-Identifier: Apache-2.0
//! Serial, audio, and rigctld discovery for the setup wizard.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LabeledPort {
    pub name: String,
    pub label: String,
    /// USB vendor and product id, for USB serial ports.
    pub usb_id: Option<(u16, u16)>,
}

/// Silicon Labs CP2102N, the serial chip in the Digirig Mobile.
const DIGIRIG_USB_ID: (u16, u16) = (0x10c4, 0xea60);

pub fn serial_ports() -> Vec<LabeledPort> {
    #[cfg(feature = "setup-probe")]
    {
        serialport::available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let label = port_label(&p);
                let usb_id = match &p.port_type {
                    serialport::SerialPortType::UsbPort(u) => Some((u.vid, u.pid)),
                    _ => None,
                };
                LabeledPort {
                    name: p.port_name,
                    label,
                    usb_id,
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

/// The port that looks like a Digirig. On Linux it shows up as its serial
/// chip, "CP2102N USB to UART Bridge Controller · Silicon Labs" (QA #220),
/// so the chip name and USB id count too.
pub fn find_digirig(ports: &[LabeledPort]) -> Option<String> {
    let named = |p: &&LabeledPort| p.label.to_ascii_lowercase().contains("digirig");
    let chip = |p: &&LabeledPort| {
        p.usb_id == Some(DIGIRIG_USB_ID) || p.label.to_ascii_lowercase().contains("cp2102n")
    };
    ports
        .iter()
        .find(named)
        .or_else(|| ports.iter().find(chip))
        .map(|p| p.name.clone())
}

/// The sound card that looks like a Digirig's (a CM108 USB codec): ALSA
/// calls it `CARD=Device`, macOS and Windows "USB PnP Sound Device" or
/// "USB Audio Device". On Linux `plughw:` comes first, since it converts the
/// sample rate for modem73.
pub fn find_digirig_audio(devices: &[String]) -> Option<String> {
    const HINTS: &[&str] = &[
        "plughw:card=device,",
        "digirig",
        "usb pnp sound device",
        "usb audio device",
        "card=device,",
    ];
    HINTS.iter().find_map(|h| {
        devices
            .iter()
            .find(|d| d.to_ascii_lowercase().contains(h))
            .cloned()
    })
}

/// Stops ALSA printing its own errors to stderr. Listing devices probes
/// every alias (dmix, dsnoop, surround…), and each one that does not fit
/// prints `ALSA lib pcm_….c: …`: 17 lines in the wizard, and bursts of 14 in
/// the journal and the web page's log (QA #220). The errors still come back
/// to the caller as error codes.
pub fn quiet_alsa() {
    #[cfg(all(feature = "setup-probe", target_os = "linux"))]
    {
        use std::os::raw::{c_char, c_int};
        // ALSA calls the handler as a C variadic function. This one ignores
        // every argument, which is safe with the Linux C calling conventions.
        type Handler =
            unsafe extern "C" fn(*const c_char, c_int, *const c_char, c_int, *const c_char);
        #[link(name = "asound")]
        extern "C" {
            fn snd_lib_error_set_handler(handler: Option<Handler>) -> c_int;
        }
        unsafe extern "C" fn ignore(
            _file: *const c_char,
            _line: c_int,
            _function: *const c_char,
            _err: c_int,
            _fmt: *const c_char,
        ) {
        }
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            snd_lib_error_set_handler(Some(ignore));
        });
    }
}

pub fn list_audio_inputs() -> Vec<String> {
    list_audio_devices(true)
}

pub fn list_audio_outputs() -> Vec<String> {
    list_audio_devices(false)
}

fn list_audio_devices(inputs: bool) -> Vec<String> {
    #[cfg(feature = "setup-probe")]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        quiet_alsa();
        let host = cpal::default_host();
        let names = if inputs {
            host.input_devices()
                .ok()
                .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        } else {
            host.output_devices()
                .ok()
                .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        };
        names.unwrap_or_default()
    }
    #[cfg(not(feature = "setup-probe"))]
    {
        let _ = inputs;
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
            usb_id: None,
        }];
        assert_eq!(find_digirig(&ports).as_deref(), Some("COM5"));
    }

    #[test]
    fn digirig_matches_its_serial_chip_on_linux() {
        let ports = vec![
            LabeledPort {
                name: "/dev/ttyAMA0".into(),
                label: "/dev/ttyAMA0".into(),
                usb_id: None,
            },
            LabeledPort {
                name: "/dev/ttyUSB0".into(),
                label: "/dev/ttyUSB0 — CP2102N USB to UART Bridge Controller · Silicon Labs".into(),
                usb_id: Some((0x10c4, 0xea60)),
            },
        ];
        assert_eq!(find_digirig(&ports).as_deref(), Some("/dev/ttyUSB0"));
        let by_id = vec![LabeledPort {
            name: "/dev/ttyUSB1".into(),
            label: "/dev/ttyUSB1".into(),
            usb_id: Some((0x10c4, 0xea60)),
        }];
        assert_eq!(find_digirig(&by_id).as_deref(), Some("/dev/ttyUSB1"));
    }

    #[test]
    fn digirig_audio_prefers_plughw_on_the_usb_card() {
        // The Pi 3 list from QA #220, outputs.
        let devs: Vec<String> = [
            "hw:CARD=Headphones,DEV=0",
            "plughw:CARD=Headphones,DEV=0",
            "default:CARD=Headphones",
            "hw:CARD=Device,DEV=0",
            "plughw:CARD=Device,DEV=0",
            "default:CARD=Device",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            find_digirig_audio(&devs).as_deref(),
            Some("plughw:CARD=Device,DEV=0")
        );
        let mac = vec![
            "MacBook Pro Microphone".to_string(),
            "USB PnP Sound Device".into(),
        ];
        assert_eq!(
            find_digirig_audio(&mac).as_deref(),
            Some("USB PnP Sound Device")
        );
        assert_eq!(find_digirig_audio(&["default".to_string()]), None);
    }
}
