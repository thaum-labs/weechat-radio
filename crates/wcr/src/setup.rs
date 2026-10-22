//! SPDX-License-Identifier: Apache-2.0
//! First-run wizard: callsign, grid, radio path, PTT, preset.

use crate::config::{self, Config};
use crate::error::Result;
use crate::modes::Mode;
use crate::presets::Preset;
use crate::proto::Callsign;
use crate::ui_style;
use dialoguer::{Confirm, Input, Select};
use std::io::Write;

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
                "Detected {label} (shown on the public map if you enable telemetry)"
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

    let paths = [
        "Internet only (no radio yet)",
        "Handheld + Digirig (VHF/UHF FM)",
        "Any radio with a plain audio cable (VOX)",
        "HF rig with CAT (rigctl)",
        "VR-N76 / UV-PRO / GA-5WB over Bluetooth (built-in KISS TNC)",
        "KISS TNC on a serial port (Mobilinkd, rfcomm0, Bluetooth COM)",
    ];
    let path = Select::with_theme(&theme)
        .with_prompt(ui_style::step(
            3,
            total_steps,
            "How will you get on the air?",
        ))
        .items(&paths)
        .default(0)
        .interact()?;

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
            cfg.modem.com_port = pick_serial_port(&theme, "Digirig serial port", true)?;
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
        4 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.backend = "bluetooth".into();
            cfg.modem.manage = false;
            cfg.modem.ptt = "tnc".into();
            cfg.modem.preset = Preset::Afsk1200.as_str().into();
            println!("  On the radio: General Settings → KISS TNC → Enable, Digital Mode off,");
            println!(
                "  then Menu → Pairing. Close the HT phone app (one Bluetooth client at a time)."
            );
            let _ = std::io::stdout().flush();
            bluetooth_pick(&theme, &mut cfg)?;
        }
        5 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.backend = "serial".into();
            cfg.modem.manage = false;
            cfg.modem.ptt = "tnc".into();
            cfg.modem.preset = Preset::Afsk1200.as_str().into();
            cfg.tnc.serial = pick_serial_port(&theme, "TNC serial port", false)?;
        }
        _ => {}
    }

    if path != 0 && !cfg.modem.uses_tnc() {
        let presets: Vec<&str> = Preset::all().iter().map(|p| p.as_str()).collect();
        let idx = Select::with_theme(&theme)
            .with_prompt(ui_style::step(
                4,
                total_steps,
                "Modem preset (you can change this later)",
            ))
            .items(&presets)
            .default(match path {
                1 => 0,
                2 => 4,
                3 => 1,
                _ => 0,
            })
            .interact()?;
        cfg.modem.preset = Preset::all()[idx].as_str().into();
        let default_mhz = if path == 3 { "7.045" } else { "144.950" };
        let freq: String = Input::with_theme(&theme)
            .with_prompt(if path == 3 {
                "Frequency on the dial (MHz) — CAT will override when the rig is connected"
            } else {
                "Frequency your radio is on (MHz)"
            })
            .default(default_mhz.into())
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
        cfg.modem.audio_input = pick_audio_device(
            &theme,
            5,
            total_steps,
            "Audio in (decode — radio speaker / interface capture)",
            crate::discover::list_audio_inputs(),
        )?;
        cfg.modem.audio_output = pick_audio_device(
            &theme,
            6,
            total_steps,
            "Audio out (transmit — radio mic / interface playback)",
            crate::discover::list_audio_outputs(),
        )?;
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
    println!("  Next:  wcr gui   or   wcr tui");
    println!("  With WeeChat:  wcr node   then   wcr weechat");
    println!("  How you know it worked: the status bar shows your callsign and mode.");
    ui_style::rule();
    let _ = std::io::stdout().flush();
    Ok(cfg)
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
    prefer_digirig: bool,
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
    let default = if prefer_digirig {
        crate::discover::find_digirig(&ports).and_then(|name| {
            ports
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&name))
        })
    } else {
        None
    }
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
    let mut items = vec!["(system default)".to_string()];
    items.extend(devs);
    let idx = Select::with_theme(theme)
        .with_prompt(ui_style::step(step, total_steps, prompt))
        .items(&items)
        .default(0)
        .interact()?;
    if idx == 0 {
        Ok(String::new())
    } else {
        Ok(items[idx].clone())
    }
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
    let outcome = bluetooth::find_radio("");
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
                .default("VR-N76".into())
                .interact_text()?;
            cfg.tnc.bt_name = name.trim().to_string();
        }
    }
    println!("  The node connects to the radio itself; no COM port needed.");
    Ok(())
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
    fn internet_only_names_the_skipped_radio_step() {
        assert_eq!(skipped_radio_step_note(), "Step 6 skipped (radio only).");
    }
}
