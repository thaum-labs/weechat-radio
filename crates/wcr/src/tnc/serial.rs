//! SPDX-License-Identifier: Apache-2.0
//! Serial KISS transport: a Bluetooth COM port, `/dev/rfcomm0`, or a USB TNC.

use crate::tnc::link::Connection;
use std::fs::{File, OpenOptions};
use std::io;

/// Open a serial path in raw 8N1 mode with short read timeouts.
pub fn open(path: &str) -> io::Result<Connection> {
    let file = open_raw(path)?;
    let writer = file.try_clone()?;
    Ok(Connection {
        reader: Box::new(file),
        writer: Box::new(writer),
        zero_is_eof: false,
        label: path.to_string(),
    })
}

#[cfg(windows)]
fn open_raw(path: &str) -> io::Result<File> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Devices::Communication::{
        GetCommState, SetCommState, SetCommTimeouts, COMMTIMEOUTS, DCB,
    };
    use windows_sys::Win32::Foundation::HANDLE;

    let upper = path.trim().to_ascii_uppercase();
    let full = if upper.starts_with("COM") && !upper.starts_with("\\\\") {
        format!("\\\\.\\{upper}")
    } else {
        path.trim().to_string()
    };
    let file = OpenOptions::new().read(true).write(true).open(&full)?;
    let h = file.as_raw_handle() as HANDLE;
    unsafe {
        // Return whatever is buffered, or wait at most 200 ms for the first byte.
        let timeouts = COMMTIMEOUTS {
            ReadIntervalTimeout: u32::MAX,
            ReadTotalTimeoutMultiplier: u32::MAX,
            ReadTotalTimeoutConstant: 200,
            WriteTotalTimeoutMultiplier: 0,
            WriteTotalTimeoutConstant: 3000,
        };
        if SetCommTimeouts(h, &timeouts) == 0 {
            return Err(io::Error::last_os_error());
        }
        // Baud is ignored by Bluetooth SPP ports but matters for USB TNCs.
        let mut dcb: DCB = std::mem::zeroed();
        dcb.DCBlength = std::mem::size_of::<DCB>() as u32;
        if GetCommState(h, &mut dcb) != 0 {
            dcb.BaudRate = 9600;
            dcb.ByteSize = 8;
            dcb.Parity = 0;
            dcb.StopBits = 0;
            // fBinary | fDtrControl=ENABLE | fRtsControl=ENABLE; no flow control.
            dcb._bitfield = 0x0001 | 0x0010 | 0x1000;
            let _ = SetCommState(h, &dcb);
        }
    }
    Ok(file)
}

#[cfg(unix)]
fn open_raw(path: &str) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open(path.trim())?;
    let fd = file.as_raw_fd();
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut tio) != 0 {
            // Not a tty (e.g. a FIFO in tests): use as-is.
            return Ok(file);
        }
        libc::cfmakeraw(&mut tio);
        tio.c_cflag |= libc::CLOCAL | libc::CREAD;
        tio.c_cflag &= !(libc::CRTSCTS);
        // VMIN=0, VTIME=2: read returns after 200 ms with whatever arrived.
        tio.c_cc[libc::VMIN] = 0;
        tio.c_cc[libc::VTIME] = 2;
        let _ = libc::cfsetispeed(&mut tio, libc::B9600);
        let _ = libc::cfsetospeed(&mut tio, libc::B9600);
        if libc::tcsetattr(fd, libc::TCSANOW, &tio) != 0 {
            return Err(io::Error::last_os_error());
        }
        libc::tcflush(fd, libc::TCIOFLUSH);
    }
    Ok(file)
}

#[cfg(not(any(windows, unix)))]
fn open_raw(path: &str) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).open(path.trim())
}
