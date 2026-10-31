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

/// Setup path for a supported CAT radio ([`Radio::setup_path`]).
const SUPPORTED_CAT: usize = 6;
/// Setup path for modem73 over a radio's Bluetooth audio channel.
const BT_AUDIO: usize = 7;

pub fn run_wizard() -> Result<Config> {
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

    let total_steps = 7;
    let (grid_tx, grid_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = grid_tx.send(crate::grid::detect_from_ip());
    });

    let call = read_callsign_uppercase(
        &theme,
        ui_style::step(
            1,
            total_steps,
            "Callsign (guests: ~NICK, max 8 chars including ~)",
        ),
    )?;

    let guessed = grid_rx.recv().ok().flatten();
    let confirm_theme = ui_style::bracket_confirm_theme();
    let mut grid_prompt = Input::with_theme(&theme)
        .with_prompt(ui_style::step(
            2,
            total_steps,
            "Maidenhead grid square (Enter to accept, or type your own)",
        ))
        .allow_empty(true);
    let mut accepted_detected = false;
    if let Some(hit) = &guessed {
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
            Some(hit) => grid_from_setup(accepted_detected, &hit.grid, &typed),
            None => typed.trim().to_ascii_uppercase(),
        };
        match crate::grid::normalize(&grid) {
            Err(e) if !grid.is_empty() => println!("  {}", ui_style::dim().apply_to(e)),
            _ => break grid,
        }
    };

    // Supported radios first: picking one fills in everything but the port.
    let mut paths: Vec<String> = vec!["Internet only (no radio yet)".into()];
    paths.extend(RADIOS.iter().map(|r| r.name.to_string()));
    paths.push("Other setup (Digirig, VOX cable, any CAT rig, KISS TNC)".into());
    let picked = Select::with_theme(&theme)
        .with_prompt(ui_style::step(
            3,
            total_steps,
            "Which radio? (pick Other for VOX, Digirig and the rest)",
        ))
        .items(&paths)
        .default(0)
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
                .default(0)
                .interact()?;
            others[i].0
        }
    };

    let mut cfg = Config::default();
    cfg.callsign = call.trim().to_ascii_uppercase();
    cfg.grid = grid.trim().to_ascii_uppercase();

    match path {
        0 => {
            cfg.mode = Mode::Internet;
            cfg.modem.manage = false;
            cfg.modem.ptt = "none".into();
            let public_hub = Confirm::with_theme(&confirm_theme)
                .with_prompt(ui_style::step(
                    4,
                    total_steps,
                    "Use the public hub (wss://hub.weechatradio.com)?",
                ))
                .default(true)
                .interact()?;
            if !public_hub {
                let url: String = Input::with_theme(&theme)
                    .with_prompt("Hub WebSocket URL (wss://host/ws)")
                    .interact_text()?;
                cfg.hub.url = url;
            }
            let share_map = Confirm::with_theme(&confirm_theme)
                .with_prompt(ui_style::step(
                    5,
                    total_steps,
                    "Share callsign and grid on the public map? (never message text)",
                ))
                .default(false)
                .interact()?;
            if !share_map {
                cfg.telemetry.url.clear();
            }
        }
        1 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "digirig".into();
            cfg.modem.preset = Preset::VhfFm.as_str().into();
            let ports = crate::discover::serial_ports();
            let digirig = crate::discover::find_digirig(&ports);
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
            cfg.modem.rigctl = pick_rigctld(&theme)?;
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
            let guess = radio.pick_port(&ports);
            cfg.modem.hamlib_device = pick_serial_port(&theme, "Radio CAT port", guess)?;
        }
        5 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.backend = "serial".into();
            cfg.modem.manage = false;
            cfg.modem.ptt = "tnc".into();
            cfg.modem.preset = Preset::Afsk1200.as_str().into();
            cfg.tnc.serial = pick_serial_port(&theme, "TNC serial port", None)?;
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
                _ if radio.is_some() => preset_index(&offered, &cfg.modem.preset),
                1 => 0,
                2 => 4,
                3 => 1,
                _ => 0,
            })
            .interact()?;
        cfg.modem.preset = offered[idx].as_str().into();
        let cat = matches!(path, 3 | SUPPORTED_CAT);
        let default_mhz = match radio {
            Some(r) => format!("{:.3}", r.dial_khz as f64 / 1000.0),
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
            println!(
                "  {}",
                ui_style::dim().apply_to(format!(
                    "Stored {}. Change later with /radio freq",
                    crate::band::describe(khz)
                ))
            );
        }
        if !cfg.modem.is_bt_audio() {
            cfg.modem.audio_input = pick_audio_device(
                &theme,
                5,
                total_steps,
                "Audio in (decode — radio speaker / interface capture)",
                crate::discover::list_audio_inputs(),
                radio,
                path == 1,
            )?;
            cfg.modem.audio_output = pick_audio_device(
                &theme,
                6,
                total_steps,
                "Audio out (transmit — radio mic / interface playback)",
                crate::discover::list_audio_outputs(),
                radio,
                path == 1,
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

    if Confirm::with_theme(&confirm_theme)
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

    let path = Config::default_path();
    config::ensure_dirs()?;
    cfg.save(&path)?;
    ui_style::panel("SAVED", path.display().to_string().as_str());
    match crate::weechat_app::configure(&Config::default_path()) {
        Ok(msg) => {
            println!("  {msg}");
        }
        Err(e) => {
            println!("  WeeChat not configured yet: {e}");
            println!("  Re-run the official installer, or use `wcr tui`.");
        }
    }
    if headless() {
        // A Pi gateway over SSH: the browser page is the way in.
        println!("  Next (headless gateway):  wcr web password   then   sudo ~/.local/bin/wcr web install");
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

/// Linux with no desktop session (an SSH login on a Pi, for example).
fn headless() -> bool {
    cfg!(target_os = "linux")
        && std::env::var_os("DISPLAY").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_none()
}

fn read_callsign_uppercase(theme: &dyn dialoguer::theme::Theme, prompt: String) -> Result<String> {
    loop {
        let raw: String = Input::with_theme(theme)
            .with_prompt(prompt.clone())
            .interact_text()?;
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

fn pick_audio_device(
    theme: &dyn dialoguer::theme::Theme,
    step: u8,
    total_steps: u8,
    prompt: &str,
    devs: Vec<String>,
    radio: Option<&Radio>,
    digirig: bool,
) -> Result<String> {
    if devs.is_empty() {
        let audio: String = Input::with_theme(theme)
            .with_prompt(ui_style::step(
                step,
                total_steps,
                &format!("{prompt} (Enter = system default)"),
            ))
            .allow_empty(true)
            .interact_text()?;
        return Ok(audio.trim().to_string());
    }
    let devs = compact_alsa_list(devs);
    // On a Pi the system default is the headphone jack, which cannot
    // capture, so the Digirig path starts on the USB sound card (QA #220).
    let guess = radio.and_then(|r| r.pick_audio(&devs)).or_else(|| {
        digirig
            .then(|| crate::discover::find_digirig_audio(&devs))
            .flatten()
    });
    let cards = alsa_card_names();
    let mut items = vec!["(system default)".to_string()];
    items.extend(devs.iter().map(|d| with_card_name(d, &cards)));
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

/// Card id → long name from /proc/asound/cards (Linux only; empty elsewhere).
fn alsa_card_names() -> Vec<(String, String)> {
    std::fs::read_to_string("/proc/asound/cards")
        .map(|t| parse_asound_cards(&t))
        .unwrap_or_default()
}

/// ` 2 [Device         ]: USB-Audio - USB PnP Sound Device` → (`Device`,
/// `USB PnP Sound Device`).
fn parse_asound_cards(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once('[')?;
            let (id, rest) = rest.split_once(']')?;
            let (_, long) = rest.split_once(" - ")?;
            Some((id.trim().to_string(), long.trim().to_string()))
        })
        .collect()
}

fn pick_rigctld(theme: &dyn dialoguer::theme::Theme) -> Result<String> {
    let live = crate::discover::probe_rigctld_hosts();
    if live.is_empty() {
        println!(
            "  {}",
            ui_style::dim().apply_to("No rigctld seen on common ports — start Hamlib first.")
        );
        return Input::with_theme(theme)
            .with_prompt("rigctld address")
            .default("127.0.0.1:4532".into())
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
        .default(0)
        .interact()?;
    if idx == items.len() - 1 {
        Input::with_theme(theme)
            .with_prompt("rigctld host:port")
            .default("127.0.0.1:4532".into())
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
        let cards = parse_asound_cards(
            " 0 [Headphones     ]: bcm2835_headpho - bcm2835 Headphones\n                      bcm2835 Headphones\n 2 [Device         ]: USB-Audio - USB PnP Sound Device\n                      C-Media Electronics Inc. USB PnP Sound Device at usb-3f980000.usb-1.3, full speed\n",
        );
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
