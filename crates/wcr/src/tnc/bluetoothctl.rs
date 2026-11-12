//! SPDX-License-Identifier: Apache-2.0
//! Linux "Find radio": list, scan for and pair Bluetooth radios by driving
//! BlueZ's `bluetoothctl`, which Raspberry Pi OS and desktop distributions
//! ship. The data link itself stays a plain RFCOMM socket (`bluetooth.rs`).
//!
//! Pairing runs `bluetoothctl` on a pseudo-terminal so its agent prompts
//! ("Enter PIN code:", "Confirm passkey … (yes/no):") arrive unbuffered; we
//! answer PIN `0000` (the radios' fixed PIN) and `yes`, then `trust` the
//! radio so BlueZ lets it reconnect without asking.

use super::bluetooth::{format_addr, parse_addr, BtDevice};
use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PAIR_TIMEOUT: Duration = Duration::from_secs(40);

/// The error for a machine without bluetoothctl.
fn missing(e: io::Error) -> io::Error {
    if e.kind() == io::ErrorKind::NotFound {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "bluetoothctl is missing: install it with `sudo apt install bluez`",
        )
    } else {
        e
    }
}

/// Run `bluetoothctl <args>` and return what it printed. It waits forever
/// when BlueZ has no adapter, so give up after `limit`.
fn run_for(args: &[&str], limit: Duration) -> io::Result<String> {
    let mut child = Command::new("bluetoothctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(missing)?;
    let drain = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut s);
            }
            s
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|r| Box::new(r) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|r| Box::new(r) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + limit;
    let finished = loop {
        if child.try_wait()?.is_some() {
            break true;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break false;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut text = String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned();
    text.push_str(&String::from_utf8_lossy(&err.join().unwrap_or_default()));
    if let Some(why) = trouble(&text) {
        return Err(io::Error::other(why));
    }
    if !finished {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "bluetoothctl did not answer: is Bluetooth switched on (`sudo rfkill unblock bluetooth`)?",
        ));
    }
    Ok(text)
}

fn run(args: &[&str]) -> io::Result<String> {
    run_for(args, Duration::from_secs(10))
}

/// A plain-language reason when bluetoothctl's output shows BlueZ can't be used.
fn trouble(text: &str) -> Option<String> {
    let t = text.to_ascii_lowercase();
    if t.contains("not authorized") || t.contains("notauthorized") || t.contains("access denied") {
        Some(
            "BlueZ refused access: add this user to the bluetooth group \
             (`sudo usermod -aG bluetooth $USER`), then log in again"
                .into(),
        )
    } else if t.contains("no default controller") || t.contains("unable to open mgmt_socket") {
        Some(
            "no Bluetooth adapter found (on a Pi: `sudo rfkill unblock bluetooth`, \
             and check that dtoverlay=disable-bt is not in config.txt)"
                .into(),
        )
    } else if t.contains("waiting to connect to bluetoothd") {
        Some("the Bluetooth service is not running: `sudo systemctl enable --now bluetooth`".into())
    } else {
        None
    }
}

/// Remove ANSI colour codes and carriage returns from bluetoothctl output.
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => {
                // ESC [ ... final byte in @..~
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for d in chars.by_ref() {
                        if ('@'..='~').contains(&d) {
                            break;
                        }
                    }
                }
            }
            '\r' | '\x01' | '\x02' => {}
            _ => out.push(c),
        }
    }
    out
}

/// `Device 38:D2:00:01:03:49 VR-N76` lines, in order, without duplicates.
/// Also matches event lines such as `[NEW] Device … name`.
fn parse_devices(text: &str) -> Vec<(u64, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in clean(text).lines() {
        let Some(at) = line.find("Device ") else {
            continue;
        };
        let rest = &line[at + "Device ".len()..];
        let (addr_s, tail) = rest.split_at(rest.find(' ').unwrap_or(rest.len()));
        if addr_s.len() != 17 {
            continue;
        }
        let Some(addr) = parse_addr(addr_s) else {
            continue;
        };
        // `[CHG] Device … RSSI: -60` is not a name.
        let name = tail.trim();
        let name = if name.contains(": ") { "" } else { name };
        if seen.insert(addr) {
            out.push((addr, name.to_string()));
        } else if !name.is_empty() {
            if let Some(e) = out.iter_mut().find(|(a, n)| *a == addr && n.is_empty()) {
                e.1 = name.to_string();
            }
        }
    }
    out
}

/// A name that is only the address with dashes (BlueZ's stand-in) is no name.
fn real_name(addr: u64, name: &str) -> String {
    if name
        .replace('-', ":")
        .eq_ignore_ascii_case(&format_addr(addr))
    {
        String::new()
    } else {
        name.to_string()
    }
}

