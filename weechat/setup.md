# Connect WeeChat to a local WeeChat Radio node

## What you need

- `wcr node` or `wcr tui` already running on this computer
- WeeChat with the Python plugin

The official installer does this for you. After `wcr node`, run `wcr weechat`.

Manual steps, if you installed WeeChat yourself:

1. Copy `radio.py` into WeeChat's Python directory:

   - Linux: `~/.weechat/python/`
   - macOS: `~/.weechat/python/`
   - Windows (WSL): same as Linux inside WSL

2. In WeeChat:

   ```
   /server add radio 127.0.0.1/6667 -autoconnect
   /set irc.server.radio.tls off
   /set irc.server.radio.capabilities "message-tags,echo-message,server-time,msgid"
   /connect radio
   /script load radio.py
   ```

3. You land in `#bulletin`. Chat as usual. `/radio help` lists node commands.

4. Optional: match the tron colours used by `wcr tui` and the website. The script defaults to theme `tron` (teal bar, mode token in its colour). Switch it off with:

   ```
   /set plugins.var.python.radio.theme plain
   ```

   To restyle WeeChat bars and nick colours as well, paste the lines from [`tron.weechat`](tron.weechat) into WeeChat.

## How you know it worked

The `radio` bar at the bottom of WeeChat shows your mode and SNR, separated like `INTERNET-RADIO │ IDLE │ VHF-FM │ SNR 12`. Sending a message adds delivery ticks on the line (`✓` then `✓✓`).
