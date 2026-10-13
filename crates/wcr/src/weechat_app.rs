//! SPDX-License-Identifier: Apache-2.0
//! Find, install, configure, and launch the real WeeChat client against the local node.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(windows)]
use std::time::{Duration, Instant};

const RADIO_PY: &str = include_str!("../../../weechat/radio.py");
#[cfg(windows)]
const MINTTYRC: &str = include_str!("../../../weechat/minttyrc");

pub fn run(configure_only: bool, cfg_path: &Path) -> Result<()> {
    if configure_only {
        println!("{}", configure(cfg_path)?);
        return Ok(());
    }
    let _ = configure(cfg_path);
    launch()
}

pub fn configure(cfg_path: &Path) -> Result<String> {
    configure_home(cfg_path, &weechat_home())
}

fn configure_home(cfg_path: &Path, home: &Path) -> Result<String> {
    ensure_weechat()?;
    let script = install_radio_py()?;
    std::fs::create_dir_all(home.join("python"))?;

    run_headless_configure(&script, cfg_path, home)?;
    std::fs::create_dir_all(home.join("python").join("autoload"))?;
    let dest = home.join("python").join("radio.py");
    std::fs::copy(&script, &dest)?;
    std::fs::copy(
        &script,
        home.join("python").join("autoload").join("radio.py"),
    )?;
    quiet_core_logs(home);
    write_minttyrc()?;
    write_launcher()?;
    let listen = irc_listen_label(cfg_path);
    Ok(configured_message(&listen, home))
}

/// What was configured. The next command is printed once, by setup or the installer.
pub fn configured_message(listen: &str, home: &Path) -> String {
    format!(
        "WeeChat is configured for {listen} (home {}).",
        home.display()
    )
}

