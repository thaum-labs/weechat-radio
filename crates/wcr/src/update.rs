//! SPDX-License-Identifier: Apache-2.0
//! Check GitHub Releases and apply `wcr update`. Never silent.

use crate::error::{Error, Result};
use serde::Deserialize;

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
    let asset = pick_asset(&rel.assets).map(|a| a.browser_download_url.clone());
    Ok(Some(ReleaseInfo {
        version: tag,
        url: asset,
    }))
}

fn pick_asset(assets: &[Asset]) -> Option<&Asset> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n.contains(os) && (n.contains(arch) || (arch == "x86_64" && n.contains("amd64")))
    })
}

pub struct ReleaseInfo {
    pub version: String,
    pub url: Option<String>,
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
    Ok(format!(
        "update {} is available at {url}\nDownload, verify the checksum on the release page, replace this binary, then restart wcr.",
        info.version
    ))
}

pub async fn check_background() -> Option<String> {
    match latest().await {
        Ok(Some(i)) => Some(i.version),
        _ => None,
    }
}
