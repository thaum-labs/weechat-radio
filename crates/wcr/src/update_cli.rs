//! SPDX-License-Identifier: Apache-2.0
//! `wcr update`, `wcr status` and `wcr restart` (#280): one plain line at
//! the end, no sudo. A running `wcr web` restarts itself when asked (SIGHUP)
//! once the radio is idle; see [`crate::restart`].

use crate::config::{Config, StatusConfig};
use crate::error::{Error, Result};
use crate::restart;
use crate::status::StatusSnapshot;
use crate::update::{self, Outcome, Target};
use crate::web::software::{read_run_file, RunFile};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long `wcr update` / `wcr restart` wait for the new process, on top of
/// the wait for a quiet radio.
const RESTART_WAIT: Duration = Duration::from_secs(120);

fn status_base(cfg_path: &Path) -> String {
    let bind = Config::load(cfg_path)
        .map(|c| c.status.bind)
        .unwrap_or_else(|_| StatusConfig::default().bind);
    let port = bind.rsplit(':').next().unwrap_or("8074");
    format!("http://127.0.0.1:{port}")
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .no_proxy()
        .build()
        .map_err(|e| Error::Msg(e.to_string()))
}

/// `wcr update [--rc]`: install, restart safely, check, one line.
pub async fn update(cfg_path: &Path, rc_flag: bool) -> Result<String> {
    let cfg = Config::load(cfg_path).ok();
    let running = update::current_version();
    let rc = rc_flag
        || cfg
            .as_ref()
            .is_some_and(|c| c.update.takes_test_versions(running));
    let cfg_for_backup = cfg.as_ref().map(|_| cfg_path);
    let outcome = update::install(Target::Newest { rc }, cfg_for_backup, false, |p| {
        println!("{p}…")
    })
    .await?;
    match outcome {
        Outcome::UpToDate(msg) => Ok(msg),
        Outcome::Installed(done) => {
            println!("{}", done.message);
            restart_and_check(cfg_path, &done.to, true).await
        }
    }
}

/// `wcr restart`: restart the station's process on the installed version.
pub async fn restart(cfg_path: &Path) -> Result<String> {
    let installed = update::installed_version().unwrap_or_else(|| update::current_version().into());
    restart_and_check(cfg_path, &installed, false).await
}

/// Restart whatever runs from `cfg_path` on `want`, then say how it is.
async fn restart_and_check(cfg_path: &Path, want: &str, after_update: bool) -> Result<String> {
    let http = http()?;
    let base = status_base(cfg_path);
    if let Some(run) = live_web(cfg_path) {
        println!("Waiting for the radio to go quiet, then restarting the web page and station…");
        hang_up(run.pid)?;
        let back = wait_for_restart(cfg_path, &run, restart::IDLE_CAP + RESTART_WAIT).await?;
        if !same(&back.version, want) {
            let note = restart::last_note(&restart::install_dir());
            return Err(Error::Msg(if note.is_empty() {
                format!("Restarted, but it runs {} instead of {want}.", back.version)
            } else {
                note
            }));
        }
        if station_left_running(cfg_path) {
            wait_station_version(&http, &base, want).await;
        }
        return Ok(final_line(&http, &base, want, after_update).await);
    }
    if crate::service::installed() {
        println!("Waiting for the radio to go quiet, then restarting the station service…");
        wait_quiet(&http, &base).await?;
        if !crate::service::restart_if_installed() {
            return Err(Error::Msg(
                "could not restart the station service. Try: wcr service install".into(),
            ));
        }
        wait_station_version(&http, &base, want).await;
        return Ok(final_line(&http, &base, want, after_update).await);
    }
    if let Some(snap) = restart::fetch_status(&http, &base).await {
        if !same(&snap.version, want) {
            return Ok(format!(
                "{} The station on {base} still runs {}: press Restart in the desktop app, or stop and start it.",
                updated_or_installed(want, after_update),
                snap.version
            ));
        }
        return Ok(final_line(&http, &base, want, after_update).await);
    }
    Ok(format!(
        "{} Nothing was running; the next start runs {want}.",
        updated_or_installed(want, after_update)
    ))
}

/// The web page's saved Start/Stop says the station should be on the air.
fn station_left_running(cfg_path: &Path) -> bool {
    std::fs::read_to_string(crate::web::station_state_path(cfg_path))
        .is_ok_and(|s| s.trim() == "start")
}

fn updated_or_installed(want: &str, after_update: bool) -> String {
    if after_update {
        format!("Updated to {want}.")
    } else {
        format!("Installed version is {want}.")
    }
}

