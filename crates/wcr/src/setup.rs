//! SPDX-License-Identifier: Apache-2.0
//! First-run wizard: callsign, grid, radio path, PTT, preset.

use crate::config::{self, Config};
use crate::error::Result;
use crate::modes::Mode;
use crate::presets::Preset;
use crate::proto::Callsign;
use crate::radios::{Radio, RADIOS};
use crate::ui_style;
use dialoguer::{Confirm, Input, Select};
use std::io::Write;
use std::path::Path;

/// Setup path for a supported CAT radio ([`Radio::setup_path`]).
const SUPPORTED_CAT: usize = 6;
/// Setup path for modem73 over a radio's Bluetooth audio channel.
const BT_AUDIO: usize = 7;

/// Run the wizard and save to `path` (the `-c` file, or the default).
/// Running it again over an existing file keeps what it does not ask about
/// (the web page password, mail, store…) and offers the current sound cards
/// as the defaults (QA #222).
pub fn run_wizard(cfg_path: &Path) -> Result<Config> {
    if !console::Term::stdout().is_term() {
        return Err(crate::error::Error::Msg(
            "wcr setup needs an interactive terminal. Run it in a shell, or copy wcr.toml from the docs.".into(),
        ));
    }
    let theme = ui_style::prompt_theme();
    ui_style::panel("WEECHAT RADIO", "SETUP");
    println!(
        "  {}",
        ui_style::dim().apply_to(format!(
            "wcr v{} — You will need: your callsign, and (if you have a radio) a cable.",
            crate::update::current_version()
        ))
    );
    println!();

    let previous = cfg_path
        .exists()
        .then(|| Config::load(cfg_path).ok())
        .flatten();
    if cfg_path.exists() && previous.is_none() {
        println!(
            "  {}",
            ui_style::warn().apply_to(format!(
                "{} could not be read; this setup replaces it.",
                cfg_path.display()
            ))
        );
    }
    let total_steps = 7;
    let (grid_tx, grid_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = grid_tx.send(crate::grid::detect_from_ip());
    });

    // Setup run again: every Enter keeps what the file says (QA #284).
    let saved_call = previous
        .as_ref()
        .map(|p| p.callsign.clone())
        .unwrap_or_default();
    let call = read_callsign_uppercase(
        &theme,
        ui_style::step(
            1,
            total_steps,
            "Callsign (guests: ~NICK, max 8 chars including ~)",
        ),
        &saved_call,
    )?;

    let guessed = grid_rx.recv().ok().flatten();
    let confirm_theme = ui_style::bracket_confirm_theme();
    let saved_grid = previous
        .as_ref()
        .map(|p| p.grid.trim().to_ascii_uppercase())
        .filter(|g| !g.is_empty());
    let mut grid_prompt = Input::with_theme(&theme)
        .with_prompt(ui_style::step(
            2,
            total_steps,
            "Maidenhead grid square (Enter to accept, or type your own)",
        ))
        .allow_empty(true);
    let mut accepted_detected = false;
    if let Some(saved) = &saved_grid {
        // A saved square is the default; the guess is only shown (QA #284).
        if let Some(hit) = guessed
            .as_ref()
            .filter(|h| h.grid.to_ascii_uppercase() != *saved)
        {
            println!(
                "  {}",
                ui_style::dim().apply_to(format!(
                    "Saved {saved}. Your internet address suggests {}{}; type it to change.",
                    hit.grid,
                    if hit.label.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", hit.label)
                    }
                ))
            );
        }
        grid_prompt = grid_prompt.default(saved.clone());
    } else if let Some(hit) = &guessed {
        let label = if hit.label.is_empty() {
            hit.grid.clone()
        } else {
            format!("{} · {}", hit.grid, hit.label)
        };
        println!(
            "  {}",
            ui_style::dim().apply_to(format!(
                "Detected {label}: a guess from your internet address, so check it (shown on the public map if you enable telemetry)"
            ))
        );
        accepted_detected = Confirm::with_theme(&confirm_theme)
            .with_prompt("Use this grid on the map?")
            .default(true)
            .interact()?;
        if accepted_detected {
            // Shown as the default, not typed into the field: typing over it
            // must replace the square, not add to it. Enter keeps it.
            grid_prompt = grid_prompt.default(hit.grid.clone());
        }
    } else {
        println!(
            "  {}",
            ui_style::dim()
                .apply_to("Could not detect reliably — leave empty or type e.g. IO81UF",)
        );
    }
    // A bad square asks again instead of ending the wizard.
    let grid = loop {
        let typed: String = grid_prompt.clone().interact_text()?;
        let grid = match &guessed {
            Some(hit) if saved_grid.is_none() => {
                grid_from_setup(accepted_detected, &hit.grid, &typed)
            }
            _ => typed.trim().to_ascii_uppercase(),
        };
        match crate::grid::normalize(&grid) {
            Err(e) if !grid.is_empty() => println!("  {}", ui_style::dim().apply_to(e)),
            _ => break grid,
        }
    };

    // Radios with two ways in (built-in TNC or modem73 over Bluetooth audio)
    // say which is which before the list (#232).
    for r in RADIOS.iter().filter(|r| !r.note.is_empty()) {
        println!(
            "  {}",
            ui_style::dim().apply_to(format!("{}: {}", r.name, r.note))
        );
    }
    // Supported radios first: picking one fills in everything but the port.
    let mut paths: Vec<String> = vec!["Internet only (no radio yet)".into()];
    paths.extend(RADIOS.iter().map(|r| r.name.to_string()));
    paths.push("Other setup (Digirig, VOX cable, any CAT rig, KISS TNC)".into());
    let saved = previous.as_ref().map(saved_path);
    let picked = Select::with_theme(&theme)
        .with_prompt(ui_style::step(
            3,
            total_steps,
            "Which radio? (pick Other for VOX, Digirig and the rest)",
        ))
        .items(&paths)
        .default(match saved {
            Some((0, _)) | None => 0,
            Some((_, Some(i))) => i + 1,
            Some((_, None)) => paths.len() - 1,
        })
        .interact()?;
    let radio = picked.checked_sub(1).and_then(|i| RADIOS.get(i));
    let path = match radio {
        _ if picked == 0 => 0,
        Some(r) => r.setup_path(),
        None => {
            let others = [
                (1, "Handheld + Digirig (VHF/UHF FM)"),
                (2, "Any radio with a plain audio cable (VOX)"),
                (3, "HF rig with CAT through rigctld (any Hamlib radio)"),
                (
                    5,
                    "KISS TNC on a serial port (Mobilinkd, rfcomm0, Bluetooth COM)",
                ),
            ];
            let labels: Vec<&str> = others.iter().map(|(_, l)| *l).collect();
            let i = Select::with_theme(&theme)
                .with_prompt("How will you get on the air?")
                .items(&labels)
                .default(
                    saved
                        .and_then(|(p, _)| others.iter().position(|(o, _)| *o == p))
                        .unwrap_or(0),
                )
                .interact()?;
            others[i].0
        }
    };

    // The same radio as before keeps every modem setting setup never asks
    // about (TX level, RX gain, busy test…); a new radio starts fresh.
    let same_radio = saved
        == Some((
            path,
            radio.map(|r| RADIOS.iter().position(|x| x == r).unwrap_or(0)),
        ));
    let mut cfg = start_from(previous.as_ref(), same_radio);
    let kept = previous.as_ref().filter(|_| same_radio);
    cfg.callsign = call.trim().to_ascii_uppercase();
    cfg.grid = grid.trim().to_ascii_uppercase();

    match path {
        0 => {
            cfg.mode = Mode::Internet;
            cfg.modem.manage = false;
            cfg.modem.ptt = "none".into();
            let default_hub = config::HubConfig::default().url;
            let public_hub = Confirm::with_theme(&confirm_theme)
                .with_prompt(ui_style::step(
                    4,
                    total_steps,
                    "Use the public hub (wss://hub.weechatradio.com)?",
                ))
                .default(previous.as_ref().is_none_or(|p| p.hub.url == default_hub))
                .interact()?;
            if public_hub {
                cfg.hub.url = default_hub;
            } else {
                let url: String = Input::with_theme(&theme)
                    .with_prompt("Hub WebSocket URL (wss://host/ws)")
                    .default(cfg.hub.url.clone())
                    .interact_text()?;
                cfg.hub.url = url;
            }
            let share_map = Confirm::with_theme(&confirm_theme)
                .with_prompt(ui_style::step(
                    5,
                    total_steps,
                    "Share callsign and grid on the public map? (never message text)",
                ))
                .default(
                    previous
                        .as_ref()
                        .is_some_and(|p| !p.telemetry.url.is_empty()),
                )
                .interact()?;
            if !share_map {
                cfg.telemetry.url.clear();
            } else if cfg.telemetry.url.is_empty() {
                cfg.telemetry.url = config::TelemetryConfig::default().url;
            }
        }
        1 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "digirig".into();
            cfg.modem.preset = Preset::VhfFm.as_str().into();
            let ports = crate::discover::serial_ports();
            let digirig = kept
                .map(|p| p.modem.com_port.clone())
                .filter(|c| !c.is_empty())
                .or_else(|| crate::discover::find_digirig(&ports));
            cfg.modem.com_port = pick_serial_port(&theme, "Digirig serial port", digirig)?;
            cfg.modem.com_line = "rts".into();
            println!("  Digirig: PTT on RTS. Plug the audio jacks into the radio as in the guide.");
        }
        2 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "vox".into();
            cfg.modem.preset = Preset::VoxSafe.as_str().into();
            cfg.modem.vox_lead_ms = Preset::VOX_SAFE_LEAD_MS;
            cfg.modem.vox_tail_ms = Preset::VOX_SAFE_TAIL_MS;
            println!("  VOX uses RDM-600S (works over a speaker and mic) and a long lead");
            println!("  so the radio is keyed before data. Keep radio VOX hang long.");
        }
        3 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "rigctl".into();
            cfg.modem.preset = Preset::HfGood.as_str().into();
            cfg.modem.rigctl = pick_rigctld(&theme, kept.map(|p| p.modem.rigctl.as_str()))?;
            cfg.rig.enabled = true;
            println!("  Start rigctld for your radio before `wcr node`.");
            println!("  If that fails, you can switch PTT to VOX later with /radio ptt vox");
        }
        4 | BT_AUDIO => {
            let radio = radio.unwrap_or(&RADIOS[0]);
            radio.apply(&mut cfg);
            println!("  On the radio: {}", radio.radio_side);
            if !radio.cable.is_empty() {
                println!("  Cable: {}", radio.cable);
            }
            if path == 4 {
                println!("  Close the radio's phone app (one Bluetooth client at a time).");
            }
            let _ = std::io::stdout().flush();
            bluetooth_pick(&theme, &mut cfg)?;
        }
        SUPPORTED_CAT => {
            let radio = radio.expect("supported CAT radio");
            radio.apply(&mut cfg);
            println!("  On the radio: {}", radio.radio_side);
            if !radio.cable.is_empty() {
                println!("  Cable: {}", radio.cable);
            }
            println!("  wcr keys the radio and reads the dial over CAT. Nothing else to install.");
            let ports = crate::discover::serial_ports();
            let guess = kept
                .map(|p| p.modem.hamlib_device.clone())
                .filter(|c| !c.is_empty())
                .or_else(|| radio.pick_port(&ports));
            cfg.modem.hamlib_device = pick_serial_port(&theme, "Radio CAT port", guess)?;
        }
        5 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.backend = "serial".into();
            cfg.modem.manage = false;
            cfg.modem.ptt = "tnc".into();
            cfg.modem.preset = Preset::Afsk1200.as_str().into();
            cfg.tnc.serial = pick_serial_port(
                &theme,
                "TNC serial port",
                kept.map(|p| p.tnc.serial.clone()).filter(|c| !c.is_empty()),
            )?;
        }
        _ => {}
    }
    if let (Some(r), 1 | 2) = (radio, path) {
        // Digirig or VOX radio: the generic path above, with this radio's preset.
        r.apply(&mut cfg);
        println!("  On the radio: {}", r.radio_side);
        if !r.cable.is_empty() {
            println!("  Cable: {}", r.cable);
        }
    }

    if path != 0 && !cfg.modem.uses_tnc() {
        // Over Bluetooth audio, only the presets that survive the link.
        let offered: Vec<Preset> = Preset::all()
            .into_iter()
            .filter(|&p| !cfg.modem.is_bt_audio() || crate::btaudio::preset_allowed(p))
            .collect();
        let presets: Vec<&str> = offered.iter().map(|p| p.as_str()).collect();
        let idx = Select::with_theme(&theme)
            .with_prompt(ui_style::step(
                4,
                total_steps,
                "Modem preset (you can change this later)",
            ))
            .items(&presets)
            .default(match path {
                _ if kept.is_some() => preset_index(
                    &offered,
                    kept.map(|p| p.modem.preset.as_str()).unwrap_or(""),
                ),
                _ if radio.is_some() => preset_index(&offered, &cfg.modem.preset),
                1 => 0,
                2 => 4,
                3 => 1,
                _ => 0,
            })
            .interact()?;
        cfg.modem.preset = offered[idx].as_str().into();
        let cat = matches!(path, 3 | SUPPORTED_CAT);
        let saved_khz = previous.as_ref().map_or(0, |p| p.rf.frequency_khz);
        let default_mhz = match radio {
            Some(r) => format!("{:.3}", r.dial_for(saved_khz).0 as f64 / 1000.0),
            None if saved_khz != 0 => format!("{:.3}", saved_khz as f64 / 1000.0),
            None if cat => "7.045".into(),
            None => "144.950".into(),
        };
        let freq: String = Input::with_theme(&theme)
            .with_prompt(if cat {
                "Frequency on the dial (MHz) — CAT will override when the rig is connected"
            } else {
                "Frequency your radio is on (MHz)"
            })
            .default(default_mhz)
            .interact_text()?;
        if let Some(khz) = crate::band::parse_mhz(&freq) {
            cfg.rf.frequency_khz = khz;
            if let Some(r) = radio.filter(|r| !r.covers(khz)) {
                println!(
                    "  {}",
                    ui_style::warn().apply_to(format!(
                        "{} is not a band the {} transmits on ({}). Check the radio's dial.",
                        crate::band::describe(khz),
                        r.short_name(),
                        r.bands.join(", ")
                    ))
                );
            }
            println!(
                "  {}",
                ui_style::dim().apply_to(format!(
                    "Stored {}. Change later with /radio freq",
                    crate::band::describe(khz)
                ))
            );
        }
        if cfg.modem.is_bt_audio() {
            // The sound travels over wcr's own Bluetooth link to the radio;
            // no sound card is used, so none is saved (QA #284).
            cfg.modem.audio_input.clear();
            cfg.modem.audio_output.clear();
            println!("  {}", ui_style::dim().apply_to(bt_audio_sound_note()));
        } else {
            cfg.modem.audio_input = pick_audio_device(
                &theme,
                5,
                total_steps,
                "Audio in (decode — radio speaker / interface capture)",
                crate::discover::list_audio_inputs(),
                AudioDefaults {
                    radio,
                    digirig: path == 1,
                    current: previous.as_ref().map(|p| p.modem.audio_input.as_str()),
                    capture: true,
                },
            )?;
            cfg.modem.audio_output = pick_audio_device(
                &theme,
                6,
                total_steps,
                "Audio out (transmit — radio mic / interface playback)",
                crate::discover::list_audio_outputs(),
                AudioDefaults {
                    radio,
                    digirig: path == 1,
                    current: previous.as_ref().map(|p| p.modem.audio_output.as_str()),
                    capture: false,
                },
            )?;
        }
        if !cfg.modem.audio_input.is_empty() || !cfg.modem.audio_output.is_empty() {
            println!(
                "  {}",
                ui_style::dim().apply_to(
                    "Passed to modem73 as --input-device / --output-device when the node starts.",
                )
            );
        }
        println!();
        println!("{}", calling_card());
        println!(
            "  {}",
            ui_style::dim().apply_to("Verify your band plan before you transmit.")
        );
    }

    if path == 0 {
        println!("  {}", ui_style::dim().apply_to(skipped_radio_step_note()));
    }

    // A headless Pi runs as the wcr-web system service, set up at the end;
    // the per-user node service would fight it for the radio.
    if !headless()
        && Confirm::with_theme(&confirm_theme)
            .with_prompt(ui_style::step(
                total_steps,
                total_steps,
                "Install as a background service so relays keep running?",
            ))
            .default(false)
            .interact()?
    {
        match crate::service::install() {
            Ok(msg) => println!("  {msg}"),
            Err(e) => {
                println!("  Could not install service: {e}");
                println!("  Try again later:  wcr service install");
            }
        }
    }

    // A station on the air shows as a tower on the map (Tobias, #220).
    cfg.gateway.tower = cfg.mode.is_gateway();
    if let Some(prev) = &previous {
        keep_unasked(&mut cfg, prev);
    }
    config::ensure_dirs()?;
    if let Some(backup) = config::backup_before_setup(cfg_path)? {
        println!(
            "  {}",
            ui_style::dim().apply_to(format!("Previous settings kept in {}", backup.display()))
        );
    }
    cfg.save(cfg_path)?;
    ui_style::panel("SAVED", cfg_path.display().to_string().as_str());
    match crate::weechat_app::configure(cfg_path) {
        Ok(msg) => {
            println!("  {msg}");
        }
        Err(e) => {
            println!("  WeeChat not configured yet: {e}");
            println!("  Re-run the official installer, or use `wcr tui`.");
        }
    }
    if headless() {
        // A Pi gateway over SSH: the browser page is the way in. Finish the
        // first install here so it starts at boot with no extra commands.
        finish_headless(cfg_path, &cfg, &confirm_theme);
        println!(
            "  Guide: https://github.com/thaum-labs/weechat-radio/blob/main/docs/PI-GATEWAY.md"
        );
        println!("  How you know it worked: the web page's lights show the station, internet, hub and radio.");
    } else {
        println!("  Next:  wcr gui   or   wcr tui");
        println!("  With WeeChat:  wcr node   then   wcr weechat");
        println!("  How you know it worked: the status bar shows your callsign and mode.");
    }
    ui_style::rule();
    let _ = std::io::stdout().flush();
    Ok(cfg)
}

