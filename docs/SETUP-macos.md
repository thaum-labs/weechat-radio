# macOS setup

## What you need

- macOS with Homebrew, or the release tar from GitHub
- The installer installs WeeChat with Homebrew when available, then points it at `127.0.0.1:6667`

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

5. WeeChat is installed and configured by the official installer. After `wcr node`, run `wcr weechat`. To rewrite the server and script: `wcr weechat --configure`.

## How you know it worked

The TUI status bar shows your callsign and mode. `#bulletin` accepts a test message.