pub fn launch() -> Result<()> {
    #[cfg(windows)]
    {
        if let Some(mintty) = cygwin_root().map(|r| r.join("bin").join("mintty.exe")) {
            if mintty.is_file() {
                let home = weechat_home();
                let mut args = vec![
                    "-t".into(),
                    "WeeChat Radio".into(),
                    // Sized to sit near the GUI default (~65%×80% of a typical desktop).
                    "-s".into(),
                    "110,36".into(),
                ];
                if let Some(cfg) = minttyrc_path() {
                    args.push("-c".into());
                    args.push(cfg.to_string_lossy().into_owned());
                }
                args.extend([
                    "/bin/bash".into(),
                    "--norc".into(),
                    "--noprofile".into(),
                    "-c".into(),
                    format!(
                        "export PATH=/usr/bin:/bin; export HOME={}; exec weechat -d {}",
                        cygwin_home_unix(),
                        cygwin_path(&home)
                    ),
                ]);
                Command::new(mintty).args(args).spawn()?;
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

fn irc_listen_label(cfg_path: &Path) -> String {
    crate::config::Config::load(cfg_path)
        .map(|c| c.irc.bind)
        .unwrap_or_else(|_| "127.0.0.1:6667".into())
}

/// WeeChat `/server add` wants `host/port`, taken from `irc.bind`.
pub(crate) fn weechat_server_target(bind: &str) -> String {
    let bind = bind.trim();
    if let Some((host, port)) = bind.rsplit_once(':') {
        if !host.is_empty() && !host.contains(':') && port.chars().all(|c| c.is_ascii_digit()) {
            return format!("{host}/{port}");
        }
    }
    "127.0.0.1/6667".into()
}

pub(crate) fn configure_cmds(cfg_path: &Path) -> String {
    let cfg = crate::config::Config::load(cfg_path).ok();
    let target = cfg
        .as_ref()
        .map(|c| weechat_server_target(&c.irc.bind))
        .unwrap_or_else(|| "127.0.0.1/6667".into());
    let mut cmds = format!(
        "/server add radio {target} -autoconnect;\
         /mute -all /set irc.server.radio.tls off;\
         /mute -all /set irc.server.radio.tls_verify off;\
         /mute -all /set irc.server.radio.capabilities message-tags,echo-message,server-time,msgid;\
         /mute -all /set irc.server.radio.autojoin #bulletin;\
         /mute -all /set irc.look.server_buffer independent;\
         /mute -all /set weechat.startup.display_logo off;\
         /mute -all /set weechat.startup.display_version off;\
         /mute -all /set logger.level.core 0",
    );
    if let Some(cfg) = cfg {
        let nick = cfg.callsign.trim();
        if !nick.is_empty() {
            cmds.push_str(&format!(";/mute -all /set irc.server.radio.nicks {nick}"));
        }
    }
    cmds.push_str(";/save;/quit");
    cmds
}

fn quiet_core_logs(home: &Path) {
    let logs = home.join("logs");
    let Ok(entries) = std::fs::read_dir(&logs) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        let drop = name.contains("core")
            || name.contains("weechat.weechatlog")
            || name.contains("server.radio")
            || name == "radio.weechatlog"
            || name == "irc.radio.weechatlog";
        if drop {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn run_headless_configure(script: &Path, cfg_path: &Path, home: &Path) -> Result<()> {
    let stderr = invoke_headless(script, cfg_path, home)?;
    verify_radio_server(home, cfg_path, &stderr)
}

/// `-nc` is not "no connect": WeeChat reads it as `-c`/`--colors`, prints the
/// colour table, and exits before `-r` runs. Use `-d` and `-r` only.
fn invoke_headless(script: &Path, cfg_path: &Path, home: &Path) -> Result<String> {
    #[cfg(not(windows))]
    let _ = script;
    #[cfg(windows)]
    {
        if let Some(bash) = cygwin_root().map(|r| r.join("bin").join("bash.exe")) {
            if bash.is_file() {
                let home_unix = cygwin_path(home);
                let cmd = format!(
                    "export PATH=/usr/bin:/bin; \
                     mkdir -p \"{home_unix}/python/autoload\"; \
                     cp \"{script}\" \"{home_unix}/python/radio.py\"; \
                     cp \"{home_unix}/python/radio.py\" \"{home_unix}/python/autoload/radio.py\"; \
                     weechat-headless -d \"{home_unix}\" -r '{cmds}'",
                    cmds = configure_cmds(cfg_path),
                    script = cygwin_path(script),
                );
                let mut bash_cmd = Command::new(bash);
                bash_cmd.args(["--norc", "--noprofile", "-c", &cmd]);
                return run_command(&mut bash_cmd);
            }
        }
    }

    let Some(headless) = find_weechat_headless() else {
        return Err(Error::Msg(
            "WeeChat is not installed. Re-run the official installer, or install WeeChat and run `wcr weechat --configure`."
                .into(),
        ));
    };
    let mut wee = Command::new(&headless);
    wee.args([
        "-d",
        &home.to_string_lossy(),
        "-r",
        &configure_cmds(cfg_path),
    ]);
    run_command(&mut wee)
}

fn run_command(cmd: &mut Command) -> Result<String> {
    let out = cmd.stdout(Stdio::null()).stderr(Stdio::piped()).output()?;
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return Err(Error::Msg(format!(
            "weechat-headless exited {} while writing the radio server{}",
            out.status,
            stderr_note(&stderr)
        )));
    }
    Ok(stderr)
}

fn verify_radio_server(home: &Path, cfg_path: &Path, stderr: &str) -> Result<()> {
    let path = home.join("irc.conf");
    let text = std::fs::read_to_string(&path).map_err(|e| {
        Error::Msg(format!(
            "WeeChat did not write {} ({e}). The radio server was not configured.{}",
            path.display(),
            stderr_note(stderr)
        ))
    })?;
    let target = crate::config::Config::load(cfg_path)
        .map(|c| weechat_server_target(&c.irc.bind))
        .unwrap_or_else(|_| "127.0.0.1/6667".into());
    if !text.contains("radio.addresses") || !text.contains(&target) {
        return Err(Error::Msg(format!(
            "WeeChat {} has no radio server for {target}. Re-run `wcr weechat --configure`.{}",
            path.display(),
            stderr_note(stderr)
        )));
    }
    Ok(())
}

fn stderr_note(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("\nweechat stderr:\n{trimmed}")
    }
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
            std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Local"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cygwin-packages");
    std::fs::create_dir_all(&pkg)?;
    std::fs::create_dir_all(&root)?;
    download_file("https://www.cygwin.com/setup-x86_64.exe", &setup)?;
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

fn write_minttyrc() -> Result<()> {
    // mintty is the Windows terminal. Writing this beside wcr on Linux or
    // macOS leaves a stray file in ~/.local/bin.
    #[cfg(windows)]
    if let Some(path) = minttyrc_path() {
        std::fs::write(path, MINTTYRC)?;
    }
    Ok(())
}

fn minttyrc_path() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return Some(dir.join("minttyrc"));
        }
    }
    None
}

fn write_launcher() -> Result<()> {
    #[cfg(windows)]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let cmd = dir.join("weechat-radio.cmd");
                std::fs::write(
                    cmd,
                    "@echo off\r\n\
                     set MINTTY=%USERPROFILE%\\cygwin64\\bin\\mintty.exe\r\n\
                     if exist \"%MINTTY%\" (\r\n\
                     \"%MINTTY%\" -c \"%~dp0minttyrc\" -s 110,36 -t \"WeeChat Radio\" /bin/bash --norc --noprofile -c \"export PATH=/usr/bin:/bin; export HOME=/home/%USERNAME%; exec weechat -d /home/%USERNAME%/.weechat\"\r\n\
                     ) else (\r\n\
                     \"%~dp0wcr.exe\" weechat\r\n\
                     )\r\n",
                )?;
            }
        }
    }
    let _ = Path::new(".");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::configure_cmds;
    use super::weechat_server_target;
    use crate::config::Config;
    use std::path::Path;

    #[test]
    fn configured_message_does_not_name_a_next_command() {
        let msg = super::configured_message("127.0.0.1:6667", Path::new("/tmp/weechat"));
        assert_eq!(
            msg,
            "WeeChat is configured for 127.0.0.1:6667 (home /tmp/weechat)."
        );
        assert!(!msg.contains("wcr node"));
        assert!(!msg.contains("wcr weechat"));
        assert!(!msg.contains("wcr setup"));
    }

    #[test]
    fn install_summary_puts_path_apply_before_setup() {
        let install = include_str!("../../../install/install.sh");
        let web = include_str!("../../../web/install.sh");
        assert_eq!(
            install, web,
            "web/install.sh drifted from install/install.sh"
        );
        let gui = install.rfind("Linux GUI").expect("linux gui notice");
        let apply = install.rfind("Apply it with:").expect("path apply");
        let next = install.rfind("Next, run:").expect("next step");
        let setup = install.rfind("wcr setup").expect("wcr setup");
        assert!(
            gui < apply,
            "library notice must sit above the PATH apply line"
        );
        assert!(
            apply < next,
            "PATH apply must sit immediately before the next step"
        );
        assert!(next < setup);
        assert!(
            install.contains("WEECHAT_CFG="),
            "configure output must be captured into the summary"
        );
        let strip = install
            .find("Start the node with `wcr node`")
            .expect("old configure sentence is stripped");
        let echoed = install
            .rfind("echo \"$WEECHAT_CFG\"")
            .expect("configure line is printed in the summary");
        assert!(
            strip < echoed,
            "the node-start sentence must be removed before the summary prints it"
        );
    }

    #[test]
    fn minttyrc_is_only_written_on_windows() {
        let path = super::minttyrc_path();
        if cfg!(windows) {
            assert!(path.is_some());
        } else {
            assert!(path.is_none());
        }
    }

    #[test]
    fn server_target_uses_config_port() {
        assert_eq!(weechat_server_target("127.0.0.1:26667"), "127.0.0.1/26667");
        assert_eq!(weechat_server_target("127.0.0.1:6667"), "127.0.0.1/6667");
    }

    #[test]
    fn configure_cmds_follow_irc_bind() {
        let dir = std::env::temp_dir().join(format!("wcr-wee-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wcr.toml");
        let mut cfg = Config::default();
        cfg.callsign = "G4ABC".into();
        cfg.irc.bind = "127.0.0.1:26667".into();
        cfg.save(&path).unwrap();
        let cmds = configure_cmds(&path);
        assert!(cmds.contains("/server add radio 127.0.0.1/26667"), "{cmds}");
        assert!(!cmds.contains("127.0.0.1/6667"), "{cmds}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn headless_configure_writes_radio_server() {
        if super::find_weechat_headless().is_none() {
            eprintln!("skip: weechat-headless is not installed");
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "wcr-wee-live-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.join("weechat");
        let cfg_path = dir.join("wcr.toml");
        let mut cfg = Config::default();
        cfg.callsign = "G4ABC".into();
        cfg.irc.bind = "127.0.0.1:26667".into();
        cfg.save(&cfg_path).unwrap();
        super::configure_home(&cfg_path, &home).expect("configure writes irc.conf");
        let irc = std::fs::read_to_string(home.join("irc.conf")).expect("irc.conf");
        assert!(
            irc.contains("radio.addresses"),
            "irc.conf missing radio server:\n{irc}"
        );
        assert!(
            irc.contains("127.0.0.1/26667"),
            "irc.conf did not use the config listen address:\n{irc}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
