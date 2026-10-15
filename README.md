<p align="center">
  <a href="https://weechatradio.com">
    <img src="branding/readme-lockup.svg" alt="WeeChat Radio" width="440" height="116" />
  </a>
</p>

<p align="center"><strong>Chat that keeps working when the internet does not.</strong></p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.89-DEA584?logo=rust&logoColor=white" alt="Rust 1.89" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache%202.0-blue.svg" alt="Apache-2.0" /></a>
  <img src="https://img.shields.io/badge/version-0.1.88-7d9bff" alt="Version 0.1.88" />
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0078D6" alt="Windows, macOS and Linux" />
  <br/>
  <a href="https://weechat.org/"><img src="https://img.shields.io/badge/WeeChat-IRC-00A7E0" alt="WeeChat" /></a>
  <a href="https://github.com/RFnexus/modem73"><img src="https://img.shields.io/badge/modem73-KISS%20TNC-39ff14" alt="modem73" /></a>
  <img src="https://img.shields.io/badge/desktop-GUI-7d9bff" alt="desktop GUI" />
  <img src="https://img.shields.io/badge/SQLite-store--and--forward-003B57?logo=sqlite&logoColor=white" alt="SQLite" />
  <a href="https://weechatradio.com"><img src="https://img.shields.io/website?url=https%3A%2F%2Fweechatradio.com&up_message=live&down_message=down&label=weechatradio.com" alt="weechatradio.com status" /></a>
</p>

## What is this?

