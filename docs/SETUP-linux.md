On the web: https://weechatradio.com/guides/linux.html

# Linux setup

## What you need

- A current Linux (Debian, Ubuntu, Fedora, Arch, Raspberry Pi OS)

The install command is the same as macOS. Then run setup, then the window.

## Steps

1. Install:

   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

   Before WeeChat is installed, the installer says it will use apt or dnf and that sudo may ask for your password. If `~/.local/bin` is not on PATH, it adds that directory to your bash, zsh, or fish profile and prints the line to run in this terminal.

2. If you will use a USB radio cable, add your user to the serial group (then log out and back in):

   ```
   sudo usermod -aG dialout $USER
   ```

3. Run setup first:

   ```
   wcr setup
   ```

4. Then open the chat window:

   ```
   wcr gui
   ```

5. Background service:

   ```
   wcr service install
   ```

6. WeeChat is installed and configured by the official installer. After `wcr node`, run `wcr weechat`. To rewrite the server and script: `wcr weechat --configure`.

## How you know it worked

`wcr gui` shows your callsign. A message in `#bulletin` gets a `[tx]`. `wcr service status` prints `active` if the service is installed.