/// What a headless first install still needs after setup has saved.
#[derive(Debug, PartialEq, Eq)]
struct HeadlessSteps {
    password: bool,
    boot_service: bool,
}

/// Only what is missing: a working gateway (password set, service in
/// place) is left exactly as it is.
fn headless_steps(password_set: bool, service_installed: bool) -> HeadlessSteps {
    HeadlessSteps {
        password: !password_set,
        boot_service: !service_installed,
    }
}

/// Pi over SSH: set the web page password and start the station at boot
/// (`sudo wcr web install`), each only if it is not done yet (#280).
fn finish_headless(cfg_path: &Path, cfg: &Config, theme: &dyn dialoguer::theme::Theme) {
    let service_installed = Path::new(crate::web::UNIT_PATH).exists();
    let steps = headless_steps(!cfg.web.password_hash.is_empty(), service_installed);
    if steps.password {
        println!("  Choose a password for the station's web page.");
        if let Err(e) = crate::web::set_password(cfg_path) {
            println!("  Password not set: {e}");
            println!("  Next:  wcr web password   then   sudo ~/.local/bin/wcr web install");
            return;
        }
    }
    if !steps.boot_service {
        return;
    }
    let yes = Confirm::with_theme(theme)
        .with_prompt("Start the station now and at every boot? (sudo asks for your password once)")
        .default(true)
        .interact()
        .unwrap_or(false);
    let manual = "  Later:  sudo ~/.local/bin/wcr web install";
    if !yes {
        println!("{manual}");
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            println!("  Could not find wcr: {e}");
            println!("{manual}");
            return;
        }
    };
    let ok = std::process::Command::new("sudo")
        .arg(&exe)
        .arg("-c")
        .arg(cfg_path)
        .args(["web", "install"])
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        println!("  The boot service was not installed.");
        println!("{manual}");
    }
}

