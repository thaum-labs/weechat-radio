On the web: https://weechatradio.com/guides/macos.html

# macOS setup

## What you need

- A Mac
- Terminal (Command + Space, type `Terminal`, press Enter)

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

3. Run `wcr setup`, or skip this and open the window in the next step (it asks the same things).

   Windows and Linux already bundle modem73 (the radio sound box). On a Mac, install modem73 from https://modem73.app if you want radio.

4. Start:

   ```
   wcr gui
   ```

   Terminal instead: `wcr tui`.

5. To run in the background:

   ```
   wcr service install
   ```

6. WeeChat is optional. To add it later, see https://weechatradio.com/guides/weechat.html (offline: `wcr help` then the WeeChat page on the site).

## How you know it worked

The status bar shows your callsign and mode. `#bulletin` accepts a test message.
