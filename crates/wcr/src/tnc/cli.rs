//! SPDX-License-Identifier: Apache-2.0
//! `wcr tnc scan|find|test` — check the radio link without starting the node.

use crate::cli::TncAction;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::modem::kiss::{self, KissDecoder};
use crate::presets::Preset;
use crate::tnc::{ax25, bluetooth, link, serial};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

pub fn run(action: TncAction, cfg_path: &Path) -> Result<()> {
    match action {
        TncAction::Scan { inquiry } => scan(inquiry),
        TncAction::Find { name } => find(&name, cfg_path),
        TncAction::Test { seconds, tx } => test(cfg_path, seconds, tx),
    }
}

fn scan(inquiry: bool) -> Result<()> {
    if inquiry {
        println!("  Scanning for ~8 s. Put the radio in Pairing mode (Menu → Pairing).");
    }
    let devices = if inquiry {
        bluetooth::discover(8)
    } else {
        bluetooth::paired_devices()
    }
    .map_err(|e| Error::Msg(e.to_string()))?;
    if devices.is_empty() {
        println!("  No Bluetooth devices found.");
        return Ok(());
    }
    for d in devices {
        let tag = if d.is_known_radio() {
            "RADIO "
        } else {
            "      "
        };
        println!(
            "  {tag}{:<20} {}  {}{}",
            d.short_name(),
            d.addr_str(),
            if d.paired { "paired" } else { "not paired" },
            if d.connected { ", connected" } else { "" }
        );
    }
    Ok(())
}

fn find(name: &str, cfg_path: &Path) -> Result<()> {
    use bluetooth::FindOutcome;
    println!("  Looking for a paired radio, then scanning for ~8 s…");
    let dev = match bluetooth::find_radio(name) {
        FindOutcome::Ready(d) => {
            println!("  {} is paired and ready.", d.label());
            d
        }
        FindOutcome::Paired(d) => {
            println!("  Paired with {}.", d.label());
            d
        }
        FindOutcome::PairFailed(d, why) => {
            return Err(Error::Msg(format!(
                "found {} but pairing failed: {why}. Pair it in Bluetooth settings (PIN 0000) and try again.",
                d.label()
            )));
        }
        FindOutcome::NotFound { paired_others } => {
            let others = paired_others
                .iter()
                .map(|d| d.short_name())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Error::Msg(format!(
                "no VR-N76 / UV-PRO / GA-5WB seen. Radio on, Bluetooth on, Pairing mode? Paired devices: {}",
                if others.is_empty() { "none".into() } else { others }
            )));
        }
        FindOutcome::Unsupported(why) => return Err(Error::Msg(why)),
    };
    let mut cfg = if cfg_path.exists() {
        Config::load(cfg_path)?
    } else {
        Config::default()
    };
    cfg.modem.backend = "bluetooth".into();
    cfg.modem.manage = false;
    cfg.modem.ptt = "tnc".into();
    cfg.modem.preset = Preset::Afsk1200.as_str().into();
    cfg.tnc.bt_name = dev.short_name();
    cfg.tnc.bt_addr = dev.addr_str();
    if cfg.mode == crate::modes::Mode::Internet {
        cfg.mode = crate::modes::Mode::InternetRadio;
    }
    crate::config::ensure_dirs()?;
    cfg.save(cfg_path)?;
    println!(
        "  Saved to {}. Start the station and watch RADIO in the status panel.",
        cfg_path.display()
    );
    println!(
        "  On the radio: General Settings → KISS TNC → Enable, Digital Mode off, HT app closed."
    );
    Ok(())
}

fn test(cfg_path: &Path, seconds: u64, tx: bool) -> Result<()> {
    let mut cfg = if cfg_path.exists() {
        Config::load(cfg_path)?
    } else {
        Config::default()
    };
    if !cfg.modem.uses_tnc() {
        println!(
            "  modem.backend is {}; testing as bluetooth.",
            cfg.modem.backend
        );
        cfg.modem.backend = "bluetooth".into();
    }
    let conn = if cfg.modem.is_bluetooth() {
        let dev = bluetooth::resolve(&cfg.tnc).map_err(Error::Msg)?;
        println!("  Connecting to {}…", dev.label());
        bluetooth::connect(&dev).map_err(|e| {
            Error::Msg(format!(
                "{}: {e}. Radio on, KISS TNC enabled, HT app closed?",
                dev.short_name()
            ))
        })?
    } else {
        println!("  Opening {}…", cfg.tnc.serial);
        serial::open(&cfg.tnc.serial).map_err(|e| Error::Msg(format!("{}: {e}", cfg.tnc.serial)))?
    };
    let link::Connection {
        mut reader,
        mut writer,
        zero_is_eof,
        label,
    } = conn;
    println!("  {label} linked. The radio should show its Bluetooth-data icon.");

    let txdelay = (cfg.tnc.txdelay_ms / 10).clamp(1, 255) as u8;
    let mut params = kiss::encode_param(kiss::CMD_TXDELAY, txdelay);
    params.extend(kiss::encode_param(kiss::CMD_PERSIST, cfg.tnc.persist));
    writer.write_all(&params)?;
    writer.flush()?;

    if tx {
        if cfg.callsign.is_empty() {
            return Err(Error::config(
                "set a callsign first (wcr setup) before transmitting",
            ));
        }
        let src = ax25::Address::from_station(&cfg.callsign);
        let dest = ax25::Address::from_station(&cfg.tnc.ax25_dest);
        let info = format!("WeeChat Radio link test de {}", cfg.callsign);
        let frame = ax25::wrap_ui(&src, &dest, info.as_bytes());
        writer.write_all(&kiss::encode_frame(&frame))?;
        writer.flush()?;
        println!("  TX  {}>{}: {info}", src, dest);
    }

    println!("  Listening for {seconds} s (Ctrl-C to stop)…");
    let mut decoder = KissDecoder::new();
    let mut buf = [0u8; 1024];
    let end = Instant::now() + Duration::from_secs(seconds);
    let mut frames = 0usize;
    while Instant::now() < end {
        match reader.read(&mut buf) {
            Ok(0) if zero_is_eof => {
                println!("  Link closed by the radio.");
                break;
            }
            Ok(0) => continue,
            Ok(n) => {
                for f in decoder.push(&buf[..n]) {
                    frames += 1;
                    match ax25::unwrap_ui(&f) {
                        Some(ui) => {
                            let path = if ui.digis.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    ",{}",
                                    ui.digis
                                        .iter()
                                        .map(|d| d.to_string())
                                        .collect::<Vec<_>>()
                                        .join(",")
                                )
                            };
                            let wcr = crate::proto::Envelope::decode(ui.info).is_ok();
                            println!(
                                "  RX  {}>{}{path}: {} ({} bytes{})",
                                ui.src,
                                ui.dest,
                                printable(ui.info),
                                ui.info.len(),
                                if wcr { ", WeeChat Radio envelope" } else { "" }
                            );
                        }
                        None => println!("  RX  raw {} bytes: {}", f.len(), printable(&f)),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => {
                println!("  Link error: {e}");
                break;
            }
        }
    }
    println!(
        "  Done: {frames} frame{} heard.",
        if frames == 1 { "" } else { "s" }
    );
    Ok(())
}

fn printable(b: &[u8]) -> String {
    let s: String = b
        .iter()
        .take(80)
        .map(|&c| {
            if (0x20..0x7F).contains(&c) {
                c as char
            } else {
                '·'
            }
        })
        .collect();
    if b.len() > 80 {
        format!("{s}…")
    } else {
        s
    }
}
