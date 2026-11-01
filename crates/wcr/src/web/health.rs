//! SPDX-License-Identifier: Apache-2.0
//! The machine's health for the gateway view (#231): CPU, memory,
//! temperature, power/throttling, disk, uptime, network and clock.
//!
//! Read-only, from /proc and /sys (and `vcgencmd` on a Raspberry Pi), no
//! sudo. Every reading is optional: what a system does not have is left
//! out, and the page hides it.

use serde_json::{json, Value};
use std::path::Path;

/// One snapshot. `served_ip` is the address the browser used (Host header),
/// so the page can say which interface it came in on. `node_pid` is the
/// station child of `wcr web`, when it runs.
pub fn snapshot(
    data_dir: &Path,
    served_ip: Option<std::net::IpAddr>,
    node_pid: Option<u32>,
) -> Value {
    let temp = temperature();
    json!({
        "cpu": cpu(),
        "memory": memory(node_pid),
        "temperature": temp,
        "temperature_word": temp.map(temp_word),
        "power": power(),
        "disks": disks(data_dir),
        "uptime_secs": uptime_secs(),
        "network": network(served_ip),
        "clock_synced": clock_synced(),
        "system": system(),
    })
}

// ---- words for the page ----------------------------------------------------

/// Pi 3/4 throttle at 80 °C and cap the clock from 60 °C (soft limit) on
/// some models; "warm" starts a little earlier so the warning shows first.
pub fn temp_word(c: f64) -> &'static str {
    if c >= 80.0 {
        "hot"
    } else if c >= 70.0 {
        "warm"
    } else {
        "ok"
    }
}

/// Free space on a disk: "very low" under 5 % or 200 MB, "low" under 10 %
/// or 1 GB.
pub fn disk_word(free: u64, total: u64) -> &'static str {
    const MB: u64 = 1024 * 1024;
    let pct = (free * 100).checked_div(total).unwrap_or(100);
    if pct < 5 || free < 200 * MB {
        "very low"
    } else if pct < 10 || free < 1024 * MB {
        "low"
    } else {
        "ok"
    }
}

/// Wi-Fi signal in plain words.
pub fn wifi_word(dbm: f64) -> &'static str {
    if dbm >= -60.0 {
        "good"
    } else if dbm >= -70.0 {
        "fair"
    } else {
        "weak"
    }
}

/// `vcgencmd get_throttled` bits in plain words: (now, since boot).
pub fn throttle_words(bits: u32) -> (Vec<&'static str>, Vec<&'static str>) {
    const NAMES: [&str; 4] = [
        "under-voltage",
        "CPU frequency capped",
        "throttled",
        "soft temperature limit",
    ];
    let pick = |shift: u32| {
        NAMES
            .iter()
            .enumerate()
            .filter(|(i, _)| bits & (1 << (shift + *i as u32)) != 0)
            .map(|(_, n)| *n)
            .collect::<Vec<_>>()
    };
    (pick(0), pick(16))
}

/// The number in `throttled=0x50005`.
pub fn parse_throttled(text: &str) -> Option<u32> {
    let hex = text.trim().strip_prefix("throttled=")?;
    u32::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
}

// ---- readings ----------------------------------------------------------------

#[cfg(target_os = "linux")]
fn read(path: impl AsRef<Path>) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Busy and total jiffies for "cpu" (all) and each "cpuN" line of /proc/stat.
pub fn parse_proc_stat(text: &str) -> Vec<(u64, u64)> {
    text.lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            let n: Vec<u64> = l
                .split_whitespace()
                .skip(1)
                .filter_map(|v| v.parse().ok())
                .collect();
            let total: u64 = n.iter().take(8).sum();
            // idle + iowait
            let idle = n.get(3).copied().unwrap_or(0) + n.get(4).copied().unwrap_or(0);
            (total.saturating_sub(idle), total)
        })
        .collect()
}