/// Addresses in one `bluetoothctl devices <filter>` list (BlueZ 5.65+), else
/// the older `paired-devices` command when asked for paired ones.
fn filtered(filter: &str) -> io::Result<HashSet<u64>> {
    let text = run(&["devices", filter])?;
    let lower = text.to_ascii_lowercase();
    let text = if filter == "Paired" && (lower.contains("invalid") || lower.contains("too many")) {
        run(&["paired-devices"])?
    } else {
        text
    };
    Ok(parse_devices(&text).into_iter().map(|(a, _)| a).collect())
}

pub fn paired_devices() -> io::Result<Vec<BtDevice>> {
    let paired = filtered("Paired")?;
    let connected = filtered("Connected").unwrap_or_default();
    let all = parse_devices(&run(&["devices"])?);
    Ok(all
        .into_iter()
        .filter(|(a, _)| paired.contains(a))
        .map(|(addr, name)| BtDevice {
            name: real_name(addr, &name),
            addr,
            paired: true,
            connected: connected.contains(&addr),
        })
        .collect())
}

/// Scan for `seconds` and return what answered (paired ones included).
pub fn discover(seconds: u8) -> io::Result<Vec<BtDevice>> {
    let power = run(&["power", "on"])?;
    if power.to_ascii_lowercase().contains("blocked") {
        return Err(io::Error::other(
            "Bluetooth is blocked: run `sudo rfkill unblock bluetooth`",
        ));
    }
    let secs = seconds.max(1).to_string();
    let scan = run_for(
        &["--timeout", &secs, "scan", "on"],
        Duration::from_secs(u64::from(seconds) + 10),
    )?;
    let heard: Vec<u64> = parse_devices(&scan).into_iter().map(|(a, _)| a).collect();
    let paired = filtered("Paired").unwrap_or_default();
    let known = parse_devices(&run(&["devices"])?);
    // Prefer what the scan printed; an old bluetoothctl may print nothing,
    // then fall back to every device BlueZ knows.
    let pick: Vec<(u64, String)> = if heard.is_empty() {
        known
    } else {
        heard
            .iter()
            .map(|a| {
                let name = known
                    .iter()
                    .find(|(k, _)| k == a)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_default();
                (*a, name)
            })
            .collect()
    };
    Ok(pick
        .into_iter()
        .map(|(addr, name)| BtDevice {
            name: real_name(addr, &name),
            addr,
            paired: paired.contains(&addr),
            connected: false,
        })
        .collect())
}

/// What to type next into an interactive bluetoothctl, given everything it
/// printed so far and how many prompts we already answered.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Wait,
    Answer(&'static str),
    Paired,
    Failed(String),
}

fn next_step(text: &str, answered: usize) -> Step {
    let t = clean(text);
    if t.contains("Pairing successful") || t.contains("AlreadyExists") {
        return Step::Paired;
    }
    if let Some(line) = t
        .lines()
        .find(|l| l.contains("Failed to pair") || l.contains("not available"))
    {
        return Step::Failed(line.trim().to_string());
    }
    // Each prompt is answered once, in the order it appeared.
    let prompts: Vec<&'static str> = t
        .char_indices()
        .filter_map(|(i, _)| {
            let s = &t[i..];
            if s.starts_with("Enter PIN code") {
                Some("0000")
            } else if s.starts_with("Confirm passkey")
                || s.starts_with("Accept pairing")
                || s.starts_with("Authorize service")
            {
                Some("yes")
            } else {
                None
            }
        })
        .collect();
    match prompts.get(answered) {
        Some(a) => Step::Answer(a),
        None => Step::Wait,
    }
}

/// A pseudo-terminal pair: (master, slave). bluetoothctl only flushes its
/// agent prompts when it talks to a terminal.
fn open_pty() -> io::Result<(OwnedFd, OwnedFd)> {
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC);
        if master < 0 {
            return Err(io::Error::last_os_error());
        }
        let master = OwnedFd::from_raw_fd(master);
        use std::os::fd::AsRawFd;
        let m = master.as_raw_fd();
        if libc::grantpt(m) != 0 || libc::unlockpt(m) != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut name = [0 as libc::c_char; 128];
        if libc::ptsname_r(m, name.as_mut_ptr(), name.len()) != 0 {
            return Err(io::Error::last_os_error());
        }
        let slave = libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        );
        if slave < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((master, OwnedFd::from_raw_fd(slave)))
    }
}

