//! SPDX-License-Identifier: Apache-2.0
//! Check GitHub Releases and apply `wcr update`. Never silent.

use crate::error::{Error, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
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
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent("wcr")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(Error::Net(format!("GitHub releases HTTP {}", res.status())));
    }
    let rel: Release = res.json().await.map_err(|e| Error::Net(e.to_string()))?;
    let tag = rel.tag_name.trim_start_matches('v').to_string();
    if tag == current_version() {
        return Ok(None);
    }
    let picked = pick_asset(&rel.assets);
    let aname = picked.map(|a| a.name.clone());
    Ok(Some(ReleaseInfo {
        version: tag,
        url: picked.map(|a| a.browser_download_url.clone()),
        name: aname.clone(),
        sums_url: rel.assets.iter().find_map(|a| {
            if a.name.eq_ignore_ascii_case("SHA256SUMS.txt") {
                Some(a.browser_download_url.clone())
            } else if aname
                .as_ref()
                .is_some_and(|n| a.name.eq_ignore_ascii_case(&format!("{n}.sha256")))
            {
                Some(a.browser_download_url.clone())
            } else {
                None
            }
        }),
    }))
}

fn pick_asset(assets: &[Asset]) -> Option<&Asset> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
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
    let Some(info) = latest().await? else {
        return Ok(format!("already up to date ({})", current_version()));
    };
    let Some(url) = info.url else {
        return Err(Error::Msg(format!(
            "update {} is available but no binary for this OS. See https://github.com/{REPO}/releases",
            info.version
        )));
    };
    let client = reqwest::Client::builder()
        .user_agent("wcr")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let bytes = client
        .get(&url)
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

    let exe = std::env::current_exe()?;
    let extracted = extract_payload(&bytes, info.name.as_deref())?;
    install_binary(&exe, &extracted)?;
    Ok(format!(
        "updated to {} — restart wcr (service or terminal) to run the new binary",
        info.version
    ))
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
    let direct = dir.join(name);
    if direct.is_file() {
        return Ok(direct);
    }
    for entry in fs::read_dir(dir).map_err(|e| Error::Msg(e.to_string()))? {
        let entry = entry.map_err(|e| Error::Msg(e.to_string()))?;
        let p = entry.path();
        if p.is_file() && p.file_name().and_then(|s| s.to_str()) == Some(name) {
            return Ok(p);
        }
    }
    Err(Error::Msg(
        "release archive did not contain wcr binary".into(),
    ))
}

fn install_binary(dest: &Path, src: &Path) -> Result<()> {
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
        let new_path = dest.with_extension("new.exe");
        fs::copy(src, &new_path)?;
        let script = format!(
            "timeout /t 2 /nobreak >nul & move /y \"{}\" \"{}\"",
            new_path.display(),
            dest.display()
        );
        Command::new("cmd")
            .args(["/C", &script])
            .spawn()
            .map_err(|e| Error::Msg(format!("could not schedule swap: {e}")))?;
        Ok(())
    }
}

pub async fn check_background() -> Option<String> {
    match latest().await {
        Ok(Some(i)) => Some(i.version),
        _ => None,
    }
}
