//! SPDX-License-Identifier: Apache-2.0
//! Bluetooth Classic (RFCOMM / SPP) transport to the radio's KISS TNC.
//!
//! Windows: enumerate paired devices, run an inquiry, pair (PIN 0000 /
//! numeric comparison accepted automatically), and open the SPP socket by
//! address — no COM port hunting. Linux: connect by address over an RFCOMM
//! socket (pair once with `bluetoothctl`). macOS: use `backend = "serial"`
//! with the `/dev/cu.VR-N76` port the OS creates on pairing.

use crate::config::TncConfig;
use crate::tnc::link::Connection;
use std::io;

/// A Bluetooth Classic device as seen by the OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtDevice {
    pub name: String,
    /// 48-bit address, `0x38D200010349` for `38:D2:00:01:03:49`.
    pub addr: u64,
    pub paired: bool,
    pub connected: bool,
}

impl BtDevice {
    pub fn addr_str(&self) -> String {
        format_addr(self.addr)
    }

    pub fn label(&self) -> String {
        crate::tnc::label(&self.name, &self.addr_str())
    }

    /// `VR-N76` or the address when the name is unknown.
    pub fn short_name(&self) -> String {
        if self.name.trim().is_empty() {
            self.addr_str()
        } else {
            self.name.trim().to_string()
        }
    }

    pub fn is_known_radio(&self) -> bool {
        crate::tnc::is_known_radio(&self.name)
    }
}

/// Result of the one-button "Find radio" flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindOutcome {
    /// A matching radio is paired and ready to use.
    Ready(BtDevice),
    /// Found in an inquiry and paired just now.
    Paired(BtDevice),
    /// Found in an inquiry but pairing failed; the OS dialog may be needed.
    PairFailed(BtDevice, String),
    /// Nothing matching was seen.
    NotFound { paired_others: Vec<BtDevice> },
    /// This platform cannot enumerate; the user must supply an address.
    Unsupported(String),
}

pub fn format_addr(addr: u64) -> String {
    let b = addr.to_be_bytes();
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        b[2], b[3], b[4], b[5], b[6], b[7]
    )
}

/// Parse `38:D2:00:01:03:49`, `38-D2-00-01-03-49`, or `38D200010349`.
pub fn parse_addr(s: &str) -> Option<u64> {
    let hex: String = s.trim().chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return None;
    }
    u64::from_str_radix(&hex, 16).ok()
}

/// Pick the radio from config: explicit address first, else a paired device
/// whose name matches `bt_name`, else any paired radio we know.
pub fn resolve(cfg: &TncConfig) -> Result<BtDevice, String> {
    if let Some(addr) = parse_addr(&cfg.bt_addr) {
        let name = paired_devices()
            .ok()
            .and_then(|v| v.into_iter().find(|d| d.addr == addr).map(|d| d.name))
            .unwrap_or_else(|| cfg.bt_name.trim().to_string());
        return Ok(BtDevice {
            name,
            addr,
            paired: true,
            connected: false,
        });
    }
    let devices = paired_devices().map_err(|e| {
        if e.kind() == io::ErrorKind::Unsupported {
            format!("{e}")
        } else {
            format!("Bluetooth not available: {e}")
        }
    })?;
    let wanted = cfg.bt_name.trim();
    if let Some(d) = devices
        .iter()
        .find(|d| crate::tnc::name_matches(&d.name, wanted))
        .or_else(|| devices.iter().find(|d| d.is_known_radio()))
    {
        return Ok(d.clone());
    }
    let shown = if wanted.is_empty() { "VR-N76" } else { wanted };
    Err(format!(
        "{shown} is not paired — put the radio in Pairing mode and press Find radio (or pair it in Bluetooth settings)"
    ))
}

/// One-button flow used by the GUI and the wizard: prefer an already paired
/// radio, otherwise run an inquiry and pair the first radio we recognise.
pub fn find_radio(wanted: &str) -> FindOutcome {
    let paired = match paired_devices() {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::Unsupported => {
            return FindOutcome::Unsupported(e.to_string())
        }
        Err(e) => return FindOutcome::Unsupported(format!("Bluetooth not available: {e}")),
    };
    if let Some(d) = paired
        .iter()
        .find(|d| d.paired && crate::tnc::name_matches(&d.name, wanted))
        .or_else(|| paired.iter().find(|d| d.paired && d.is_known_radio()))
    {
        return FindOutcome::Ready(d.clone());
    }
    let seen = discover(8).unwrap_or_default();
    let candidate = seen
        .iter()
        .find(|d| crate::tnc::name_matches(&d.name, wanted))
        .or_else(|| seen.iter().find(|d| d.is_known_radio()))
        .cloned();
    match candidate {
        Some(mut dev) => match pair(dev.addr) {
            Ok(()) => {
                dev.paired = true;
                FindOutcome::Paired(dev)
            }
            Err(e) => FindOutcome::PairFailed(dev, e.to_string()),
        },
        None => FindOutcome::NotFound {
            paired_others: paired.into_iter().filter(|d| d.paired).collect(),
        },
    }
}

