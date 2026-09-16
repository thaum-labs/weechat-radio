# Linux setup

## What you need

- A current Linux (Debian, Ubuntu, Fedora, Arch, Raspberry Pi OS)
- Optional: WeeChat (`sudo apt install weechat`)

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

6. WeeChat (optional):

   ```
   /server add radio 127.0.0.1/6667
   /connect radio
   /script load radio.py
   ```

   Copy `weechat/radio.py` into `~/.weechat/python/`.

## How you know it worked

`wcr tui` shows your callsign. A message in `#bulletin` gets a send tick. `wcr service status` prints `active` if the service is installed.
