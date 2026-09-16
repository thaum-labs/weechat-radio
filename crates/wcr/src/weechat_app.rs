//! SPDX-License-Identifier: Apache-2.0
//! Find, install, configure, and launch the real WeeChat client against the local node.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::time::{Duration, Instant};

const RADIO_PY: &str = include_str!("../../../weechat/radio.py");

pub fn run(configure_only: bool) -> Result<()> {
    if configure_only {
        println!("{}", configure()?);
        return Ok(());
    }
    let _ = configure();
    launch()
}

pub fn configure() -> Result<String> {
    ensure_weechat()?;
    let script = install_radio_py()?;
    let home = weechat_home();
    std::fs::create_dir_all(home.join("python").join("autoload"))?;
    let dest = home.join("python").join("radio.py");
    std::fs::copy(&script, &dest)?;
    std::fs::copy(
        &script,
        home.join("python").join("autoload").join("radio.py"),
    )?;

    run_headless_configure(&script)?;
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

fn ensure_weechat() -> Result<()> {
    if find_weechat().is_some() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        eprintln!("Installing WeeChat via Cygwin (a few minutes)...");
        return install_cygwin_weechat();
    }
    #[cfg(not(windows))]
    Err(Error::Msg(
        "WeeChat is not installed. Re-run the official installer, or install WeeChat and run `wcr weechat --configure`."
            .into(),
    ))
}

fn run_headless_configure(script: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        if let Some(bash) = cygwin_root().map(|r| r.join("bin").join("bash.exe")) {
            if bash.is_file() {
                let cmd = format!(
                    "export PATH=/usr/bin:/bin; export HOME={home}; \
                     mkdir -p \"$HOME/.weechat/python/autoload\"; \
                     cp \"{script}\" \"$HOME/.weechat/python/radio.py\"; \
                     cp \"$HOME/.weechat/python/radio.py\" \"$HOME/.weechat/python/autoload/radio.py\"; \
                     weechat-headless -d \"$HOME/.weechat\" -r '/server add radio 127.0.0.1/6667 -autoconnect;/set irc.server.radio.capabilities message-tags,echo-message,server-time,msgid;/script load radio.py;/save;/quit'",
                    home = cygwin_home_unix(),
                    script = cygwin_path(script)
                );
                let status = Command::new(bash)
                    .args(["--norc", "--noprofile", "-c", &cmd])
                    .status()?;
                if !status.success() {
                    return Err(Error::Msg(format!(
                        "weechat-headless exited {status} while writing the radio server"
                    )));
                }
                return Ok(());
            }
        }
    }

    let Some(headless) = find_weechat_headless() else {
        return Err(Error::Msg(
            "WeeChat is not installed. Re-run the official installer, or install WeeChat and run `wcr weechat --configure`."
                .into(),
        ));
    };
    let home = weechat_home();
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
    Ok(())
}

fn install_radio_py() -> Result<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("radio.py");
            if beside.is_file() {
                return Ok(beside);
            }
            if let Err(e) = std::fs::write(&beside, RADIO_PY) {
                tracing::warn!(error = %e, "could not write radio.py next to wcr");
            } else {
                return Ok(beside);
            }
        }
    }
    let tmp = std::env::temp_dir().join("wcr-radio.py");
    std::fs::write(&tmp, RADIO_PY)?;
    Ok(tmp)
}

fn find_weechat() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(cyg) = cygwin_root().map(|r| r.join("bin").join("weechat.exe")) {
            if cyg.is_file() {
                return Some(cyg);
            }
        }
    }
    which("weechat")
}

fn find_weechat_headless() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(cyg) = cygwin_root().map(|r| r.join("bin").join("weechat-headless.exe")) {
            if cyg.is_file() {
                return Some(cyg);
            }
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
fn install_cygwin_weechat() -> Result<()> {
    let root = std::env::var_os("USERPROFILE")
        .map(|p| PathBuf::from(p).join("cygwin64"))
        .ok_or_else(|| Error::Msg("USERPROFILE is not set".into()))?;
    let setup = std::env::temp_dir().join("cygwin-setup-x86_64.exe");
    let pkg = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|p| PathBuf::from(p).join("AppData").join("Local"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cygwin-packages");
    std::fs::create_dir_all(&pkg)?;
    std::fs::create_dir_all(&root)?;
    download_file(
        "https://www.cygwin.com/setup-x86_64.exe",
        &setup,
    )?;
    let status = Command::new(&setup)
        .args([
            "--quiet-mode",
            "--only-site",
            "--no-admin",
            "--no-desktop",
            "--no-shortcuts",
            "--no-startmenu",
            "--root",
            &root.to_string_lossy(),
            "--local-package-dir",
            &pkg.to_string_lossy(),
            "--site",
            "https://mirrors.kernel.org/sourceware/cygwin/",
            "--packages",
            "weechat,weechat-python",
        ])
        .status()?;
    if !status.success() {
        return Err(Error::Msg(format!(
            "Cygwin setup exited {status} while installing WeeChat"
        )));
    }
    let wee = root.join("bin").join("weechat.exe");
    let deadline = Instant::now() + Duration::from_secs(120);
    while !wee.is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(2));
    }
    if !wee.is_file() {
        return Err(Error::Msg(
            "Cygwin finished but weechat.exe is missing. Re-run the official installer.".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn download_file(url: &str, dest: &Path) -> Result<()> {
    let dest_s = dest.display().to_string().replace('\'', "''");
    let url_s = url.replace('\'', "''");
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Invoke-WebRequest -Uri '{url_s}' -OutFile '{dest_s}'"),
        ])
        .status()?;
    if status.success() && dest.is_file() {
        return Ok(());
    }
    Err(Error::Msg(format!(
        "failed to download {url} (powershell exited {status})"
    )))
}

#[cfg(windows)]
fn cygwin_root() -> Option<PathBuf> {
    let candidates = [
        std::env::var_os("WCR_CYGWIN").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("cygwin64")),
        Some(PathBuf::from(r"C:\cygwin64")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|p| p.join("bin").join("weechat.exe").is_file())
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
    } else if let Some(rest) = s.strip_prefix("c:") {
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
                std::fs::write(cmd, "@echo off\r\n\"%~dp0wcr.exe\" weechat\r\n")?;
            }
        }
    }
    let _ = Path::new(".");
    Ok(())
}
