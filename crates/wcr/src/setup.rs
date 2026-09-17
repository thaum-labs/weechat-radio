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
    let theme = ui_style::prompt_theme();
    ui_style::panel("WEECHAT RADIO", "SETUP");
    println!(
        "  {}",
        ui_style::dim()
            .apply_to("You will need: your callsign, and (if you have a radio) a cable.")
    );
    println!();

    let (grid_tx, grid_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = grid_tx.send(crate::grid::detect_from_ip());
    });

    let call: String = Input::with_theme(&theme)
        .with_prompt(ui_style::step(1, 5, "Callsign (guests: ~NICK)"))
        .validate_with(|s: &String| Callsign::parse(s).map(|_| ()).map_err(|e| e.to_string()))
        .interact_text()?;

    let guessed = grid_rx.recv().ok().flatten();
    let mut grid_prompt = Input::with_theme(&theme)
        .with_prompt(ui_style::step(
            2,
            5,
            "Maidenhead grid square (Enter to accept, or type your own)",
        ))
        .allow_empty(true);
    if let Some(hit) = &guessed {
        let label = if hit.label.is_empty() {
            hit.grid.clone()
        } else {
            format!("{} · {}", hit.grid, hit.label)
        };
        println!(
            "  {}",
            ui_style::dim().apply_to(format!("Detected {label}"))
        );
        grid_prompt = grid_prompt.default(hit.grid.clone());
    } else {
        println!(
            "  {}",
            ui_style::dim()
                .apply_to("Could not detect reliably — leave empty or type e.g. IO81UF",)
        );
    }
    let grid: String = grid_prompt.interact_text()?;
    if !grid.is_empty() {
        crate::grid::normalize(&grid)?;
    }

    let paths = [
        "Internet only (no radio yet)",
        "Handheld + Digirig (VHF/UHF FM)",
        "Any radio with a plain audio cable (VOX)",
        "HF rig with CAT (rigctl)",
        "VR-N76 / UV-PRO / GA-5WB over Bluetooth (built-in KISS TNC)",
        "KISS TNC on a serial port (Mobilinkd, rfcomm0, Bluetooth COM)",
    ];
    let path = Select::with_theme(&theme)
        .with_prompt(ui_style::step(3, 5, "How will you get on the air?"))
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
        }
        1 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "digirig".into();
            cfg.modem.preset = Preset::VhfFm.as_str().into();
            let com: String = Input::with_theme(&theme)
                .with_prompt("Digirig serial port (COM5 or /dev/ttyUSB0)")
                .interact_text()?;
            cfg.modem.com_port = com;
            cfg.modem.com_line = "rts".into();
            println!("  Digirig: PTT on RTS. Plug the audio jacks into the radio as in the guide.");
        }
        2 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "vox".into();
            cfg.modem.preset = Preset::VoxSafe.as_str().into();
            println!("  VOX adds a short delay before the radio keys. We pad the start of each");
            println!("  transmission so the first symbols are not clipped.");
        }
        3 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "rigctl".into();
            cfg.modem.preset = Preset::HfGood.as_str().into();
            let host: String = Input::with_theme(&theme)
                .with_prompt("rigctld address")
                .default("127.0.0.1:4532".into())
                .interact_text()?;
            cfg.modem.rigctl = host;
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
            let port: String = Input::with_theme(&theme)
                .with_prompt("TNC serial port (COM7, /dev/rfcomm0, /dev/cu.VR-N76)")
                .interact_text()?;
            cfg.tnc.serial = port.trim().to_string();
        }
        _ => {}
    }

    if path != 0 && !cfg.modem.uses_tnc() {
        let presets: Vec<&str> = Preset::all().iter().map(|p| p.as_str()).collect();
        let idx = Select::with_theme(&theme)
            .with_prompt(ui_style::step(
                4,
                5,
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
    }

    println!();
    println!("  Telemetry: in Internet and Internet+Radio modes, this node uploads");
    println!("  metadata (callsign, grid, mode, SNR — never message text) to");
    println!("  weechatradio.com so the live map can show the network.");
    println!();

    if Confirm::with_theme(&theme)
        .with_prompt(ui_style::step(
            5,
            5,
            "Install as a background service so relays keep running?",
        ))
        .default(false)
        .interact()?
    {
        println!("  After saving, run:  wcr service install");
    }

    let path = Config::default_path();
    config::ensure_dirs()?;
    cfg.save(&path)?;
    ui_style::panel("SAVED", path.display().to_string().as_str());
    match crate::weechat_app::configure() {
        Ok(msg) => {
            println!("  {msg}");
        }
        Err(e) => {
            println!("  WeeChat not configured yet: {e}");
            println!("  Re-run the official installer, or use `wcr tui`.");
        }
    }
    println!("  Next:  wcr node     then  wcr weechat");
    println!("  Or use the built-in UI:  wcr tui");
    println!("  How you know it worked: the status bar shows your callsign and mode.");
    ui_style::rule();
    let _ = std::io::stdout().flush();
    Ok(cfg)
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
