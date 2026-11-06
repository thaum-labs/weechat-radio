//! SPDX-License-Identifier: Apache-2.0
//! Restart after an update without a terminal or sudo (#280).
//!
//! The update writes a small note (`wcr-update.json`) beside the binary:
//! which version it came from, which it installed, and which files it
//! replaced (each keeps its `.bak`). `wcr web` and the desktop app read it
//! when they start:
//!
//! - a new version on probation counts its starts; one that keeps dying
//!   before it is confirmed puts the previous files back and restarts on
//!   them ("rolled back"),
//! - one that stays up (and keeps its station up) for [`PROBATION`] is
//!   confirmed.
//!
//! Restarting itself is done in place: `wcr web` replaces its own process
//! with the new binary (same PID, same systemd unit, same user), so nothing
//! needs root. Before that it waits for the radio to be idle
//! ([`IdleWatch`]), so a restart never cuts off a transmission or a mail
//! lease.

use crate::status::StatusSnapshot;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// The update note beside the binary.
pub const MARKER: &str = "wcr-update.json";
/// A new version must stay up this long before it is kept.
pub const PROBATION: Duration = Duration::from_secs(60);
/// Starts of a new version allowed before it is rolled back. The first is
/// the restart itself; the second covers one crash (systemd starts it again).
pub const MAX_STARTS: u32 = 2;
/// The radio must stay idle this long before a restart.
pub const IDLE_SETTLE: Duration = Duration::from_secs(5);
/// A restart gives up (and says so) if the radio is busy for this long.
pub const IDLE_CAP: Duration = Duration::from_secs(15 * 60);
/// A beacon due this soon counts as busy.
const BEACON_SOON_SECS: u64 = 10;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Installed, the new version has not been confirmed yet.
    #[default]
    Pending,
    /// The new version stayed up: kept.
    Done,
    /// The new version failed to start: the previous one is back.
    RolledBack,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Marker {
    pub from: String,
    pub to: String,
    /// Files the update replaced. Each has its backup beside it.
    pub files: Vec<PathBuf>,
    pub phase: Phase,
    /// Starts of `to` so far while pending.
    pub starts: u32,
    /// QA: treat the new version as failing to start (from the update
    /// panel's "Test the rollback").
    pub simulate_failure: bool,
    /// One line for the page and the app: "Updated from 1.0.15 to 1.0.16",
    /// "Rolled back to 1.0.15: …".
    pub note: String,
    /// Unix seconds of the last change.
    pub at: u64,
}