A small program (`wcr`) that talks to [WeeChat](https://weechat.org/) like an IRC server, and talks to [modem73](https://github.com/RFnexus/modem73) like a TNC. You can use the internet, HF/VHF radio, or both. Messages store-and-forward so nobody has to leave a radio on all night.

With WeeChat Radio you can:

1. Chat from the desktop app (`wcr gui`), a terminal UI (`wcr tui`), or WeeChat
2. Run internet-only, radio-only, or both at once
3. Store and forward messages so stations catch up later
4. Appear on the public map: [weechatradio.com](https://weechatradio.com)

The site, desktop app, TUI, and CLI share one dark terminal shell: indigo `#7d9bff` prompts, orange accents, and the chevron-and-waves mark. The mark recolours with the live station mode (cyan / green / amber / magenta). The default UI theme is `tron`; change it with `/radio theme hacker` or `/radio theme terminal`.

**You do not need a radio** to try internet mode.
**You do not need WeeChat** — the desktop app or TUI is enough.

WeeChat is a trademark of Sébastien Helleu. This project is independent.

> Early software — it works, but things may still change.

---

## Quick start

Best way to get the **latest** version: install from [weechatradio.com](https://weechatradio.com).

### Linux / macOS

```bash
curl -fsSL https://weechatradio.com/install.sh | sh
wcr setup   # 7 steps: callsign, grid, radio path, hub/map choices, service
wcr gui
```

Prefer a terminal? `wcr tui` is the same station.

### Windows (PowerShell)

```powershell
irm https://weechatradio.com/install.ps1 | iex
```

Open **WeeChat Radio** from the Start menu. First launch is setup; then you chat in the window.

```powershell
# optional, same window from a terminal:
wcr gui
```

How you know it worked: the status bar shows your callsign. Send a line in `#bulletin`. Marks on your own lines:

- `[..]` waiting
- `[tx]` sent
- `[rl]` you heard your own frame on the radio again (a speaker echo or a relay, not a receipt). A noisy path can skip this. In `radio` mode this is as far as the frame goes: it is marked to stay off the internet, so a gateway will not upload it.
- `[gw]` a gateway accepted the frame onto the hub. This is not an ACK. Retries keep going until `[ok]` or `[all]`.
- `[ok]` delivered. A direct message stops here. `#bulletin` stops here too: it is public and has no member list, so it never reaches `[all]`.
- `[all]` every member of a closed group has acknowledged. Only a closed group can show this. The first member ACK paints `[ok]` and the retries continue until every stored member has ACKed, or the retries run out. A partial ACK does not paint `[no]`.
- `[no]` the hub refused the send, or the last copy went out and one more full retry interval passed with no acknowledgement
- `[xN]` how many times this line was keyed. `[x1]` is the first send. With the default of 3 retries the last copy is `[x4]` (`retry-3/3`). `[no]` comes after that copy has waited one more interval, not in the same second.

Routine retry waits are 45s, then 90s, then 180s, then 360s. The first three are the gaps before the extra copies. The 360s wait is after `[x4]`: the station listens for an ACK and does not key again. Each wait is multiplied by a random factor from 0.75 to 1.25 when `retry_jitter` is on (the default), so a run can look like about 40s, then 100s, then 210s, then about 270–450s before `[no]`. Emergency waits are 8s, 16s, 32s, then 64s with the same jitter. Priority waits are 20s, 40s, 80s, then 160s. A busy channel can hold the next copy after the timer has already elapsed (modem CSMA), so the gap between keyed copies can run longer than that band. The band is the timer, not a cap on how long the radio waits for a clear channel.

Unacked outbound older than `outbound_max_age_secs` (default 1 hour) is marked `[no]` instead of being keyed again, including after a restart. `/radio flush` marks this station's unacked outbound `[no]` immediately and drops relay holds that have not gone out.

A closed group is any channel except `#bulletin`. `/join #net` creates it. `MODE #net` replies with `+i`. `/invite CALL` adds that station and keys a direct notice to them; the notice names you, the inviter. The reply says whether that notice was sent. They are notified only if that direct message arrives. `NAMES` after you join lists the members stored on this station, not the whole chain. Creating the group or inviting someone also keys a roster at 6 hops; a station that never hears it still shows only its own list, and the end of `NAMES` says so. `/radio status` includes `relayed` (every frame this station forwarded, including a repeat of the same message), `unique` (distinct messages in that count), and `groups` (the `group_prios` list: each closed group's default priority, `routine`, `priority`, or `emergency`). `#bulletin` is always routine and is not in that list. Set a default with `/prio emergency` on the channel. Delivery marks are sent only to the client that sent the line, with the message id, so another station does not paint your `[ok]` or `[all]`.

Radio guides: [VR-N76 / UV-PRO over Bluetooth](https://weechatradio.com/guides/vr-n76.html) · [handheld + Digirig](https://weechatradio.com/guides/handheld.html) · [VOX cable](https://weechatradio.com/guides/vox.html) · [HF + CAT](https://weechatradio.com/guides/hf.html)

---

## Modes

Switch with `/radio mode <name>` or `F2` in the TUI. The mark and tray icon follow the mode.

`radio` and `radio-plus` both post map telemetry when `[telemetry] url` is set, and that post never contains message text. Neither mode sends chat to the hub or the LAN from this computer. The only difference is whether an internet-radio gateway that hears you may forward your messages (and email) to the internet.

**internet.** Chat with no radio. This computer talks to the hub.

**internet-radio.** Your radio plus the internet. This computer is a gateway: it keys the dial and also talks to the hub, and it can carry other stations' radio-plus traffic onward.

**radio.** Chat on the radio only. A gateway that hears you must leave the message on the radio.

**radio-plus.** Chat on the radio only, from this computer. The one difference from radio is that a gateway that hears you may carry the message, and email, onto the internet.

| | internet | internet-radio | radio | radio-plus |
| --- | --- | --- | --- | --- |
| Chat over RF? | No. The radio stays off. | Yes. Every chat line is keyed on the radio. | Yes. | Yes. |
| Chat over hub/LAN from this computer? | Yes. This computer sends chat to the hub and the LAN. | Yes, when the station you are calling was not heard on this dial. A closed group uses the hub when no member was heard here. `#bulletin` always goes to the hub and the LAN as well as the radio. | No. | No. |
| Can a gateway forward my messages to the internet? | This computer already sends them to the hub. It does not key a radio, so a gateway has nothing of yours to hear. | You are the gateway. You upload radio-plus frames you hear, and your own chat when the radio will not reach the other station. | No. The frame says do not forward. A gateway that hears you leaves it on the radio. | Yes, if an internet-radio gateway hears you. It may forward a direct message, a closed group, `#bulletin`, and email. |
| Map data (callsign/grid/tx-rx, never text) sent if `[telemetry]` url set? | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. |
| Receipts from internet stations? | Yes. They come back on the hub. A receipt is the other station's ACK, shown as `[ok]`. | Yes. They come back on the hub. Stations you can hear also ACK on the radio. | No. Only a station that hears you on the radio can ACK. | Not on a hub socket from this computer. The gateway can bring an internet station's ACK back on the radio if it has heard you on this dial. |
| Email? | Yes. This computer posts mail to the hub. | Yes. Your own mail goes to the hub. You also post radio-plus mail you hear. | No. Email stays grey. | Yes, by radio to the Mail gateway callsign in setup. That station posts it. This computer does not open the hub. |
| Map colour | Cyan (`#00e5ff`) | Green (`#39ff14`) | Amber (`#ffbf00`) | Magenta (`#ff4dff`) |

Which mode should I pick:

- No radio, and you only want the hub: `internet`.
- You have a radio and an internet link, and you want to be the station others use to reach the hub: `internet-radio`.
- You have a radio, and you want the message to stay on the air even if a gateway hears it: `radio`.
- You have a radio, no internet on this computer, and you want a nearby gateway to carry the line to internet stations: `radio-plus`.

Worked example, radio-plus. You are in `radio-plus` on 144.950. You send a direct message. `#bulletin` and a closed group use this same path.

1. This computer keys the message on the radio. It does not open the hub.
2. An `internet-radio` gateway on 144.950 hears the frame and sees that it may go to the internet.
3. The gateway sends it to the hub.
4. The hub delivers it to the internet station.
5. That station's receipt comes back to the gateway. The gateway keys the receipt on the radio if it has heard you on this dial.
6. Your line can show `[gw]` when the gateway accepted the frame, then `[ok]` when the receipt comes back on the radio. `[rl]` only means you heard your own frame on the radio again.

In `radio` mode the same air path runs. The gateway does not upload the frame, and there is no receipt from the internet station.

If the hub dies while you are in `internet-radio`, radio keeps working. The bar says **Internet down, radio only**.

Same text: [modes](https://weechatradio.com/docs/modes.html). Offline: `wcr help modes`.

---

## Radio

You need a valid licence, a radio, and audio into the PC. [modem73](https://github.com/RFnexus/modem73) is the TNC. Windows, macOS, and Linux installers put it next to `wcr`. If it is missing, `wcr` downloads the copy published with release `v0.1.79` and checks `third_party/modem73/sha256.json`. To move that pin, see `third_party/modem73/NOTICE`. Common paths:

| Path | What you need |
|------|----------------|
| VR-N76 / UV-PRO (Bluetooth) | Vero VR-N76, BTECH UV-PRO or Radioddity GA-5WB — built-in KISS TNC, no cable; the app pairs and connects (`wcr tnc find`) |
| KISS TNC on a serial port | Mobilinkd, `/dev/rfcomm0`, or any KISS TNC on a COM port |
| Handheld + Digirig | VHF/UHF HT and a [Digirig](https://digirig.net/) (or AIOC) USB cable |
| VOX cable | Any radio that keys on VOX, plus a 3.5 mm audio cable |
| HF + CAT | HF rig with `rigctl` (Hamlib) for PTT and frequency |

Run `wcr setup` and pick the path that matches your station. Suggested calling frequencies: [calling](https://weechatradio.com/docs/calling.html). You are the control operator — check your band plan before you transmit.

---

## Using WeeChat

WeeChat is optional. On Linux the installer may install it. On a Mac it does not, because Homebrew often compiles WeeChat from source. Chat is `wcr gui` or `wcr tui`. To add WeeChat later:

```
wcr weechat --configure
wcr node
wcr weechat
```

`wcr weechat --configure` rewrites the `radio` server to the node's IRC listen address from `wcr.toml` (default `127.0.0.1:6667`; `wcr weechat --configure -c other.toml` follows that file) and loads [`weechat/radio.py`](weechat/radio.py). The built-in UI is still `wcr gui` or `wcr tui`.

Step-by-step: [WeeChat guide](https://weechatradio.com/guides/weechat.html)

---

## Mini glossary

| Word | Meaning |
|------|---------|
| **`wcr`** | The WeeChat Radio program (node, hub, desktop app, and TUI) |
| **Callsign** | Your amateur radio ID, or a guest name starting with `~` |
| **Grid** | Short location code for the map (Maidenhead) |
| **Hub** | Public internet rendezvous at `hub.weechatradio.com` |
| **TNC** | The box (here: modem73) that turns data into radio audio |
| **Store-and-forward** | Messages wait on disk until the next station can take them |

---

## Helpful links

- Live map + hub: [weechatradio.com](https://weechatradio.com)
- Changelog: [changelog](https://weechatradio.com/changelog.html)
- 5-minute start: [guides/start](https://weechatradio.com/guides/start.html)
- Radio setup: [guides/radio](https://weechatradio.com/guides/radio.html)
- Protocol: [docs/protocol](https://weechatradio.com/docs/protocol.html)
- Public API: [docs/api](https://weechatradio.com/docs/api.html)
- Licence: [Apache-2.0](LICENSE) (WeeChat script is [GPL-3.0-or-later](weechat/LICENSE))

---

## Build from source

```bash
git clone https://github.com/thaum-labs/weechat-radio.git
cd weechat-radio
cargo install --path crates/wcr
wcr setup
wcr gui
```

You need a Rust toolchain (1.89 or newer). On Linux, `wcr gui` needs GTK 3, `libxkbcommon-x11`, and Ayatana AppIndicator (`libayatana-appindicator3-1` on Debian). On Windows, the MSVC build tools are required for the bundled SQLite. `--no-default-features` skips the desktop GUI if you only want the TUI.

---

## For developers

<details>
<summary>Click to expand</summary>

```
crates/wcr/     node daemon, hub, desktop GUI, TUI, modem73 client, local IRC server
weechat/        WeeChat helper script (GPL-3.0-or-later)
web/            official site source (weechatradio.com)
web/icons/      brand mark and per-mode icons
docs/           operator guides
install/        install.sh, install.ps1, Homebrew / Scoop / WinGet stubs
deploy/         DigitalOcean droplet, Caddy, Docker Compose
branding/       README lockup and app icon (accent mark)
```

```bash
cargo test --workspace
cargo fmt --all
python web/icons/render.py   # regenerate PNG/ICO marks
```

Copy `.githooks/pre-commit` to `.git/hooks/pre-commit` so a commit of unformatted Rust is blocked before Ubuntu CI fails. Cursor/VS Code format Rust on save via `.vscode/settings.json`.

The same `wcr` binary is a station (`wcr node` / `wcr gui` / `wcr tui`) or the public hub (`wcr hub`). Hub deploy notes: [deploy](https://weechatradio.com/docs/deploy.html).

</details>

---

## Licence

Apache-2.0 for the daemon, hub, and website. The WeeChat script `weechat/radio.py` is GPL-3.0-or-later (it runs inside WeeChat).

See [`LICENSE`](LICENSE), [`NOTICE`](NOTICE), and [`weechat/LICENSE`](weechat/LICENSE).

---

Created by **Thaum Labs**

**WeeChat Radio** — chat that keeps working when the internet does not
