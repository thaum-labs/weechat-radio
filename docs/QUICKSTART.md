On the web (with pictures): https://weechatradio.com/guides/start.html

# 5-minute start

## What you need

- This computer (Windows, macOS, or Linux)
- A ham radio callsign, or a guest name starting with `~` (example: `~alice`)
- A grid square if you want to show up on the map (example: `IO91wm`). Search the web for “maidenhead grid square map” and click where you live.
- Optional: a radio and a cable (Digirig, VOX audio cable, or an HF rig)

## Steps

1. Open a command window.

   - Windows: Start, type `PowerShell`, press Enter
   - Mac: Command + Space, type `Terminal`, press Enter
   - Linux: open Terminal from the applications menu

2. Install WeeChat Radio (pick your system):

   **Linux / macOS**
   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

   **Windows (PowerShell)**
   ```
   irm https://weechatradio.com/install.ps1 | iex
   ```

3. Run setup first:

   ```
   wcr setup
   ```

   If you have no radio, choose internet-only. You can add a radio later.

4. Then open the chat window:

   ```
   wcr gui
   ```

## How you know it worked

The status bar at the bottom shows your callsign and mode. After you send, a mark appears on your line:

- `[..]` waiting to go out
- `[tx]` sent
- `[rl]` you heard your own frame come back (a speaker echo, or someone relaying it). Not a receipt.
- `[ok]` delivered — they sent an ACK. A direct message and `#bulletin` stop here. `#bulletin` is public, so one ACK is enough and the mark does not repeat for every listener.
- `[all]` every member of a closed group ACKed
- `[no]` the send was refused (for example the hub said the callsign is already claimed)

The window and the TUI use these same marks.

The TUI uses the `tron` theme by default (indigo / orange, like the website). Switch with `/radio theme hacker` or `/radio theme terminal`, or set `theme` under `[ui]` in `wcr.toml`.
