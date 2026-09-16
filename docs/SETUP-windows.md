# Windows setup

## What you need

- Windows 10 or 11
- PowerShell
- The installer installs WeeChat (Cygwin) and points it at `127.0.0.1:6667`. Use `wcr tui` if you skip that.

## Steps

1. Install:

   ```
   irm https://weechatradio.com/install.ps1 | iex
   ```

   Or download the zip from GitHub Releases and put `wcr.exe` on your PATH.

2. Run `wcr setup` and answer the questions.

3. Start chatting:

   ```
   wcr tui
   ```

4. To keep relaying when the window is closed, open PowerShell as Administrator and run:

   ```
   wcr service install
   ```

5. WeeChat is installed and configured by the official installer. After `wcr node`, run `wcr weechat`. To rewrite the server and script: `wcr weechat --configure`. Native Windows IRC clients can also use `127.0.0.1:6667`.

## How you know it worked

`wcr tui` shows your callsign in the status bar. Sending a message in `#bulletin` adds a `✓`.
