//! SPDX-License-Identifier: Apache-2.0
//! The web page's Software card (#280): see and install updates, restart on
//! the installed version, and turn test versions (release candidates) on or
//! off. No terminal and no password: `wcr web` replaces its own process with
//! the new binary once the radio is idle ([`crate::restart`]).

use super::Web;
use crate::config::Config;
use crate::restart::{self, OnStart};
use crate::update::{self, Outcome, Target};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How often the page looks for a new release by itself.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// "Check now" more often than this reuses the last answer (GitHub limits
/// unauthenticated checks).
const CHECK_AT_MOST: Duration = Duration::from_secs(60);
/// The installed binary's version is asked again after this long.
const INSTALLED_FOR: Duration = Duration::from_secs(10);
/// The file next to wcr.toml that tells `wcr restart` and `wcr status` which
/// process serves the page.
pub const RUN_FILE: &str = "web-run.json";
/// Logins handed to the next process across a restart.
const SESSIONS_FILE: &str = "web-sessions";

#[derive(Default)]
struct Inner {
    /// What is happening now ("Downloading 1.0.16", "Waiting for the radio
    /// to go quiet (sending)", "Restarting"). Empty when nothing is.
    phase: String,
    /// The last thing that went wrong, in one line.
    error: String,
    /// The last update's outcome from the update note.
    note: String,
    check: Option<update::Check>,
    checked_at: Option<Instant>,
    check_error: String,
    installed: Option<(Instant, Option<String>)>,
}

#[derive(Default)]
pub(super) struct Panel {
    inner: Mutex<Inner>,
}

impl Panel {
    fn set_phase(&self, phase: impl Into<String>) {
        let mut g = self.inner.lock();
        g.phase = phase.into();
        g.error.clear();
    }

    fn fail(&self, error: impl Into<String>) {
        let mut g = self.inner.lock();
        g.phase.clear();
        g.error = error.into();
    }

    fn busy(&self) -> bool {
        !self.inner.lock().phase.is_empty()
    }

    fn installed(&self) -> Option<String> {
        {
            let g = self.inner.lock();
            if let Some((at, v)) = &g.installed {
                if at.elapsed() < INSTALLED_FOR {
                    return v.clone();
                }
            }
        }
        let v = update::installed_version();
        self.inner.lock().installed = Some((Instant::now(), v.clone()));
        v
    }

    fn forget_installed(&self) {
        self.inner.lock().installed = None;
    }
}

/// Run as `wcr web` starts, before the station: settle the last update.
/// A new version that keeps failing puts the old one back and restarts on
/// it; one on probation is watched for [`restart::PROBATION`].
pub(super) fn on_start(web: &Web) -> bool {
    let dir = restart::install_dir();
    let running = update::current_version();
    match restart::on_start(&dir, running) {
        OnStart::Normal { note } => {
            web.update.inner.lock().note = note;
            false
        }
        OnStart::Probation => {
            web.station.note(&format!(
                "running {running} after an update; keeping it if it stays up for {} s",
                restart::PROBATION.as_secs()
            ));
            true
        }
        OnStart::RollBack { reason } => {
            let note = restart::roll_back(&dir, &reason);
            web.station.note(&note);
            if note.starts_with("Rolled back") {
                // The logins just taken back go on to the previous version.
                save_sessions(web);
                let err = restart::relaunch();
                web.station
                    .note(&format!("could not start the previous version: {err}"));
            }
            web.update.inner.lock().note = note;
            false
        }
    }
}

/// Keep the new version if `wcr web` and its station stay up through
/// probation; otherwise roll back and restart on the previous version.
pub(super) async fn watch_probation(web: Web) {
    let restarts = web.station.state().await.restarts;
    tokio::time::sleep(restart::PROBATION).await;
    let st = web.station.state().await;
    let wanted = web.station.wanted().await;
    let dir = restart::install_dir();
    if wanted && (!st.running || st.restarts > restarts) {
        let why = if st.last_exit.is_empty() {
            "the station did not stay up".to_string()
        } else {
            format!("the station did not stay up: {}", st.last_exit)
        };
        let note = restart::roll_back(&dir, &why);
        web.station.note(&note);
        web.update.inner.lock().note = note.clone();
        if note.starts_with("Rolled back") {
            web.station.shutdown().await;
            save_sessions(&web);
            let err = restart::relaunch();
            web.station
                .note(&format!("could not start the previous version: {err}"));
            web.station.resume().await;
        }
        return;
    }
    if let Some(note) = restart::confirm(&dir) {
        web.station.note(&note);
        web.update.inner.lock().note = note;
    }
}

