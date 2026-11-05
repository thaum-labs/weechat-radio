//! SPDX-License-Identifier: Apache-2.0
//! Check GitHub Releases and apply `wcr update`. Never silent.

use crate::error::{Error, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO: &str = "thaum-labs/weechat-radio";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Test hooks for the update-and-restart harness (tests/update_restart.rs):
/// a version this build reports instead of its own, and a release list to
/// read instead of GitHub's. Never set in normal use.
pub const TEST_VERSION_ENV: &str = "WCR_TEST_FAKE_VERSION";
pub const TEST_RELEASES_ENV: &str = "WCR_TEST_RELEASES_URL";

pub fn current_version() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION
        .get_or_init(|| {
            std::env::var(TEST_VERSION_ENV)
                .ok()
                .filter(|v| parse_version(v).is_some())
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
        })
        .as_str()
}

fn releases_url() -> String {
    std::env::var(TEST_RELEASES_ENV)
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{REPO}/releases?per_page=30"))
}

fn is_rc(tag: &str) -> bool {
    tag.contains("-rc")
}

async fn fetch_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let res = client
        .get(releases_url())
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if !res.status().is_success() {
        return Err(github_http_error("releases", res.status()));
    }
    res.json().await.map_err(|e| Error::Net(e.to_string()))
}

#[derive(Deserialize)]
struct Tag {
    name: String,
}

async fn fetch_tag_names(client: &reqwest::Client) -> Result<Vec<String>> {
    if std::env::var_os(TEST_RELEASES_ENV).is_some() {
        return Ok(Vec::new());
    }
    let url = format!("https://api.github.com/repos/{REPO}/tags?per_page=10");
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if !res.status().is_success() {
        return Err(github_http_error("tags", res.status()));
    }
    let tags: Vec<Tag> = res.json().await.map_err(|e| Error::Net(e.to_string()))?;
    Ok(tags.into_iter().map(|t| t.name).collect())
}

/// GitHub answers 403 or 429 when it rate-limits (several updates in a row
/// from one address), so say that in plain words instead of a bare code.
fn github_http_error(what: &str, status: reqwest::StatusCode) -> Error {
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Error::Net(format!(
            "GitHub is rate-limiting update checks, try again in a minute (GitHub {what} HTTP {status})"
        ));
    }
    Error::Net(format!("GitHub {what} HTTP {status}"))
}

fn github_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("wcr")
        .build()
        .map_err(|e| Error::Net(e.to_string()))
}

/// `vX.Y.Z` or `vX.Y.Z-rcN`. The last number orders release candidates
/// before their final release: 1.0.0-rc1 < 1.0.0-rc2 < 1.0.0.
fn parse_version(tag: &str) -> Option<(u64, u64, u64, u64)> {
    let tag = tag.trim_start_matches('v');
    let (base, rc) = match tag.split_once("-rc") {
        Some((base, n)) => (base, n.parse().ok()?),
        None => (tag, u64::MAX),
    };
    let mut parts = base.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, rc))
}

fn version_cmp(a: &str, b: &str) -> Ordering {
    match (parse_version(a), parse_version(b)) {
        (Some(va), Some(vb)) => va.cmp(&vb),
        _ => a.cmp(b),
    }
}

/// Newer than `current`, and not a release candidate unless `rc` allows it.
fn is_offered(tag: &str, current: &str, rc: bool) -> bool {
    version_cmp(tag, current) == Ordering::Greater && (rc || !is_rc(tag))
}

fn highest_newer_tag(releases: &[Release], current: &str, rc: bool) -> Option<String> {
    let names: Vec<String> = releases.iter().map(|rel| rel.tag_name.clone()).collect();
    highest_newer_name(&names, current, rc)
}

/// Highest version name offered over `current`, without the `v`.
fn highest_newer_name(names: &[String], current: &str, rc: bool) -> Option<String> {
    names
        .iter()
        .filter_map(|name| {
            let tag = name.trim_start_matches('v');
            if is_offered(tag, current, rc) {
                parse_version(tag).map(|_| tag.to_string())
            } else {
                None
            }
        })
        .max_by(|a, b| version_cmp(a, b))
}

