# Render the Android mockups

These scripts redraw the PNGs in `../material/` (Material 3, the spec) and `../desktop-look/` (desktop-look, reference). They are static HTML mockups. They are not the Android app, and they do not key a radio.

The two trees are already unpacked:

- `android/src/` — desktop-look generator
- `android-material/src/` — Material 3 generator

`android-material/src/s3.py` loads map geometry from `android/src/screens3.py` (`../../android/src` from that file). Leave the two trees as siblings.

Frames are 412 × 915 dp, rendered at 2× (824 × 1830).

## What you need

- Python 3.10 or newer
- `playwright`, `pillow`, `numpy`, and `scipy`
- A Chromium or Chrome that Playwright can launch. The scripts look for `/usr/bin/google-chrome`, `google-chrome-stable`, `chromium`, and `chromium-browser`. Set `CHROME` to another binary if you need to.
- The font files already in this tree (SIL Open Font License):
  - `android/src/fonts/fira-mono-400.woff2` — Fira Mono, desktop-look UI
  - `android-material/src/fonts/Roboto-Variable.ttf` — Roboto variable, Material UI
  - `android-material/src/fonts/RobotoMono-Variable.ttf` — Roboto Mono variable, callsigns, frequencies, grids, and presets

## 1. Install the Python packages and a browser

```bash
pip install playwright pillow numpy scipy
```

If you have no system Chrome or Chromium:

```bash
python3 -m playwright install chromium
python3 -m playwright install-deps chromium
```

## 2. Install the fonts on the machine

Screen PNGs load the files above through `@font-face`. The Material overview and compare boards also ask the system for a family named Roboto, so install the same files for fontconfig:

```bash
mkdir -p "$HOME/.local/share/fonts/wcr"
cp android-material/src/fonts/Roboto-Variable.ttf "$HOME/.local/share/fonts/wcr/"
cp android-material/src/fonts/RobotoMono-Variable.ttf "$HOME/.local/share/fonts/wcr/"
# Fira Mono Regular (family name "Fira Mono"). Any Regular TTF of that family is fine
# alongside the woff2 already in android/src/fonts/.
fc-cache -f
```

Emoji fall back to Noto Color Emoji. Install that system font if the chat screens show empty boxes instead of emoji.

## 3. Build the terrain basemap

The basemap is a generated placeholder, not real terrain. Build it once and copy it into both trees. `terrain.py` writes relative to the current directory.

```bash
mkdir -p android/src/img android-material/src/img
cd android/src && python3 terrain.py && cd ../..
cp android/src/img/terrain.png android-material/src/img/terrain.png
```

## 4. Render every PNG

Render the desktop-look set first. The Material compare board reads `../android/*.png`.

```bash
cd android/src
python3 build.py
python3 board.py
cd ../..

cd android-material/src
python3 build.py
python3 board.py
cd ../..
```

Optional filter: `python3 build.py 01-chat 05-mail` renders only matching names.

That writes:

- `android/01-chat.png` through `android/22-notification-listening.png`, plus `android/00-overview-board.png`
- `android-material/` screen PNGs, including `08b-mail-compose-bluetooth.png`, `22-notification-listening.png`, `01-chat-light.png`, `05-mail-list-light.png`, and `13-radio-usb-light.png`
- `android-material/00-overview-board.png` and `android-material/00-compare-board.png`

`build.py` and `board.py` also write scratch HTML (`_*.html`, `board.html`, `compare.html`) beside the scripts. Those files are not part of this folder.

## 5. Copy the PNGs into the published folders

From this directory (`docs/design/android/src`):

```bash
cp android/*.png ../desktop-look/
cp android-material/*.png ../material/
```

## How you know it worked

`desktop-look/` has 24 PNGs: 23 screens plus `00-overview-board.png`. `material/` has 28 PNGs: 26 screens (the same set, plus three light screens) plus `00-overview-board.png` and `00-compare-board.png`.

Open `material/01-chat.png` and `desktop-look/01-chat.png`. Both are 824 × 1830 and full of text. Material chat is Roboto, with Roboto Mono on the callsigns. Desktop-look chat is Fira Mono. A blank frame or a fallback face means the font file or the browser did not load.
