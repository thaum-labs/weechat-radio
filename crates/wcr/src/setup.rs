//! SPDX-License-Identifier: Apache-2.0
//! First-run wizard: callsign, grid, radio path, PTT, preset.

use crate::config::{self, Config};
use crate::error::Result;
use crate::modes::Mode;
use crate::presets::Preset;
use crate::proto::Callsign;
use dialoguer::{Confirm, Input, Select};
use std::io::Write;

pub fn run_wizard() -> Result<Config> {
    println!();
    println!("  WeeChat Radio  —  5-minute setup");
    println!("  --------------------------------");
    println!("  You will need: your callsign, and (if you have a radio) a cable.");
    println!();

    let call: String = Input::new()
        .with_prompt("Callsign (guests: ~NICK)")
        .validate_with(|s: &String| Callsign::parse(s).map(|_| ()).map_err(|e| e.to_string()))
        .interact_text()?;
    let grid: String = Input::new()
        .with_prompt("Maidenhead grid square (e.g. IO91wm)")
        .allow_empty(true)
        .interact_text()?;
    if !grid.is_empty() {
        crate::grid::normalize(&grid)?;
    }

    let paths = [
        "Internet only (no radio yet)",
        "Handheld + Digirig (VHF/UHF FM)",
        "Any radio with a plain audio cable (VOX)",
        "HF rig with CAT (rigctl)",
    ];
    let path = Select::new()
        .with_prompt("How will you get on the air?")
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
            let com: String = Input::new()
                .with_prompt("Digirig serial port (COM5 or /dev/ttyUSB0)")
                .interact_text()?;
            cfg.modem.com_port = com;
            cfg.modem.com_line = "rts".into();
            println!("  Digirig: PTT on RTS. Plug the audio jacks into the radio as in the guide.");
            println!("  modem73 is bundled with the installer; wcr starts it for you.");
        }
        2 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "vox".into();
            cfg.modem.preset = Preset::VoxSafe.as_str().into();
            println!("  VOX adds a short delay before the radio keys. We pad the start of each");
            println!("  transmission so the first symbols are not clipped.");
            println!("  modem73 is bundled with the installer; wcr starts it for you.");
        }
        3 => {
            cfg.mode = Mode::InternetRadio;
            cfg.modem.ptt = "rigctl".into();
            cfg.modem.preset = Preset::HfGood.as_str().into();
            let host: String = Input::new()
                .with_prompt("rigctld address")
                .default("127.0.0.1:4532".into())
                .interact_text()?;
            cfg.modem.rigctl = host;
            cfg.rig.enabled = true;
            println!("  Start rigctld for your radio before `wcr node`.");
            println!("  If that fails, you can switch PTT to VOX later with /radio ptt vox");
            println!("  modem73 is bundled with the installer; wcr starts it for you.");
        }
        _ => {}
    }

    if path != 0 {
        let presets: Vec<&str> = Preset::all().iter().map(|p| p.as_str()).collect();
        let idx = Select::new()
            .with_prompt("Modem preset (you can change this later)")
            .items(&presets)
            .default(match path {
                1 => 0,
                2 => 4,
                3 => 1,
                _ => 0,
            })
            .interact()?;
        cfg.modem.preset = Preset::all()[idx].as_str().into();
    }

    println!();
    println!("  Telemetry: in Internet and Internet+Radio modes, this node uploads");
    println!("  metadata (callsign, grid, mode, SNR — never message text) to");
    println!("  weechatradio.com so the live map can show the network.");
    println!();

    if Confirm::new()
        .with_prompt("Install as a background service so relays keep running?")
        .default(false)
        .interact()?
    {
        println!("  After saving, run:  wcr service install");
    }

    let path = Config::default_path();
    config::ensure_dirs()?;
    cfg.save(&path)?;
    println!();
    println!("  Saved {}", path.display());
    println!("  Next:  wcr node     (or wcr tui)");
    println!("  How you know it worked: the status bar shows your callsign and mode.");
    let _ = std::io::stdout().flush();
    Ok(cfg)
}

pub fn calling_card() -> &'static str {
    r#"WeeChat Radio — suggested calling presets (verify your band plan)

UK  VHF  144.950 MHz FM   preset vhf-fm
EU  VHF  144.950 MHz FM   preset vhf-fm
US  VHF  145.530 MHz FM   preset vhf-fm
AU  VHF  146.550 MHz FM   preset vhf-fm
UK  HF   7.045 MHz USB    preset hf-poor
EU  HF   7.045 MHz USB    preset hf-poor
US  HF   7.090 MHz USB    preset hf-poor
AU  HF   7.090 MHz USB    preset hf-poor

The wizard can set frequency via rigctl only if you confirm.
"#
}
