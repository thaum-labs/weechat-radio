On the web: https://weechatradio.com/guides/windows.html

# Windows setup

## What you need

- Windows 10 or 11
- PowerShell (a normal window; the install is for your user only)

## Steps

1. Click Start, type `PowerShell`, and open **Windows PowerShell**. A normal window is enough: it installs into your own AppData. Paste:

   ```
   irm https://weechatradio.com/install.ps1 | iex
   ```

   Leave the window open. The installer downloads the program, then WeeChat through Cygwin. It prints `waiting for WeeChat...` and can sit for several minutes, sometimes toward 20. It is finished when it prints the next step. That adds WeeChat Radio to your user PATH. Open a new PowerShell window before `wcr setup`. Or download the zip from GitHub Releases and put `wcr.exe` on your PATH. More fixes: https://weechatradio.com/guides/troubleshooting.html

2. Run setup first:

   ```
   wcr setup
   ```

3. Then open the chat window:

   ```
   wcr gui
   ```

   Or open **WeeChat Radio** from the Start menu.

4. To keep relaying when the window is closed, open PowerShell as Administrator and run:

   ```
   wcr service install
   ```

   `wcr update` replaces the binary. Stop the station and start it again, including this service, so the new code is the one running. A station you started by hand keeps the old code until you do.

5. WeeChat is still installed. Use **Open WeeChat** in the GUI, or `wcr weechat`. Native IRC clients can use `127.0.0.1:6667`.

## How you know it worked

The GUI status column shows your callsign and mode. A message in `#bulletin` is heard as modem audio if radio is enabled.
