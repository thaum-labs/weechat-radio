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
    if cfg!(windows) {
        Some("modem73-windows-x86_64.exe")
    } else if cfg!(all(unix, not(target_os = "macos"))) {
        Some("modem73-linux-x86_64")
    } else {
        None
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
/// the binary we ship on GitHub Releases (Windows and Linux).
pub async fn ensure_binary(configured: &str) -> Result<PathBuf> {
    let resolved = resolve_binary(configured);
    if resolved.is_file() {
        return Ok(resolved);
    }
    let dest = beside_wcr(Path::new(exe_name()))
        .filter(|p| p.parent().map(|d| is_writable_dir(d)).unwrap_or(false))
        .or_else(data_path)
        .ok_or_else(|| Error::Modem("cannot pick a folder for modem73".into()))?;
    if dest.is_file() {
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
    download(asset, &dest).await?;
    Ok(dest)
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
    let client = reqwest::Client::builder()
        .user_agent("wcr")
        .build()
        .map_err(|e| Error::Net(e.to_string()))?;
    let res = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if !res.status().is_success() {
        return Err(Error::Modem(format!(
            "could not download {url}: HTTP {}",
            res.status()
        )));
    }
    let bytes = res
        .bytes()
        .await
        .map_err(|e| Error::Net(e.to_string()))?;
    if bytes.len() < 1024 {
        return Err(Error::Modem(format!(
            "downloaded modem73 is too small ({})",
            bytes.len()
        )));
    }
    let tmp = dest.with_extension("download");
    std::fs::write(&tmp, &bytes)?;
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
