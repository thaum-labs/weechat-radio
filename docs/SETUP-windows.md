On the web: https://weechatradio.com/guides/windows.html

# Windows setup

## What you need

- Windows 10 or 11
- PowerShell

## Steps

1. Install:

   ```
   irm https://weechatradio.com/install.ps1 | iex
   ```

   Or download the zip from GitHub Releases and put `wcr.exe` on your PATH.

2. Run setup first:

   ```
   wcr setup
   ```

3. Then open the chat window:

   ```
   wcr gui
   ```

   Or open **WeeChat Radio** from the Start menu.

3. To keep relaying when the window is closed, open PowerShell as Administrator and run:

   ```
   wcr service install
   ```

4. WeeChat is still installed. Use **Open WeeChat** in the GUI, or `wcr weechat`. Native IRC clients can use `127.0.0.1:6667`.

## How you know it worked

The GUI status column shows your callsign and mode. A message in `#bulletin` is heard as modem audio if radio is enabled.