/// Look for a release now and then, so the card can say "Update available"
/// without anyone pressing anything.
pub(super) async fn check_now_and_then(web: Web) {
    loop {
        refresh(&web, true).await;
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

fn test_versions(cfg_path: &Path) -> bool {
    Config::load(cfg_path)
        .map(|c| c.update.takes_test_versions(update::current_version()))
        .unwrap_or(false)
}

async fn refresh(web: &Web, force: bool) {
    let recent = web
        .update
        .inner
        .lock()
        .checked_at
        .is_some_and(|t| t.elapsed() < CHECK_AT_MOST);
    if recent && !force {
        return;
    }
    let rc = test_versions(&web.cfg_path);
    let result = update::check(rc).await;
    let mut g = web.update.inner.lock();
    g.checked_at = Some(Instant::now());
    match result {
        Ok(c) => {
            g.check = Some(c);
            g.check_error.clear();
        }
        Err(e) => g.check_error = e.to_string(),
    }
}

/// `GET /api/update`: what runs, what is installed, what is offered, and
/// what is happening now.
pub(super) async fn get(State(web): State<Web>) -> Json<Value> {
    Json(snapshot(&web))
}

fn snapshot(web: &Web) -> Value {
    let running = update::current_version();
    let installed = web.update.installed();
    let g = web.update.inner.lock();
    let check = g.check.clone().unwrap_or_default();
    let restart_needed = installed
        .as_deref()
        .is_some_and(|i| i.trim_start_matches('v') != running);
    json!({
        "running": running,
        "installed": installed,
        "restart_needed": restart_needed,
        "available": check.available,
        "stable": check.stable,
        "stable_note": check.stable_note,
        "building": check.building,
        "checked": g.checked_at.map(|t| t.elapsed().as_secs()),
        "check_error": g.check_error,
        "test_versions": test_versions(&web.cfg_path),
        "on_test_version": update::is_test_version(running),
        "phase": g.phase,
        "error": g.error,
        "note": g.note,
    })
}

/// `POST /api/update/check`: "Check now".
pub(super) async fn check(State(web): State<Web>) -> Json<Value> {
    refresh(&web, false).await;
    Json(snapshot(&web))
}

#[derive(Deserialize)]
pub(super) struct ApplyBody {
    /// `newest` (default) or `stable` (back from a test version).
    #[serde(default)]
    target: String,
    /// QA: the new version pretends to fail its start, to test the rollback.
    /// Taken only while test versions are on.
    #[serde(default)]
    simulate_failure: bool,
}

/// `POST /api/update/apply`: install the update, then restart on it once
/// the radio is idle. Answers at once; the card follows `phase`.
pub(super) async fn apply(State(web): State<Web>, Json(b): Json<ApplyBody>) -> Response {
    if web.update.busy() {
        return refuse("An update or restart is already under way.");
    }
    let rc = test_versions(&web.cfg_path);
    let target = if b.target == "stable" {
        Target::BackToStable
    } else {
        Target::Newest { rc }
    };
    let simulate = b.simulate_failure && rc;
    web.update.set_phase("Checking for updates");
    web.station.note(&format!(
        "update pressed on the web page{}",
        if simulate { " (rollback test)" } else { "" }
    ));
    tokio::spawn(async move {
        let panel = web.update.clone();
        let outcome = update::install(target, Some(&web.cfg_path), simulate, |p| {
            panel.set_phase(p)
        })
        .await;
        web.update.forget_installed();
        match outcome {
            Ok(Outcome::UpToDate(msg)) => web.update.fail(msg),
            Ok(Outcome::Installed(done)) => {
                web.station
                    .note(&format!("installed {} (was {})", done.to, done.from));
                safe_restart(&web).await;
            }
            Err(e) => {
                web.station.note(&format!("update failed: {e}"));
                web.update.fail(format!("Update failed: {e}"));
            }
        }
    });
    Json(snapshot_ok()).into_response()
}

fn snapshot_ok() -> Value {
    json!({ "ok": true })
}

fn refuse(msg: &str) -> Response {
    (StatusCode::CONFLICT, Json(json!({ "error": msg }))).into_response()
}

/// `POST /api/update/restart`: "Restart to finish".
pub(super) async fn restart_now(State(web): State<Web>) -> Response {
    if web.update.busy() {
        return refuse("An update or restart is already under way.");
    }
    web.station.note("restart pressed on the web page");
    start_safe_restart(web);
    Json(snapshot_ok()).into_response()
}

/// Begin a safe restart in the background (also SIGHUP, `wcr restart`).
pub(super) fn start_safe_restart(web: Web) {
    if web.update.busy() {
        return;
    }
    web.update.set_phase("Waiting for the radio to go quiet");
    tokio::spawn(async move { safe_restart(&web).await });
}

#[derive(Deserialize)]
pub(super) struct TestVersionsBody {
    on: bool,
}

/// `POST /api/update/test-versions`: the "Get test versions (rc)" switch.
/// Changes only `[update] test_versions` in wcr.toml.
pub(super) async fn set_test_versions(
    State(web): State<Web>,
    Json(b): Json<TestVersionsBody>,
) -> Response {
    let mut cfg = match Config::load(&web.cfg_path) {
        Ok(c) => c,
        Err(e) => return refuse(&e.to_string()),
    };
    cfg.update.test_versions = Some(b.on);
    if let Err(e) = cfg.save(&web.cfg_path) {
        return refuse(&e.to_string());
    }
    web.station.note(&format!(
        "test versions turned {} on the web page",
        if b.on { "on" } else { "off" }
    ));
    web.update.inner.lock().checked_at = None;
    refresh(&web, true).await;
    Json(snapshot(&web)).into_response()
}

/// Wait for the radio to be idle, stop the station (its Start/Stop choice
/// is kept), and replace this process with the installed binary.
async fn safe_restart(web: &Web) {
    let panel = web.update.clone();
    panel.set_phase("Waiting for the radio to go quiet");
    let waited = restart::wait_idle_async(
        &web.http,
        &web.local,
        restart::IdleWatch::standard(),
        Duration::from_millis(500),
        |why| panel.set_phase(format!("Waiting for the radio to go quiet ({why})")),
    )
    .await;
    if let Err(msg) = waited {
        web.station.note(&msg);
        panel.fail(msg);
        return;
    }
    panel.set_phase("Restarting");
    web.station.note(&format!(
        "restarting on the installed version ({}); radio idle",
        update::installed_version().unwrap_or_else(|| "unknown".into())
    ));
    web.station.shutdown().await;
    save_sessions(web);
    let err = restart::relaunch();
    let msg = format!("Could not restart: {err}");
    web.station.note(&msg);
    panel.fail(msg);
    web.station.resume().await;
}

fn sessions_path(cfg_path: &Path) -> PathBuf {
    cfg_path.with_file_name(SESSIONS_FILE)
}

fn save_sessions(web: &Web) {
    if let Err(e) = web.sessions.save_for_restart(&sessions_path(&web.cfg_path)) {
        web.station
            .note(&format!("logins not kept across the restart: {e}"));
    }
}

/// Take back the logins the previous process left, if any.
pub(super) fn restore_sessions(web: &Web) {
    let n = web
        .sessions
        .restore_after_restart(&sessions_path(&web.cfg_path));
    if n > 0 {
        web.station
            .note(&format!("kept {n} login(s) across the restart"));
    }
}

/// What `web-run.json` holds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RunFile {
    pub pid: u32,
    pub version: String,
    /// Unix seconds this process (re)started.
    pub started: u64,
    /// The page's port, for `wcr status`.
    pub port: String,
}

pub fn run_file_path(cfg_path: &Path) -> PathBuf {
    cfg_path.with_file_name(RUN_FILE)
}

pub fn write_run_file(cfg_path: &Path, port: &str) {
    let run = RunFile {
        pid: std::process::id(),
        version: update::current_version().to_string(),
        started: restart::unix_now(),
        port: port.to_string(),
    };
    if let Ok(text) = serde_json::to_string(&run) {
        let _ = std::fs::write(run_file_path(cfg_path), text);
    }
}

pub fn read_run_file(cfg_path: &Path) -> Option<RunFile> {
    serde_json::from_str(&std::fs::read_to_string(run_file_path(cfg_path)).ok()?).ok()
}

/// SIGHUP (`wcr restart`, `wcr update`): a safe restart on the installed
/// version.
pub(super) async fn restart_on_hangup(web: Web) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let Ok(mut hup) = signal(SignalKind::hangup()) else {
            return;
        };
        while hup.recv().await.is_some() {
            web.station.note("restart asked for (wcr restart)");
            start_safe_restart(web.clone());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = web;
    }
}
