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
        ensure::strip_quarantine(&binary);
        if let Some(parent) = binary.parent() {
            ensure::strip_quarantine(&parent.join("libs"));
        }
        if !cfg.callsign.trim().is_empty() {
            args.extend(["--callsign".into(), cfg.callsign.clone()]);
        }
        if !cfg.modem.audio_input.trim().is_empty() {
            args.extend(["--input-device".into(), cfg.modem.audio_input.clone()]);
        }
        if !cfg.modem.audio_output.trim().is_empty() {
            args.extend(["--output-device".into(), cfg.modem.audio_output.clone()]);
        }
        let log_path = crate::config::default_data_dir().join("modem73.log");
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let log = std::fs::File::create(&log_path)
            .map_err(|e| Error::Modem(format!("cannot write {}: {e}", log_path.display())))?;
        let err_log = log.try_clone().map_err(|e| Error::Modem(e.to_string()))?;
        let mut cmd = Command::new(&binary);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err_log))
            .kill_on_drop(true);
        if let Some(dir) = binary.parent() {
            cmd.current_dir(dir);
        }
        let child = cmd.spawn().map_err(|e| {
            Error::Modem(format!(
                "could not start {}: {e}. Re-run the installer or set modem.binary in wcr.toml.",
                binary.display()
            ))
        })?;
        tracing::info!("started {} (log {})", binary.display(), log_path.display());
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
