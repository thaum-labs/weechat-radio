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

pub use imp::{connect, connect_audio, discover, pair, paired_devices};

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
    /// The radio's "BS AOC" audio service: 39144315-32FA-40DB-85ED-FBFEBA2D86E6.
    const AUDIO_UUID: GUID = GUID {
        data1: 0x3914_4315,
        data2: 0x32FA,
        data3: 0x40DB,
        data4: [0x85, 0xED, 0xFB, 0xFE, 0xBA, 0x2D, 0x86, 0xE6],
    };
    const AUDIO_FALLBACK_CHANNEL: u32 = 2;

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
        connect_service(dev, SPP_UUID, RFCOMM_FALLBACK_CHANNEL)
    }

    /// Open the radio's audio channel (SBC audio, see `crate::btaudio`).
    pub fn connect_audio(dev: &BtDevice) -> io::Result<Connection> {
        connect_service(dev, AUDIO_UUID, AUDIO_FALLBACK_CHANNEL)
    }

    fn connect_service(dev: &BtDevice, service: GUID, fallback: u32) -> io::Result<Connection> {
        unsafe {
            let mut wsa: WSADATA = std::mem::zeroed();
            WSAStartup(0x0202, &mut wsa);
        }
        let by_service = SOCKADDR_BTH {
            addressFamily: AF_BTH,
            btAddr: dev.addr,
            serviceClassId: service,
            port: 0,
        };
        let stream = match try_connect(&by_service) {
            Ok(s) => s,
            Err(first) => {
                let by_channel = SOCKADDR_BTH {
                    addressFamily: AF_BTH,
                    btAddr: dev.addr,
                    serviceClassId: unsafe { std::mem::zeroed() },
                    port: fallback,
                };
                // A timeout means the radio is off or out of range; retrying on
                // the fixed channel would only double the wait.
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
    use std::io::{self, Read, Write};
    use std::os::unix::io::FromRawFd;
    use std::os::unix::net::UnixStream;
    use std::sync::Arc;

    const AF_BLUETOOTH: libc::c_int = 31;
    const BTPROTO_L2CAP: libc::c_int = 0;
    const BTPROTO_RFCOMM: libc::c_int = 3;
    const RFCOMM_CHANNEL: u8 = 1;
    /// The radio's "BS AOC" audio service: 39144315-32FA-40DB-85ED-FBFEBA2D86E6.
    const AUDIO_UUID: [u8; 16] = [
        0x39, 0x14, 0x43, 0x15, 0x32, 0xFA, 0x40, 0xDB, 0x85, 0xED, 0xFB, 0xFE, 0xBA, 0x2D, 0x86,
        0xE6,
    ];
    const AUDIO_FALLBACK_CHANNEL: u8 = 2;
    const SDP_PSM: u16 = 1;

    #[repr(C)]
    struct SockaddrRc {
        rc_family: libc::sa_family_t,
        rc_bdaddr: [u8; 6],
        rc_channel: u8,
    }

    #[repr(C)]
    struct SockaddrL2 {
        l2_family: libc::sa_family_t,
        l2_psm: u16,
        l2_bdaddr: [u8; 6],
        l2_cid: u16,
        l2_bdaddr_type: u8,
    }

    /// bdaddr_t is little-endian: byte 0 is the last printed octet.
    fn bdaddr(addr: u64) -> [u8; 6] {
        let be = addr.to_be_bytes();
        let mut bd = [0u8; 6];
        for i in 0..6 {
            bd[i] = be[7 - i];
        }
        bd
    }

    fn connect_fd(fd: libc::c_int, sa: *const libc::sockaddr, len: usize) -> io::Result<()> {
        if unsafe { libc::connect(fd, sa, len as libc::socklen_t) } != 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }
        Ok(())
    }

    /// Ask the radio's SDP server which RFCOMM channel carries `uuid`.
    fn sdp_channel(addr: u64, uuid: &[u8; 16]) -> io::Result<Option<u8>> {
        let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_SEQPACKET, BTPROTO_L2CAP) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let sa = SockaddrL2 {
            l2_family: AF_BLUETOOTH as libc::sa_family_t,
            l2_psm: SDP_PSM.to_le(),
            l2_bdaddr: bdaddr(addr),
            l2_cid: 0,
            l2_bdaddr_type: 0,
        };
        connect_fd(
            fd,
            &sa as *const SockaddrL2 as *const libc::sockaddr,
            std::mem::size_of::<SockaddrL2>(),
        )?;
        let sock = unsafe { UnixStream::from_raw_fd(fd) };
        sock.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        let mut attrs = Vec::new();
        let mut cont: Vec<u8> = vec![0];
        for tid in 1u16..=16 {
            let req = super::sdp::search_attr_request(tid, uuid, &cont);
            (&sock).write_all(&req)?;
            let mut buf = [0u8; 1024];
            let n = (&sock).read(&mut buf)?;
            let Some((chunk, next)) = super::sdp::parse_search_attr_response(&buf[..n]) else {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "bad SDP reply"));
            };
            attrs.extend_from_slice(chunk);
            if next == [0] {
                break;
            }
            cont = next.to_vec();
        }
        Ok(super::sdp::rfcomm_channel(&attrs))
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
        connect_channel(dev, RFCOMM_CHANNEL)
    }

    /// Open the radio's audio channel (SBC audio, see `crate::btaudio`): the
    /// channel its SDP record names, else channel 2.
    pub fn connect_audio(dev: &BtDevice) -> io::Result<Connection> {
        let channel = match sdp_channel(dev.addr, &AUDIO_UUID) {
            Ok(Some(ch)) => ch,
            Ok(None) => AUDIO_FALLBACK_CHANNEL,
            Err(e) => {
                tracing::debug!("SDP lookup on {}: {e}", dev.addr_str());
                AUDIO_FALLBACK_CHANNEL
            }
        };
        connect_channel(dev, channel)
    }

    fn connect_channel(dev: &BtDevice, channel: u8) -> io::Result<Connection> {
        let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let sa = SockaddrRc {
            rc_family: AF_BLUETOOTH as libc::sa_family_t,
            rc_bdaddr: bdaddr(dev.addr),
            rc_channel: channel,
        };
        connect_fd(
            fd,
            &sa as *const SockaddrRc as *const libc::sockaddr,
            std::mem::size_of::<SockaddrRc>(),
        )?;
        let stream = Arc::new(unsafe { UnixStream::from_raw_fd(fd) });
        Ok(Connection {
            reader: Box::new(Shared(stream.clone())),
            writer: Box::new(Shared(stream)),
            zero_is_eof: true,
            label: dev.short_name(),
        })
    }
}

