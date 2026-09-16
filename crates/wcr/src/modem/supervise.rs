//! SPDX-License-Identifier: Apache-2.0
//! Optional supervision of `modem73 --headless`.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::presets::Preset;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::{Child, Command};

fn resolve_binary(configured: &str) -> PathBuf {
    let raw = PathBuf::from(configured);
    if raw.is_absolute() {
        return raw;
    }
    let name = if cfg!(windows) && raw.extension().is_none() {
        raw.with_extension("exe")
    } else {
        raw
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join(&name);
            if beside.is_file() {
                return beside;
            }
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::resolve_binary;
    use std::path::Path;

    #[test]
    fn keeps_absolute_paths() {
        let p = if cfg!(windows) {
            r"C:\tools\modem73.exe"
        } else {
            "/usr/local/bin/modem73"
        };
        assert_eq!(resolve_binary(p), Path::new(p));
    }
}

pub struct ModemProcess {
    child: Child,
}

impl ModemProcess {
    pub fn spawn(cfg: &Config) -> Result<Self> {
        let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm);
        let mut args = vec!["--headless".to_string()];
        args.extend(preset.modem73_args());
        match cfg.modem.ptt.as_str() {
            "vox" => {
                args.extend([
                    "--ptt".into(),
                    "vox".into(),
                    "--vox-lead".into(),
                    cfg.modem.vox_lead_ms.to_string(),
                    "--vox-tail".into(),
                    cfg.modem.vox_tail_ms.to_string(),
                ]);
            }
            "digirig" | "com" => {
                args.extend([
                    "--ptt".into(),
                    "com".into(),
                    "--com-port".into(),
                    cfg.modem.com_port.clone(),
                    "--com-line".into(),
                    cfg.modem.com_line.clone(),
                ]);
            }
            "cm108" => {
                args.extend([
                    "--ptt".into(),
                    "cm108".into(),
                    "--cm108-gpio".into(),
                    cfg.modem.cm108_gpio.to_string(),
                ]);
            }
            "rigctl" => {
                args.extend(["--rigctl".into(), cfg.modem.rigctl.clone()]);
            }
            _ => {}
        }
        let binary = resolve_binary(&cfg.modem.binary);
        let child = Command::new(&binary)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                Error::Modem(format!(
                    "could not start {}: {e}. Re-run the installer or set modem.binary in wcr.toml.",
                    binary.display()
                ))
            })?;
        Ok(Self { child })
    }

    pub async fn wait(&mut self) -> Result<()> {
        let status = self.child.wait().await?;
        if !status.success() {
            return Err(Error::Modem(format!("modem73 exited with {status}")));
        }
        Ok(())
    }
}