/// Newest release newer than `current` that ships a binary for this OS/arch.
fn best_downloadable(releases: &[Release], current: &str, rc: bool) -> Option<ReleaseInfo> {
    best_downloadable_for(
        releases,
        current,
        rc,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

fn best_downloadable_for(
    releases: &[Release],
    current: &str,
    rc: bool,
    os: &str,
    arch: &str,
) -> Option<ReleaseInfo> {
    releases
        .iter()
        .filter_map(|rel| {
            let tag = rel.tag_name.trim_start_matches('v');
            if !is_offered(tag, current, rc) {
                return None;
            }
            let asset = pick_asset_for(&rel.assets, os, arch)?;
            let ver = parse_version(tag)?;
            Some((ver, release_info(rel, asset)))
        })
        .max_by_key(|(ver, _)| *ver)
        .map(|(_, info)| info)
}

fn release_info(rel: &Release, asset: &Asset) -> ReleaseInfo {
    let tag = rel.tag_name.trim_start_matches('v').to_string();
    let name = asset.name.clone();
    ReleaseInfo {
        version: tag,
        url: Some(asset.browser_download_url.clone()),
        name: Some(name.clone()),
        sums_url: rel.assets.iter().find_map(|a| {
            if a.name.eq_ignore_ascii_case("SHA256SUMS.txt") {
                Some(a.browser_download_url.clone())
            } else if a.name.eq_ignore_ascii_case(&format!("{name}.sha256")) {
                Some(a.browser_download_url.clone())
            } else {
                None
            }
        }),
    }
}

fn pick_asset_for<'a>(assets: &'a [Asset], os: &str, arch: &str) -> Option<&'a Asset> {
    let preferred = match (os, arch) {
        // WoA and x64 Windows share the x86_64 release zip today.
        ("windows", "x86_64" | "aarch64") => Some("wcr-windows-x86_64.zip"),
        ("linux", "x86_64") => Some("wcr-linux-x86_64.tar.gz"),
        ("linux", "aarch64") => Some("wcr-linux-aarch64.tar.gz"),
        ("macos", "x86_64") => Some("wcr-macos-x86_64.tar.gz"),
        ("macos", "aarch64") => Some("wcr-macos-aarch64.tar.gz"),
        _ => None,
    };
    if let Some(name) = preferred {
        if let Some(a) = assets.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
            return Some(a);
        }
    }
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        if !n.starts_with("wcr-") {
            return false;
        }
        n.contains(os) && (n.contains(arch) || (arch == "x86_64" && n.contains("x86_64")))
    })
}

pub struct ReleaseInfo {
    pub version: String,
    pub url: Option<String>,
    pub name: Option<String>,
    pub sums_url: Option<String>,
}

/// What to install: the newest release offered, or (from a release
/// candidate, test versions off) the newest stable release, even if older.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Newest { rc: bool },
    BackToStable,
}

/// What the update panel and `wcr status` show.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Check {
    pub running: String,
    /// Newer release offered, without the `v`.
    pub available: Option<String>,
    /// Running a release candidate with test versions off: the newest stable
    /// release to go back to.
    pub stable: Option<String>,
    /// A newer tag still building (no assets yet).
    pub building: Option<String>,
    /// Said before going back to `stable` when that version cannot update
    /// itself from the app yet.
    pub stable_note: String,
}

/// The first release that updates and restarts itself from the app (#280).
pub const FIRST_SELF_UPDATING: &str = "1.0.16-rc3";

/// Going back to `version` loses the Update button when it is older than
/// [`FIRST_SELF_UPDATING`]; say so before.
pub fn stable_note(version: &str) -> String {
    if version_cmp(version, FIRST_SELF_UPDATING) == Ordering::Less {
        format!(
            "{version} has no Update button yet: to come back to a test version later, run the installer or wcr update --rc in a terminal."
        )
    } else {
        String::new()
    }
}

/// Look for updates. `rc`: test versions are on (or `--rc`).
pub async fn check(rc: bool) -> Result<Check> {
    let client = github_client()?;
    let releases = fetch_releases(&client).await?;
    let current = current_version();
    let available = best_downloadable(&releases, current, rc).map(|i| i.version);
    let stable = if is_rc(current) && !rc && available.is_none() {
        newest_stable(&releases).map(|i| i.version)
    } else {
        None
    };
    let building = if available.is_none() {
        highest_newer_tag(&releases, current, rc)
    } else {
        None
    };
    Ok(Check {
        running: current.to_string(),
        available,
        stable_note: stable.as_deref().map(stable_note).unwrap_or_default(),
        stable,
        building,
    })
}

/// The newest stable release with a binary for this platform.
fn newest_stable(releases: &[Release]) -> Option<ReleaseInfo> {
    best_downloadable(releases, "0.0.0", false)
}

/// What an install did.
pub struct Installed {
    pub from: String,
    pub to: String,
    /// The files replaced (each has its `.bak`).
    pub files: Vec<PathBuf>,
    pub message: String,
}

pub enum Outcome {
    /// Nothing to install; the message says why.
    UpToDate(String),
    Installed(Installed),
}

