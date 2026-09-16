# SPDX-License-Identifier: Apache-2.0
# Install wcr on Windows from GitHub Releases.
$ErrorActionPreference = "Stop"
$repo = "thaum-labs/weechat-radio"
$asset = "wcr-windows-x86_64.zip"
$url = "https://github.com/$repo/releases/latest/download/$asset"
$tmp = Join-Path $env:TEMP "wcr-install"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$zip = Join-Path $tmp "wcr.zip"
Write-Host "Downloading $url"
try {
    Invoke-WebRequest -Uri $url -OutFile $zip
} catch {
    Write-Host "No release asset yet. Build from source with rustup, then: cargo install --path crates/wcr"
    exit 1
}
Expand-Archive -Path $zip -DestinationPath $tmp -Force
$dir = Join-Path $env:LOCALAPPDATA "wcr"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Copy-Item (Join-Path $tmp "wcr.exe") (Join-Path $dir "wcr.exe") -Force
$envPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($envPath -notlike "*$dir*") {
    [Environment]::SetEnvironmentVariable("Path", "$envPath;$dir", "User")
}
Write-Host "Installed $dir\wcr.exe"
Write-Host "Open a new terminal, then:  wcr setup"
