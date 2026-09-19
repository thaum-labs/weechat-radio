//! SPDX-License-Identifier: Apache-2.0
//! Background service installers for Linux/macOS/Windows.

use crate::error::{Error, Result};
use std::env;
#[allow(unused_imports)]
use std::path::PathBuf;

pub fn install() -> Result<String> {
    let exe = env::current_exe()?;
    #[cfg(target_os = "linux")]
    {
        return install_systemd(&exe);
    }
    #[cfg(target_os = "macos")]
    {
        return install_launchd(&exe);
    }
    #[cfg(target_os = "windows")]
    {
        return install_windows(&exe);
    }
    #[allow(unreachable_code)]
    Err(Error::Msg(
        "service install is not supported on this OS".into(),
    ))
}

pub fn uninstall() -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        let unit = unit_path();
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", "wcr.service"])
            .status();
        if unit.exists() {
            std::fs::remove_file(&unit)?;
        }
        return Ok("removed systemd user service wcr.service".into());
    }
    #[cfg(target_os = "macos")]
    {
        let plist = plist_path();
        let _ = std::process::Command::new("launchctl")
            .args(["unload", plist.to_str().unwrap_or("")])
            .status();
        if plist.exists() {
            std::fs::remove_file(&plist)?;
        }
        return Ok("removed launchd agent com.thaum-labs.wcr".into());
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("sc")
            .args(["stop", "wcr"])
            .status();
        let _ = std::process::Command::new("sc")
            .args(["delete", "wcr"])
            .status();
        return Ok("removed Windows service wcr (if it existed)".into());
    }
    #[allow(unreachable_code)]
    Err(Error::Msg("unsupported".into()))
}

pub fn status() -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("systemctl")
            .args(["--user", "is-active", "wcr.service"])
            .output()?;
        return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("launchctl")
            .args(["list", "com.thaum-labs.wcr"])
            .output()?;
        return Ok(if out.status.success() {
            "loaded".into()
        } else {
            "not loaded".into()
        });
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("sc")
            .args(["query", "wcr"])
            .output()?;
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    #[allow(unreachable_code)]
    Ok("unknown".into())
}

#[cfg(target_os = "linux")]
fn unit_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/systemd/user/wcr.service")
}

#[cfg(target_os = "linux")]
fn install_systemd(exe: &std::path::Path) -> Result<String> {
    let path = unit_path();
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let body = format!(
        "[Unit]\nDescription=WeeChat Radio node\nAfter=network.target\n\n[Service]\nExecStart={} node\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n",
        exe.display()
    );
    std::fs::write(&path, body)?;
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", "wcr.service"])
        .status();
    Ok(format!("installed {}", path.display()))
}

#[cfg(target_os = "macos")]
fn plist_path() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Library/LaunchAgents/com.thaum-labs.wcr.plist")
}

#[cfg(target_os = "macos")]
fn install_launchd(exe: &std::path::Path) -> Result<String> {
    let path = plist_path();
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let log = crate::config::default_data_dir().join("node.log");
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.thaum-labs.wcr</string>
  <key>ProgramArguments</key><array><string>{}</string><string>node</string></array>
  <key>WorkingDirectory</key><string>{}</string>
  <key>ProcessType</key><string>Interactive</string>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{}</string>
  <key>StandardErrorPath</key><string>{}</string>
  <key>EnvironmentVariables</key><dict>
    <key>PATH</key><string>/usr/bin:/bin:/usr/sbin:/sbin:{}</string>
  </dict>
</dict></plist>
"#,
        exe.display(),
        exe.parent().unwrap_or(exe).display(),
        log.display(),
        log.display(),
        exe.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "/usr/bin".into()),
    );
    std::fs::write(&path, body)?;
    let _ = std::process::Command::new("launchctl")
        .args(["load", path.to_str().unwrap_or("")])
        .status();
    Ok(format!("installed {}", path.display()))
}

#[cfg(target_os = "windows")]
fn install_windows(exe: &std::path::Path) -> Result<String> {
    let bin = exe.display().to_string();
    let status = std::process::Command::new("sc")
        .args([
            "create",
            "wcr",
            "binPath=",
            &format!("\"{bin}\" node"),
            "start=",
            "auto",
            "DisplayName=",
            "WeeChat Radio",
        ])
        .status()?;
    if !status.success() {
        return Err(Error::Msg(
            "sc create failed. Run this terminal as Administrator and try again.".into(),
        ));
    }
    let _ = std::process::Command::new("sc")
        .args(["start", "wcr"])
        .status();
    Ok("installed Windows service wcr".into())
}
