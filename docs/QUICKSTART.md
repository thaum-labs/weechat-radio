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

3. Run the wizard:

   ```
   wcr setup
   ```

   Type your callsign, grid square, and how you connect a radio.
   If you have no radio, choose internet-only. You can add a radio later.
   Windows and Linux already have modem73 beside `wcr`. On macOS, install modem73 from https://modem73.app if you want radio.

4. Start the chat window:

   ```
   wcr tui
   ```

   Want WeeChat instead? Run `wcr node` in one window and `wcr weechat` in another.

5. You land in a room called `#bulletin`. Type a short hello and press Enter.

## How you know it worked

The status bar at the bottom shows your callsign and mode. After you send, a tick appears: `·` means queued, `✓` means sent.

The TUI uses the `tron` theme by default (indigo / orange, like the website). Switch with `/radio theme hacker` or `/radio theme terminal`, or set `theme` under `[ui]` in `wcr.toml`.