/// Linux with no desktop session (an SSH login on a Pi, for example).
fn headless() -> bool {
    cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

fn read_callsign_uppercase(
    theme: &dyn dialoguer::theme::Theme,
    prompt: String,
    saved: &str,
) -> Result<String> {
    loop {
        let mut input = Input::with_theme(theme).with_prompt(prompt.clone());
        if !saved.is_empty() {
            input = input.default(saved.to_string());
        }
        let raw: String = input.interact_text()?;
        let call = raw.trim().to_ascii_uppercase();
        match Callsign::parse(&call) {
            Ok(_) => return Ok(call),
            Err(e) => {
                let msg = e
                    .to_string()
                    .strip_prefix("protocol: ")
                    .map(str::to_string)
                    .unwrap_or_else(|| e.to_string());
                println!("  {}", ui_style::warn().apply_to(msg));
            }
        }
    }
}

fn pick_serial_port(
    theme: &dyn dialoguer::theme::Theme,
    prompt: &str,
    guess: Option<String>,
) -> Result<String> {
    let ports = crate::discover::serial_ports();
    if ports.is_empty() {
        return Input::with_theme(theme)
            .with_prompt(format!("{prompt} (COM5 or /dev/ttyUSB0)"))
            .interact_text()
            .map_err(Into::into);
    }
    let mut items: Vec<String> = ports.iter().map(|p| p.label.clone()).collect();
    items.push("(type a port manually)".into());
    let default = guess
        .and_then(|name| {
            ports
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&name))
        })
        .unwrap_or(0)
        .min(items.len().saturating_sub(1));
    let idx = Select::with_theme(theme)
        .with_prompt(prompt)
        .items(&items)
        .default(default)
        .interact()?;
    if idx >= ports.len() {
        Input::with_theme(theme)
            .with_prompt("Serial port name")
            .interact_text()
            .map_err(Into::into)
    } else {
        Ok(ports[idx].name.clone())
    }
}