/// macOS: IOBluetooth. Pair the radio in System Settings first. Every
/// IOBluetooth call for a channel runs on that channel's own thread, which
/// spins a run loop so the delegate gets the radio's data.
#[cfg(target_os = "macos")]
mod imp {
    use super::{BtDevice, Connection};
    use objc::declare::ClassDecl;
    use objc::runtime::{Class, Object, Sel, BOOL, NO};
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::{c_void, CStr};
    use std::io;
    use std::sync::mpsc;
    use std::sync::Once;
    use std::time::{Duration, Instant};

    #[link(name = "IOBluetooth", kind = "framework")]
    extern "C" {}

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, after_source: u8) -> i32;
    }

    const SPP_UUID: [u8; 16] = [
        0x00, 0x00, 0x11, 0x01, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34,
        0xFB,
    ];
    /// The radio's "BS AOC" audio service: 39144315-32FA-40DB-85ED-FBFEBA2D86E6.
    const AUDIO_UUID: [u8; 16] = [
        0x39, 0x14, 0x43, 0x15, 0x32, 0xFA, 0x40, 0xDB, 0x85, 0xED, 0xFB, 0xFE, 0xBA, 0x2D, 0x86,
        0xE6,
    ];
    const SPP_FALLBACK_CHANNEL: u8 = 1;
    const AUDIO_FALLBACK_CHANNEL: u8 = 2;
    const SDP_WAIT: Duration = Duration::from_secs(6);

    type Id = *mut Object;

    fn nsstring(s: &str) -> Id {
        let c = std::ffi::CString::new(s).unwrap_or_default();
        unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
    }

    fn to_string(ns: Id) -> String {
        if ns.is_null() {
            return String::new();
        }
        unsafe {
            let p: *const std::os::raw::c_char = msg_send![ns, UTF8String];
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }

    /// Runs `f` inside an autorelease pool.
    fn pooled<T>(f: impl FnOnce() -> T) -> T {
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let out = f();
            let _: () = msg_send![pool, drain];
            out
        }
    }

    fn spin(seconds: f64) {
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, 0);
        }
    }

    pub fn paired_devices() -> io::Result<Vec<BtDevice>> {
        pooled(|| unsafe {
            let list: Id = msg_send![class!(IOBluetoothDevice), pairedDevices];
            if list.is_null() {
                return Ok(Vec::new());
            }
            let n: usize = msg_send![list, count];
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let d: Id = msg_send![list, objectAtIndex: i];
                let name = to_string(msg_send![d, name]);
                let addr = to_string(msg_send![d, addressString]);
                let paired: BOOL = msg_send![d, isPaired];
                let connected: BOOL = msg_send![d, isConnected];
                if let Some(addr) = super::parse_addr(&addr) {
                    out.push(BtDevice {
                        name: name.trim().to_string(),
                        addr,
                        paired: paired != NO,
                        connected: connected != NO,
                    });
                }
            }
            Ok(out)
        })
    }

    pub fn discover(_seconds: u8) -> io::Result<Vec<BtDevice>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "pair the radio in System Settings → Bluetooth first",
        ))
    }

    pub fn pair(_addr: u64) -> io::Result<()> {
        discover(0).map(|_| ())
    }

    pub fn connect(dev: &BtDevice) -> io::Result<Connection> {
        open(dev, SPP_UUID, SPP_FALLBACK_CHANNEL)
    }

    /// Open the radio's audio channel (SBC audio, see `crate::btaudio`).
    pub fn connect_audio(dev: &BtDevice) -> io::Result<Connection> {
        open(dev, AUDIO_UUID, AUDIO_FALLBACK_CHANNEL)
    }

    /// Data from the radio, or `None` once the channel closed.
    type Inbox = mpsc::Sender<Option<Vec<u8>>>;

    fn delegate_class() -> &'static Class {
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| {
            let mut decl = ClassDecl::new("WcrRfcommDelegate", class!(NSObject))
                .expect("register WcrRfcommDelegate");
            decl.add_ivar::<*mut c_void>("inbox");
            extern "C" fn on_data(this: &Object, _: Sel, _ch: Id, data: *mut c_void, len: usize) {
                unsafe {
                    let inbox = *this.get_ivar::<*mut c_void>("inbox") as *const Inbox;
                    if !inbox.is_null() && !data.is_null() {
                        let bytes = std::slice::from_raw_parts(data as *const u8, len).to_vec();
                        let _ = (*inbox).send(Some(bytes));
                    }
                }
            }
            extern "C" fn on_closed(this: &Object, _: Sel, _ch: Id) {
                unsafe {
                    let inbox = *this.get_ivar::<*mut c_void>("inbox") as *const Inbox;
                    if !inbox.is_null() {
                        let _ = (*inbox).send(None);
                    }
                }
            }
            unsafe {
                decl.add_method(
                    sel!(rfcommChannelData:data:length:),
                    on_data as extern "C" fn(&Object, Sel, Id, *mut c_void, usize),
                );
                decl.add_method(
                    sel!(rfcommChannelClosed:),
                    on_closed as extern "C" fn(&Object, Sel, Id),
                );
            }
            decl.register();
        });
        Class::get("WcrRfcommDelegate").expect("WcrRfcommDelegate registered")
    }

    /// The RFCOMM channel the device's SDP record gives for `uuid`, asking the
    /// radio again if the cached records do not have it.
    unsafe fn sdp_channel(device: Id, uuid: &[u8; 16]) -> Option<u8> {
        let sdp_uuid: Id = msg_send![class!(IOBluetoothSDPUUID),
            uuidWithBytes: uuid.as_ptr() as *const c_void length: 16u32];
        let lookup = || -> Option<u8> {
            let record: Id = msg_send![device, getServiceRecordForUUID: sdp_uuid];
            if record.is_null() {
                return None;
            }
            let mut ch: u8 = 0;
            let rc: i32 = msg_send![record, getRFCOMMChannelID: &mut ch as *mut u8];
            (rc == 0 && ch > 0).then_some(ch)
        };
        if let Some(ch) = lookup() {
            return Some(ch);
        }
        let _: i32 = msg_send![device, performSDPQuery: std::ptr::null_mut::<Object>()];
        let until = Instant::now() + SDP_WAIT;
        while Instant::now() < until {
            spin(0.1);
            if let Some(ch) = lookup() {
                return Some(ch);
            }
        }
        None
    }

    fn open(dev: &BtDevice, uuid: [u8; 16], fallback: u8) -> io::Result<Connection> {
        let (ready_tx, ready_rx) = mpsc::channel::<io::Result<()>>();
        let (in_tx, in_rx) = mpsc::channel::<Option<Vec<u8>>>();
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
        let address = dev.addr_str().replace(':', "-");
        std::thread::Builder::new()
            .name("bt-rfcomm".into())
            .spawn(move || channel_thread(address, uuid, fallback, ready_tx, in_tx, out_rx))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Connection {
                reader: Box::new(Inbound {
                    rx: in_rx,
                    pending: Vec::new(),
                }),
                writer: Box::new(Outbound(out_tx)),
                zero_is_eof: true,
                label: dev.short_name(),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(io::Error::other("Bluetooth thread ended")),
        }
    }

    fn channel_thread(
        address: String,
        uuid: [u8; 16],
        fallback: u8,
        ready: mpsc::Sender<io::Result<()>>,
        inbox: Inbox,
        outbox: mpsc::Receiver<Vec<u8>>,
    ) {
        let inbox = Box::new(inbox);
        let opened = pooled(|| unsafe {
            let device: Id = msg_send![class!(IOBluetoothDevice),
                deviceWithAddressString: nsstring(&address)];
            if device.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "unknown Bluetooth address",
                ));
            }
            let ch = sdp_channel(device, &uuid).unwrap_or(fallback);
            let delegate: Id = msg_send![delegate_class(), new];
            (*delegate).set_ivar::<*mut c_void>("inbox", &*inbox as *const Inbox as *mut c_void);
            let mut channel: Id = std::ptr::null_mut();
            let rc: i32 = msg_send![device, openRFCOMMChannelSync: &mut channel as *mut Id
                withChannelID: ch delegate: delegate];
            if rc != 0 || channel.is_null() {
                let _: () = msg_send![delegate, release];
                return Err(io::Error::other(format!(
                    "could not open RFCOMM channel {ch} (IOReturn {rc:#x})"
                )));
            }
            let _: Id = msg_send![channel, retain];
            Ok((channel, delegate))
        });
        let (channel, delegate) = match opened {
            Ok(v) => {
                let _ = ready.send(Ok(()));
                v
            }
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        let open = |channel: Id| -> bool { unsafe { msg_send![channel, isOpen] } };
        loop {
            pooled(|| spin(0.01));
            if !open(channel) {
                break;
            }
            match outbox.try_recv() {
                Ok(bytes) => {
                    let ok = pooled(|| unsafe {
                        let mtu: u16 = msg_send![channel, getMTU];
                        bytes.chunks(mtu.max(16) as usize).all(|c| {
                            let rc: i32 = msg_send![channel,
                                writeSync: c.as_ptr() as *mut c_void length: c.len() as u16];
                            rc == 0
                        })
                    });
                    if !ok {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        unsafe {
            let _: i32 = msg_send![channel, closeChannel];
            let _: () = msg_send![channel, release];
            (*delegate).set_ivar::<*mut c_void>("inbox", std::ptr::null_mut());
            let _: () = msg_send![delegate, release];
        }
        let _ = inbox.send(None);
    }

    struct Inbound {
        rx: mpsc::Receiver<Option<Vec<u8>>>,
        pending: Vec<u8>,
    }

    impl io::Read for Inbound {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pending.is_empty() {
                match self.rx.recv() {
                    Ok(Some(bytes)) => self.pending = bytes,
                    Ok(None) | Err(_) => return Ok(0),
                }
            }
            let n = buf.len().min(self.pending.len());
            buf[..n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            Ok(n)
        }
    }

    struct Outbound(mpsc::Sender<Vec<u8>>);

    impl io::Write for Outbound {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.send(buf.to_vec()).map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "Bluetooth channel closed")
            })?;
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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

    pub fn connect_audio(_dev: &BtDevice) -> io::Result<Connection> {
        Err(unsupported())
    }
}

/// Just enough of an SDP client to find which RFCOMM channel a service uses.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod sdp {
    const SEARCH_ATTR_REQUEST: u8 = 0x06;
    const SEARCH_ATTR_RESPONSE: u8 = 0x07;
    /// Attribute 0x0004, ProtocolDescriptorList.
    const PROTOCOL_DESCRIPTOR_LIST: u16 = 0x0004;

    /// ServiceSearchAttributeRequest for one 128-bit UUID, asking only for
    /// its ProtocolDescriptorList. `cont` is the continuation state (`[0]` first).
    pub fn search_attr_request(tid: u16, uuid: &[u8; 16], cont: &[u8]) -> Vec<u8> {
        let mut p = vec![0x35, 17, 0x1C];
        p.extend_from_slice(uuid);
        p.extend_from_slice(&0xFFFFu16.to_be_bytes());
        p.extend_from_slice(&[0x35, 3, 0x09]);
        p.extend_from_slice(&PROTOCOL_DESCRIPTOR_LIST.to_be_bytes());
        p.extend_from_slice(cont);
        let mut out = vec![SEARCH_ATTR_REQUEST];
        out.extend_from_slice(&tid.to_be_bytes());
        out.extend_from_slice(&(p.len() as u16).to_be_bytes());
        out.extend(p);
        out
    }

    /// The attribute bytes in a response and its continuation state.
    pub fn parse_search_attr_response(pdu: &[u8]) -> Option<(&[u8], &[u8])> {
        if pdu.len() < 7 || pdu[0] != SEARCH_ATTR_RESPONSE {
            return None;
        }
        let count = u16::from_be_bytes([pdu[5], pdu[6]]) as usize;
        let attrs = pdu.get(7..7 + count)?;
        let cont = pdu.get(7 + count..)?;
        (!cont.is_empty()).then_some((attrs, cont))
    }

    /// In a ProtocolDescriptorList the RFCOMM entry is UUID16 0x0003
    /// followed by a uint8 channel.
    pub fn rfcomm_channel(attrs: &[u8]) -> Option<u8> {
        attrs
            .windows(5)
            .find(|w| w[..4] == [0x19, 0x00, 0x03, 0x08])
            .map(|w| w[4])
            .filter(|&ch| (1..=30).contains(&ch))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn request_layout() {
            let r = search_attr_request(7, &[0xAB; 16], &[0]);
            assert_eq!(&r[..5], &[0x06, 0, 7, 0, (r.len() - 5) as u8]);
            assert_eq!(&r[5..8], &[0x35, 17, 0x1C]);
            assert_eq!(*r.last().unwrap(), 0);
        }

        #[test]
        fn finds_the_channel_in_a_real_shaped_reply() {
            // One record: 0x0004 -> ((L2CAP), (RFCOMM, 2)).
            let attrs = [
                0x35, 0x11, 0x35, 0x0F, 0x09, 0x00, 0x04, 0x35, 0x0A, 0x35, 0x03, 0x19, 0x01, 0x00,
                0x35, 0x05, 0x19, 0x00, 0x03, 0x08, 0x02,
            ];
            let mut pdu = vec![0x07, 0, 1, 0, 0];
            pdu.extend_from_slice(&(attrs.len() as u16).to_be_bytes());
            pdu.extend_from_slice(&attrs);
            pdu.push(0);
            let (a, cont) = parse_search_attr_response(&pdu).unwrap();
            assert_eq!(cont, &[0]);
            assert_eq!(rfcomm_channel(a), Some(2));
            assert_eq!(rfcomm_channel(&attrs[..10]), None);
        }
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
