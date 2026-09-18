#!/usr/bin/env python3
"""Update install/* hashes from GitHub release .sha256 sidecars."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INSTALL = ROOT / "install"


def read_hash(path: Path) -> str:
    return path.read_text(encoding="utf-8").strip().lower()


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: sync_install_sha256.py <download-dir>", file=sys.stderr)
        return 2
    dl = Path(sys.argv[1])
    mapping = {
        "wcr-windows-x86_64.zip.sha256": ("scoop",),
        "wcr-linux-x86_64.tar.gz.sha256": ("homebrew", "linux-intel"),
        "wcr-linux-aarch64.tar.gz.sha256": ("homebrew", "linux-arm"),
        "wcr-macos-x86_64.tar.gz.sha256": ("homebrew", "mac-intel"),
        "wcr-macos-aarch64.tar.gz.sha256": ("homebrew", "mac-arm"),
    }
    hashes: dict[str, str] = {}
    for name in mapping:
        p = dl / name
        if p.is_file():
            hashes[name] = read_hash(p)

    scoop = INSTALL / "scoop" / "wcr.json"
    if scoop.is_file() and "wcr-windows-x86_64.zip.sha256" in hashes:
        data = json.loads(scoop.read_text(encoding="utf-8"))
        data["architecture"]["64bit"]["hash"] = hashes["wcr-windows-x86_64.zip.sha256"]
        scoop.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
        print("updated", scoop)

    brew = INSTALL / "homebrew" / "wcr.rb"
    if brew.is_file():
        text = brew.read_text(encoding="utf-8")
        pairs = [
            ("wcr-linux-x86_64.tar.gz", "wcr-linux-x86_64.tar.gz.sha256"),
            ("wcr-linux-aarch64.tar.gz", "wcr-linux-aarch64.tar.gz.sha256"),
            ("wcr-macos-x86_64.tar.gz", "wcr-macos-x86_64.tar.gz.sha256"),
            ("wcr-macos-aarch64.tar.gz", "wcr-macos-aarch64.tar.gz.sha256"),
        ]
        for asset, sidecar in pairs:
            if sidecar not in hashes:
                continue
            h = hashes[sidecar]
            text, n = re.subn(
                rf'(url "{re.escape("https://github.com/thaum-labs/weechat-radio/releases/download/v")}[^/]+/{re.escape(asset)}"\s*\n\s*sha256 ")[0-9a-f]+(")',
                rf"\g<1>{h}\2",
                text,
                count=1,
            )
            if n:
                print("homebrew", asset, h[:16] + "…")
        brew.write_text(text, encoding="utf-8")

    winget = INSTALL / "winget" / "ThaumLabs.Wcr.installer.yaml"
    if winget.is_file() and "wcr-windows-x86_64.zip.sha256" in hashes:
        text = winget.read_text(encoding="utf-8")
        h = hashes["wcr-windows-x86_64.zip.sha256"].upper()
        text = re.sub(
            r"(InstallerSha256: )[0-9A-Fa-f]+",
            rf"\g<1>{h}",
            text,
            count=1,
        )
        winget.write_text(text, encoding="utf-8")
        print("updated", winget)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