/// What the audio picker starts on.
struct AudioDefaults<'a> {
    radio: Option<&'a Radio>,
    /// The Digirig path: start on the USB sound card.
    digirig: bool,
    /// The value in the file being replaced, when setup runs again.
    current: Option<&'a str>,
    /// Picking the capture (audio in) device.
    capture: bool,
}

fn pick_audio_device(
    theme: &dyn dialoguer::theme::Theme,
    step: u8,
    total_steps: u8,
    prompt: &str,
    devs: Vec<String>,
    want: AudioDefaults,
) -> Result<String> {
    let cards = alsa_cards();
    let mut devs = compact_alsa_list(devs);
    // A card the running station holds drops out of the list, so add it
    // back by name: otherwise Enter picks the headphones (QA #222).
    let busy = busy_cards(&devs, &cards, want.capture);
    devs.extend(busy.iter().cloned());
    if !busy.is_empty() {
        println!(
            "  {}",
            ui_style::warn().apply_to(
                "A sound card is in use, probably by the running station. It is listed; stop the station before testing it (sudo systemctl stop wcr-web)."
            )
        );
    }
    if devs.is_empty() {
        let audio: String = Input::with_theme(theme)
            .with_prompt(ui_style::step(
                step,
                total_steps,
                &format!("{prompt} (Enter = system default)"),
            ))
            .default(want.current.unwrap_or("").to_string())
            .allow_empty(true)
            .interact_text()?;
        return Ok(audio.trim().to_string());
    }
    // Setup run again: the card in the file. Else on a Pi the system
    // default is the headphone jack, which cannot capture, so the Digirig
    // path starts on the USB sound card (QA #220).
    let current = want.current.filter(|c| !c.is_empty()).map(str::to_string);
    let guess = current
        .filter(|c| devs.contains(c))
        .or_else(|| want.radio.and_then(|r| r.pick_audio(&devs)))
        .or_else(|| {
            want.digirig
                .then(|| crate::discover::find_digirig_audio(&devs))
                .flatten()
        });
    let names: Vec<(String, String)> = cards
        .iter()
        .map(|c| (c.id.clone(), c.long.clone()))
        .collect();
    let mut items = vec!["(system default)".to_string()];
    items.extend(devs.iter().map(|d| {
        let label = with_card_name(d, &names);
        if busy.contains(d) {
            format!("{label}  — in use")
        } else {
            label
        }
    }));
    let default = guess
        .and_then(|g| devs.iter().position(|d| *d == g))
        .map_or(0, |i| i + 1);
    let idx = Select::with_theme(theme)
        .with_prompt(ui_style::step(step, total_steps, prompt))
        .items(&items)
        .default(default)
        .interact()?;
    if idx == 0 {
        Ok(String::new())
    } else {
        Ok(devs[idx - 1].clone())
    }
}

