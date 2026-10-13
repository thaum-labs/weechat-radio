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
trap 'rm -rf "$TMP"' EXIT

verify_sha256() {
  file="$1"
  base_url="$2"
  if curl -fsSL "${base_url}.sha256" -o "$TMP/asset.sha256" 2>/dev/null; then
    expected=$(awk '{print $1}' "$TMP/asset.sha256")
    got=$(sha256sum "$file" | awk '{print $1}')
    if [ "$got" != "$expected" ]; then
      echo "SHA-256 mismatch for $base_url" >&2
      exit 1
    fi
    echo "Verified SHA-256 for $(basename "$file")"
  else
    echo "Warning: no .sha256 sidecar for $base_url" >&2
  fi
}

URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
echo "Downloading $URL"
curl -fsSL "$URL" -o "$TMP/wcr.tgz" || {
  echo "No release asset yet. Build from source:"
  echo "  git clone https://github.com/${REPO} && cd weechat-radio && cargo install --path crates/wcr"
  exit 1
}
verify_sha256 "$TMP/wcr.tgz" "$URL"
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
  RELEASE_TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
  if [ -z "$RELEASE_TAG" ]; then RELEASE_TAG="main"; fi
  curl -fsSL "https://raw.githubusercontent.com/${REPO}/${RELEASE_TAG}/weechat/radio.py" -o "$PREFIX/radio.py"
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

# Append PREFIX to the user's shell profile once. The marker keeps a second
# install from adding the same block again.
ensure_user_path() {
  prefix="$1"
  case ":${PATH}:" in
    *":${prefix}:"*)
      PATH_NOTE=""
      PATH_APPLY=""
      return 0
      ;;
  esac
  shell_name=$(basename "${SHELL:-sh}")
  case "$shell_name" in
    zsh)
      profile="$HOME/.zshrc"
      line="export PATH=\"${prefix}:\$PATH\""
      apply="export PATH=\"${prefix}:\$PATH\""
      ;;
    fish)
      profile="$HOME/.config/fish/config.fish"
      mkdir -p "$HOME/.config/fish"
      line="fish_add_path --prepend \"${prefix}\""
      apply="fish_add_path --prepend \"${prefix}\""
      ;;
    *)
      profile="$HOME/.bashrc"
      line="export PATH=\"${prefix}:\$PATH\""
      apply="export PATH=\"${prefix}:\$PATH\""
      ;;
  esac
  if [ -f "$profile" ] && grep -q "weechat-radio path" "$profile"; then
    PATH_NOTE="PATH: ${prefix} is already listed in ${profile}."
  else
    {
      echo ""
      echo "# >>> weechat-radio path >>>"
      echo "$line"
      echo "# <<< weechat-radio path <<<"
    } >> "$profile"
    PATH_NOTE="PATH: added ${prefix} to ${profile}."
  fi
  PATH_APPLY="$apply"
}

WEECHAT_NOTE=""
WEECHAT_CFG=""
if [ "${WCR_SKIP_WEECHAT:-}" != "1" ]; then
  if ! command -v weechat >/dev/null 2>&1; then
    if [ "$OS" = "darwin" ]; then
      echo "Skipping WeeChat on macOS (optional). Homebrew often compiles it from source."
      echo "Chat without it:  wcr gui"
      echo "Add it later: https://weechatradio.com/guides/weechat.html"
      WEECHAT_NOTE="WeeChat: not installed (skipped on macOS). Chat with: wcr gui"
    elif command -v apt-get >/dev/null 2>&1; then
      echo "WeeChat is the chat program this station connects to."
      echo "Installing it with apt. sudo may ask for your password."
      sudo apt-get update -y && sudo apt-get install -y weechat-curses weechat-python || true
      WEECHAT_TRIED="apt"
    elif command -v dnf >/dev/null 2>&1; then
      echo "WeeChat is the chat program this station connects to."
      echo "Installing it with dnf. sudo may ask for your password."
      sudo dnf install -y weechat || true
      WEECHAT_TRIED="dnf"
    else
      echo "WeeChat is the chat program this station connects to."
      echo "This system has no apt or dnf. Install WeeChat from https://weechat.org/ then run: wcr weechat --configure"
      WEECHAT_NOTE="WeeChat: not installed. Install it from https://weechat.org/ then run: wcr weechat --configure"
    fi
  else
    WEECHAT_NOTE="WeeChat: already installed."
  fi
  if command -v weechat >/dev/null 2>&1; then
    if [ -n "${WEECHAT_TRIED:-}" ]; then
      WEECHAT_NOTE="WeeChat: installed with ${WEECHAT_TRIED}."
    fi
    if ! WEECHAT_CFG=$("$PREFIX/wcr" weechat --configure 2>&1); then
      echo "WeeChat configure failed; run: wcr weechat --configure" >&2
      WEECHAT_CFG=""
    fi
  elif [ "$OS" != "darwin" ] && [ -z "$WEECHAT_NOTE" ]; then
    WEECHAT_NOTE="WeeChat: install did not succeed. Install it from https://weechat.org/ then run: wcr weechat --configure"
    echo "$WEECHAT_NOTE" >&2
  fi
else
  WEECHAT_NOTE="WeeChat: skipped (WCR_SKIP_WEECHAT=1)."
fi

if [ -f "$PREFIX/minttyrc" ]; then
  rm -f "$PREFIX/minttyrc"
  echo "Removed $PREFIX/minttyrc (mintty settings are only used on Windows)."
fi

ensure_user_path "$PREFIX"
echo ""
echo "Installed $PREFIX/wcr"
if [ -n "$WEECHAT_NOTE" ]; then
  echo "$WEECHAT_NOTE"
fi
if [ -n "$WEECHAT_CFG" ]; then
  echo "$WEECHAT_CFG"
fi
if [ -n "$PATH_NOTE" ]; then
  echo "$PATH_NOTE"
fi
if [ "$OS" = "linux" ]; then
  echo "Linux GUI (wcr gui): needs libxkbcommon-x11 and libayatana-appindicator3 (Debian: libxkbcommon-x11-0 libayatana-appindicator3-1)."
fi
if [ -n "$PATH_APPLY" ]; then
  echo "This terminal does not see that yet. Apply it with:"
  echo "  $PATH_APPLY"
fi
echo "Next, run:"
echo "  wcr setup"