/// "Updated to 1.0.16, running, radio idle."
async fn final_line(http: &reqwest::Client, base: &str, want: &str, after_update: bool) -> String {
    let lead = if after_update {
        format!("Updated to {want}, running")
    } else {
        format!("Restarted on {want}, running")
    };
    match restart::fetch_status(http, base).await {
        Some(s) => format!("{lead}, {}.", radio_words(&s)),
        None => format!("{lead}, station stopped (as it was left)."),
    }
}

fn radio_words(s: &StatusSnapshot) -> String {
    match restart::busy_reason(s, restart::unix_now()) {
        None => "radio idle".into(),
        Some(why) => format!("radio busy ({why})"),
    }
}

fn same(a: &str, b: &str) -> bool {
    a.trim_start_matches('v') == b.trim_start_matches('v')
}

/// The `wcr web` serving this config, if it still runs.
fn live_web(cfg_path: &Path) -> Option<RunFile> {
    let run = read_run_file(cfg_path)?;
    pid_alive(run.pid).then_some(run)
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if pid == 0 {
            return false;
        }
        // SAFETY: signal 0 only checks that the process exists.
        let rc = unsafe { libc::kill(pid as i32, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn hang_up(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        // SAFETY: a plain signal to the `wcr web` process named in its run file.
        let rc = unsafe { libc::kill(pid as i32, libc::SIGHUP) };
        if rc != 0 {
            let e = std::io::Error::last_os_error();
            return Err(Error::Msg(format!(
                "could not ask the web page (pid {pid}) to restart: {e}. Run this as the user the station runs as (not with sudo)."
            )));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err(Error::Msg(
            "restart the station from the desktop app's Restart button".into(),
        ))
    }
}

/// Wait until `wcr web` has started again (its run file changes).
async fn wait_for_restart(cfg_path: &Path, before: &RunFile, cap: Duration) -> Result<RunFile> {
    let began = Instant::now();
    let mut said = String::new();
    while began.elapsed() < cap {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Some(now) = read_run_file(cfg_path) {
            if now.started != before.started || now.pid != before.pid {
                // Probation: give the new process its first seconds.
                return Ok(now);
            }
        }
        if !pid_alive(before.pid) && said != "gone" {
            said = "gone".into();
            println!("The web page stopped; waiting for it to come back…");
        }
    }
    Err(Error::Msg(format!(
        "the web page did not restart within {} minutes. The radio may still be busy: try `wcr restart` later, or press Restart on the web page.",
        cap.as_secs() / 60
    )))
}

async fn wait_quiet(http: &reqwest::Client, base: &str) -> Result<()> {
    let mut last = String::new();
    restart::wait_idle_async(
        http,
        base,
        restart::IdleWatch::standard(),
        Duration::from_millis(500),
        |why| {
            if why != last {
                println!("  radio busy ({why})…");
                last = why.to_string();
            }
        },
    )
    .await
    .map_err(Error::Msg)
}

async fn wait_station_version(http: &reqwest::Client, base: &str, want: &str) {
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(60) {
        if restart::fetch_status(http, base)
            .await
            .is_some_and(|s| same(&s.version, want))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// `wcr status`: installed and running versions, station and radio, updates.
pub async fn status(cfg_path: &Path) -> Result<String> {
    let http = http()?;
    let base = status_base(cfg_path);
    let installed = update::installed_version().unwrap_or_else(|| update::current_version().into());
    let mut lines = vec![format!("Installed: {installed}")];
    match live_web(cfg_path) {
        Some(run) => lines.push(format!(
            "Web page:  running {} (pid {}, port {}){}",
            run.version,
            run.pid,
            run.port,
            if same(&run.version, &installed) {
                String::new()
            } else {
                format!(". Not the installed {installed}: run wcr restart")
            }
        )),
        None => lines.push("Web page:  not running".into()),
    }
    match restart::fetch_status(&http, &base).await {
        Some(s) => lines.push(format!(
            "Station:   running {} as {}, {}",
            s.version,
            s.callsign,
            radio_words(&s)
        )),
        None => lines.push("Station:   stopped".into()),
    }
    let rc = Config::load(cfg_path)
        .map(|c| c.update.takes_test_versions(update::current_version()))
        .unwrap_or(false);
    match update::check(rc).await {
        Ok(c) => lines.push(match (c.available, c.building) {
            (Some(v), _) => format!(
                "Update:    {v} available. Press Update on the web page or in the app, or run wcr update"
            ),
            (None, Some(v)) => format!("Update:    {v} is still being built"),
            (None, None) => format!(
                "Update:    up to date{}",
                if rc { " (test versions on)" } else { "" }
            ),
        }),
        Err(e) => lines.push(format!("Update:    could not check ({e})")),
    }
    let note = restart::last_note(&restart::install_dir());
    if !note.is_empty() {
        lines.push(format!("Last:      {note}"));
    }
    Ok(lines.join("\n"))
}
