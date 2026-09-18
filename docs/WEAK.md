On the web: https://weechatradio.com/docs/weak.html

# Weak signals

Chat still works when the path is noisy, fading, or barely above the noise. You pick a slower preset; the program retries and steps down if the other station does not acknowledge.

You do not need to understand the radio maths. Upgrade every station to this build first — older nodes speak a different packet layout.

## What you need

- This build of `wcr` (hub deployed 17 Sep 2026, commit `ce4a108`)
- A radio path already working on a clear signal (see radio setup)
- For HF: start with `hf-poor`. Drop to `hf-weak` or `hf-deep` only if frames keep failing

## Presets

Switch with `/radio preset <name>` or `/preset <name>` in the window.

| Preset | Use when |
|--------|----------|
| `vhf-fm` | Local VHF/UHF FM, a clean signal |
| `vox-safe` | Any radio keyed by VOX (extra lead-in so the first symbols are not clipped) |
| `hf-good` | Steady HF SSB |
| `hf-poor` | Fading HF / NVIS (the usual HF calling preset) |
| `hf-weak` | The other station is faint |
| `hf-deep` | Last resort. Very slow. Short messages only |

The receiver hears all of these at once. You can send `hf-weak` while they are still set to `hf-poor`.

## What the program does for you

1. If nobody ACKs your message, it sends it again (three tries by default).
2. Each retry uses a slower, tougher waveform. The status bar **TX** field shows the current one (for example `RDM-300S`).
3. **RETRY** counts how many extra sends have gone out.
4. When an ACK comes back, it includes how strong they heard you. A good report steps the waveform back up.
5. Lines that start with `!!` (emergency) go out twice, a moment apart, so a fade is less likely to take both.
6. Group chat on HF is split into pieces. Any two of three pieces rebuild the message. If several internet gateways each hear a different piece, the hub stitches them.
7. If the frequency is already busy, the program waits for a gap (status bar **PTT** reads `wait`). Beacons stay off while occupancy is high. Group ACKs and retries are staggered so they do not all key at once.

## Busy channel

modem73 carrier-sense is on for every preset. On top of that, `wcr` keeps a single air queue: one frame (or fragment burst) at a time, emergency first, then ACKs, then your chat, then relays, then beacons.

**OCC** in the station panel is channel occupancy 0–100. **wait** means a frame is queued until the channel is idle. After `max_defer_ms` (3 s for emergency) it sends anyway — the modem still will not key over a carrier it can hear.

## Steps (HF fading)

1. Confirm audio is **good**, not **low** or **hot**. Distortion is worse than a weak but clean signal.
2. Set a starting preset:

   ```
   /radio preset hf-poor
   ```

3. Send a short test to a station you can already hear, or to `#bulletin`.
4. Watch the status bar. If **RETRY** climbs and **TX** changes (QPSK → RDM-1200S → RDM-600S → RDM-300S → MFSK-32R), the program is already stepping down. Leave it.
5. If nothing decodes after several tries, set `hf-weak` yourself, then `hf-deep` for a one-line check.

## Knobs in `wcr.toml`

```
[rf]
max_retries = 3       # extra sends of your own unacked messages
frag_k = 2            # pieces of data
frag_m = 1            # spare pieces; any 2 of 3 rebuild the message
emergency_dup_ms = 400
csma = true
slot_ms = 100
quiet_ms = 300
max_defer_ms = 15000
emergency_max_defer_ms = 3000
turnaround_ms = 250
congested_pct = 60
ack_dither_ms = 800
beacon_jitter_s = 15
retry_jitter = true
```

## How you know it worked

- A faint station's callsign appears in **HEARD** with a low or negative SNR.
- Your line gets `[tx]` then `[ok]` (delivered), even if **RETRY** was 1 or 2 first. The TUI may show `✓` then `✓✓` instead.
- **TX** may show a slower mode than your preset. That is expected on a rough path.
- On a busy frequency, **OCC** climbs and **PTT** may show `wait` instead of `idle`. That is the air queue holding your frame.

## Upgrade note

This hub build speaks protocol version 2 on radio and on the internet. Version 1 frames are still decoded. New frames are smaller. Every station that wants to talk to this hub over radio should run this build.
