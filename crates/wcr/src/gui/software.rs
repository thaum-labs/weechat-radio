//! SPDX-License-Identifier: Apache-2.0
//! The desktop app's Software window (#280): "Update available", Update,
//! "Installed x / Running y" with Restart to finish. Release candidates are
//! command-line only (`wcr update --rc`). Same steps as the web page: install with the SHA-256 check, wait
//! for the radio to be idle, stop the station, start the new app with the
//! station as it was, and roll back if the new version does not stay up.

use super::{hair_btn, ACCENT, AMBER, DIM, FG, GREEN, ORANGE, RED};
use crate::config::Config;
use crate::restart::{self, IdleWatch, OnStart, Verdict};
use crate::status::StatusSnapshot;
use crate::update::{self, Outcome};
use eframe::egui::{self, RichText};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Look for a release this often while the app is open.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Ask the installed `wcr --version` again after this long.
const INSTALLED_FOR: Duration = Duration::from_secs(10);
/// Tells the new app whether the station was running before the restart.
pub const STATION_ENV: &str = "WCR_GUI_STATION";

#[derive(Default)]
struct Shared {
    check: Option<update::Check>,
    check_error: String,
    checking: bool,
    phase: String,
    error: String,
    note: String,
    /// The install finished: restart once the radio is idle.
    installed: bool,
}

pub struct Software {
    shared: Arc<Mutex<Shared>>,
    pub open: bool,
    last_check: Option<Instant>,
    installed: Option<(Instant, Option<String>)>,
    /// A restart waiting for the radio to go quiet.
    restart: Option<IdleWatch>,
    probation_until: Option<Instant>,
}

/// What the app must do this frame.
pub enum Tick {
    Nothing,
    /// Stop the station and start the installed app.
    Relaunch,
}

