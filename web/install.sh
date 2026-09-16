#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Install wcr from GitHub Releases onto Linux or macOS.
set -eu
REPO="thaum-labs/weechat-radio"
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
esac
ASSET="wcr-${OS}-${ARCH}.tar.gz"
if [ "$OS" = "darwin" ]; then ASSET="wcr-macos-${ARCH}.tar.gz"; fi
if [ "$OS" = "linux" ]; then ASSET="wcr-linux-${ARCH}.tar.gz"; fi
TMP=$(mktemp -d)
URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
echo "Downloading $URL"
curl -fsSL "$URL" -o "$TMP/wcr.tgz" || {
  echo "No release asset yet. Build from source:"
  echo "  git clone https://github.com/${REPO} && cd weechat-radio && cargo install --path crates/wcr"
  exit 1
}
tar -xzf "$TMP/wcr.tgz" -C "$TMP"
BIN="$TMP/wcr"
chmod +x "$BIN"
PREFIX="${PREFIX:-$HOME/.local/bin}"
mkdir -p "$PREFIX"
mv "$BIN" "$PREFIX/wcr"
if [ -f "$TMP/modem73" ]; then
  chmod +x "$TMP/modem73"
  mv "$TMP/modem73" "$PREFIX/modem73"
  echo "Installed $PREFIX/modem73"
fi
echo "Installed $PREFIX/wcr"
echo "Next:  wcr setup"
