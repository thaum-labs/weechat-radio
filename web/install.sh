#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Install wcr, modem73, and WeeChat. Configures WeeChat for the local node.
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
MODEM=""
if [ -f "$TMP/modem73" ]; then MODEM="$TMP/modem73"; fi
if [ -z "$MODEM" ]; then MODEM=$(find "$TMP" -maxdepth 2 -type f -name modem73 2>/dev/null | head -n 1); fi
if [ "$OS" = "linux" ]; then
  if [ -z "$MODEM" ]; then
    echo "Release archive is missing modem73. Use v0.1.1 or newer." >&2
    exit 1
  fi
  chmod +x "$MODEM"
  mv "$MODEM" "$PREFIX/modem73"
  echo "Installed $PREFIX/modem73"
elif [ -n "$MODEM" ]; then
  chmod +x "$MODEM"
  mv "$MODEM" "$PREFIX/modem73"
  echo "Installed $PREFIX/modem73"
fi
RADIO=""
if [ -f "$TMP/radio.py" ]; then RADIO="$TMP/radio.py"; fi
if [ -z "$RADIO" ]; then RADIO=$(find "$TMP" -maxdepth 3 -type f -name radio.py 2>/dev/null | head -n 1); fi
if [ -n "$RADIO" ]; then
  cp "$RADIO" "$PREFIX/radio.py"
else
  curl -fsSL "https://raw.githubusercontent.com/${REPO}/main/weechat/radio.py" -o "$PREFIX/radio.py"
fi
echo "Installed $PREFIX/wcr"

if [ "${WCR_SKIP_WEECHAT:-}" != "1" ]; then
  if ! command -v weechat >/dev/null 2>&1; then
    echo "Installing WeeChat..."
    if [ "$OS" = "darwin" ] && command -v brew >/dev/null 2>&1; then
      brew install weechat || true
    elif command -v apt-get >/dev/null 2>&1; then
      sudo apt-get update -y && sudo apt-get install -y weechat weechat-python || true
    elif command -v dnf >/dev/null 2>&1; then
      sudo dnf install -y weechat || true
    else
      echo "Install WeeChat from https://weechat.org/ then run: wcr weechat --configure"
    fi
  fi
  if command -v weechat >/dev/null 2>&1 || command -v weechat-headless >/dev/null 2>&1; then
    "$PREFIX/wcr" weechat --configure || true
  fi
fi

echo "Next:"
echo "  wcr setup"
echo "  wcr node"
echo "  wcr weechat"
echo "Or use the built-in UI:  wcr tui"
