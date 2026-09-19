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

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub async fn latest() -> Result<Option<ReleaseInfo>> {
    let client = github_client()?;
    let releases = fetch_releases(&client).await?;
    Ok(best_downloadable(&releases, current_version()))
}

async fn fetch_releases(client: &reqwest::Client) -> Result<Vec<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases?per_page=30");
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if !res.status().is_success() {
        return Err(Error::Net(format!("GitHub releases HTTP {}", res.status())));
    }
    res.json().await.map_err(|e| Error::Net(e.to_string()))
}

fn github_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("wcr")
        .build()
        .map_err(|e| Error::Net(e.to_string()))
}

fn parse_version(tag: &str) -> Option<(u64, u64, u64)> {
    let mut parts = tag.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn version_cmp(a: &str, b: &str) -> Ordering {
    match (parse_version(a), parse_version(b)) {
        (Some(va), Some(vb)) => va.cmp(&vb),
        _ => a.cmp(b),
    }
}

fn is_newer_than_current(tag: &str, current: &str) -> bool {
    version_cmp(tag, current) == Ordering::Greater
}

fn highest_newer_tag(releases: &[Release], current: &str) -> Option<String> {
    releases
        .iter()
        .filter_map(|rel| {
            let tag = rel.tag_name.trim_start_matches('v');
            if is_newer_than_current(tag, current) {
                parse_version(tag).map(|_| tag.to_string())
            } else {
                None
            }
        })
        .max_by(|a, b| version_cmp(a, b))
}

/// Newest release newer than `current` that ships a binary for this OS/arch.
fn best_downloadable(releases: &[Release], current: &str) -> Option<ReleaseInfo> {
    best_downloadable_for(
        releases,
        current,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

fn best_downloadable_for(
    releases: &[Release],
    current: &str,
    os: &str,
    arch: &str,
) -> Option<ReleaseInfo> {
    releases
        .iter()
        .filter_map(|rel| {
            let tag = rel.tag_name.trim_start_matches('v');
            if !is_newer_than_current(tag, current) {
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

pub async fn apply() -> Result<String> {
    let client = github_client()?;
    let releases = fetch_releases(&client).await?;
    let current = current_version();
    let Some(info) = best_downloadable(&releases, current) else {
        if highest_newer_tag(&releases, current).is_some() {
            return Err(Error::Msg(format!(
                "a newer release is still publishing for this platform — try again in a few minutes, or install manually from https://github.com/{REPO}/releases"
            )));
        }
        return Ok(format!("already up to date ({current})"));
    };
    let url = info
        .url
        .as_ref()
        .ok_or_else(|| unsupported_platform_msg(&info.version))?;
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

    let exe = running_exe()?;
    let extracted = extract_payload(&bytes, info.name.as_deref())?;
    install_binary(&exe, &extracted)?;
    if let Some(dir) = extracted.parent() {
        if let Some(parent) = exe.parent() {
            let gui_name = if cfg!(windows) {
                "wcr-gui.exe"
            } else {
                "wcr-gui"
            };
            if let Some(gui_src) = find_named(dir, gui_name) {
                let _ = install_binary(&parent.join(gui_name), &gui_src);
            }
            let modem_name = if cfg!(windows) {
                "modem73.exe"
            } else {
                "modem73"
            };
            if let Some(modem_src) = find_named(dir, modem_name) {
                let _ = install_binary(&parent.join(modem_name), &modem_src);
            }
            if let Some(libs_src) = find_dir_named(dir, "libs") {
                let _ = replace_dir(&libs_src, &parent.join("libs"));
            }
        }
    }
    let restarted = crate::service::restart_if_installed();
    if restarted {
        Ok(format!(
            "updated to {} at {} — station restarted so radio audio keeps working",
            info.version,
            exe.display()
        ))
    } else {
        Ok(format!(
            "updated to {} at {} — close this window and run wcr --version in a new terminal",
            info.version,
            exe.display()
        ))
    }
}

fn running_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(fs::canonicalize(&exe).unwrap_or(exe))
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
/// Move the current file aside, then copy the new one into place.
#[cfg(windows)]
fn replace_windows_exe(dest: &Path, src: &Path) -> Result<()> {
    if fs::copy(src, dest).is_ok() {
        return Ok(());
    }
    if !dest.exists() {
        return Err(Error::Msg(format!("could not write {}", dest.display())));
    }
    let bak = dest.with_extension("exe.bak");
    let _ = fs::remove_file(&bak);
    fs::rename(dest, &bak).map_err(|e| {
        Error::Msg(format!(
            "could not replace {} ({e}). Close WeeChat Radio and retry, or run: irm https://weechatradio.com/install.ps1 | iex",
            dest.display()
        ))
    })?;
    if let Err(e) = fs::copy(src, dest) {
        let _ = fs::rename(&bak, dest);
        return Err(Error::Msg(format!(
            "could not write {}: {e}",
            dest.display()
        )));
    }
    Ok(())
}

pub async fn check_background() -> Option<String> {
    match latest().await {
        Ok(Some(i)) => Some(i.version),
        _ => None,
    }
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
        let info = best_downloadable_for(&releases, "0.1.0", "windows", "x86_64")
            .expect("should find 0.1.15");
        assert_eq!(info.version, "0.1.15");
        assert_eq!(info.name.as_deref(), Some("wcr-windows-x86_64.zip"));
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
        let info = best_downloadable_for(&releases, "0.1.15", "windows", "x86_64");
        assert!(info.is_none());
        assert_eq!(
            highest_newer_tag(&releases, "0.1.15").as_deref(),
            Some("0.1.16")
        );
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