/// Download, check the SHA-256, back up the config, install, test that the
/// new binary starts, and leave the update note for the restart
/// ([`crate::restart`]). Does not restart anything.
pub async fn install(
    target: Target,
    cfg_path: Option<&Path>,
    simulate_failure: bool,
    progress: impl Fn(&str),
) -> Result<Outcome> {
    progress("Checking for updates");
    let client = github_client()?;
    let releases = fetch_releases(&client).await?;
    let current = current_version();
    let picked = match target {
        Target::Newest { rc } => best_downloadable(&releases, current, rc),
        Target::BackToStable => {
            newest_stable(&releases).filter(|i| version_cmp(&i.version, current) != Ordering::Equal)
        }
    };
    let Some(info) = picked else {
        let rc = matches!(target, Target::Newest { rc: true });
        return up_to_date(&client, &releases, current, rc).await;
    };
    let url = info
        .url
        .as_ref()
        .ok_or_else(|| unsupported_platform_msg(&info.version))?;
    progress(&format!("Downloading {}", info.version));
    let bytes = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?
        .bytes()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;

    if let (Some(sums_url), Some(name)) = (&info.sums_url, &info.name) {
        if let Ok(res) = client.get(sums_url).send().await {
            if res.status().is_success() {
                if let Ok(body) = res.text().await {
                    verify_sha256(&body, name, &bytes)?;
                }
            }
        }
    }

    progress(&format!("Installing {}", info.version));
    if let Some(cfg) = cfg_path {
        backup_config(cfg)?;
    }
    let exe = running_exe()?;
    let extracted = extract_payload(&bytes, info.name.as_deref())?;
    let replaced = install_files(&exe, &extracted)?;
    // QA v0.1.113: a build that cannot start on this machine (newer glibc)
    // must not be left in place, or `wcr update` can never run again.
    if let Err(why) = new_binary_starts(&exe) {
        return Err(Error::Msg(rollback_message(
            &info.version,
            &why,
            &restore_files(&replaced),
        )));
    }
    let removed_mintty = remove_stray_minttyrc(&exe);
    let mut note = crate::restart::Marker {
        from: current.to_string(),
        to: info.version.clone(),
        files: replaced.clone(),
        simulate_failure,
        ..Default::default()
    };
    if let Some(dir) = exe.parent() {
        note.save(dir).map_err(|e| {
            Error::Msg(format!(
                "could not write the update note in {}: {e}",
                dir.display()
            ))
        })?;
    }
    Ok(Outcome::Installed(Installed {
        from: current.to_string(),
        to: info.version.clone(),
        message: update_success_message(&info.version, &exe, &replaced, false, removed_mintty),
        files: replaced,
    }))
}

async fn up_to_date(
    client: &reqwest::Client,
    releases: &[Release],
    current: &str,
    rc: bool,
) -> Result<Outcome> {
    if highest_newer_tag(releases, current, rc).is_some() {
        return Err(Error::Msg(format!(
            "a newer release is still publishing for this platform. Try again in a few minutes, or install it from https://github.com/{REPO}/releases"
        )));
    }
    // The release page only appears once every platform is built (up to
    // an hour after the tag), so say so instead of a flat "up to date".
    let building = fetch_tag_names(client)
        .await
        .ok()
        .and_then(|tags| highest_newer_name(&tags, current, rc));
    Ok(Outcome::UpToDate(match building {
        Some(tag) => format!(
            "Already up to date ({current}). {tag} is still being built; try again later (it can take up to an hour)."
        ),
        None => match highest_newer_tag(releases, current, true) {
            Some(tag) if !rc => format!(
                "Already up to date ({current}). Test version {tag} is out: turn on \"Get test versions\" (or run wcr update --rc) to try it."
            ),
            _ => format!("Already up to date ({current})."),
        },
    }))
}

/// Keep a copy of the config as it was before this update
/// (`wcr.toml.before-update`).
pub fn backup_config(cfg: &Path) -> Result<Option<PathBuf>> {
    if !cfg.is_file() {
        return Ok(None);
    }
    let mut name = cfg.as_os_str().to_owned();
    name.push(".before-update");
    let to = PathBuf::from(name);
    fs::copy(cfg, &to).map_err(|e| {
        Error::Msg(format!(
            "could not back up {} before the update: {e}",
            cfg.display()
        ))
    })?;
    Ok(Some(to))
}

