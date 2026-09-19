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
        macos_bootout();
        let plist = plist_path();
        if let Some(p) = plist.to_str() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", "-w", p])
                .status();
        }
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

/// Stop a running node job so Quit/Stop can actually kill `wcr`.
/// On macOS the LaunchAgent used to use KeepAlive=true, which immediately
/// respawned the process after pkill.
pub fn stop_job() {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", "wcr.service"])
            .status();
    }
    #[cfg(target_os = "macos")]
    {
        macos_bootout();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("sc")
            .args(["stop", "wcr"])
            .status();
    }
}

/// Remove the macOS login agent so a reboot does not start a headless `wcr`.
/// Other platforms leave an explicitly installed service in place.
pub fn forget_login_agent() {
    #[cfg(target_os = "macos")]
    {
        let _ = uninstall();
    }
}

/// Rewrite an old KeepAlive=true LaunchAgent so a killed node stays dead.
pub fn soften_keep_alive() {
    #[cfg(target_os = "macos")]
    {
        macos_soften_keep_alive();
    }
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

/// Bounce a previously installed background node so `wcr update` does not leave
/// the old binary running (which drops Mac speaker audio).
pub fn restart_if_installed() -> bool {
    #[cfg(target_os = "macos")]
    {
        let path = plist_path();
        if !path.is_file() {
            return false;
        }
        let plist = path.to_str().unwrap_or("");
        let _ = std::process::Command::new("launchctl")
            .args(["unload", plist])
            .status();
        return std::process::Command::new("launchctl")
            .args(["load", plist])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    #[cfg(target_os = "linux")]
    {
        if !unit_path().is_file() {
            return false;
        }
        return std::process::Command::new("systemctl")
            .args(["--user", "restart", "wcr.service"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
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
    let workdir = exe.parent().unwrap_or(exe);
    let path_env = workdir.to_str().unwrap_or("/usr/bin");
    let body = macos_agent_plist(
        &exe.display().to_string(),
        &workdir.display().to_string(),
        &log.display().to_string(),
        path_env,
    );
    macos_bootout();
    std::fs::write(&path, body)?;
    macos_bootstrap(&path);
    Ok(format!("installed {}", path.display()))
}

#[cfg(target_os = "macos")]
fn macos_bootout() {
    let uid = unsafe { libc::getuid() };
    let domain = format!("gui/{uid}/com.thaum-labs.wcr");
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &domain])
        .status();
    let plist = plist_path();
    if let Some(p) = plist.to_str() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", p])
            .status();
    }
}

#[cfg(target_os = "macos")]
fn macos_bootstrap(plist: &std::path::Path) {
    let Some(p) = plist.to_str() else {
        return;
    };
    let uid = unsafe { libc::getuid() };
    let domain = format!("gui/{uid}");
    let _ = std::process::Command::new("launchctl")
        .args(["bootstrap", &domain, p])
        .status();
    let _ = std::process::Command::new("launchctl")
        .args(["load", "-w", p])
        .status();
}

#[cfg(target_os = "macos")]
fn macos_soften_keep_alive() {
    let path = plist_path();
    let Ok(body) = std::fs::read_to_string(&path) else {
        return;
    };
    if !body.contains("<key>KeepAlive</key><true/>") {
        return;
    }
    let updated = body.replace("<key>KeepAlive</key><true/>", MACOS_KEEP_ALIVE);
    let _ = std::fs::write(&path, updated);
    macos_bootout();
    macos_bootstrap(&path);
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

/// Restart only if the node crashed — not after Quit, Stop, or SIGTERM.
const MACOS_KEEP_ALIVE: &str = r#"<key>KeepAlive</key>
  <dict>
    <key>Crashed</key>
    <true/>
  </dict>"#;

fn macos_agent_plist(exe: &str, workdir: &str, log: &str, path_env: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.thaum-labs.wcr</string>
  <key>ProgramArguments</key><array><string>{exe}</string><string>node</string></array>
  <key>WorkingDirectory</key><string>{workdir}</string>
  <key>ProcessType</key><string>Interactive</string>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
  <key>RunAtLoad</key><true/>
  {MACOS_KEEP_ALIVE}
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
  <key>EnvironmentVariables</key><dict>
    <key>PATH</key><string>/usr/bin:/bin:/usr/sbin:/sbin:{path_env}</string>
  </dict>
</dict></plist>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_plist_does_not_respawn_on_quit() {
        let body = macos_agent_plist("/opt/wcr", "/opt", "/tmp/n.log", "/opt");
        assert!(body.contains("<key>Crashed</key>"));
        assert!(body.contains("<key>KeepAlive</key>"));
        assert!(
            !body.contains("<key>KeepAlive</key><true/>"),
            "boolean KeepAlive respawns the node after Quit"
        );
        assert!(body.contains("<key>RunAtLoad</key><true/>"));
    }
}