impl Software {
    /// Settle the last update as the app starts. A new version that keeps
    /// failing puts the previous one back and starts it instead.
    pub fn start() -> Self {
        let dir = restart::install_dir();
        let mut note = String::new();
        let mut probation_until = None;
        match restart::on_start(&dir, update::current_version()) {
            OnStart::Normal { note: n } => note = n,
            OnStart::Probation => probation_until = Some(Instant::now() + restart::PROBATION),
            OnStart::RollBack { reason } => {
                note = restart::roll_back(&dir, &reason);
                if note.starts_with("Rolled back") {
                    let _ = restart::relaunch();
                }
            }
        }
        Self {
            shared: Arc::new(Mutex::new(Shared {
                note,
                ..Shared::default()
            })),
            open: false,
            last_check: None,
            installed: None,
            restart: None,
            probation_until,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn installed_version(&mut self) -> Option<String> {
        if let Some((at, v)) = &self.installed {
            if at.elapsed() < INSTALLED_FOR {
                return v.clone();
            }
        }
        let v = update::installed_version();
        self.installed = Some((Instant::now(), v.clone()));
        v
    }

    fn restart_needed(&mut self) -> Option<String> {
        let running = update::current_version();
        self.installed_version()
            .filter(|i| i.trim_start_matches('v') != running)
    }

    fn check_now(&mut self) {
        if self.lock().checking {
            return;
        }
        self.last_check = Some(Instant::now());
        self.lock().checking = true;
        let shared = self.shared.clone();
        std::thread::spawn(move || {
            let result = runtime().and_then(|rt| rt.block_on(update::check(false)));
            let mut g = shared.lock().unwrap_or_else(|e| e.into_inner());
            g.checking = false;
            match result {
                Ok(c) => {
                    g.check = Some(c);
                    g.check_error.clear();
                }
                Err(e) => g.check_error = e.to_string(),
            }
        });
    }

    fn apply(&mut self) {
        let shared = self.shared.clone();
        {
            let mut g = self.lock();
            g.phase = "Checking for updates".into();
            g.error.clear();
        }
        std::thread::spawn(move || {
            let set = |p: &str| {
                let mut g = shared.lock().unwrap_or_else(|e| e.into_inner());
                g.phase = p.to_string();
            };
            let cfg = Config::active_path();
            let result = runtime()
                .and_then(|rt| rt.block_on(update::install(false, Some(&cfg), false, set)));
            let mut g = shared.lock().unwrap_or_else(|e| e.into_inner());
            match result {
                Ok(Outcome::Installed(done)) => {
                    g.phase = format!("Installed {}. Waiting for the radio to go quiet", done.to);
                    g.installed = true;
                }
                Ok(Outcome::UpToDate(msg)) => {
                    g.phase.clear();
                    g.error = msg;
                }
                Err(e) => {
                    g.phase.clear();
                    g.error = format!("Update failed: {e}");
                }
            }
        });
    }

    fn begin_restart(&mut self) {
        self.restart = Some(IdleWatch::standard());
        let mut g = self.lock();
        g.phase = "Waiting for the radio to go quiet".into();
        g.error.clear();
    }

    /// Called every frame with the station's status. `station_failed`: the
    /// operator wants the station on, but it has stopped by itself.
    pub fn tick(&mut self, status: Option<&StatusSnapshot>, station_failed: bool) -> Tick {
        if self.last_check.is_none_or(|t| t.elapsed() > CHECK_EVERY) {
            self.check_now();
        }
        if std::mem::take(&mut self.lock().installed) {
            self.installed = None;
            self.begin_restart();
        }
        if let Some(until) = self.probation_until {
            if Instant::now() >= until {
                self.probation_until = None;
                let dir = restart::install_dir();
                if station_failed {
                    let note = restart::roll_back(&dir, "the station did not stay up");
                    let rolled = note.starts_with("Rolled back");
                    self.lock().note = note;
                    if rolled {
                        return Tick::Relaunch;
                    }
                } else if let Some(note) = restart::confirm(&dir) {
                    self.lock().note = note;
                }
            }
        }
        let Some(watch) = self.restart.as_mut() else {
            return Tick::Nothing;
        };
        match watch.feed(status, Instant::now(), restart::unix_now()) {
            Verdict::Idle => {
                self.restart = None;
                self.lock().phase = "Restarting".into();
                Tick::Relaunch
            }
            Verdict::Busy(why) => {
                self.lock().phase = format!("Waiting for the radio to go quiet ({why})");
                Tick::Nothing
            }
            Verdict::GaveUp(why) => {
                self.restart = None;
                let mut g = self.lock();
                g.phase.clear();
                g.error = restart::gave_up_message(why);
                Tick::Nothing
            }
        }
    }

    /// The top bar's link: shown only when there is something to do.
    pub fn top_link(&mut self, ui: &mut egui::Ui) {
        let (phase, available) = {
            let g = self.lock();
            (
                g.phase.clone(),
                g.check.as_ref().and_then(|c| c.available.clone()),
            )
        };
        let label = if !phase.is_empty() {
            Some(("updating…".to_string(), AMBER))
        } else if let Some(v) = self.restart_needed() {
            Some((format!("restart to finish ({v})"), AMBER))
        } else {
            available.map(|v| (format!("update available ({v})"), AMBER))
        };
        if let Some((text, color)) = label {
            let b = egui::Button::new(RichText::new(text).color(color).monospace())
                .fill(egui::Color32::TRANSPARENT)
                .frame(false);
            if ui.add(b).clicked() {
                self.open = true;
            }
        }
    }

    /// The Software window.
    pub fn window(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let running = update::current_version().to_string();
        let restart_needed = self.restart_needed();
        let installed = self.installed_version();
        let (check, check_error, checking, phase, error, note) = {
            let g = self.lock();
            (
                g.check.clone(),
                g.check_error.clone(),
                g.checking,
                g.phase.clone(),
                g.error.clone(),
                g.note.clone(),
            )
        };
        let busy = !phase.is_empty();
        let mut open = self.open;
        let mut action: Option<&str> = None;
        egui::Window::new("Software")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("Running {running}")).color(FG).monospace());
                let c = check.clone().unwrap_or_default();
                let (line, color) = if let Some(i) = &restart_needed {
                    (
                        format!(
                            "Installed {} / Running {running}. Restart to finish the update.",
                            i
                        ),
                        AMBER,
                    )
                } else if let Some(v) = &c.available {
                    (format!("Update available ({v})."), AMBER)
                } else if let Some(v) = &c.building {
                    (format!("{v} is still being built."), DIM)
                } else if !check_error.is_empty() {
                    (format!("Could not check for updates: {check_error}"), RED)
                } else if checking || check.is_none() {
                    ("Checking for updates…".into(), DIM)
                } else {
                    ("Up to date.".into(), GREEN)
                };
                ui.add_space(6.0);
                ui.label(RichText::new(line).color(color));
                if busy {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&phase).color(AMBER));
                }
                if !error.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&error).color(ORANGE));
                }
                ui.add_space(8.0);
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if restart_needed.is_some() {
                            if ui.add(hair_btn("Restart to finish", ACCENT)).clicked() {
                                action = Some("restart");
                            }
                        } else {
                            if let Some(v) = &c.available {
                                if ui.add(hair_btn(&format!("Update to {v}"), ACCENT)).clicked() {
                                    action = Some("update");
                                }
                            }
                        }
                        if ui.add(hair_btn("Check now", DIM)).clicked() {
                            action = Some("check");
                        }
                    });
                });
                if let Some(i) = &installed {
                    if restart_needed.is_none() && i != &running {
                        ui.label(RichText::new(format!("Installed {i}")).color(DIM).small());
                    }
                }
                if !note.is_empty() {
                    ui.add_space(6.0);
                    let color = if note.starts_with("Rolled back") || note.starts_with("Could not") {
                        ORANGE
                    } else {
                        DIM
                    };
                    ui.label(RichText::new(&note).color(color).small());
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new("The station restarts only when the radio is quiet, and comes back as it was.")
                        .color(DIM)
                        .small(),
                );
            });
        self.open = open;
        match action {
            Some("update") => self.apply(),
            Some("restart") => self.begin_restart(),
            Some("check") => self.check_now(),
            _ => {}
        }
    }
}

fn runtime() -> crate::error::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(crate::error::Error::from)
}

/// Start the installed app with the station as it was, and end this one.
pub fn relaunch(station_running: bool) -> std::io::Error {
    std::env::set_var(STATION_ENV, if station_running { "on" } else { "off" });
    // Asked for by the operator: a version on probation is kept (QA #284).
    let _ = restart::confirm_before_deliberate_restart(
        &restart::install_dir(),
        update::current_version(),
    );
    restart::relaunch()
}

/// True when the app was restarted by an update with the station stopped.
pub fn station_was_stopped() -> bool {
    let off = std::env::var(STATION_ENV).is_ok_and(|v| v == "off");
    std::env::remove_var(STATION_ENV);
    off
}