/// Pair and trust `addr`, answering PIN 0000 and yes/no prompts.
pub fn pair(addr: u64) -> io::Result<()> {
    let a = format_addr(addr);
    let (master, slave) = open_pty()?;
    let mut child = Command::new("bluetoothctl")
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave))
        .spawn()
        .map_err(missing)?;
    let mut writer = std::fs::File::from(master.try_clone()?);
    let mut reader = std::fs::File::from(master);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new()
        .name("bluetoothctl-pair".into())
        .spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        })?;

    let mut say = |line: &str| writer.write_all(format!("{line}\n").as_bytes());
    say("agent on")?;
    say("default-agent")?;
    say(&format!("pair {a}"))?;

    let deadline = Instant::now() + PAIR_TIMEOUT;
    let mut text = String::new();
    let mut answered = 0;
    let result = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the radio did not answer the pairing request (is it in Menu → Pairing?)",
            ));
        }
        match rx.recv_timeout(left.min(Duration::from_millis(250))) {
            Ok(chunk) => text.push_str(&String::from_utf8_lossy(&chunk)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err(io::Error::other("bluetoothctl stopped while pairing"));
            }
        }
        if let Some(why) = trouble(&text) {
            break Err(io::Error::other(why));
        }
        match next_step(&text, answered) {
            Step::Wait => {}
            Step::Answer(reply) => {
                if let Err(e) = say(reply) {
                    break Err(e);
                }
                answered += 1;
            }
            Step::Paired => break Ok(()),
            Step::Failed(why) => break Err(io::Error::other(why)),
        }
    };
    if result.is_ok() {
        // Trusted devices reconnect without a prompt.
        let _ = say(&format!("trust {a}"));
        std::thread::sleep(Duration::from_millis(500));
    }
    let _ = say("quit");
    let stop = Instant::now() + Duration::from_secs(2);
    while Instant::now() < stop {
        if let Ok(Some(_)) = child.try_wait() {
            return result;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_device_lists_with_colour_and_events() {
        let out = "Device 38:D2:00:01:03:49 VR-N76\n\
                   \x1b[0;93m[CHG]\x1b[0m Device 38:D2:00:01:03:49 RSSI: -61\n\
                   [\x1b[0;92mNEW\x1b[0m] Device AA:BB:CC:DD:EE:FF AA-BB-CC-DD-EE-FF\r\n\
                   [CHG] Device 11:22:33:44:55:66 RSSI: -70\n\
                   [CHG] Device 11:22:33:44:55:66 Name: UV-PRO\n\
                   Discovery started\n";
        let got = parse_devices(out);
        assert_eq!(
            got,
            vec![
                (0x38D200010349, "VR-N76".to_string()),
                (0xAABBCCDDEEFF, "AA-BB-CC-DD-EE-FF".to_string()),
                (0x112233445566, String::new()),
            ]
        );
        assert_eq!(real_name(0xAABBCCDDEEFF, "AA-BB-CC-DD-EE-FF"), "");
        assert_eq!(real_name(0x38D200010349, "VR-N76"), "VR-N76");
    }

    #[test]
    fn answers_each_prompt_once_then_sees_the_result() {
        let mut t = String::from("Agent registered\nAttempting to pair with 38:D2:00:01:03:49\n");
        assert_eq!(next_step(&t, 0), Step::Wait);
        t.push_str("\x1b[0;94m[agent]\x1b[0m Enter PIN code: ");
        assert_eq!(next_step(&t, 0), Step::Answer("0000"));
        assert_eq!(next_step(&t, 1), Step::Wait);
        t.push_str("0000\n[agent] Confirm passkey 123456 (yes/no): ");
        assert_eq!(next_step(&t, 1), Step::Answer("yes"));
        t.push_str("yes\n[CHG] Device 38:D2:00:01:03:49 Paired: yes\nPairing successful\n");
        assert_eq!(next_step(&t, 2), Step::Paired);
    }

    #[test]
    fn reports_a_failed_pairing_and_a_known_radio() {
        let t = "Attempting to pair with 38:D2:00:01:03:49\n\
                 Failed to pair: org.bluez.Error.AuthenticationFailed\n";
        assert_eq!(
            next_step(t, 0),
            Step::Failed("Failed to pair: org.bluez.Error.AuthenticationFailed".into())
        );
        let t = "Failed to pair: org.bluez.Error.AlreadyExists\n";
        assert_eq!(next_step(t, 0), Step::Paired);
    }

    #[test]
    fn explains_common_bluez_problems() {
        assert!(trouble("No default controller available")
            .unwrap()
            .contains("rfkill"));
        assert!(trouble("Failed: org.bluez.Error.NotAuthorized")
            .unwrap()
            .contains("bluetooth group"));
        assert!(trouble("Waiting to connect to bluetoothd...")
            .unwrap()
            .contains("systemctl"));
        assert_eq!(trouble("Device 38:D2:00:01:03:49 VR-N76"), None);
    }
}

/// Drives the real bluetoothctl: `WCR_TEST_BT=1 cargo test -- --ignored live_`.
#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn live_find_and_pair() {
        if std::env::var("WCR_TEST_BT").is_err() {
            return;
        }
        eprintln!("paired: {:?}", super::paired_devices());
        eprintln!("seen: {:?}", super::discover(4));
        eprintln!("pair: {:?}", super::pair(0x38D200010349));
    }
}
