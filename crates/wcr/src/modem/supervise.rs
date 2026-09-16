//! SPDX-License-Identifier: Apache-2.0
//! Optional supervision of `modem73 --headless`.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::modem::ensure;
use crate::presets::Preset;
use std::process::Stdio;
use tokio::process::{Child, Command};

pub struct ModemProcess {
    child: Child,
}

impl ModemProcess {
    pub async fn spawn(cfg: &Config) -> Result<Self> {
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
        let binary = ensure::ensure_binary(&cfg.modem.binary).await?;
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
