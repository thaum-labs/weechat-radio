//! SPDX-License-Identifier: Apache-2.0
//! Optional supervision of `modem73 --headless`.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::modem::ensure;
use crate::presets::Preset;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use tokio::process::{Child, Command};

/// Dynamic linker failures (missing libncurses) show up well inside this window.
const STARTUP_GRACE: Duration = Duration::from_millis(400);

pub struct ModemProcess {
    child: Child,
    log_path: PathBuf,
}

/// Process id of the modem73 this station started, so a stop signal can end
/// it too (QA v0.1.104: stopping `wcr node` left modem73 running).
static MANAGED_PID: Mutex<Option<u32>> = Mutex::new(None);

/// modem73 takes A-Z, 0-9 and `/`, 1 to 9 characters, and exits on anything
/// else. An unlicensed `~QA1A` goes to it as `QA1A`. `None`: pass no callsign.
pub fn modem_callsign(call: &str) -> Option<String> {
    let clean: String = call
        .trim()
        .to_ascii_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '/')
        .take(9)
        .collect();
    (!clean.is_empty()).then_some(clean)
}

/// Sets `--csma-threshold` to `db`, replacing the preset's value.
fn set_csma_threshold(args: &mut Vec<String>, db: i16) {
    if let Some(i) = args.iter().position(|a| a == "--csma-threshold") {
        args.drain(i..(i + 2).min(args.len()));
    }
    args.extend(["--csma-threshold".into(), db.to_string()]);
}

/// Ends the modem73 this station started. For a stop signal, where the
/// station exits without dropping its [`ModemProcess`].
pub fn stop_managed() {
    let Some(pid) = MANAGED_PID.lock().ok().and_then(|mut p| p.take()) else {
        return;
    };
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", pid.to_string().as_str()])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for ModemProcess {
    fn drop(&mut self) {
        // kill_on_drop ends the child; forget its pid.
        if let Ok(mut p) = MANAGED_PID.lock() {
            if *p == self.child.id() {
                *p = None;
            }
        }
    }
}

impl ModemProcess {
    pub async fn spawn(cfg: &Config) -> Result<Self> {
        let preset = Preset::parse(&cfg.modem.preset).unwrap_or(Preset::VhfFm);
        let mut args = vec![
            "--headless".to_string(),
            "-p".into(),
            cfg.modem.kiss_port.to_string(),
            "--control-port".into(),
            cfg.modem.control_port.to_string(),
        ];
        args.extend(preset.modem73_args());
        if cfg.modem.is_vox() {
            set_csma_threshold(&mut args, Preset::VOX_CSMA_THRESHOLD_DB);
        }
        match cfg.modem.ptt.as_str() {
            "vox" => {
                args.extend([
                    "--ptt".into(),
                    "vox".into(),
                    "--vox-lead".into(),
                    preset.vox_lead_ms(cfg.modem.vox_lead_ms).to_string(),
                    "--vox-tail".into(),
                    preset.vox_tail_ms(cfg.modem.vox_tail_ms).to_string(),
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
        // Extra decode attempt on a failed ROBUST frame (Tobias, #96). Receive
        // only, so it cannot make sending worse.
        if preset.is_robust()
            && ensure::binary_version(&binary).is_some_and(|v| ensure::has_robust_retry(&v))
        {
            args.push("--robust-enhanced-retry".into());
        }
        if let Some(call) = modem_callsign(&cfg.callsign) {
            args.extend(["--callsign".into(), call]);
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
        let mut child = cmd.spawn().map_err(|e| {
            Error::Modem(format!(
                "could not start {}: {e}. Re-run the installer or set modem.binary in wcr.toml.",
                binary.display()
            ))
        })?;
        confirm_still_running(&mut child, &log_path).await?;
        if let Ok(mut p) = MANAGED_PID.lock() {
            *p = child.id();
        }
        tracing::info!("started {} (log {})", binary.display(), log_path.display());
        Ok(Self { child, log_path })
    }

    pub async fn wait(&mut self) -> Result<()> {
        let status = self.child.wait().await?;
        if !status.success() {
            return Err(Error::Modem(format!("modem73 exited with {status}")));
        }
        Ok(())
    }

    /// True when the child has already exited (bad CLI args, missing audio, …).
    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }

    /// Exit status plus the last log line, when the child is already gone.
    pub fn exit_note(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(format!(
                "modem73 exited ({status}). {}Log: {}",
                log_tail(&self.log_path),
                self.log_path.display()
            )),
            _ => None,
        }
    }
}

async fn confirm_still_running(child: &mut Child, log_path: &Path) -> Result<()> {
    tokio::time::sleep(STARTUP_GRACE).await;
    if let Some(status) = child.try_wait()? {
        return Err(Error::Modem(format!(
            "modem73 exited immediately ({status}). {}Log: {}",
            log_tail(log_path),
            log_path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlicensed_callsign_is_modem_safe() {
        assert_eq!(modem_callsign("~QA1A").as_deref(), Some("QA1A"));
        assert_eq!(modem_callsign(" n0call ").as_deref(), Some("N0CALL"));
        assert_eq!(modem_callsign("VK2ABC/P").as_deref(), Some("VK2ABC/P"));
        assert_eq!(modem_callsign("~ABCDEFGHIJK").as_deref(), Some("ABCDEFGHI"));
        assert_eq!(modem_callsign("~"), None);
        assert_eq!(modem_callsign(""), None);
    }

    #[test]
    fn vox_threshold_replaces_the_preset_one() {
        let mut args = Preset::VoxSafe.modem73_args();
        set_csma_threshold(&mut args, Preset::VOX_CSMA_THRESHOLD_DB);
        let at: Vec<_> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| *a == "--csma-threshold")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(at.len(), 1);
        assert_eq!(args[at[0] + 1], "-30");
    }

    #[tokio::test]
    async fn early_exit_reports_the_log_instead_of_started() {
        let dir = std::env::temp_dir().join(format!("wcr-modem-exit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log_path = dir.join("modem73.log");
        let log = std::fs::File::create(&log_path).unwrap();
        let err = log.try_clone().unwrap();
        let mut child = Command::new("sh")
            .args([
                "-c",
                "echo 'error while loading shared libraries: libncurses.so.6: cannot open shared object file' >&2; exit 127",
            ])
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .spawn()
            .unwrap();
        // A busy runner can take longer than the 400 ms grace to schedule
        // `sh`. Wait until it has actually exited, then check the report.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while child.try_wait().ok().flatten().is_none() {
            if tokio::time::Instant::now() > deadline {
                let _ = child.kill().await;
                panic!("shell did not exit");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let err = confirm_still_running(&mut child, &log_path)
            .await
            .expect_err("exited child");
        let msg = err.to_string();
        assert!(msg.contains("exited immediately"), "{msg}");
        assert!(msg.contains("libncurses"), "{msg}");
        assert!(!msg.contains("started"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn log_tail(path: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    if line.is_empty() {
        String::new()
    } else {
        format!("{line}. ")
    }
}