/// The setup path and supported-radio index a saved config was made with
/// (the wizard's step 3), so setup run again starts on the same choice.
fn saved_path(cfg: &Config) -> (usize, Option<usize>) {
    let radio = crate::radios::from_config(cfg).and_then(|r| RADIOS.iter().position(|x| x == r));
    let path = if cfg.mode == Mode::Internet {
        0
    } else if let Some(i) = radio {
        RADIOS[i].setup_path()
    } else if cfg.modem.is_bluetooth() {
        4
    } else if cfg.modem.is_bt_audio() {
        BT_AUDIO
    } else if cfg.modem.uses_tnc() {
        5
    } else {
        match cfg.modem.ptt.as_str() {
            "digirig" => 1,
            "vox" => 2,
            "rigctl" => 3,
            _ => 0,
        }
    };
    (path, radio)
}

/// Setup run again starts from the saved file, so every value it does not
/// ask about (TX level, RX gain, busy test, the web page, mail…) stays as
/// it is (QA #284). A different radio than before gets fresh modem settings:
/// a TX level tuned for a Digirig is wrong for a Bluetooth radio.
fn start_from(previous: Option<&Config>, same_radio: bool) -> Config {
    let Some(prev) = previous else {
        return Config::default();
    };
    let mut cfg = prev.clone();
    if !same_radio {
        cfg.modem = config::ModemConfig::default();
        cfg.rig = config::RigConfig::default();
    }
    cfg
}

/// Bluetooth audio radios: the wizard and the app say why no sound card is
/// picked (QA #284: a Mac kept "Built-in Microphone" in the file).
pub fn bt_audio_sound_note() -> &'static str {
    "Sound goes over wcr's own Bluetooth link to the radio, not a sound card: no sound device is saved, and the Mac's speaker and microphone stay out of it."
}