pub use imp::{connect, discover, pair, paired_devices};

#[cfg(windows)]
mod imp {
    use super::{BtDevice, Connection};
    use crate::tnc::link::Shared;
    use std::io;
    use std::net::TcpStream;
    use std::os::windows::io::FromRawSocket;
    use std::sync::Arc;
    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::Bluetooth::{
        BluetoothAuthenticateDeviceEx, BluetoothFindDeviceClose, BluetoothFindFirstDevice,
        BluetoothFindNextDevice, BluetoothRegisterForAuthenticationEx,
        BluetoothSendAuthenticationResponseEx, BluetoothUnregisterAuthentication,
        MITMProtectionNotRequiredBonding, AF_BTH, BLUETOOTH_AUTHENTICATE_RESPONSE,
        BLUETOOTH_AUTHENTICATION_CALLBACK_PARAMS, BLUETOOTH_AUTHENTICATION_METHOD_LEGACY,
        BLUETOOTH_AUTHENTICATION_METHOD_NUMERIC_COMPARISON,
        BLUETOOTH_AUTHENTICATION_METHOD_PASSKEY,
        BLUETOOTH_AUTHENTICATION_METHOD_PASSKEY_NOTIFICATION, BLUETOOTH_DEVICE_INFO,
        BLUETOOTH_DEVICE_SEARCH_PARAMS, BTHPROTO_RFCOMM, SOCKADDR_BTH,
    };
    use windows_sys::Win32::Foundation::{BOOL, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows_sys::Win32::Networking::WinSock::{
        closesocket, connect as ws_connect, socket, WSAGetLastError, WSAStartup, INVALID_SOCKET,
        SOCKADDR, SOCK_STREAM, WSADATA,
    };

    /// Serial Port Profile: 00001101-0000-1000-8000-00805F9B34FB.
    const SPP_UUID: GUID = GUID {
        data1: 0x0000_1101,
        data2: 0x0000,
        data3: 0x1000,
        data4: [0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB],
    };
    const RFCOMM_FALLBACK_CHANNEL: u32 = 1;

    fn device_from_info(info: &BLUETOOTH_DEVICE_INFO) -> BtDevice {
        let end = info
            .szName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szName.len());
        BtDevice {
            name: String::from_utf16_lossy(&info.szName[..end])
                .trim()
                .to_string(),
            addr: unsafe { info.Address.Anonymous.ullLong } & 0xFFFF_FFFF_FFFF,
            paired: info.fAuthenticated != 0,
            connected: info.fConnected != 0,
        }
    }

