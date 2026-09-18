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

3. Open **WeeChat Radio** (Start menu / applications menu, or `wcr gui`). First launch asks for your callsign. After that the window starts the station and `#bulletin` chat. No extra terminal.

   Terminal options: `wcr tui`, or `wcr node` plus `wcr weechat`.

## How you know it worked

The status bar at the bottom shows your callsign and mode. After you send, a mark appears on your line:

- `[.]` waiting to go out
- `[v]` sent
- `[vv]` relayed or delivered
- `[vvv]` every station in a group got it

The terminal TUI may show `·` / `✓` / `✓✓` / `✓✓✓` instead when Unicode is on. WeeChat uses the same Unicode ticks.

The TUI uses the `tron` theme by default (indigo / orange, like the website). Switch with `/radio theme hacker` or `/radio theme terminal`, or set `theme` under `[ui]` in `wcr.toml`.