impl Marker {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(MARKER)
    }

    pub fn load(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(Self::path(dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&mut self, dir: &Path) -> std::io::Result<()> {
        self.at = unix_now();
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(Self::path(dir), text)
    }
}

/// What a starting `wcr web` or desktop app should do about the last update.
#[derive(Debug, Clone, PartialEq)]
pub enum OnStart {
    /// Nothing pending.
    Normal { note: String },
    /// This is a new version on probation: confirm it after [`PROBATION`].
    Probation,
    /// This new version must give way: roll back, then restart.
    RollBack { reason: String },
}

/// Read the note in `dir` as version `running` starts, and count the start.
pub fn on_start(dir: &Path, running: &str) -> OnStart {
    let Some(mut m) = Marker::load(dir) else {
        return OnStart::Normal {
            note: String::new(),
        };
    };
    if m.phase != Phase::Pending || !same_version(&m.to, running) {
        return OnStart::Normal { note: m.note };
    }
    if m.simulate_failure {
        return OnStart::RollBack {
            reason: "test: the new version was told to fail its start".into(),
        };
    }
    m.starts += 1;
    let _ = m.save(dir);
    if m.starts > MAX_STARTS {
        return OnStart::RollBack {
            reason: format!(
                "it stopped {} times while starting",
                m.starts.saturating_sub(1)
            ),
        };
    }
    OnStart::Probation
}

/// The new version stayed up: keep it.
pub fn confirm(dir: &Path) -> Option<String> {
    let mut m = Marker::load(dir)?;
    if m.phase != Phase::Pending {
        return None;
    }
    m.phase = Phase::Done;
    m.note = format!("Updated from {} to {}", m.from, m.to);
    let _ = m.save(dir);
    Some(m.note)
}

/// Put the previous files back and note why. Returns the note for the page.
pub fn roll_back(dir: &Path, reason: &str) -> String {
    let Some(mut m) = Marker::load(dir) else {
        return format!("Could not roll back ({reason}): no update note found");
    };
    let missed = crate::update::restore_files(&m.files);
    m.phase = Phase::RolledBack;
    m.simulate_failure = false;
    m.note = if missed.is_empty() {
        format!(
            "Rolled back to {}: {} did not start ({reason})",
            m.from, m.to
        )
    } else {
        format!(
            "Could not roll back to {} ({reason}): {} not restored. Re-run the installer from https://weechatradio.com",
            m.from,
            missed.join(", ")
        )
    };
    let _ = m.save(dir);
    m.note
}

/// Did the last update fail to roll back cleanly, or roll back at all?
pub fn last_note(dir: &Path) -> String {
    Marker::load(dir).map(|m| m.note).unwrap_or_default()
}

fn same_version(a: &str, b: &str) -> bool {
    a.trim_start_matches('v') == b.trim_start_matches('v')
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

static INSTALL_EXE: OnceLock<PathBuf> = OnceLock::new();

/// Test hook (tests/update_restart.rs): where the harness's fake new
/// version says it is installed. Never set in normal use.
pub const TEST_INSTALL_EXE_ENV: &str = "WCR_TEST_INSTALL_EXE";

/// Note where this binary is installed, before an update can move it. On
/// Linux `current_exe` follows the running file, so after the update renames
/// it to `wcr.bak` it would name the backup.
pub fn remember_install_exe() {
    let _ = install_exe();
}

/// Where this binary is installed (the path the update writes to).
pub fn install_exe() -> PathBuf {
    INSTALL_EXE
        .get_or_init(|| {
            if let Some(p) = std::env::var_os(TEST_INSTALL_EXE_ENV) {
                return PathBuf::from(p);
            }
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("wcr"));
            std::fs::canonicalize(&exe).unwrap_or(exe)
        })
        .clone()
}

/// The folder holding the binary and the update note.
pub fn install_dir() -> PathBuf {
    install_exe()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `wcr` beside this binary (the desktop app is `wcr-gui`).
pub fn wcr_exe() -> PathBuf {
    let exe = install_exe();
    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("wcr");
    if stem.eq_ignore_ascii_case("wcr-gui") {
        exe.with_file_name(format!("wcr{}", std::env::consts::EXE_SUFFIX))
    } else {
        exe
    }
}

/// A restart the operator asked for (`wcr restart`, Restart to finish,
/// an Update) while the running version is still on probation keeps it:
/// it was up, and the stop is not a crash. Without this, a `wcr restart`
/// within [`PROBATION`] of an update counted as a failed start and rolled
/// the update back (QA #284). Returns the note when it confirmed.
pub fn confirm_before_deliberate_restart(dir: &Path, running: &str) -> Option<String> {
    let m = Marker::load(dir)?;
    if m.phase != Phase::Pending || !same_version(&m.to, running) || m.simulate_failure {
        return None;
    }
    confirm(dir)
}

/// Replace this process with the installed binary, same arguments. On Unix
/// the PID stays (systemd, launchd and the terminal see no change). On
/// Windows a new process starts and this one ends. Returns only on error.
pub fn relaunch() -> std::io::Error {
    relaunch_with(&install_exe(), std::env::args_os().skip(1).collect())
}

pub fn relaunch_with(exe: &Path, args: Vec<OsString>) -> std::io::Error {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        fresh_command(exe).args(args).exec()
    }
    #[cfg(not(unix))]
    {
        match fresh_command(exe).args(args).spawn() {
            Ok(_) => std::process::exit(0),
            Err(e) => e,
        }
    }
}

/// The installed binary, without this process's test hooks (a rolled-back
/// version must not inherit the new one's).
fn fresh_command(exe: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(exe);
    cmd.env_remove(crate::update::TEST_VERSION_ENV)
        .env_remove(TEST_INSTALL_EXE_ENV);
    cmd
}

/// Why the radio is busy right now, in a few words; `None` when idle.
pub fn busy_reason(s: &StatusSnapshot, unix_now: u64) -> Option<&'static str> {
    if s.ptt_on || s.channel == "tx" {
        return Some("sending");
    }
    if s.mail_lease {
        return Some("sending mail");
    }
    if s.queue_air > 0 {
        return Some("frames waiting to go out");
    }
    if s.channel == "rx" {
        return Some("receiving");
    }
    let due = u64::from(s.beacon_due);
    if due > 0 && s.beacon_note.is_empty() && due <= unix_now + BEACON_SOON_SECS {
        return Some("a beacon is due");
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Idle long enough (or no station is running): restart now.
    Idle,
    /// Keep waiting.
    Busy(&'static str),
    /// Busy for [`IDLE_CAP`]: do not restart.
    GaveUp(&'static str),
}

/// Watches the station's `/status` until the radio has been idle for
/// `settle` in a row.
pub struct IdleWatch {
    settle: Duration,
    cap: Duration,
    began: Instant,
    idle_since: Option<Instant>,
    last_busy: &'static str,
}

impl IdleWatch {
    pub fn new(settle: Duration, cap: Duration, now: Instant) -> Self {
        Self {
            settle,
            cap,
            began: now,
            idle_since: None,
            last_busy: "",
        }
    }

    pub fn standard() -> Self {
        Self::new(IDLE_SETTLE, IDLE_CAP, Instant::now())
    }

    /// `snap` is `None` when the station does not answer (not running), which
    /// counts as idle at once: there is nothing on the air to cut off.
    pub fn feed(&mut self, snap: Option<&StatusSnapshot>, now: Instant, unix: u64) -> Verdict {
        let Some(snap) = snap else {
            return Verdict::Idle;
        };
        match busy_reason(snap, unix) {
            Some(why) => {
                self.idle_since = None;
                self.last_busy = why;
                if now.saturating_duration_since(self.began) >= self.cap {
                    Verdict::GaveUp(why)
                } else {
                    Verdict::Busy(why)
                }
            }
            None => {
                let since = *self.idle_since.get_or_insert(now);
                if now.saturating_duration_since(since) >= self.settle {
                    Verdict::Idle
                } else {
                    Verdict::Busy(if self.last_busy.is_empty() {
                        "checking the radio is quiet"
                    } else {
                        self.last_busy
                    })
                }
            }
        }
    }
}

/// "The radio stayed busy (sending) for 15 minutes …".
pub fn gave_up_message(why: &str) -> String {
    format!(
        "Not restarted: the radio stayed busy ({why}) for {} minutes. Nothing was cut off. Try again when it is quiet.",
        IDLE_CAP.as_secs() / 60
    )
}

/// Ask a station's loopback `/status` (async, for `wcr web` and the CLI).
pub async fn fetch_status(http: &reqwest::Client, base: &str) -> Option<StatusSnapshot> {
    http.get(format!("{base}/status"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()
}

/// Wait for the radio behind `base` (`http://127.0.0.1:8074`) to be idle.
/// `on_busy` hears each reason while waiting. Err holds the give-up line.
pub async fn wait_idle_async(
    http: &reqwest::Client,
    base: &str,
    mut watch: IdleWatch,
    poll: Duration,
    mut on_busy: impl FnMut(&str),
) -> Result<(), String> {
    loop {
        let snap = fetch_status(http, base).await;
        match watch.feed(snap.as_ref(), Instant::now(), unix_now()) {
            Verdict::Idle => return Ok(()),
            Verdict::Busy(why) => on_busy(why),
            Verdict::GaveUp(why) => return Err(gave_up_message(why)),
        }
        tokio::time::sleep(poll).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> StatusSnapshot {
        StatusSnapshot {
            channel: "idle".into(),
            ..StatusSnapshot::default()
        }
    }

    #[test]
    fn each_kind_of_traffic_counts_as_busy() {
        let now = 1_000_000;
        assert_eq!(busy_reason(&idle(), now), None);
        let mut s = idle();
        s.ptt_on = true;
        assert_eq!(busy_reason(&s, now), Some("sending"));
        let mut s = idle();
        s.mail_lease = true;
        assert_eq!(busy_reason(&s, now), Some("sending mail"));
        let mut s = idle();
        s.queue_air = 2;
        assert_eq!(busy_reason(&s, now), Some("frames waiting to go out"));
        let mut s = idle();
        s.channel = "rx".into();
        assert_eq!(busy_reason(&s, now), Some("receiving"));
        let mut s = idle();
        s.beacon_due = (now + 5) as u32;
        assert_eq!(busy_reason(&s, now), Some("a beacon is due"));
        s.beacon_due = (now + 60) as u32;
        assert_eq!(busy_reason(&s, now), None);
        // VOX or beacons off: nothing will key.
        s.beacon_due = (now + 5) as u32;
        s.beacon_note = "off".into();
        assert_eq!(busy_reason(&s, now), None);
    }

    #[test]
    fn waits_for_idle_radio_before_restart() {
        let t0 = Instant::now();
        let mut w = IdleWatch::new(Duration::from_secs(5), Duration::from_secs(900), t0);
        let mut sending = idle();
        sending.ptt_on = true;
        let at = |s: u64| t0 + Duration::from_secs(s);
        assert_eq!(w.feed(Some(&sending), at(0), 0), Verdict::Busy("sending"));
        // Idle, but not for long enough yet.
        assert_eq!(w.feed(Some(&idle()), at(1), 0), Verdict::Busy("sending"));
        assert_eq!(w.feed(Some(&idle()), at(4), 0), Verdict::Busy("sending"));
        // A receipt keys again: the quiet time starts over.
        assert_eq!(w.feed(Some(&sending), at(5), 0), Verdict::Busy("sending"));
        assert_eq!(w.feed(Some(&idle()), at(6), 0), Verdict::Busy("sending"));
        assert_eq!(w.feed(Some(&idle()), at(10), 0), Verdict::Busy("sending"));
        assert_eq!(w.feed(Some(&idle()), at(11), 0), Verdict::Idle);
    }

    #[test]
    fn a_radio_busy_for_too_long_is_not_restarted() {
        let t0 = Instant::now();
        let mut w = IdleWatch::new(Duration::from_secs(5), Duration::from_secs(60), t0);
        let mut mail = idle();
        mail.mail_lease = true;
        assert_eq!(
            w.feed(Some(&mail), t0 + Duration::from_secs(30), 0),
            Verdict::Busy("sending mail")
        );
        assert_eq!(
            w.feed(Some(&mail), t0 + Duration::from_secs(61), 0),
            Verdict::GaveUp("sending mail")
        );
        assert!(gave_up_message("sending mail").contains("Nothing was cut off"));
    }

    #[test]
    fn a_stopped_station_is_idle_at_once() {
        let mut w = IdleWatch::standard();
        assert_eq!(w.feed(None, Instant::now(), 0), Verdict::Idle);
    }

    #[test]
    fn a_new_version_that_keeps_dying_is_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        // No note: a normal start.
        assert_eq!(
            on_start(dir.path(), "1.0.16"),
            OnStart::Normal {
                note: String::new()
            }
        );
        let mut m = Marker {
            from: "1.0.15".into(),
            to: "1.0.16".into(),
            ..Marker::default()
        };
        m.save(dir.path()).unwrap();
        // The old version still running (not restarted yet) changes nothing.
        assert!(matches!(
            on_start(dir.path(), "1.0.15"),
            OnStart::Normal { .. }
        ));
        assert_eq!(on_start(dir.path(), "1.0.16"), OnStart::Probation);
        assert_eq!(on_start(dir.path(), "v1.0.16"), OnStart::Probation);
        match on_start(dir.path(), "1.0.16") {
            OnStart::RollBack { reason } => assert!(reason.contains("stopped 2 times"), "{reason}"),
            other => panic!("{other:?}"),
        }
    }

    /// QA #284: `wcr restart` 10 s after an update is not a crash.
    #[test]
    fn a_deliberate_restart_on_probation_keeps_the_new_version() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = Marker {
            from: "1.0.16-rc3".into(),
            to: "1.0.16-rc4".into(),
            ..Marker::default()
        };
        m.save(dir.path()).unwrap();
        assert_eq!(on_start(dir.path(), "1.0.16-rc4"), OnStart::Probation);
        // The old version (not restarted yet) or a test build changes nothing.
        assert_eq!(
            confirm_before_deliberate_restart(dir.path(), "1.0.16-rc3"),
            None
        );
        assert_eq!(
            confirm_before_deliberate_restart(dir.path(), "1.0.16-rc4").as_deref(),
            Some("Updated from 1.0.16-rc3 to 1.0.16-rc4")
        );
        // Restart after restart: each is a normal start, never a rollback.
        for _ in 0..3 {
            assert!(matches!(
                on_start(dir.path(), "1.0.16-rc4"),
                OnStart::Normal { .. }
            ));
        }
        // Already confirmed: nothing more to say.
        assert_eq!(
            confirm_before_deliberate_restart(dir.path(), "1.0.16-rc4"),
            None
        );
        // A forced bad build still rolls back on its deliberate restart.
        let mut m = Marker {
            from: "1.0.16-rc4".into(),
            to: "1.0.16-rc5".into(),
            simulate_failure: true,
            ..Marker::default()
        };
        m.save(dir.path()).unwrap();
        assert_eq!(
            confirm_before_deliberate_restart(dir.path(), "1.0.16-rc5"),
            None
        );
        assert!(matches!(
            on_start(dir.path(), "1.0.16-rc5"),
            OnStart::RollBack { .. }
        ));
    }

    #[test]
    fn a_new_version_that_stays_up_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = Marker {
            from: "1.0.16-rc2".into(),
            to: "1.0.16-rc3".into(),
            ..Marker::default()
        };
        m.save(dir.path()).unwrap();
        assert_eq!(on_start(dir.path(), "1.0.16-rc3"), OnStart::Probation);
        assert_eq!(
            confirm(dir.path()).as_deref(),
            Some("Updated from 1.0.16-rc2 to 1.0.16-rc3")
        );
        // Later starts are normal and still show the note.
        assert_eq!(
            on_start(dir.path(), "1.0.16-rc3"),
            OnStart::Normal {
                note: "Updated from 1.0.16-rc2 to 1.0.16-rc3".into()
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn rolls_back_on_failed_start() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("wcr");
        std::fs::write(&exe, "new").unwrap();
        std::fs::write(dir.path().join("wcr.bak"), "old").unwrap();
        let mut m = Marker {
            from: "1.0.15".into(),
            to: "1.0.16".into(),
            files: vec![exe.clone()],
            simulate_failure: true,
            ..Marker::default()
        };
        m.save(dir.path()).unwrap();
        let OnStart::RollBack { reason } = on_start(dir.path(), "1.0.16") else {
            panic!("a simulated failure must roll back");
        };
        let note = roll_back(dir.path(), &reason);
        assert!(
            note.starts_with("Rolled back to 1.0.15: 1.0.16 did not start"),
            "{note}"
        );
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "old");
        let m = Marker::load(dir.path()).unwrap();
        assert_eq!(m.phase, Phase::RolledBack);
        assert!(!m.simulate_failure);
        // The old version starting again shows the note and does not loop.
        assert_eq!(on_start(dir.path(), "1.0.15"), OnStart::Normal { note });
    }
}
