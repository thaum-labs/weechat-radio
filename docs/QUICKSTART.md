# 5-minute start

## What you need

- This computer (Windows, macOS, or Linux)
- A ham radio callsign, or a guest name starting with `~`
- Optional: a radio and a cable (Digirig, VOX audio cable, or an HF rig)

## Steps

1. Install WeeChat Radio (pick your system):

   **Linux / macOS**
   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

   **Windows (PowerShell)**
   ```
   irm https://weechatradio.com/install.ps1 | iex
   ```

2. Run the wizard:

   ```
   wcr setup
   ```

   Type your callsign, grid square (for example `IO91wm`), and how you connect a radio.
   Windows and Linux already have modem73 beside `wcr`. On macOS, install modem73 from https://modem73.app if you want radio.

3. Start the node:

   ```
   wcr tui
   ```

   Or start in the background with `wcr node` and connect WeeChat (see the WeeChat guide).

4. Say hello in `#bulletin`. Type a message and press Enter.

## How you know it worked

The status bar at the bottom shows your callsign and mode. After you send, a tick appears: `·` means queued, `✓` means sent.
