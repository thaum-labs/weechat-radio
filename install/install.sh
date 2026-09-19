#!/bin/sh
# SPDX-License-Identifier: Apache-2.0
# Install wcr, modem73, and WeeChat. Configures WeeChat for the local node.
set -eu
REPO="thaum-labs/weechat-radio"

fetch_macos_modem73() {
  dest="$1"
  arch="$2"
  if [ "$arch" = "aarch64" ]; then
    needle="macos-arm64.tar.gz"
  else
    needle="macos-x86_64.tar.gz"
  fi
  echo "Downloading macOS modem73 (radio sound) from RFnexus..."
  json=$(curl -fsSL -H "User-Agent: wcr-install" "https://api.github.com/repos/RFnexus/modem73/releases/latest")
  url=$(printf '%s\n' "$json" | tr ',' '\n' | grep browser_download_url | grep "$needle" | grep -v sha | head -n 1 | sed 's/.*"browser_download_url": *"//;s/".*//')
  if [ -z "$url" ]; then
    echo "Could not find RFnexus modem73 for $needle" >&2
    return 1
  fi
  curl -fsSL "$url" -o "$dest/m73.tgz"
  mkdir -p "$dest/m73out"
  tar -xzf "$dest/m73.tgz" -C "$dest/m73out"
  bin=$(find "$dest/m73out" -type f -name modem73 | head -n 1)
  libs=$(find "$dest/m73out" -type d -name libs | head -n 1)
  if [ -z "$bin" ] || [ -z "$libs" ]; then
    echo "RFnexus archive is missing modem73 or libs/" >&2
    return 1
  fi
  cp "$bin" "$dest/modem73"
  rm -rf "$dest/libs"
  cp -R "$libs" "$dest/libs"
}
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
if [ -f "$TMP/wcr-gui" ]; then
  chmod +x "$TMP/wcr-gui"
  mv "$TMP/wcr-gui" "$PREFIX/wcr-gui"
fi
MODEM=""
if [ -f "$TMP/modem73" ]; then MODEM="$TMP/modem73"; fi
if [ -z "$MODEM" ]; then MODEM=$(find "$TMP" -maxdepth 2 -type f -name modem73 2>/dev/null | head -n 1); fi
if [ ! -d "$TMP/libs" ]; then
  FOUND_LIBS=$(find "$TMP" -maxdepth 3 -type d -name libs 2>/dev/null | head -n 1)
  if [ -n "$FOUND_LIBS" ]; then cp -R "$FOUND_LIBS" "$TMP/libs"; fi
fi
if [ "$OS" = "darwin" ] && { [ -z "$MODEM" ] || [ ! -d "$TMP/libs" ]; }; then
  fetch_macos_modem73 "$TMP" "$ARCH" || {
    echo "Could not install modem73 (needed for radio sound)." >&2
    exit 1
  }
  MODEM="$TMP/modem73"
fi
if [ "$OS" = "linux" ] || [ "$OS" = "darwin" ]; then
  if [ -z "$MODEM" ]; then
    echo "Release archive is missing modem73. Use v0.1.33 or newer." >&2
    exit 1
  fi
  chmod +x "$MODEM"
  mv "$MODEM" "$PREFIX/modem73"
  echo "Installed $PREFIX/modem73"
  if [ -d "$TMP/libs" ]; then
    rm -rf "$PREFIX/libs"
    cp -R "$TMP/libs" "$PREFIX/libs"
    echo "Installed $PREFIX/libs (modem73 audio libraries)"
  elif [ "$OS" = "darwin" ]; then
    echo "Release archive is missing modem73 libs. Use v0.1.33 or newer." >&2
    exit 1
  fi
  if [ "$OS" = "darwin" ]; then
    xattr -dr com.apple.quarantine "$PREFIX/wcr" "$PREFIX/wcr-gui" "$PREFIX/modem73" "$PREFIX/libs" 2>/dev/null || true
  fi
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
GUI_EXEC="$PREFIX/wcr-gui"
if [ ! -x "$GUI_EXEC" ]; then GUI_EXEC="$PREFIX/wcr gui"; fi
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$APPS"
ICON_LINE=""
if [ -f "$TMP/weechat-radio.png" ]; then
  ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
  mkdir -p "$ICON_DIR"
  cp "$TMP/weechat-radio.png" "$ICON_DIR/weechat-radio.png"
  ICON_LINE="Icon=$ICON_DIR/weechat-radio.png"
fi
cat > "$APPS/weechat-radio.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=WeeChat Radio
Comment=Chat over internet and HF/VHF radio
Exec=$GUI_EXEC
Terminal=false
Categories=Network;HamRadio;
$ICON_LINE
EOF
echo "Menu launcher: $APPS/weechat-radio.desktop"

if [ "${WCR_SKIP_WEECHAT:-}" != "1" ]; then
  if ! command -v weechat >/dev/null 2>&1; then
    if [ "$OS" = "darwin" ]; then
      echo "Skipping WeeChat on macOS (optional). Homebrew often compiles it from source."
      echo "Chat without it:  wcr gui"
      echo "Add it later: https://weechatradio.com/guides/weechat.html"
    else
      echo "Installing WeeChat..."
      if command -v apt-get >/dev/null 2>&1; then
        sudo apt-get update -y && sudo apt-get install -y weechat weechat-python || true
      elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y weechat || true
      else
        echo "Install WeeChat from https://weechat.org/ then run: wcr weechat --configure"
      fi
    fi
  fi
  if command -v weechat >/dev/null 2>&1 || command -v weechat-headless >/dev/null 2>&1; then
    "$PREFIX/wcr" weechat --configure
  elif [ "$OS" != "darwin" ]; then
    echo "WeeChat did not install. Install it from https://weechat.org/ then run: wcr weechat --configure" >&2
  fi
fi

echo "Next:"
echo "  wcr setup"
echo "  wcr gui"
