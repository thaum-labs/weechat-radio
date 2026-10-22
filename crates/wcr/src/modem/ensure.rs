//! SPDX-License-Identifier: Apache-2.0
//! Find a bundled modem73, or download the one shipped on our GitHub Releases.

use crate::config;
use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const PINNED_SHA256_JSON: &str = include_str!("../../../../third_party/modem73/sha256.json");

/// SHA-256 of this platform's modem73 asset, set by the release workflow
/// from the exact file it bundles and uploads beside this wcr build.
const RELEASE_MODEM73_SHA256: Option<&str> = option_env!("WCR_MODEM73_SHA256");

/// Release builds download modem73 from their own tag, so the fallback is the
/// same binary the release bundles. Other builds (cargo build, CI) use the
/// older pin checked by `third_party/modem73/sha256.json`.
/// Never `/releases/latest`: a newer tag would not match the hash.
fn modem73_release() -> String {
    if RELEASE_MODEM73_SHA256.is_some() {
        format!("v{}", env!("CARGO_PKG_VERSION"))
    } else {
        DEV_MODEM73_RELEASE.to_string()
    }
}

const DEV_MODEM73_RELEASE: &str = "v0.1.79";

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
    let url = format!(
        "https://github.com/thaum-labs/weechat-radio/releases/download/{}/{asset}",
        modem73_release()
    );
    let bytes = match RELEASE_MODEM73_SHA256 {
        Some(expected) => {
            let bytes = fetch_bytes(&url).await?;
            verify_sha256_hex(&bytes, expected)?;
            bytes
        }
        None => fetch_verified_bytes(&url).await?,
    };
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
    let bytes = fetch_verified_bytes(&url).await?;
    unpack_tar_gz(&bytes, dest)
}

/// modem73 has no `--version` and its control port does not report one. Its
/// TUI title bar is the literal `" v" MODEM73_VERSION`, so that string sits
/// in every build as `\0 v2.4.2\0`. Read it from the file on disk.
pub fn binary_version(binary: &Path) -> Option<String> {
    version_in(&std::fs::read(binary).ok()?)
}

fn version_in(bytes: &[u8]) -> Option<String> {
    const MARK: &[u8] = b"\0 v";
    let mut from = 0;
    while let Some(at) = find(&bytes[from..], MARK) {
        let start = from + at + MARK.len();
        let rest = &bytes[start..];
        let end = rest.iter().position(|b| *b == 0)?;
        let text = std::str::from_utf8(&rest[..end]).unwrap_or("");
        if looks_like_version(text) {
            return Some(text.to_string());
        }
        from = start;
    }
    None
}

/// `2.4.2`, `2.4.3-rc1`: three dotted numbers, an optional short suffix.
fn looks_like_version(text: &str) -> bool {
    if text.len() > 24 {
        return false;
    }
    let core = text.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn pinned_sha256_table() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(PINNED_SHA256_JSON).unwrap_or_else(|e| {
            tracing::warn!("modem73 pinned sha256 table invalid: {e}");
            HashMap::new()
        })
    })
}

fn asset_name_from_url(url: &str) -> Option<&str> {
    url.rsplit('/').next().filter(|s| !s.is_empty())
}

async fn fetch_verified_bytes(url: &str) -> Result<bytes::Bytes> {
    let bytes = fetch_bytes(url).await?;
    let asset = asset_name_from_url(url).unwrap_or_default();
    if let Some(expected) = pinned_sha256_table().get(asset) {
        if expected.chars().all(|c| c == '0') {
            return Err(Error::Modem(format!(
                "pinned SHA-256 for {asset} is not set in-repo; update third_party/modem73/sha256.json"
            )));
        }
        verify_sha256_hex(&bytes, expected)?;
        return Ok(bytes);
    }
    if let Ok(expected) = fetch_bytes(&format!("{url}.sha256")).await {
        verify_sha256_file(&bytes, &expected)?;
        return Ok(bytes);
    }
    Err(Error::Modem(format!(
        "no SHA-256 checksum for {url} (pin it in third_party/modem73/sha256.json or publish a .sha256 sidecar)"
    )))
}

fn verify_sha256_hex(bytes: &[u8], expected: &str) -> Result<()> {
    let expected = expected.trim().to_ascii_lowercase();
    if expected.len() != 64 {
        return Err(Error::Modem("invalid pinned SHA-256".into()));
    }
    let got = hex::encode(Sha256::digest(bytes));
    if got != expected {
        return Err(Error::Modem(format!(
            "checksum mismatch (expected {expected}, got {got})"
        )));
    }
    Ok(())
}

fn verify_sha256_file(bytes: &[u8], sidecar: &[u8]) -> Result<()> {
    let line = sidecar
        .split(|b| *b == b'\n' || *b == b'\r')
        .next()
        .unwrap_or(sidecar);
    let expected = std::str::from_utf8(line)
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if expected.len() != 64 {
        return Err(Error::Modem("invalid .sha256 sidecar".into()));
    }
    let got = hex::encode(Sha256::digest(bytes));
    if got != expected {
        return Err(Error::Modem(format!(
            "checksum mismatch (expected {expected}, got {got})"
        )));
    }
    Ok(())
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

    #[test]
    fn modem_download_is_a_tagged_release() {
        let tag = super::modem73_release();
        assert!(tag.starts_with('v'));
        assert!(!tag.contains("latest"));
    }

    #[test]
    fn version_is_read_from_the_title_bar_string() {
        let mut bin = b"\x7fELF junk\0 v1.2\0version 2.3>\0".to_vec();
        bin.extend_from_slice(b"\0 v2.4.2\0more\0");
        assert_eq!(super::version_in(&bin).as_deref(), Some("2.4.2"));
        assert_eq!(super::version_in(b"\0 vdev\0"), None);
        assert_eq!(super::version_in(b"nothing here"), None);
    }
}
