//! SPDX-License-Identifier: Apache-2.0
//! Find a bundled modem73, or download the one shipped on our GitHub Releases.

use crate::config;
use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

const RELEASES: &str = "https://github.com/thaum-labs/weechat-radio/releases/latest/download";

pub fn exe_name() -> &'static str {
    if cfg!(windows) {
        "modem73.exe"
    } else {
        "modem73"
    }
}

fn release_asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", _) => Some("modem73-windows-x86_64.exe"),
        ("linux", "aarch64") => Some("modem73-linux-aarch64"),
        ("linux", _) => Some("modem73-linux-x86_64"),
        ("macos", "aarch64") => Some("modem73-macos-aarch64.tar.gz"),
        ("macos", _) => Some("modem73-macos-x86_64.tar.gz"),
        _ => None,
    }
}

pub fn resolve_binary(configured: &str) -> PathBuf {
    let raw = PathBuf::from(configured);
    if raw.is_absolute() {
        return raw;
    }
    let name = if cfg!(windows) && raw.extension().is_none() {
        raw.with_extension("exe")
    } else {
        raw
    };
    if let Some(beside) = beside_wcr(&name) {
        if beside.is_file() {
            return beside;
        }
    }
    if let Some(found) = search_path(&name) {
        return found;
    }
    if let Some(data) = data_path() {
        if data.is_file() {
            return data;
        }
    }
    name
}

fn beside_wcr(name: &Path) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(name))
}

fn data_path() -> Option<PathBuf> {
    Some(config::default_data_dir().join(exe_name()))
}

fn search_path(name: &Path) -> Option<PathBuf> {
    let file = name.file_name()?;
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(file);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Locate modem73 next to `wcr`, on PATH, or in the data dir. If missing, download
/// the binary we ship on GitHub Releases (Windows, Linux, and macOS).
pub async fn ensure_binary(configured: &str) -> Result<PathBuf> {
    let resolved = resolve_binary(configured);
    let user_path = Path::new(configured).is_absolute();
    if resolved.is_file() && (user_path || bundle_complete(&resolved)) {
        return Ok(resolved);
    }
    let dest = beside_wcr(Path::new(exe_name()))
        .filter(|p| p.parent().map(|d| is_writable_dir(d)).unwrap_or(false))
        .or_else(data_path)
        .ok_or_else(|| Error::Modem("cannot pick a folder for modem73".into()))?;
    if dest.is_file() && bundle_complete(&dest) {
        return Ok(dest);
    }
    let Some(asset) = release_asset() else {
        return Err(Error::Modem(
            "no official modem73 build for this OS. Install it from https://modem73.app and set modem.binary in wcr.toml.".into(),
        ));
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    tracing::info!("downloading bundled modem73 to {}", dest.display());
    match download(asset, &dest).await {
        Ok(()) => Ok(dest),
        Err(e) => {
            #[cfg(target_os = "macos")]
            {
                tracing::warn!("{e}; trying RFnexus/modem73");
                download_rfnexus_macos(&dest).await?;
                Ok(dest)
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err(e)
            }
        }
    }
}

/// macOS modem73 loads hamlib from `@executable_path/libs`.
fn bundle_complete(binary: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return true;
    }
    binary
        .parent()
        .map(|p| p.join("libs").is_dir())
        .unwrap_or(false)
}

fn is_writable_dir(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(".wcr-write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

async fn download(asset: &str, dest: &Path) -> Result<()> {
    let url = format!("{RELEASES}/{asset}");
    let bytes = fetch_bytes(&url).await?;
    if asset.ends_with(".tar.gz") {
        unpack_tar_gz(&bytes, dest)?;
    } else {
        write_binary(dest, &bytes)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn download_rfnexus_macos(dest: &Path) -> Result<()> {
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    let needle = format!("macos-{arch}.tar.gz");
    let client = reqwest::Client::builder()
        .user_agent("wcr")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let json: serde_json::Value = client
        .get("https://api.github.com/repos/RFnexus/modem73/releases/latest")
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Net(e.to_string()))?
        .json()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    let url = json["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|a| {
            let name = a["name"].as_str()?;
            if name.contains(&needle) && !name.contains("sha") {
                a["browser_download_url"].as_str().map(str::to_string)
            } else {
                None
            }
        })
        .ok_or_else(|| Error::Modem(format!("no RFnexus modem73 asset matching {needle}")))?;
    tracing::info!("downloading {url}");
    let bytes = fetch_bytes(&url).await?;
    unpack_tar_gz(&bytes, dest)
}

async fn fetch_bytes(url: &str) -> Result<bytes::Bytes> {
    let client = reqwest::Client::builder()
        .user_agent("wcr")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if !res.status().is_success() {
        return Err(Error::Modem(format!(
            "could not download {url}: HTTP {}",
            res.status()
        )));
    }
    let bytes = res.bytes().await.map_err(|e| Error::Net(e.to_string()))?;
    if bytes.len() < 1024 {
        return Err(Error::Modem(format!(
            "downloaded modem73 is too small ({})",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn write_binary(dest: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = dest.with_extension("download");
    std::fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

fn unpack_tar_gz(bytes: &[u8], dest_binary: &Path) -> Result<()> {
    let parent = dest_binary
        .parent()
        .ok_or_else(|| Error::Modem("modem73 path has no folder".into()))?;
    let scratch = parent.join(".modem73-unpack");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)?;
    let tgz = scratch.join("bundle.tar.gz");
    std::fs::write(&tgz, bytes)?;
    let status = std::process::Command::new("tar")
        .arg("-xzf")
        .arg(&tgz)
        .arg("-C")
        .arg(&scratch)
        .status()
        .map_err(|e| Error::Modem(e.to_string()))?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&scratch);
        return Err(Error::Modem("could not unpack modem73 archive".into()));
    }
    let bin = find_file_named(&scratch, "modem73")
        .ok_or_else(|| Error::Modem("modem73 archive does not contain a modem73 binary".into()))?;
    write_binary(dest_binary, &std::fs::read(&bin)?)?;
    let libs = find_libs_dir(&scratch)
        .ok_or_else(|| Error::Modem("modem73 archive is missing libs/".into()))?;
    let dest_libs = parent.join("libs");
    let _ = std::fs::remove_dir_all(&dest_libs);
    copy_dir(&libs, &dest_libs)?;
    strip_quarantine(dest_binary);
    strip_quarantine(&dest_libs);
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(())
}

pub(crate) fn strip_quarantine(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(path)
            .status();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
    }
}

fn find_file_named(root: &Path, name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if entry.file_name() == name {
                return Some(path);
            }
        }
    }
    None
}

fn find_libs_dir(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if entry.file_name() == "libs" {
                return Some(path);
            }
            stack.push(path);
        }
    }
    None
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_binary;
    use std::path::Path;

    #[test]
    fn keeps_absolute_paths() {
        let p = if cfg!(windows) {
            r"C:\tools\modem73.exe"
        } else {
            "/usr/local/bin/modem73"
        };
        assert_eq!(resolve_binary(p), Path::new(p));
    }
}
