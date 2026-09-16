# SPDX-License-Identifier: Apache-2.0
# Install wcr, modem73, and WeeChat (Cygwin) on Windows. Configures WeeChat
# to connect to the local WeeChat Radio node.
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
$modem = Get-ChildItem $tmp -Filter "modem73.exe" -Recurse | Select-Object -First 1
if (-not $modem) {
    throw "Release zip is missing modem73.exe. Download v0.1.1 or newer from https://github.com/$repo/releases"
}
Copy-Item $modem.FullName (Join-Path $dir "modem73.exe") -Force
Write-Host "Installed $($dir)\modem73.exe"
$radio = Get-ChildItem $tmp -Filter "radio.py" -Recurse | Select-Object -First 1
if ($radio) {
    Copy-Item $radio.FullName (Join-Path $dir "radio.py") -Force
} else {
    Write-Host "Fetching radio.py"
    Invoke-WebRequest -Uri "https://raw.githubusercontent.com/$repo/main/weechat/radio.py" -OutFile (Join-Path $dir "radio.py")
}
$envPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($envPath -notlike "*$dir*") {
    [Environment]::SetEnvironmentVariable("Path", "$envPath;$dir", "User")
}
Write-Host "Installed $dir\wcr.exe"

if ($env:WCR_SKIP_WEECHAT -ne "1") {
    $cyg = Join-Path $env:USERPROFILE "cygwin64"
    $wee = Join-Path $cyg "bin\weechat.exe"
    if (-not (Test-Path $wee)) {
        Write-Host "Installing WeeChat via Cygwin (a few minutes)..."
        $setup = Join-Path $env:TEMP "cygwin-setup-x86_64.exe"
        $pkg = Join-Path $env:LOCALAPPDATA "cygwin-packages"
        New-Item -ItemType Directory -Force -Path $pkg, $cyg | Out-Null
        Invoke-WebRequest -Uri "https://www.cygwin.com/setup-x86_64.exe" -OutFile $setup
        $setupArgs = @(
            "--quiet-mode", "--only-site", "--no-admin", "--no-desktop",
            "--no-shortcuts", "--no-startmenu",
            "--root", $cyg,
            "--local-package-dir", $pkg,
            "--site", "https://mirrors.kernel.org/sourceware/cygwin/",
            "--packages", "weechat,weechat-python"
        )
        $p = Start-Process -FilePath $setup -ArgumentList $setupArgs -PassThru
        $deadline = (Get-Date).AddMinutes(20)
        while (-not (Test-Path $wee) -and (Get-Date) -lt $deadline) {
            if ($p.HasExited -and -not (Test-Path $wee)) { break }
            Start-Sleep -Seconds 5
            Write-Host "  waiting for WeeChat..."
        }
        if (-not $p.HasExited) { $p.WaitForExit(120000) | Out-Null }
    }
    if (-not (Test-Path $wee)) {
        throw "WeeChat did not install. Run the installer again, or install WeeChat from https://weechat.org/"
    }
    Write-Host "Configuring WeeChat for 127.0.0.1:6667..."
    & (Join-Path $dir "wcr.exe") weechat --configure
    if ($LASTEXITCODE -ne 0) { throw "WeeChat auto-configure failed" }
    Write-Host "Launcher: $dir\weechat-radio.cmd"
}

Write-Host ""
Write-Host "Open a new terminal, then:"
Write-Host "  wcr setup"
Write-Host "  wcr node"
Write-Host "  wcr weechat"
Write-Host "Or use the built-in UI:  wcr tui"
Write-Host "Skip WeeChat next time with:  `$env:WCR_SKIP_WEECHAT=1"
