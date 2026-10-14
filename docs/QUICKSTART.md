On the web (with pictures): https://weechatradio.com/guides/start.html

# 5-minute start

## What you need

- This computer (Windows, macOS, or Linux)
- A ham radio callsign, or a guest name starting with `~` (example: `~alice`)
- A grid square if you want to show up on the map (example: `IO91wm`). Search the web for “maidenhead grid square map” and click where you live.
- Optional: a radio and a cable (Digirig, VOX audio cable, or an HF rig)

## Steps

1. Open a command window.

   - Windows: Start, type `PowerShell`, press Enter
   - Mac: Command + Space, type `Terminal`, press Enter
   - Linux: open Terminal from the applications menu

2. Install WeeChat Radio (pick your system):

   **Linux / macOS**
   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

   **Windows (PowerShell)**
   ```
   irm https://weechatradio.com/install.ps1 | iex
   ```

3. Run setup first:

   ```
   wcr setup
   ```

   If you have no radio, choose internet-only. You can add a radio later.

4. Then open the chat window:

   ```
   wcr gui
   ```

## How you know it worked

The status bar at the bottom shows your callsign and mode. After you send, a mark appears on your line:

- `[..]` waiting to go out
- `[tx]` sent
- `[rl]` you heard your own frame come back (a speaker echo, or someone relaying it). Not a receipt. A noisy path can skip it.
- `[ok]` delivered — they sent an ACK. A direct message and `#bulletin` stop here. `#bulletin` is public, so one ACK is enough and the mark does not go on to `[all]`.
- `[all]` every member of a closed group ACKed. A `#bulletin` post cannot reach this.
- `[no]` the send was refused (for example the hub said the callsign is already claimed), or the retries are used up
- `[xN]` how many times the line was keyed. `[x1]` is the first send. The default is 3 retries, so the last copy is `[x4]` before `[no]`.

Routine retries wait 45s, then 90s, then 180s, each ±25% (`retry_jitter`). Emergency is 8s / 16s / 32s. Priority is 20s / 40s / 80s.

## Closed groups

`/join #net` creates a closed group (`MODE #net` shows `+i`). `#bulletin` stays public (`+`). `/invite CALL` adds that callsign and keys a direct over-air notice; the reply says whether the notice was sent. The invitee is notified only when that direct message arrives. `/radio status` shows `relayed` (frames forwarded for someone else) and `groups` (each closed group's priority). That list is `group_prios` on the status page. `/prio emergency` on the channel sets it. `#bulletin` is always routine and is omitted.

The window and the TUI use these same marks.

The TUI uses the `tron` theme by default (indigo / orange, like the website). Switch with `/radio theme hacker` or `/radio theme terminal`, or set `theme` under `[ui]` in `wcr.toml`.