/// Put the new `wcr` (and `wcr-gui`, `modem73`, `libs/` when the package
/// has them) in place. Returns the files replaced.
fn install_files(exe: &Path, extracted: &Path) -> Result<Vec<PathBuf>> {
    install_binary(exe, extracted)?;
    let mut replaced = vec![exe.to_path_buf()];
    let (Some(dir), Some(parent)) = (extracted.parent(), exe.parent()) else {
        return Ok(replaced);
    };
    for name in ["wcr-gui", "modem73"] {
        let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
        if let Some(src) = find_named(dir, &name) {
            let dest = parent.join(&name);
            if install_binary(&dest, &src).is_ok() {
                replaced.push(dest);
            }
        }
    }
    if let Some(libs_src) = find_dir_named(dir, "libs") {
        let _ = replace_dir(&libs_src, &parent.join("libs"));
    }
    Ok(replaced)
}

/// The version of the `wcr` installed beside this program (what a restart
/// would run), from `wcr --version`. `None` if it does not answer.
pub fn installed_version() -> Option<String> {
    let out = quiet_command(&crate::restart::wcr_exe())
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_version_line(&String::from_utf8_lossy(&out.stdout))
}

/// A command that opens no console window on Windows (the desktop app asks
/// `wcr --version` every few seconds).
fn quiet_command(exe: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// `wcr 1.0.16-rc3` → `1.0.16-rc3`.
fn parse_version_line(line: &str) -> Option<String> {
    let v = line.split_whitespace().nth(1)?.trim_start_matches('v');
    parse_version(v).map(|_| v.to_string())
}

/// True when this version is a release candidate.
pub fn is_test_version(v: &str) -> bool {
    is_rc(v)
}

/// Run the freshly installed `wcr --version`; Err holds why it did not start.
fn new_binary_starts(exe: &Path) -> std::result::Result<(), String> {
    match quiet_command(exe).arg("--version").output() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(if err.is_empty() {
                format!("it exited with {}", out.status)
            } else {
                err
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

/// The backup `install_binary` leaves for `dest` (`wcr.bak`, `wcr.exe.bak`).
fn backup_path(dest: &Path) -> PathBuf {
    if cfg!(windows) {
        dest.with_extension("exe.bak")
    } else {
        dest.with_extension("bak")
    }
}

/// Put each replaced file's backup back in place. Returns the files that
/// could not be restored. The new file is first moved aside as `.failed`:
/// on Windows a running .exe cannot be deleted, but it can be renamed.
pub fn restore_files(replaced: &[PathBuf]) -> Vec<String> {
    replaced
        .iter()
        .filter(|dest| !restore_one(dest))
        .map(|dest| dest.display().to_string())
        .collect()
}

fn restore_one(dest: &Path) -> bool {
    let backup = backup_path(dest);
    if !backup.is_file() {
        // Nothing was there before this update: nothing to put back.
        return true;
    }
    let mut failed = dest.as_os_str().to_owned();
    failed.push(".failed");
    let failed = PathBuf::from(failed);
    let _ = fs::remove_file(&failed);
    if dest.exists() && fs::rename(dest, &failed).is_err() {
        return false;
    }
    if fs::rename(&backup, dest).is_ok() {
        return true;
    }
    // Put the new file back rather than leave nothing in place.
    let _ = fs::rename(&failed, dest);
    false
}

fn rollback_message(version: &str, why: &str, not_restored: &[String]) -> String {
    let mut msg =
        format!("update to {version} cancelled: the new wcr does not start here ({why}).");
    if not_restored.is_empty() {
        msg.push_str(" Put the previous version back.");
    } else {
        msg.push_str(&format!(
            " Could not put back: {}. Re-run the installer from https://weechatradio.com",
            not_restored.join(", ")
        ));
    }
    msg
}

/// Each running station leaves its status port here, so `wcr update` can
/// ask every station on this machine, not only the one in its own config
/// (QA v1.0.2: stations with their own `WCR_HOME` were missed).
fn station_marks_dir() -> PathBuf {
    std::env::temp_dir().join("wcr-stations")
}

fn status_port(bind: &str) -> &str {
    bind.rsplit(':').next().unwrap_or("8074")
}

/// Called once the station's status HTTP is listening on `bind`.
pub fn note_running_station(bind: &str) {
    let dir = station_marks_dir();
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(dir.join(status_port(bind)), bind);
    }
}

/// The status ports to ask after an update: this config's, then every
/// station that noted itself, without repeats.
fn station_ports(own_bind: &str, marks: &Path) -> Vec<String> {
    let mut ports = vec![status_port(own_bind).to_string()];
    if let Ok(entries) = fs::read_dir(marks) {
        let mut noted: Vec<String> = entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|name| name.parse::<u16>().is_ok())
            .collect();
        noted.sort();
        for port in noted {
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
    }
    ports
}

/// A station started by hand keeps running the old code after `wcr update`
/// (only a service is restarted), while `wcr --version` already shows the new
/// one. Ask each running station which version it runs. A port that no
/// longer answers has stopped, so its mark is dropped.
pub async fn running_station_notes(own_bind: &str, installed: &str) -> Vec<String> {
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
    else {
        return Vec::new();
    };
    let marks = station_marks_dir();
    let mut notes = Vec::new();
    for port in station_ports(own_bind, &marks) {
        let res = client
            .get(format!("http://127.0.0.1:{port}/status"))
            .send()
            .await;
        let snap: Option<serde_json::Value> = match res {
            Ok(r) => r.json().await.ok(),
            Err(e) => {
                if e.is_connect() {
                    let _ = fs::remove_file(marks.join(&port));
                }
                None
            }
        };
        let running = snap
            .as_ref()
            .and_then(|s| s.get("version"))
            .and_then(|v| v.as_str());
        if let Some(note) = running.and_then(|r| stale_station_note(&port, r, installed)) {
            notes.push(note);
        }
    }
    notes
}

fn stale_station_note(port: &str, running: &str, installed: &str) -> Option<String> {
    (running.trim_start_matches('v') != installed.trim_start_matches('v')).then(|| {
        format!(
            "the station on status port {port} still runs {running}: stop it and start it again to run {installed}"
        )
    })
}

fn update_success_message(
    version: &str,
    exe: &Path,
    replaced: &[PathBuf],
    restarted: bool,
    removed_mintty: bool,
) -> String {
    let mut msg = if restarted {
        format!(
            "updated to {version} at {} — station restarted so radio audio keeps working",
            exe.display()
        )
    } else {
        format!(
            "updated to {version} at {} — close this window and run wcr --version in a new terminal",
            exe.display()
        )
    };
    msg.push_str(&kept_backup_sentence(&fresh_backups(replaced)));
    if removed_mintty {
        msg.push_str(". Removed minttyrc (only used on Windows)");
    }
    msg
}

/// QA v1.0.2: name only the backups this update made, not an older `.bak`
/// left beside a file the new package did not replace.
fn fresh_backups(replaced: &[PathBuf]) -> Vec<String> {
    replaced
        .iter()
        .map(|dest| backup_path(dest))
        .filter(|bak| bak.is_file())
        .filter_map(|bak| bak.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

fn kept_backup_sentence(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => format!(". Kept the previous binary as {one}"),
        many => format!(". Kept the previous binaries as {}", many.join(", ")),
    }
}

/// `install.sh` drops minttyrc on Unix. `wcr update` replaces the binary in
/// place, so an older install can leave the file beside `wcr`.
fn remove_stray_minttyrc(exe: &Path) -> bool {
    #[cfg(windows)]
    {
        let _ = exe;
        false
    }
    #[cfg(not(windows))]
    {
        let Some(dir) = exe.parent() else {
            return false;
        };
        let path = dir.join("minttyrc");
        path.is_file() && fs::remove_file(&path).is_ok()
    }
}

/// Where this binary is installed. Taken once at start, since on Linux the
/// running file's path becomes `wcr.bak` after an update renames it.
fn running_exe() -> Result<PathBuf> {
    Ok(without_verbatim(crate::restart::wcr_exe()))
}

/// On Windows `canonicalize` returns `\\?\C:\...`. Drop that prefix so
/// `wcr update` prints the path people know (`\\?\UNC\` becomes `\\`).
fn without_verbatim(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy().into_owned();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match s.strip_prefix(r"\\?\") {
        Some(rest) => PathBuf::from(rest),
        None => p,
    }
}

fn verify_sha256(manifest: &str, asset_name: &str, bytes: &[u8]) -> Result<()> {
    let want = manifest
        .lines()
        .find_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            if line.len() == 64 && line.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(line.to_ascii_lowercase());
            }
            let (hash, file) = line.split_once("  ")?;
            if file == asset_name || file.ends_with(asset_name) {
                Some(hash.to_ascii_lowercase())
            } else {
                None
            }
        })
        .ok_or_else(|| Error::Msg("checksum file has no hash for this asset".into()))?;
    let got = hex::encode(Sha256::digest(bytes));
    if got != want {
        return Err(Error::Msg("checksum mismatch — download rejected".into()));
    }
    Ok(())
}

fn extract_payload(bytes: &[u8], name: Option<&str>) -> Result<PathBuf> {
    let tmp = std::env::temp_dir().join(format!("wcr-update-{}", std::process::id()));
    fs::create_dir_all(&tmp)?;
    let fname = name.unwrap_or("wcr.bin");
    let lower = fname.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        let zip_path = tmp.join("bundle.zip");
        fs::write(&zip_path, bytes)?;
        extract_zip(&zip_path, &tmp)?;
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let tgz = tmp.join("bundle.tgz");
        fs::write(&tgz, bytes)?;
        let status = Command::new("tar")
            .args(["-xzf", tgz.to_str().unwrap_or("")])
            .current_dir(&tmp)
            .status()
            .map_err(|e| Error::Msg(format!("tar failed: {e}")))?;
        if !status.success() {
            return Err(Error::Msg("could not unpack release archive".into()));
        }
    } else {
        let bin = tmp.join("wcr");
        fs::write(&bin, bytes)?;
        return Ok(bin);
    }
    find_wcr_binary(&tmp)
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let ps = format!(
            "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
            zip_path.display(),
            dest.display()
        );
        let status = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .status()
            .map_err(|e| Error::Msg(format!("Expand-Archive failed: {e}")))?;
        if !status.success() {
            return Err(Error::Msg("could not unpack zip release".into()));
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("unzip")
            .args([
                "-o",
                zip_path.to_str().unwrap_or(""),
                "-d",
                dest.to_str().unwrap_or(""),
            ])
            .status()
            .map_err(|e| Error::Msg(format!("unzip failed: {e}")))?;
        if !status.success() {
            return Err(Error::Msg("could not unpack zip release".into()));
        }
        Ok(())
    }
}

