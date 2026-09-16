//! SPDX-License-Identifier: Apache-2.0
//! Find, configure, and launch the real WeeChat client against the local node.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn run(configure_only: bool) -> Result<()> {
    if configure_only {
        println!("{}", configure()?);
        return Ok(());
    }
    let _ = configure();
    launch()
}

pub fn configure() -> Result<String> {
    let script = install_radio_py()?;
    let Some(headless) = find_weechat_headless() else {
        return Err(Error::Msg(
            "WeeChat is not installed. Re-run the official installer, or install WeeChat and run `wcr weechat --configure`."
                .into(),
        ));
    };
    let home = weechat_home();
    std::fs::create_dir_all(home.join("python").join("autoload"))?;
    let dest = home.join("python").join("radio.py");
    std::fs::copy(&script, &dest)?;
    std::fs::copy(&script, home.join("python").join("autoload").join("radio.py"))?;

    let status = Command::new(&headless)
        .args([
            "-d",
            &home.to_string_lossy(),
            "-r",
            "/server add radio 127.0.0.1/6667 -autoconnect;/set irc.server.radio.capabilities message-tags,echo-message,server-time,msgid;/script load radio.py;/save;/quit",
        ])
        .status()?;
    if !status.success() {
        return Err(Error::Msg(format!(
            "weechat-headless exited {status} while writing the radio server"
        )));
    }
    write_launcher()?;
    Ok(format!(
        "WeeChat is configured for 127.0.0.1:6667 (home {}). Start the node with `wcr node`, then `wcr weechat`.",
        home.display()
    ))
}

pub fn launch() -> Result<()> {
    #[cfg(windows)]
    {
        if let Some(mintty) = cygwin_root().map(|r| r.join("bin").join("mintty.exe")) {
            if mintty.is_file() {
                let home = weechat_home();
                Command::new(mintty)
                    .args([
                        "-t",
                        "WeeChat Radio",
                        "/bin/bash",
                        "--norc",
                        "--noprofile",
                        "-c",
                        &format!(
                            "export PATH=/usr/bin:/bin; export HOME={}; exec weechat -d {}",
                            cygwin_home_unix(),
                            cygwin_path(&home)
                        ),
                    ])
                    .spawn()?;
                return Ok(());
            }
        }
    }
    let wee = find_weechat().ok_or_else(|| {
        Error::Msg("WeeChat not found. Re-run the installer or put weechat on PATH.".into())
    })?;
    let status = Command::new(wee)
        .args(["-d", &weechat_home().to_string_lossy()])
        .status()?;
    if !status.success() {
        return Err(Error::Msg(format!("weechat exited {status}")));
    }
    Ok(())
}

fn install_radio_py() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("radio.py");
            if beside.is_file() {
                return Ok(beside);
            }
        }
    }
    Err(Error::Msg(
        "radio.py is missing. Re-run the official installer so it is placed next to wcr.".into(),
    ))
}

fn find_weechat() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let cyg = cygwin_root()?.join("bin").join("weechat.exe");
        if cyg.is_file() {
            return Some(cyg);
        }
    }
    which("weechat")
}

fn find_weechat_headless() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let cyg = cygwin_root()?.join("bin").join("weechat-headless.exe");
        if cyg.is_file() {
            return Some(cyg);
        }
    }
    which("weechat-headless").or_else(|| which("weechat"))
}

fn which(name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) && !name.ends_with(".exe") {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(&file);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn weechat_home() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(root) = cygwin_root() {
            let user = std::env::var("USERNAME").unwrap_or_else(|_| "weechat".into());
            let home = root.join("home").join(&user);
            if home.is_dir() || root.join("bin").join("weechat.exe").is_file() {
                return home.join(".weechat");
            }
        }
    }
    dirs_weechat()
}

fn dirs_weechat() -> PathBuf {
    if let Some(p) = std::env::var_os("WEECHAT_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".weechat")
}

#[cfg(windows)]
fn cygwin_root() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("WCR_CYGWIN").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("cygwin64")),
        Some(PathBuf::from(r"C:\cygwin64")),
    ];
    candidates.into_iter().flatten().find(|p| p.join("bin").join("weechat.exe").is_file())
}

#[cfg(windows)]
fn cygwin_home_unix() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "weechat".into());
    format!("/home/{user}")
}

#[cfg(windows)]
fn cygwin_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if let Some(rest) = s.strip_prefix("C:") {
        format!("/cygdrive/c{rest}")
    } else {
        s
    }
}

fn write_launcher() -> Result<()> {
    #[cfg(windows)]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let cmd = dir.join("weechat-radio.cmd");
                std::fs::write(
                    cmd,
                    "@echo off\r\n\"%~dp0wcr.exe\" weechat\r\n",
                )?;
            }
        }
    }
    let _ = Path::new(".");
    Ok(())
}
