# macOS setup

## What you need

- macOS with Homebrew, or the release tar from GitHub
- Optional: WeeChat (`brew install weechat`)

## Steps

1. Install with Homebrew:

   ```
   brew tap thaum-labs/tap
   brew install wcr
   ```

   Or:

   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

2. Run `wcr setup`.

3. Start:

   ```
   wcr tui
   ```

4. To run in the background:

   ```
   wcr service install
   ```

5. WeeChat (optional):

   ```
   /server add radio 127.0.0.1/6667
   /connect radio
   ```

   Load `weechat/radio.py` from this repo for the status bar and ticks.

## How you know it worked

The TUI status bar shows your callsign and mode. `#bulletin` accepts a test message.
