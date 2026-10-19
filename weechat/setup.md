On the web: https://weechatradio.com/guides/weechat.html

# Connect WeeChat to a local WeeChat Radio node

## What you need

- `wcr node` or `wcr tui` already running on this computer
- WeeChat with the Python plugin

On Linux, the official installer tries to install WeeChat. On a Mac it does not — install WeeChat yourself, then run `wcr weechat --configure`. After `wcr node`, run `wcr weechat`.

Manual steps, if you installed WeeChat yourself:

1. Copy `radio.py` into WeeChat's Python directory:

   - Linux (WeeChat ≥ 3.2): `~/.local/share/weechat/python/`
   - Linux (older): `~/.weechat/python/`
   - macOS: `~/.weechat/python/`
   - Windows (WSL): same as Linux inside WSL

2. In WeeChat:

   The local IRC server requires a **valid amateur callsign or guest nick** (`~NAME`, up to 8 characters including `~`). A plain name like `tobias` is rejected with `432 Erroneous nickname`. Use the same callsign you set in `wcr setup` (e.g. `G4ABC` or `~ALICE`).

   ```
   /server add radio 127.0.0.1/6667 -autoconnect
   /set irc.server.radio.tls off
   /set irc.server.radio.capabilities "message-tags,echo-message,server-time,msgid"
   /set irc.server.radio.nicks "G4ABC"
   /connect radio
   /script load radio.py
   ```

   Replace `G4ABC` with your callsign or guest nick from `wcr.toml`.

3. You land in `#bulletin`. Chat as usual. `/radio help` lists node commands.
   The nicklist shows each station's band (`[2m] G4ABC`). Tell the app your dial with `/radio freq 144.950` if you have no CAT.

4. The script defaults to theme `tron` (indigo / orange, same as the site and `wcr tui`). It restyles WeeChat bars on load. Switch it off with:

   ```
   /set plugins.var.python.radio.theme plain
   ```

   To restyle WeeChat bars and nick colours as well, paste the lines from [`tron.weechat`](tron.weechat) into WeeChat.

## How you know it worked

The `radio` bar at the bottom of WeeChat shows your mode and SNR, separated like `INTERNET-RADIO │ IDLE │ VHF-FM │ SNR 12`. Sending a message adds the same marks as the node: `[tx]`, `[rl]`, `[gw]`, `[ok]`, `[all]`, `[no]`, and `[xN]`. With 3 retries the last copy is `[x4]`, and `[no]` waits two more intervals after that. `#bulletin` stops at `[ok]` and does not show `[all]`. A closed group (`/join #net`, `MODE #net` replies `+i`) shows `[all]` when every member this station has stored has ACKed. `/invite CALL` adds them and keys a direct notice that names you; the reply says whether that notice was sent. `NAMES` lists stored members. `HELP` lists these commands. `/radio status` includes `relayed` (every forwarded frame, repeats included), `unique` (distinct messages), and `groups` (the `group_prios` priorities). Routine retries wait 45s, 90s, 180s, then 360s twice before `[no]`, each ±25%. A busy channel can make a gap longer. `/radio flush` marks unacked outbound `[no]`.