/// CPU busy % since the previous call (any page's), overall then per core.
fn cpu() -> Value {
    #[cfg(target_os = "linux")]
    {
        use parking_lot::Mutex;
        static LAST: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());
        let now = read("/proc/stat")
            .map(|t| parse_proc_stat(&t))
            .unwrap_or_default();
        let mut last = LAST.lock();
        let pct: Vec<Option<f64>> = now
            .iter()
            .enumerate()
            .map(|(i, &(busy, total))| {
                let (b0, t0) = last.get(i).copied()?;
                let dt = total.checked_sub(t0).filter(|&d| d > 0)?;
                Some(busy.saturating_sub(b0) as f64 * 100.0 / dt as f64)
            })
            .collect();
        *last = now;
        let mhz = read("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
            .and_then(|t| t.trim().parse::<u64>().ok())
            .map(|khz| khz / 1000);
        json!({
            "pct": pct.first().copied().flatten(),
            "per_core": pct.iter().skip(1).copied().collect::<Vec<_>>(),
            "cores": pct.len().saturating_sub(1),
            "load": load(),
            "mhz": mhz,
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        json!({ "load": load() })
    }
}

fn load() -> Option<[f64; 3]> {
    #[cfg(unix)]
    {
        let mut l = [0f64; 3];
        let n = unsafe { libc::getloadavg(l.as_mut_ptr(), 3) };
        (n == 3).then_some(l)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// `MemTotal: 948304 kB` style lines, in bytes.
pub fn meminfo_bytes(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

fn memory(node_pid: Option<u32>) -> Value {
    #[cfg(target_os = "linux")]
    {
        let info = read("/proc/meminfo").unwrap_or_default();
        let get = |k| meminfo_bytes(&info, k);
        let swap_used = get("SwapTotal")
            .zip(get("SwapFree"))
            .map(|(t, f)| t.saturating_sub(f));
        let rss = |pid: &str| {
            read(format!("/proc/{pid}/status"))
                .and_then(|t| meminfo_bytes(&t.replace('\t', " "), "VmRSS"))
        };
        // This station's modem73 is a child of its node; another station's
        // modem73 on the same machine is not ours to show (#235).
        let modem73 = node_pid
            .and_then(|p| child_named(p, "modem73"))
            .and_then(|pid| rss(&pid.to_string()));
        json!({
            "total": get("MemTotal"),
            "available": get("MemAvailable"),
            "swap_total": get("SwapTotal"),
            "swap_used": swap_used,
            "rss": {
                "wcr web": rss("self"),
                "wcr node": node_pid.and_then(|p| rss(&p.to_string())),
                "modem73": modem73,
            },
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = node_pid;
        Value::Null
    }
}

/// The child of `parent` whose name is `name` (from /proc/*/comm and the
/// PPid line of /proc/*/status).
#[cfg(target_os = "linux")]
fn child_named(parent: u32, name: &str) -> Option<u32> {
    std::fs::read_dir("/proc").ok()?.flatten().find_map(|e| {
        let pid: u32 = e.file_name().to_str()?.parse().ok()?;
        if read(e.path().join("comm"))?.trim() != name {
            return None;
        }
        (parent_pid(&read(e.path().join("status"))?) == Some(parent)).then_some(pid)
    })
}

/// `PPid:\t1234` from a /proc/*/status text.
pub fn parent_pid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("PPid:"))
        .and_then(|v| v.trim().parse().ok())
}

fn temperature() -> Option<f64> {
    // QA can force a reading: WCR_TEST_TEMP_C=82.
    if let Some(c) = std::env::var("WCR_TEST_TEMP_C")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return Some(c);
    }
    #[cfg(target_os = "linux")]
    {
        let milli: f64 = read("/sys/class/thermal/thermal_zone0/temp")?
            .trim()
            .parse()
            .ok()?;
        Some(milli / 1000.0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// `vcgencmd get_throttled`, asked at most every 10 s (it forks a process).
/// Null where there is no vcgencmd (not a Pi, or not allowed to ask).
fn power() -> Value {
    #[cfg(target_os = "linux")]
    {
        use parking_lot::Mutex;
        use std::time::{Duration, Instant};
        static CACHE: Mutex<Option<(Instant, Option<u32>)>> = Mutex::new(None);
        let mut cache = CACHE.lock();
        let fresh = cache
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(10));
        if !fresh {
            let bits = std::process::Command::new("vcgencmd")
                .arg("get_throttled")
                .stderr(std::process::Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| parse_throttled(&String::from_utf8_lossy(&o.stdout)));
            *cache = Some((Instant::now(), bits));
        }
        // QA can force a reading where a real one cannot be made to happen:
        // WCR_TEST_THROTTLED=0x50005 (under-voltage and throttled).
        let forced = std::env::var("WCR_TEST_THROTTLED")
            .ok()
            .and_then(|v| parse_throttled(&format!("throttled={v}")));
        let Some(bits) = forced.or_else(|| cache.as_ref().and_then(|(_, b)| *b)) else {
            return Value::Null;
        };
        let (now, since_boot) = throttle_words(bits);
        json!({ "bits": format!("{bits:#x}"), "now": now, "since_boot": since_boot })
    }
    #[cfg(not(target_os = "linux"))]
    {
        Value::Null
    }
}

/// The root filesystem, and the one holding wcr's data when it is another.
fn disks(data_dir: &Path) -> Vec<Value> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mut out = Vec::new();
        let mut seen = Vec::new();
        for (label, path) in [("/", Path::new("/")), ("wcr data", data_dir)] {
            let Some(dev) = std::fs::metadata(path).ok().map(|m| m.dev()) else {
                continue;
            };
            let Some((free, used)) = statvfs(path) else {
                continue;
            };
            if seen.contains(&dev) {
                continue;
            }
            seen.push(dev);
            // As `df` counts it: space for files is used + free to users
            // (the root-only reserve is left out).
            let size = used + free;
            out.push(json!({
                "label": label,
                "path": path.display().to_string(),
                "free": free,
                "size": size,
                "pct_used": if size == 0 { 0 } else { (used * 100).div_ceil(size) },
                "word": disk_word(free, size),
            }));
        }
        out
    }
    #[cfg(not(unix))]
    {
        let _ = data_dir;
        Vec::new()
    }
}

/// (bytes free to users, bytes used), as `df` reports them.
#[cfg(unix)]
fn statvfs(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)]
    let (frsize, blocks, bfree, bavail) = (
        s.f_frsize as u64,
        s.f_blocks as u64,
        s.f_bfree as u64,
        s.f_bavail as u64,
    );
    Some((bavail * frsize, blocks.saturating_sub(bfree) * frsize))
}

fn uptime_secs() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let t = read("/proc/uptime")?;
        let secs: f64 = t.split_whitespace().next()?.parse().ok()?;
        Some(secs as u64)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// `wlan0: 0000   54.  -56.  -256 …` lines of /proc/net/wireless:
/// (interface, link quality out of 70, signal dBm).
pub fn parse_wireless(text: &str) -> Vec<(String, f64, f64)> {
    text.lines()
        .skip(2)
        .filter_map(|l| {
            let (name, rest) = l.split_once(':')?;
            let mut f = rest.split_whitespace().skip(1);
            let num = |s: Option<&str>| s?.trim_end_matches('.').parse::<f64>().ok();
            Some((name.trim().to_string(), num(f.next())?, num(f.next())?))
        })
        .collect()
}

fn network(served_ip: Option<std::net::IpAddr>) -> Value {
    let interfaces: Vec<Value> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback() && i.ip().is_ipv4())
        .map(|i| {
            json!({
                "name": i.name,
                "ip": i.ip().to_string(),
                "serving": Some(i.ip()) == served_ip,
            })
        })
        .collect();
    #[cfg(target_os = "linux")]
    let wifi: Vec<Value> = read("/proc/net/wireless")
        .map(|t| parse_wireless(&t))
        .unwrap_or_default()
        .into_iter()
        // A dBm of 0 means the driver does not report it.
        .filter(|(_, _, dbm)| *dbm < 0.0)
        .map(|(name, quality, dbm)| {
            json!({
                "name": name,
                "dbm": dbm,
                "quality_pct": (quality * 100.0 / 70.0).round().min(100.0),
                "word": wifi_word(dbm),
            })
        })
        .collect();
    #[cfg(not(target_os = "linux"))]
    let wifi: Vec<Value> = Vec::new();
    json!({ "interfaces": interfaces, "wifi": wifi })
}

/// Whether the kernel clock is NTP-synchronised (adjtimex, no process).
fn clock_synced() -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let mut tx: libc::timex = unsafe { std::mem::zeroed() };
        let state = unsafe { libc::adjtimex(&mut tx) };
        (state >= 0).then_some(state != libc::TIME_ERROR)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn system() -> Value {
    #[cfg(target_os = "linux")]
    {
        let model =
            read("/proc/device-tree/model").map(|m| m.trim_end_matches('\0').trim().to_string());
        let os = read("/etc/os-release").and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_string())
        });
        let kernel = read("/proc/sys/kernel/osrelease").map(|k| k.trim().to_string());
        json!({ "model": model, "os": os, "kernel": kernel })
    }
    #[cfg(not(target_os = "linux"))]
    {
        json!({ "os": std::env::consts::OS })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_pid_reads_the_ppid_line() {
        assert_eq!(
            parent_pid("Name:\tmodem73\nPid:\t812\nPPid:\t640\n"),
            Some(640)
        );
        assert_eq!(parent_pid("Name:\tx\n"), None);
    }

    /// Another station's process of the same name is not ours (#235).
    #[cfg(target_os = "linux")]
    #[test]
    fn only_our_own_child_counts() {
        let mut other = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let me = std::process::id();
        assert_eq!(child_named(me, "sleep"), Some(other.id()));
        let _ = other.kill();
        let _ = other.wait();
    }

    #[test]
    fn throttled_bits_read_as_words() {
        // 0x50005: under-voltage and throttled now, and both since boot.
        let bits = parse_throttled("throttled=0x50005\n").unwrap();
        let (now, boot) = throttle_words(bits);
        assert_eq!(now, ["under-voltage", "throttled"]);
        assert_eq!(boot, ["under-voltage", "throttled"]);
        assert_eq!(
            throttle_words(parse_throttled("throttled=0x0").unwrap())
                .0
                .len(),
            0
        );
        assert!(parse_throttled("error").is_none());
    }

    #[test]
    fn proc_files_parse() {
        let stat = "cpu  100 0 100 700 100 0 0 0 0 0\ncpu0 50 0 50 350 50 0 0 0 0 0\nintr 1 2\n";
        assert_eq!(parse_proc_stat(stat), vec![(200, 1000), (100, 500)]);
        let mem = "MemTotal:         948304 kB\nMemAvailable:     512000 kB\n";
        assert_eq!(meminfo_bytes(mem, "MemTotal"), Some(948304 * 1024));
        assert_eq!(meminfo_bytes(mem, "SwapTotal"), None);
        let wl = "Inter-| sta-|   Quality        |   Discarded packets\n face | tus | link level noise |  nwid  crypt\n wlan0: 0000   54.  -56.  -256        0      0\n";
        assert_eq!(parse_wireless(wl), vec![("wlan0".into(), 54.0, -56.0)]);
    }

    #[test]
    fn thresholds_in_words() {
        assert_eq!(temp_word(55.0), "ok");
        assert_eq!(temp_word(72.0), "warm");
        assert_eq!(temp_word(81.0), "hot");
        const GB: u64 = 1 << 30;
        assert_eq!(disk_word(10 * GB, 32 * GB), "ok");
        assert_eq!(disk_word(5 * GB / 2, 32 * GB), "low");
        assert_eq!(disk_word(GB / 10, 32 * GB), "very low");
        assert_eq!(wifi_word(-55.0), "good");
        assert_eq!(wifi_word(-75.0), "weak");
    }
}