/// Copy the settings the wizard never asks about from the file it replaces,
/// so running setup again on a gateway keeps the web page password, mail,
/// store and the rest (QA #222).
fn keep_unasked(cfg: &mut Config, prev: &Config) {
    cfg.web = prev.web.clone();
    cfg.irc = prev.irc.clone();
    cfg.store = prev.store.clone();
    cfg.group = prev.group.clone();
    cfg.lan = prev.lan.clone();
    cfg.ui = prev.ui.clone();
    cfg.status = prev.status.clone();
    cfg.relay = prev.relay.clone();
    cfg.mail = prev.mail.clone();
    cfg.gps = prev.gps.clone();
    cfg.gateway.rf_egress = prev.gateway.rf_egress;
    cfg.gateway.third_party = prev.gateway.third_party.clone();
    cfg.gateway.mode = prev.gateway.mode.clone();
}

/// ALSA lists every alias of every card (hw, plughw, default, sysdefault,
/// front, surround40, iec958, dmix…): 13 outputs on a Pi with no hint which
/// to pick (QA #220). Keep one `plughw:` entry per card (it converts the
/// sample rate for modem73) and the plain names (`default`, `pulse`,
/// `pipewire`). Other systems' lists, or ALSA lists with no `plughw:`, stay
/// as they are.
fn compact_alsa_list(devs: Vec<String>) -> Vec<String> {
    if !devs.iter().any(|d| d.starts_with("plughw:")) {
        return devs;
    }
    devs.into_iter()
        .filter(|d| d.starts_with("plughw:") || !d.contains(':'))
        .collect()
}

/// `plughw:CARD=Device,DEV=0  (USB PnP Sound Device)`: the ALSA name plus
/// the card's long name, when known.
fn with_card_name(dev: &str, cards: &[(String, String)]) -> String {
    let id = dev
        .split("CARD=")
        .nth(1)
        .map(|rest| rest.split(',').next().unwrap_or(rest));
    match id.and_then(|id| cards.iter().find(|(c, _)| c == id)) {
        Some((_, long)) => format!("{dev}  ({long})"),
        None => dev.to_string(),
    }
}

/// One line of /proc/asound/cards.
#[derive(Debug, Clone, PartialEq)]
struct AlsaCard {
    index: u32,
    id: String,
    long: String,
}

/// The sound cards from /proc/asound/cards (Linux only; empty elsewhere).
fn alsa_cards() -> Vec<AlsaCard> {
    std::fs::read_to_string("/proc/asound/cards")
        .map(|t| parse_asound_cards(&t))
        .unwrap_or_default()
}

/// ` 2 [Device         ]: USB-Audio - USB PnP Sound Device` → index 2, id
/// `Device`, long name `USB PnP Sound Device`.
fn parse_asound_cards(text: &str) -> Vec<AlsaCard> {
    text.lines()
        .filter_map(|line| {
            let (index, rest) = line.split_once('[')?;
            let index = index.trim().parse().ok()?;
            let (id, rest) = rest.split_once(']')?;
            let (_, long) = rest.split_once(" - ")?;
            Some(AlsaCard {
                index,
                id: id.trim().to_string(),
                long: long.trim().to_string(),
            })
        })
        .collect()
}

/// `plughw:` names for cards that can capture (or play) but are missing
/// from the device list: another program has them open.
fn busy_cards(devs: &[String], cards: &[AlsaCard], capture: bool) -> Vec<String> {
    let has_pcm = |c: &AlsaCard| {
        let dir = if capture { "pcm0c" } else { "pcm0p" };
        Path::new(&format!("/proc/asound/card{}/{dir}", c.index)).exists()
    };
    missing_cards(devs, cards, has_pcm)
}

fn missing_cards(
    devs: &[String],
    cards: &[AlsaCard],
    has_pcm: impl Fn(&AlsaCard) -> bool,
) -> Vec<String> {
    cards
        .iter()
        .filter(|c| {
            let tag = format!("CARD={},", c.id);
            !devs.iter().any(|d| d.contains(&tag)) && has_pcm(c)
        })
        .map(|c| format!("plughw:CARD={},DEV=0", c.id))
        .collect()
}

fn pick_rigctld(theme: &dyn dialoguer::theme::Theme, saved: Option<&str>) -> Result<String> {
    let typed_default = saved
        .filter(|s| !s.is_empty())
        .unwrap_or("127.0.0.1:4532")
        .to_string();
    let live = crate::discover::probe_rigctld_hosts();
    if live.is_empty() {
        println!(
            "  {}",
            ui_style::dim().apply_to("No rigctld seen on common ports — start Hamlib first.")
        );
        return Input::with_theme(theme)
            .with_prompt("rigctld address")
            .default(typed_default)
            .interact_text()
            .map_err(Into::into);
    }
    println!(
        "  {}",
        ui_style::dim().apply_to(format!("Found rigctld: {}", live.join(", ")))
    );
    let mut items = live;
    items.push("(type address manually)".into());
    let idx = Select::with_theme(theme)
        .with_prompt("rigctld address")
        .items(&items)
        .default(items.iter().position(|i| *i == typed_default).unwrap_or(0))
        .interact()?;
    if idx == items.len() - 1 {
        Input::with_theme(theme)
            .with_prompt("rigctld host:port")
            .default(typed_default)
            .interact_text()
            .map_err(Into::into)
    } else {
        Ok(items[idx].clone())
    }
}

