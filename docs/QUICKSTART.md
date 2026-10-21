On the web (with pictures): https://weechatradio.com/guides/start.html

# 5-minute start

## What you need

- This computer (Windows, macOS, or Linux)
- A ham radio callsign, or a guest name starting with `~` (example: `~alice`)
- A grid square if you want to show up on the map (example: `IO91wm`). Search the web for “maidenhead grid square map” and click where you live.
- Optional: a radio and a cable (Digirig, VOX audio cable, or an HF rig)

## Steps

1. Open a command window.

   - Windows: Start, type `PowerShell`, open **Windows PowerShell** (no administrator needed)
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

   Leave the window open until it prints the next step. On Windows, WeeChat downloads after the program and the window can sit for several minutes. Fixes: https://weechatradio.com/guides/troubleshooting.html

3. Run setup first:

   ```
   wcr setup
   ```

   If you have no radio, choose internet-only. You can add a radio later.

4. Then open the chat window:

   ```
   wcr gui
   ```

`wcr update` replaces the binary with a newer release. On Linux and macOS it restarts a station installed with `wcr service install`. A station you started by hand keeps the old code until you stop it and start it again. On Windows, restart that station yourself, including a service.

## How you know it worked

The status bar at the bottom shows your callsign and mode. After you send, a mark appears on your line:

- `[..]` waiting to go out
- `[tx]` sent
- `[rl]` you heard your own frame on the radio again (a speaker echo, or someone relaying it). Not a receipt. A noisy path can skip it. In `radio` mode the frame is marked to stay off the internet, so `[rl]` is as far as it goes.
- `[gw]` a gateway accepted the frame onto the hub. Not an ACK. The line can still move on to `[ok]` or `[all]`.
- `[ok]` delivered — they sent an ACK. A direct message and `#bulletin` stop here. `#bulletin` is public, so one ACK is enough and the mark does not go on to `[all]`.
- `[all]` every member of a closed group ACKed. The first ACK paints `[ok]` and retries continue until every stored member has ACKed, or the retries run out. The mark is shown once. A `#bulletin` post cannot reach this.
- `[no]` the send was refused (for example the hub said the callsign is already claimed), or the last copy waited out its interval with no ACK. A line a relay was heard carrying on stays `[rl]` instead, until its receipt comes or it is an hour old. If some members of a closed group have ACKed and others have not, the line stays `[ok]` instead of `[no]`, and a late ACK still makes it `[all]`. Marks never move back.
- `[xN]` how many times the line was keyed, counted until `[rl]`, `[gw]` or `[ok]`. `[x1]` is the first send. The default is 3 retries, so the last copy is `[x4]`. `[no]` waits two more full intervals after that copy.

In the desktop app, the **key** button above the chat lists these marks, and hovering a mark explains it. A line that reached you by radio also shows the SNR it was decoded at, for example `-4 dB`. `/snr off` hides it and `/snr on` shows it again.

## Signal reports and weather

- `/report CALL` sends that station `SNR? Please send me a signal report.` A station on v0.1.108 or later answers on its own with the SNR it heard you at, for example `SNR -4 dB: your request reached me by radio at -4 dB.` It answers each callsign at most once every 5 minutes. `/report auto off` stops your station answering, and `/report auto on` turns it back on. An older station just shows the request as a normal line.
- `/wx` shows the current weather at the centre of your grid square, for example `WX IO81UF 20:45Z: 12C, light rain, wind SW 15 km/h`. `/wx CALL` sends that line to CALL. The weather comes from Open-Meteo, so your station needs internet for this. The station receiving it does not.

Routine retries wait 45s, then 90s, then 180s, then 360s twice with no further key, each ±25% (`retry_jitter`). Emergency is 8s / 16s / 32s / 64s twice. Priority is 20s / 40s / 80s / 160s twice. A busy channel can make the gap between copies longer than that band. `/radio flush` marks unacked outbound `[no]`. Rows older than one hour are not keyed again after a restart.

On the radio the status bar counts down to the next beacon: about a minute apart for the first three, then every 5 minutes. It shows `after exchange` while a receipt or a mail acceptance is still due, `pending` until the beacon reaches the modem, and `busy` while a decoded frame is still fresh. A line sent in the first `rf.startup_hold_s` seconds after start (120) waits until another station has been heard (not on VOX, which sends no beacons).

## Closed groups

`/join #net` creates a closed group (`MODE #net` shows `+i`). `#bulletin` stays public (`+`). `/invite CALL` adds that callsign and keys a direct over-air notice that names you; the reply says whether the notice was sent. The invitee is notified only when that direct message arrives; it carries the group's 6 hops. `NAMES` lists the members stored on this station, not the whole chain. Creating the group or inviting someone keys a roster at 6 hops so a neighbour can add callsigns it missed. The sender and every relay repeat that roster until they hear a station further along, so a station four hops out gets the whole list. A station that sends on the channel, or answers one of its lines, is added on every member station that hears it, so lists catch up when a roster is lost. A station that never hears any of that still shows only its own list, and the end of `NAMES` says so. `/radio status` shows `relayed` (every forwarded frame, repeats included), `unique` (distinct messages), and `groups` (each closed group's priority). That list is `group_prios` on the status page. `/prio emergency` on the channel sets it. `#bulletin` is always routine and is omitted.

The window and the TUI use these same marks.

The TUI uses the `tron` theme by default (indigo / orange, like the website). Switch with `/radio theme hacker` or `/radio theme terminal`, or set `theme` under `[ui]` in `wcr.toml`.
