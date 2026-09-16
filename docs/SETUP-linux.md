# Linux setup

## What you need

- A current Linux (Debian, Ubuntu, Fedora, Arch, Raspberry Pi OS)
- The installer installs WeeChat when apt, dnf, or brew is available, then points it at `127.0.0.1:6667`

## Steps

1. Install:

   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

2. If you will use a USB radio cable, add your user to the serial group (then log out and back in):

   ```
   sudo usermod -aG dialout $USER
   ```

3. Run `wcr setup`.

4. Start:

   ```
   wcr tui
   ```

5. Background service:

   ```
   wcr service install
   ```

6. WeeChat is installed and configured by the official installer. After `wcr node`, run `wcr weechat`. To rewrite the server and script: `wcr weechat --configure`.

## How you know it worked

`wcr tui` shows your callsign. A message in `#bulletin` gets a send tick. `wcr service status` prints `active` if the service is installed.