/// Find a paired Benshi radio, or scan and pair one; fall back to a typed address.
fn bluetooth_pick(theme: &dyn dialoguer::theme::Theme, cfg: &mut Config) -> Result<()> {
    use crate::tnc::bluetooth::{self, FindOutcome};
    println!(
        "  {}",
        ui_style::dim().apply_to("Looking for a paired radio, then scanning for ~8 s…")
    );
    let _ = std::io::stdout().flush();
    let outcome = bluetooth::find_radio(&cfg.tnc.bt_name.clone());
    let dev = match outcome {
        FindOutcome::Ready(d) => {
            println!("  Found {} (paired).", d.label());
            Some(d)
        }
        FindOutcome::Paired(d) => {
            println!("  Paired with {}.", d.label());
            Some(d)
        }
        FindOutcome::PairFailed(d, why) => {
            println!("  Found {} but pairing failed: {why}", d.label());
            println!("  Pair it in your Bluetooth settings (PIN 0000) and run `wcr setup` again.");
            Some(d)
        }
        FindOutcome::NotFound { paired_others } => {
            if paired_others.is_empty() {
                println!("  No radio seen. Is it in Pairing mode with Bluetooth on?");
            } else {
                println!(
                    "  No VR-N76 / UV-PRO / GA-5WB seen. Paired devices: {}",
                    paired_others
                        .iter()
                        .map(|d| d.short_name())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            None
        }
        FindOutcome::Unsupported(why) => {
            println!("  {why}");
            None
        }
    };
    match dev {
        Some(d) => {
            cfg.tnc.bt_name = d.short_name();
            cfg.tnc.bt_addr = d.addr_str();
        }
        None => {
            let addr: String = Input::with_theme(theme)
                .with_prompt(
                    "Radio Bluetooth address (38:D2:00:xx:xx:xx), or Enter to search by name later",
                )
                .allow_empty(true)
                .interact_text()?;
            if let Some(a) = bluetooth::parse_addr(&addr) {
                cfg.tnc.bt_addr = bluetooth::format_addr(a);
            }
            let name: String = Input::with_theme(theme)
                .with_prompt("Radio Bluetooth name")
                .default(cfg.tnc.bt_name.clone())
                .interact_text()?;
            cfg.tnc.bt_name = name.trim().to_string();
        }
    }
    println!("  The node connects to the radio itself; no COM port needed.");
    Ok(())
}

fn preset_index(offered: &[Preset], preset: &str) -> usize {
    offered
        .iter()
        .position(|p| p.as_str() == preset)
        .unwrap_or(0)
}

pub fn calling_card() -> String {
    crate::band::calling_card_text()
}

/// Internet-only setup has no audio step. The service prompt stays 7/7.
pub fn skipped_radio_step_note() -> &'static str {
    "Step 6 skipped (radio only)."
}

/// Enter on an empty field keeps the detected square when the operator
/// accepted it. A typed square wins either way. Declining and pressing
/// Enter stores nothing.
pub fn grid_from_setup(accepted_detected: bool, detected: &str, typed: &str) -> String {
    let typed = typed.trim();
    if typed.is_empty() && accepted_detected {
        detected.trim().to_ascii_uppercase()
    } else {
        typed.to_ascii_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #280: a first Pi install is finished by setup; a working gateway
    /// (password set, boot service in place) gets no new questions.
    #[test]
    fn headless_setup_only_offers_what_is_missing() {
        assert_eq!(
            headless_steps(false, false),
            HeadlessSteps {
                password: true,
                boot_service: true
            }
        );
        assert_eq!(
            headless_steps(true, false),
            HeadlessSteps {
                password: false,
                boot_service: true
            }
        );
        assert_eq!(
            headless_steps(true, true),
            HeadlessSteps {
                password: false,
                boot_service: false
            }
        );
    }

    /// /proc/asound/cards on the QA Pi 3 with a Digirig.
    const PI_CARDS: &str = " 0 [Headphones     ]: bcm2835_headpho - bcm2835 Headphones\n                      bcm2835 Headphones\n 2 [Device         ]: USB-Audio - USB PnP Sound Device\n                      C-Media Electronics Inc. USB PnP Sound Device at usb-3f980000.usb-1.3, full speed\n";

    #[test]
    fn a_card_the_station_holds_is_still_offered() {
        let cards = parse_asound_cards(PI_CARDS);
        assert_eq!(cards[1].index, 2);
        assert_eq!(cards[1].id, "Device");
        // QA #222: modem73 holds the Digirig, so capture lists nothing and
        // playback only the headphones. The headphones cannot capture.
        let capture = |c: &AlsaCard| c.id == "Device";
        assert_eq!(
            missing_cards(&[], &cards, capture),
            vec!["plughw:CARD=Device,DEV=0"]
        );
        let outs = vec!["default".to_string(), "plughw:CARD=Headphones,DEV=0".into()];
        assert_eq!(
            missing_cards(&outs, &cards, |_| true),
            vec!["plughw:CARD=Device,DEV=0"]
        );
        // Free card: listed already, nothing added.
        let both = vec![
            "plughw:CARD=Headphones,DEV=0".to_string(),
            "plughw:CARD=Device,DEV=0".into(),
        ];
        assert!(missing_cards(&both, &cards, |_| true).is_empty());
        assert_eq!(
            crate::discover::find_digirig_audio(&missing_cards(&[], &cards, capture)).as_deref(),
            Some("plughw:CARD=Device,DEV=0")
        );
    }

    #[test]
    fn setup_again_keeps_the_web_password_and_mail() {
        let mut prev = Config::default();
        prev.web.password_hash = "$argon2id$v=19$x".into();
        prev.web.bind = "0.0.0.0:9443".into();
        prev.web.allow_public = true;
        prev.web.cert = "/etc/c.pem".into();
        prev.web.key = "/etc/k.pem".into();
        prev.mail.gateway = "G4GW".into();
        prev.gateway.third_party = "allow".into();
        prev.modem.audio_input = "plughw:CARD=Device,DEV=0".into();
        let mut cfg = Config::default();
        keep_unasked(&mut cfg, &prev);
        assert_eq!(cfg.web.password_hash, prev.web.password_hash);
        assert_eq!(cfg.web.bind, "0.0.0.0:9443");
        assert!(cfg.web.allow_public);
        assert_eq!(cfg.web.cert, "/etc/c.pem");
        assert_eq!(cfg.web.key, "/etc/k.pem");
        assert_eq!(cfg.mail.gateway, "G4GW");
        assert!(cfg.gateway.third_party_allow());
        // Asked again in the wizard, so not copied here.
        assert_eq!(cfg.modem.audio_input, "");
    }

    /// QA #284: setup run again on a working gateway, Enter through every
    /// question, left tx_level_db = -36 and the saved grid alone.
    #[test]
    fn setup_again_on_the_same_radio_keeps_every_modem_setting() {
        let mut prev = Config {
            callsign: "M7TJF".into(),
            grid: "IO81UF".into(),
            ..Config::default()
        };
        prev.modem.ptt = "digirig".into();
        prev.modem.com_port = "/dev/ttyUSB0".into();
        prev.modem.tx_level_db = -36.0;
        prev.modem.rx_gain_db = Some(-6.0);
        prev.modem.csma_mode = "level".into();
        prev.modem.audio_input = "plughw:CARD=Device,DEV=0".into();
        prev.web.password_hash = "$argon2id$v=19$x".into();
        assert_eq!(saved_path(&prev), (1, None));
        let toml_of = |c: &Config| toml::to_string(c).unwrap();
        let same = start_from(Some(&prev), true);
        assert_eq!(toml_of(&same), toml_of(&prev));
        // Another radio: the levels start over, the rest stays.
        let other = start_from(Some(&prev), false);
        assert_eq!(other.modem.tx_level_db, 0.0);
        assert_eq!(other.modem.rx_gain_db, None);
        assert_eq!(other.modem.com_port, "");
        assert_eq!(other.web.password_hash, prev.web.password_hash);
        assert_eq!(other.grid, "IO81UF");
        assert_eq!(
            toml_of(&start_from(None, false)),
            toml_of(&Config::default())
        );
        // A supported radio comes back as its own list entry.
        let mut bt = Config::default();
        RADIOS
            .iter()
            .find(|r| r.name == "VR-N76 / UV-PRO / GA-5WB (Bluetooth audio, modem73)")
            .unwrap()
            .apply(&mut bt);
        let (path, radio) = saved_path(&bt);
        assert_eq!(path, BT_AUDIO);
        assert_eq!(RADIOS[radio.unwrap()].name, bt.modem.radio);
        let net = Config {
            mode: Mode::Internet,
            ..Config::default()
        };
        assert_eq!(saved_path(&net), (0, None));
    }

    #[test]
    fn enter_keeps_detected_grid_only_when_accepted() {
        assert_eq!(grid_from_setup(true, "IO81uf", ""), "IO81UF");
        assert_eq!(grid_from_setup(true, "IO81UF", "   "), "IO81UF");
        assert_eq!(grid_from_setup(true, "IO81UF", "jo01"), "JO01");
        assert_eq!(grid_from_setup(false, "IO81UF", ""), "");
        assert_eq!(grid_from_setup(false, "IO81UF", "jn68"), "JN68");
    }

    #[test]
    fn audio_list_keeps_one_plughw_per_card_with_its_name() {
        // Pi 3 + Digirig outputs from QA #220.
        let devs: Vec<String> = [
            "default",
            "hw:CARD=Headphones,DEV=0",
            "plughw:CARD=Headphones,DEV=0",
            "default:CARD=Headphones",
            "sysdefault:CARD=Headphones",
            "dmix:CARD=Headphones,DEV=0",
            "hw:CARD=Device,DEV=0",
            "plughw:CARD=Device,DEV=0",
            "front:CARD=Device,DEV=0",
            "surround40:CARD=Device,DEV=0",
            "iec958:CARD=Device,DEV=0",
        ]
        .map(String::from)
        .to_vec();
        let short = compact_alsa_list(devs);
        assert_eq!(
            short,
            vec![
                "default",
                "plughw:CARD=Headphones,DEV=0",
                "plughw:CARD=Device,DEV=0"
            ]
        );
        let cards: Vec<(String, String)> = parse_asound_cards(PI_CARDS)
            .into_iter()
            .map(|c| (c.id, c.long))
            .collect();
        assert_eq!(
            with_card_name("plughw:CARD=Device,DEV=0", &cards),
            "plughw:CARD=Device,DEV=0  (USB PnP Sound Device)"
        );
        assert_eq!(with_card_name("default", &cards), "default");
        // macOS and Windows names are left alone.
        let mac = vec![
            "MacBook Pro Microphone".to_string(),
            "USB PnP Sound Device".into(),
        ];
        assert_eq!(compact_alsa_list(mac.clone()), mac);
    }

    #[test]
    fn internet_only_names_the_skipped_radio_step() {
        assert_eq!(skipped_radio_step_note(), "Step 6 skipped (radio only).");
    }
}