    fn enumerate(inquiry: bool, timeout_units: u8) -> io::Result<Vec<BtDevice>> {
        let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: 1,
            fReturnRemembered: 1,
            fReturnUnknown: inquiry as BOOL,
            fReturnConnected: 1,
            fIssueInquiry: inquiry as BOOL,
            cTimeoutMultiplier: timeout_units.clamp(1, 48),
            hRadio: std::ptr::null_mut(),
        };
        let mut out = Vec::new();
        unsafe {
            let mut info: BLUETOOTH_DEVICE_INFO = std::mem::zeroed();
            info.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            let find = BluetoothFindFirstDevice(&params, &mut info);
            if find.is_null() {
                let err = io::Error::last_os_error();
                return match err.raw_os_error() {
                    Some(code) if code as u32 == ERROR_NO_MORE_ITEMS => Ok(out),
                    // No Bluetooth radio on this PC.
                    Some(1359) | Some(1168) | Some(1060) => Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "no Bluetooth adapter found on this computer",
                    )),
                    _ => Err(err),
                };
            }
            loop {
                out.push(device_from_info(&info));
                info = std::mem::zeroed();
                info.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
                if BluetoothFindNextDevice(find, &mut info) == 0 {
                    break;
                }
            }
            BluetoothFindDeviceClose(find);
        }
        Ok(out)
    }

    /// Devices Windows already knows (paired or remembered). Fast, no radio inquiry.
    pub fn paired_devices() -> io::Result<Vec<BtDevice>> {
        enumerate(false, 1)
    }

    /// Run a Bluetooth inquiry for roughly `seconds` (1.28 s units under the hood).
    pub fn discover(seconds: u8) -> io::Result<Vec<BtDevice>> {
        let units = ((seconds as f32 / 1.28).ceil() as u8).clamp(1, 48);
        enumerate(true, units)
    }

    unsafe extern "system" fn auth_callback(
        _param: *const core::ffi::c_void,
        params: *const BLUETOOTH_AUTHENTICATION_CALLBACK_PARAMS,
    ) -> BOOL {
        if params.is_null() {
            return 0;
        }
        let p = &*params;
        let mut resp: BLUETOOTH_AUTHENTICATE_RESPONSE = std::mem::zeroed();
        resp.bthAddressRemote = p.deviceInfo.Address;
        resp.authMethod = p.authenticationMethod;
        resp.negativeResponse = 0;
        match p.authenticationMethod {
            BLUETOOTH_AUTHENTICATION_METHOD_LEGACY => {
                // Benshi radios use the classic fixed PIN.
                let pin = b"0000";
                resp.Anonymous.pinInfo.pin[..pin.len()].copy_from_slice(pin);
                resp.Anonymous.pinInfo.pinLength = pin.len() as u8;
            }
            BLUETOOTH_AUTHENTICATION_METHOD_NUMERIC_COMPARISON => {
                resp.Anonymous.numericCompInfo.NumericValue = p.Anonymous.Numeric_Value;
            }
            BLUETOOTH_AUTHENTICATION_METHOD_PASSKEY
            | BLUETOOTH_AUTHENTICATION_METHOD_PASSKEY_NOTIFICATION => {
                resp.Anonymous.passkeyInfo.passkey = p.Anonymous.Passkey;
            }
            _ => {}
        }
        BluetoothSendAuthenticationResponseEx(std::ptr::null_mut(), &resp);
        1
    }

    /// Pair (bond) with the radio. Answers the PIN / confirmation prompt itself.
    pub fn pair(addr: u64) -> io::Result<()> {
        unsafe {
            let mut info: BLUETOOTH_DEVICE_INFO = std::mem::zeroed();
            info.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            info.Address.Anonymous.ullLong = addr;
            let mut reg: isize = 0;
            let r = BluetoothRegisterForAuthenticationEx(
                &info,
                &mut reg,
                Some(auth_callback),
                std::ptr::null(),
            );
            if r != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(r as i32));
            }
            let r = BluetoothAuthenticateDeviceEx(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut info,
                std::ptr::null(),
                MITMProtectionNotRequiredBonding,
            );
            BluetoothUnregisterAuthentication(reg);
            match r {
                ERROR_SUCCESS | ERROR_NO_MORE_ITEMS => Ok(()),
                other => Err(io::Error::from_raw_os_error(other as i32)),
            }
        }
    }

    fn wsa_error() -> io::Error {
        io::Error::from_raw_os_error(unsafe { WSAGetLastError() })
    }

    /// One connect attempt on a fresh socket (a failed Winsock connect leaves
    /// the socket unusable, so we never reuse one).
    fn try_connect(sa: &SOCKADDR_BTH) -> io::Result<TcpStream> {
        unsafe {
            let sock = socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM as i32);
            if sock == INVALID_SOCKET {
                return Err(wsa_error());
            }
            let rc = ws_connect(
                sock,
                sa as *const SOCKADDR_BTH as *const SOCKADDR,
                std::mem::size_of::<SOCKADDR_BTH>() as i32,
            );
            if rc != 0 {
                let err = wsa_error();
                closesocket(sock);
                return Err(err);
            }
            Ok(TcpStream::from_raw_socket(sock as _))
        }
    }

    /// Open the radio's SPP channel. Tries the SPP service UUID first (SDP lookup),
    /// then RFCOMM channel 1, which is where these radios put the TNC.
    pub fn connect(dev: &BtDevice) -> io::Result<Connection> {
        unsafe {
            let mut wsa: WSADATA = std::mem::zeroed();
            WSAStartup(0x0202, &mut wsa);
        }
        let by_service = SOCKADDR_BTH {
            addressFamily: AF_BTH,
            btAddr: dev.addr,
            serviceClassId: SPP_UUID,
            port: 0,
        };
        let stream = match try_connect(&by_service) {
            Ok(s) => s,
            Err(first) => {
                let by_channel = SOCKADDR_BTH {
                    addressFamily: AF_BTH,
                    btAddr: dev.addr,
                    serviceClassId: unsafe { std::mem::zeroed() },
                    port: RFCOMM_FALLBACK_CHANNEL,
                };
                // A timeout means the radio is off or out of range; retrying on
                // channel 1 would only double the wait.
                if first.raw_os_error() == Some(10060) {
                    return Err(first);
                }
                try_connect(&by_channel)?
            }
        };
        let stream = Arc::new(stream);
        Ok(Connection {
            reader: Box::new(Shared(stream.clone())),
            writer: Box::new(Shared(stream)),
            zero_is_eof: true,
            label: dev.short_name(),
        })
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{BtDevice, Connection};
    use crate::tnc::link::Shared;
    use std::io;
    use std::os::unix::io::FromRawFd;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;

    const AF_BLUETOOTH: libc::c_int = 31;
    const BTPROTO_RFCOMM: libc::c_int = 3;
    const RFCOMM_CHANNEL: u8 = 1;

    #[repr(C)]
    struct SockaddrRc {
        rc_family: libc::sa_family_t,
        rc_bdaddr: [u8; 6],
        rc_channel: u8,
    }

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "device listing needs bluetoothctl on Linux: run `bluetoothctl devices`, then set tnc.bt_addr",
        )
    }

    pub fn paired_devices() -> io::Result<Vec<BtDevice>> {
        Err(unsupported())
    }

    pub fn discover(_seconds: u8) -> io::Result<Vec<BtDevice>> {
        Err(unsupported())
    }

    pub fn pair(_addr: u64) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "pair once with `bluetoothctl pair <addr>` and `trust <addr>`",
        ))
    }

    /// Connect to RFCOMM channel 1 by address (pair + trust first with bluetoothctl).
    pub fn connect(dev: &BtDevice) -> io::Result<Connection> {
        unsafe {
            let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // bdaddr_t is little-endian: byte 0 is the last printed octet.
            let be = dev.addr.to_be_bytes();
            let mut bd = [0u8; 6];
            for i in 0..6 {
                bd[i] = be[7 - i];
            }
            let sa = SockaddrRc {
                rc_family: AF_BLUETOOTH as libc::sa_family_t,
                rc_bdaddr: bd,
                rc_channel: RFCOMM_CHANNEL,
            };
            if libc::connect(
                fd,
                &sa as *const SockaddrRc as *const libc::sockaddr,
                std::mem::size_of::<SockaddrRc>() as libc::socklen_t,
            ) != 0
            {
                let err = io::Error::last_os_error();
                libc::close(fd);
                return Err(err);
            }
            let stream = Arc::new(UnixStream::from_raw_fd(fd));
            Ok(Connection {
                reader: Box::new(Shared(stream.clone())),
                writer: Box::new(Shared(stream)),
                zero_is_eof: true,
                label: dev.short_name(),
            })
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::{BtDevice, Connection};
    use std::io;

    fn unsupported() -> io::Error {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Bluetooth sockets are not available here: pair the radio in System Settings, then set backend = \"serial\" and tnc.serial = \"/dev/cu.VR-N76\"",
        )
    }

    pub fn paired_devices() -> io::Result<Vec<BtDevice>> {
        Err(unsupported())
    }

    pub fn discover(_seconds: u8) -> io::Result<Vec<BtDevice>> {
        Err(unsupported())
    }

    pub fn pair(_addr: u64) -> io::Result<()> {
        Err(unsupported())
    }

    pub fn connect(_dev: &BtDevice) -> io::Result<Connection> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addr_roundtrip() {
        let a = parse_addr("38:D2:00:01:03:49").unwrap();
        assert_eq!(a, 0x38D2_0001_0349);
        assert_eq!(format_addr(a), "38:D2:00:01:03:49");
        assert_eq!(parse_addr("38-d2-00-01-03-49"), Some(a));
        assert_eq!(parse_addr("38D200010349"), Some(a));
        assert_eq!(parse_addr(""), None);
        assert_eq!(parse_addr("38:D2:00"), None);
    }

    #[test]
    fn resolve_prefers_explicit_addr() {
        let cfg = TncConfig {
            bt_addr: "38:D2:00:01:03:49".into(),
            bt_name: "VR-N76".into(),
            ..TncConfig::default()
        };
        let d = resolve(&cfg).unwrap();
        assert_eq!(d.addr, 0x38D2_0001_0349);
        assert!(d.paired);
    }

    #[test]
    fn device_labels() {
        let d = BtDevice {
            name: "VR-N76".into(),
            addr: 0x38D2_0001_0349,
            paired: true,
            connected: false,
        };
        assert_eq!(d.label(), "VR-N76 (38:D2:00:01:03:49)");
        assert!(d.is_known_radio());
        let anon = BtDevice {
            name: String::new(),
            ..d
        };
        assert_eq!(anon.short_name(), "38:D2:00:01:03:49");
    }
}
