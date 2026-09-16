# Connect WeeChat to a local WeeChat Radio node

## What you need

- `wcr node` or `wcr tui` already running on this computer
- WeeChat with the Python plugin

## Steps

1. Copy `radio.py` into WeeChat's Python directory:

   - Linux: `~/.weechat/python/`
   - macOS: `~/.weechat/python/`
   - Windows (WSL): same as Linux inside WSL

2. In WeeChat:

   ```
   /server add radio 127.0.0.1/6667 -autoconnect
   /set irc.server.radio.capabilities "message-tags,echo-message,server-time,msgid"
   /connect radio
   /script load radio.py
   ```

3. You land in `#bulletin`. Chat as usual. `/radio help` lists node commands.

## How you know it worked

The `radio` bar at the bottom of WeeChat shows your mode and SNR. Sending a message adds delivery ticks on the line (`✓` then `✓✓`).
