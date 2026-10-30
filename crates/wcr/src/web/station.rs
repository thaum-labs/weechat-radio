//! SPDX-License-Identifier: Apache-2.0
//! `wcr web` runs the station (`wcr node`) as its child, as the desktop app
//! does, so the page can stop and start it and stays up while it is down.
//! The child's log lines are kept for the page's log tail and also written
//! to our own stderr, so `journalctl -u wcr-web` has them.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Log lines kept for the page.
const LOG_LINES: usize = 300;
/// A station that stops by itself is started again after this long.
const RESTART_AFTER: Duration = Duration::from_secs(5);
/// How long a stopping station gets to end modem73 before it is killed.
const STOP_WAIT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Inner {
    child: Option<Child>,
    started: Option<Instant>,
    /// The page (or `wcr web` starting) asked for the station to run.
    want: bool,
    restarts: u32,
    last_exit: String,
    exited_at: Option<Instant>,
}

pub struct Station {
    inner: tokio::sync::Mutex<Inner>,
    log: Log,
    cfg_path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StationState {
    pub running: bool,
    pub uptime_secs: u64,
    pub restarts: u32,
    pub last_exit: String,
}

impl Station {
    pub fn new(cfg_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            inner: tokio::sync::Mutex::new(Inner::default()),
            log: Log::default(),
            cfg_path,
        })
    }

    pub fn note(&self, line: &str) {
        eprintln!("web: {line}");
        push_line(&self.log, &format!("web: {line}"));
    }

    /// The last `n` log lines, oldest first.
    pub fn log_tail(&self, n: usize) -> Vec<String> {
        let log = self.log.lock();
        log.iter()
            .skip(log.len().saturating_sub(n))
            .cloned()
            .collect()
    }

    /// The last `n` warnings and errors, newest first, so the latest one
    /// is at the top of the page's box.
    pub fn errors(&self, n: usize) -> Vec<String> {
        let log = self.log.lock();
        log.iter()
            .rev()
            .filter(|l| is_problem(l))
            .take(n)
            .cloned()
            .collect()
    }

    pub async fn state(&self) -> StationState {
        let inner = self.inner.lock().await;
        StationState {
            running: inner.child.is_some(),
            uptime_secs: inner
                .started
                .filter(|_| inner.child.is_some())
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0),
            restarts: inner.restarts,
            last_exit: inner.last_exit.clone(),
        }
    }

    pub async fn start(&self) {
        let mut inner = self.inner.lock().await;
        inner.want = true;
        if inner.child.is_none() {
            self.spawn(&mut inner);
        }
    }

    /// Stop the station and keep it stopped until Start.
    pub async fn stop(&self) {
        let mut inner = self.inner.lock().await;
        inner.want = false;
        if let Some(child) = inner.child.take() {
            self.note("stopping the station");
            end_child(child).await;
            inner.last_exit = "stopped from the web page".into();
            inner.exited_at = Some(Instant::now());
        }
    }

    /// Watches the child: one that ends by itself is started again.
    pub async fn supervise(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let mut inner = self.inner.lock().await;
            if let Some(child) = inner.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    inner.child = None;
                    inner.last_exit = format!("station ended: {status}");
                    inner.exited_at = Some(Instant::now());
                    self.note(&inner.last_exit.clone());
                }
            }
            let waited = inner.exited_at.is_none_or(|t| t.elapsed() >= RESTART_AFTER);
            if inner.want && inner.child.is_none() && waited {
                if inner.exited_at.is_some() {
                    inner.restarts += 1;
                }
                self.spawn(&mut inner);
            }
        }
    }

    fn spawn(&self, inner: &mut Inner) {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("wcr"));
        let mut cmd = Command::new(exe);
        cmd.arg("-c")
            .arg(&self.cfg_path)
            .arg("node")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        match cmd.spawn() {
            Ok(mut child) => {
                if let Some(out) = child.stdout.take() {
                    tokio::spawn(read_lines(out, self.log.clone()));
                }
                if let Some(err) = child.stderr.take() {
                    tokio::spawn(read_lines(err, self.log.clone()));
                }
                inner.child = Some(child);
                inner.started = Some(Instant::now());
                self.note("station started");
            }
            Err(e) => {
                inner.last_exit = format!("could not start the station: {e}");
                inner.exited_at = Some(Instant::now());
                self.note(&inner.last_exit.clone());
            }
        }
    }
}

/// The station's log lines, shared with the tasks reading its output.
type Log = Arc<Mutex<VecDeque<String>>>;

async fn read_lines(pipe: impl tokio::io::AsyncRead + Unpin, log: Log) {
    let mut lines = BufReader::new(pipe).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        eprintln!("{line}");
        push_line(&log, &line);
    }
}

fn push_line(log: &Mutex<VecDeque<String>>, line: &str) {
    let line = strip_ansi(line.trim_end());
    if line.is_empty() {
        return;
    }
    let mut log = log.lock();
    if log.len() >= LOG_LINES {
        log.pop_front();
    }
    log.push_back(line);
}

/// Ask the station to end (it stops modem73 on the way), then kill it.
async fn end_child(mut child: Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        // SAFETY: a plain signal to our own child process.
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        if tokio::time::timeout(STOP_WAIT, child.wait()).await.is_ok() {
            return;
        }
    }
    let _ = child.kill().await;
}

fn is_problem(line: &str) -> bool {
    line.contains(" WARN ") || line.contains(" ERROR ") || line.contains("ERR ")
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip `ESC [ … letter`.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_codes_are_dropped_and_problems_found() {
        assert_eq!(
            strip_ansi("\u{1b}[2mT\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m x"),
            "T  WARN x"
        );
        let s = Station::new(PathBuf::from("wcr.toml"));
        push_line(&s.log, "2026-10-29T15:00:00Z  INFO hub up");
        push_line(&s.log, "2026-10-29T15:00:01Z  WARN hub down");
        push_line(&s.log, "2026-10-29T15:00:02Z  WARN modem73 exited");
        assert_eq!(s.log_tail(5).len(), 3);
        assert_eq!(
            s.errors(5),
            vec![
                "2026-10-29T15:00:02Z  WARN modem73 exited",
                "2026-10-29T15:00:01Z  WARN hub down"
            ]
        );
        assert_eq!(
            s.errors(1),
            vec!["2026-10-29T15:00:02Z  WARN modem73 exited"]
        );
    }

    #[test]
    fn the_log_keeps_only_the_newest_lines() {
        let s = Station::new(PathBuf::from("wcr.toml"));
        for i in 0..LOG_LINES + 5 {
            push_line(&s.log, &format!("line {i}"));
        }
        let tail = s.log_tail(LOG_LINES + 10);
        assert_eq!(tail.len(), LOG_LINES);
        assert_eq!(tail[0], "line 5");
    }
}