fn find_wcr_binary(dir: &Path) -> Result<PathBuf> {
    let name = if cfg!(windows) { "wcr.exe" } else { "wcr" };
    find_named(dir, name)
        .ok_or_else(|| Error::Msg("release archive did not contain wcr binary".into()))
}

fn find_named(dir: &Path, name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, name: &str, depth: u8) -> Option<PathBuf> {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        if depth == 0 {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(found) = walk(&p, name, depth - 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(dir, name, 3)
}

fn find_dir_named(dir: &Path, name: &str) -> Option<PathBuf> {
    fn walk(dir: &Path, name: &str, depth: u8) -> Option<PathBuf> {
        let direct = dir.join(name);
        if direct.is_dir() {
            return Some(direct);
        }
        if depth == 0 {
            return None;
        }
        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(found) = walk(&p, name, depth - 1) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(dir, name, 3)
}

fn replace_dir(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    copy_tree(src, dst)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn install_binary(dest: &Path, src: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(src)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(src, perms)?;
        let backup = dest.with_extension("bak");
        let _ = fs::rename(dest, &backup);
        if fs::rename(src, dest).is_err() {
            fs::copy(src, dest)?;
            let _ = fs::remove_file(src);
        }
        return Ok(());
    }
    #[cfg(windows)]
    {
        replace_windows_exe(dest, src)
    }
}

/// Windows will not overwrite a running .exe, but it will rename one.
/// Move the current file aside as `.exe.bak` (every time, like Unix, so
/// the backup is always this update's), then copy the new one into place.
#[cfg(windows)]
fn replace_windows_exe(dest: &Path, src: &Path) -> Result<()> {
    if !dest.exists() {
        return fs::copy(src, dest)
            .map(|_| ())
            .map_err(|e| Error::Msg(format!("could not write {}: {e}", dest.display())));
    }
    let bak = dest.with_extension("exe.bak");
    let _ = fs::remove_file(&bak);
    if let Err(e) = fs::rename(dest, &bak) {
        // An older .exe.bak still running cannot be moved: write over the
        // file in place (no backup this time), as long as it is not running.
        if fs::copy(src, dest).is_ok() {
            return Ok(());
        }
        return Err(Error::Msg(format!(
            "could not replace {} ({e}). Close WeeChat Radio and retry, or run: irm https://weechatradio.com/install.ps1 | iex",
            dest.display()
        )));
    }
    if let Err(e) = fs::copy(src, dest) {
        let _ = fs::rename(&bak, dest);
        return Err(Error::Msg(format!(
            "could not write {}: {e}",
            dest.display()
        )));
    }
    Ok(())
}

fn unsupported_platform_msg(version: &str) -> Error {
    Error::Msg(format!(
        "update {version} is available but no binary for this OS ({}/{}). See https://github.com/{REPO}/releases",
        std::env::consts::OS,
        std::env::consts::ARCH
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // QA v0.1.113: a new wcr that cannot start must give way to the old one.
    #[cfg(unix)]
    #[test]
    fn a_new_binary_that_cannot_start_is_rolled_back() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("wcr");
        fs::write(dir.path().join("wcr.bak"), "old").unwrap();
        fs::write(&exe, "#!/bin/sh\necho 'GLIBC_2.43 not found' >&2\nexit 1\n").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        let gui = dir.path().join("wcr-gui");
        fs::write(&gui, "new gui, nothing to restore").unwrap();

        let why = new_binary_starts(&exe).unwrap_err();
        assert!(why.contains("GLIBC_2.43"), "{why}");
        assert!(restore_files(&[exe.clone(), gui.clone()]).is_empty());
        assert_eq!(fs::read_to_string(&exe).unwrap(), "old");
        assert!(!dir.path().join("wcr.bak").exists());
        assert!(gui.exists());
        assert_eq!(
            rollback_message("0.1.114", &why, &[]),
            "update to 0.1.114 cancelled: the new wcr does not start here (GLIBC_2.43 not found). Put the previous version back."
        );
    }

    #[test]
    fn update_prints_the_path_without_the_verbatim_prefix() {
        let plain = |s: &str| without_verbatim(PathBuf::from(s));
        assert_eq!(
            plain(r"\\?\C:\Users\tobo\AppData\Local\wcr\wcr.exe"),
            PathBuf::from(r"C:\Users\tobo\AppData\Local\wcr\wcr.exe")
        );
        assert_eq!(
            plain(r"\\?\UNC\server\share\wcr.exe"),
            PathBuf::from(r"\\server\share\wcr.exe")
        );
        assert_eq!(
            plain("/home/tobo/.local/bin/wcr"),
            PathBuf::from("/home/tobo/.local/bin/wcr")
        );
    }

    #[test]
    fn a_station_left_on_the_old_version_is_named() {
        assert_eq!(stale_station_note("8074", "0.1.92", "0.1.92"), None);
        assert_eq!(stale_station_note("8074", "0.1.92", "v0.1.92"), None);
        let note = stale_station_note("28303", "0.1.91", "0.1.92").unwrap();
        assert!(note.contains("0.1.91") && note.contains("start it again"));
        assert!(note.contains("28303"), "{note}");
    }

    #[test]
    fn every_noted_station_is_asked_once() {
        let marks = tempfile::tempdir().unwrap();
        for port in ["28304", "28303", "8074"] {
            fs::write(marks.path().join(port), format!("127.0.0.1:{port}")).unwrap();
        }
        fs::write(marks.path().join("junk.txt"), "x").unwrap();
        assert_eq!(
            station_ports("0.0.0.0:8074", marks.path()),
            vec!["8074", "28303", "28304"]
        );
        let none = marks.path().join("missing");
        assert_eq!(station_ports("127.0.0.1:28301", &none), vec!["28301"]);
    }

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_string(),
            browser_download_url: format!("https://example.test/{name}"),
        }
    }

    fn release(tag: &str, assets: Vec<Asset>) -> Release {
        Release {
            tag_name: tag.to_string(),
            assets,
        }
    }

    #[test]
    fn a_github_rate_limit_says_try_again() {
        let limited = github_http_error("releases", reqwest::StatusCode::FORBIDDEN).to_string();
        assert!(limited.contains("rate-limiting"), "{limited}");
        assert!(limited.contains("try again in a minute"), "{limited}");
        let other = github_http_error("releases", reqwest::StatusCode::NOT_FOUND).to_string();
        assert!(!other.contains("rate-limiting"), "{other}");
        assert!(other.contains("404"), "{other}");
    }

    #[test]
    fn a_tag_without_a_release_yet_is_named() {
        // QA v0.1.111: "already up to date (0.1.110)" while 0.1.111 was building.
        let names: Vec<String> = ["v0.1.111", "v0.1.110", "v0.1.109", "not-a-version"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            highest_newer_name(&names, "0.1.110", false).as_deref(),
            Some("0.1.111")
        );
        assert_eq!(highest_newer_name(&names, "0.1.111", false), None);
    }

    #[test]
    fn picks_newest_release_with_platform_asset() {
        let releases = vec![
            release("v0.1.16", vec![asset("wcr-macos-aarch64.tar.gz")]),
            release(
                "v0.1.15",
                vec![
                    asset("wcr-windows-x86_64.zip"),
                    asset("wcr-windows-x86_64.zip.sha256"),
                ],
            ),
        ];
        let info = best_downloadable_for(&releases, "0.1.0", false, "windows", "x86_64")
            .expect("should find 0.1.15");
        assert_eq!(info.version, "0.1.15");
        assert_eq!(info.name.as_deref(), Some("wcr-windows-x86_64.zip"));
    }

    #[test]
    fn going_back_to_an_old_stable_says_it_has_no_update_button() {
        assert!(stable_note("1.0.15").contains("no Update button"));
        assert_eq!(stable_note("1.0.16"), "");
        assert_eq!(stable_note(FIRST_SELF_UPDATING), "");
    }

    #[test]
    fn release_candidates_sort_before_their_final_release() {
        assert!(parse_version("v1.0.0-rc1") > parse_version("v0.1.114"));
        assert!(parse_version("v1.0.0-rc2") > parse_version("v1.0.0-rc1"));
        assert!(parse_version("v1.0.0") > parse_version("v1.0.0-rc2"));
        assert_eq!(parse_version("v1.0.0-beta"), None);
    }

    #[test]
    fn a_release_candidate_is_offered_only_when_asked_or_already_on_one() {
        let win = || vec![asset("wcr-windows-x86_64.zip")];
        let releases = vec![release("v1.0.0-rc1", win()), release("v0.1.114", win())];
        let plain = best_downloadable_for(&releases, "0.1.113", false, "windows", "x86_64");
        assert_eq!(plain.unwrap().version, "0.1.114");
        let info = best_downloadable_for(&releases, "0.1.114", true, "windows", "x86_64");
        assert_eq!(info.unwrap().version, "1.0.0-rc1");
        assert!(best_downloadable_for(&releases, "0.1.114", false, "windows", "x86_64").is_none());
        // A station on an RC moves on to the next RC and the final release.
        let later = vec![
            release("v1.0.0", win()),
            release("v1.0.0-rc2", win()),
            release("v1.0.0-rc1", win()),
        ];
        let info = best_downloadable_for(&later, "1.0.0-rc1", true, "windows", "x86_64");
        assert_eq!(info.unwrap().version, "1.0.0");
        // Test versions off on an RC: only the final release is offered.
        let next_rc = vec![release("v1.0.0-rc2", win()), release("v0.1.114", win())];
        assert!(best_downloadable_for(&next_rc, "1.0.0-rc1", false, "windows", "x86_64").is_none());
    }

    #[test]
    fn windows_prefers_exact_zip_over_fuzzy_match() {
        let assets = vec![
            asset("modem73-windows-x86_64.exe"),
            asset("wcr-windows-x86_64.zip"),
        ];
        let picked = pick_asset_for(&assets, "windows", "x86_64").unwrap();
        assert_eq!(picked.name, "wcr-windows-x86_64.zip");
    }

    #[test]
    fn skips_newer_tag_without_matching_asset() {
        let releases = vec![
            release("v0.1.16", vec![asset("wcr-macos-aarch64.tar.gz")]),
            release("v0.1.15", vec![asset("wcr-windows-x86_64.zip")]),
        ];
        let info = best_downloadable_for(&releases, "0.1.15", false, "windows", "x86_64");
        assert!(info.is_none());
        assert_eq!(
            highest_newer_tag(&releases, "0.1.15", false).as_deref(),
            Some("0.1.16")
        );
    }

    #[test]
    fn update_note_mentions_bak_and_drops_unix_minttyrc() {
        let dir = std::env::temp_dir().join(format!("wcr-upd-note-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) { "wcr.exe" } else { "wcr" });
        fs::write(&exe, b"new").unwrap();
        let bak = if cfg!(windows) {
            "wcr.exe.bak"
        } else {
            "wcr.bak"
        };
        fs::write(dir.join(bak), b"old").unwrap();
        fs::write(dir.join("wcr-gui.bak"), b"old").unwrap();
        fs::write(dir.join("minttyrc"), b"theme").unwrap();
        let removed = remove_stray_minttyrc(&exe);
        if cfg!(windows) {
            assert!(!removed);
            assert!(dir.join("minttyrc").is_file());
        } else {
            assert!(removed);
            assert!(!dir.join("minttyrc").exists());
        }
        let msg = update_success_message("0.1.81", &exe, &[exe.clone()], false, removed);
        assert!(msg.contains(bak), "{msg}");
        // The GUI was not replaced this time, so its older backup is not named.
        assert!(!msg.contains("wcr-gui"), "{msg}");
        assert!(msg.contains("wcr --version"), "{msg}");
        if removed {
            assert!(msg.contains("Removed minttyrc"), "{msg}");
        } else {
            assert!(!msg.contains("Removed minttyrc"), "{msg}");
        }
        let none = kept_backup_sentence(&[]);
        assert!(none.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_nested_wcr_exe() {
        let dir = std::env::temp_dir().join(format!("wcr-find-{}", std::process::id()));
        let nested = dir.join("dist");
        fs::create_dir_all(&nested).unwrap();
        let bin = nested.join("wcr.exe");
        fs::write(&bin, b"x").unwrap();
        let found = find_named(&dir, "wcr.exe").unwrap();
        assert_eq!(found, bin);
        let _ = fs::remove_dir_all(&dir);
    }
}
