On the web: https://weechatradio.com/guides/macos.html

# macOS setup

## What you need

- A Mac
- Terminal (Command + Space, type `Terminal`, press Enter)

The install command is the same as Linux. Then run setup, then the window.

## Steps

1. Install:

   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

   This downloads `wcr`. It does not run `brew install weechat`. Homebrew has no tap for this project yet, and on older macOS it would compile WeeChat from source.

2. If a new terminal says `wcr: command not found`:

   ```
   export PATH="$HOME/.local/bin:$PATH"
   ```

   Add that line to `~/.zshrc` so it sticks.

3. Run setup first:

   ```
   wcr setup
   ```

   The installer puts `modem73` (the radio sound box) next to `wcr`, same as Windows and Linux. After setup, a test line in radio mode should play tones on the speakers.

4. Then open the chat window:

   ```
   wcr gui
   ```

5. To run in the background:

   ```
   wcr service install
   ```

6. WeeChat is optional. To add it later, see https://weechatradio.com/guides/weechat.html (offline: `wcr help` then the WeeChat page on the site).

## How you know it worked

The status bar shows your callsign and mode. `#bulletin` accepts a test message.
