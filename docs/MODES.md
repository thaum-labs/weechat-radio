On the web: https://weechatradio.com/docs/modes.html

# Modes

Four modes. Switch with `/radio mode <name>` or `F2` in the TUI.

`radio` and `radio-plus` both post map telemetry when `[telemetry] url` is set, and that post never contains message text. Neither mode sends chat to the hub or the LAN from this computer. The only difference is whether an internet-radio gateway that hears you may forward your messages (and email) to the internet.

## internet

**What it's for.** Chat with no radio. This computer talks to the hub.

| | |
| --- | --- |
| Chat over RF? | No. The radio stays off. |
| Chat over hub/LAN from this computer? | Yes. This computer sends chat to the hub and the LAN. |
| Can a gateway forward my messages to the internet? | This computer already sends them to the hub. It does not key a radio, so a gateway has nothing of yours to hear. |
| Map data (callsign/grid/tx-rx, never text) sent if `[telemetry]` url set? | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. |
| Receipts from internet stations? | Yes. They come back on the hub. A receipt is the other station's ACK, shown as `[ok]`. |
| Email? | Yes. This computer posts mail to the hub. |
| Map colour | Cyan (`#00e5ff`). |

## internet-radio

**What it's for.** Your radio plus the internet. This computer is a gateway: it keys the dial and also talks to the hub, and it can carry other stations' radio-plus traffic onward.

| | |
| --- | --- |
| Chat over RF? | Yes. Every chat line is keyed on the radio. |
| Chat over hub/LAN from this computer? | Yes, when the station you are calling was not heard on this dial. A closed group uses the hub when no member was heard here. `#bulletin` always goes to the hub and the LAN as well as the radio. |
| Can a gateway forward my messages to the internet? | You are the gateway. You upload radio-plus frames you hear, and your own chat when the radio will not reach the other station. |
| Map data (callsign/grid/tx-rx, never text) sent if `[telemetry]` url set? | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. |
| Receipts from internet stations? | Yes. They come back on the hub. Stations you can hear also ACK on the radio. |
| Email? | Yes. Your own mail goes to the hub. You also post radio-plus mail you hear. |
| Map colour | Green (`#39ff14`). |

## radio

**What it's for.** Chat on the radio only. A gateway that hears you must leave the message on the radio.

| | |
| --- | --- |
| Chat over RF? | Yes. |
| Chat over hub/LAN from this computer? | No. |
| Can a gateway forward my messages to the internet? | No. The frame says do not forward. A gateway that hears you leaves it on the radio. |
| Map data (callsign/grid/tx-rx, never text) sent if `[telemetry]` url set? | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. |
| Receipts from internet stations? | No. Only a station that hears you on the radio can ACK. |
| Email? | No. Email stays grey. |
| Map colour | Amber (`#ffbf00`). |

## radio-plus

**What it's for.** Chat on the radio only, from this computer. The one difference from radio is that a gateway that hears you may carry the message, and email, onto the internet.

| | |
| --- | --- |
| Chat over RF? | Yes. |
| Chat over hub/LAN from this computer? | No. |
| Can a gateway forward my messages to the internet? | Yes, if an internet-radio gateway hears you. It may forward a direct message, a closed group, `#bulletin`, and email. |
| Map data (callsign/grid/tx-rx, never text) sent if `[telemetry]` url set? | Yes, if the URL is set. Callsign, grid, and tx/rx ids. Never message text. |
| Receipts from internet stations? | Not on a hub socket from this computer. The gateway can bring an internet station's ACK back on the radio if it has heard you on this dial. |
| Email? | Yes, by radio to the Mail gateway callsign in setup. That station posts it. This computer does not open the hub. |
| Map colour | Magenta (`#ff4dff`). |

## Which mode should I pick

- No radio, and you only want the hub: `internet`.
- You have a radio and an internet link, and you want to be the station others use to reach the hub: `internet-radio`.
- You have a radio, and you want the message to stay on the air even if a gateway hears it: `radio`.
- You have a radio, no internet on this computer, and you want a nearby gateway to carry the line to internet stations: `radio-plus`.

## Worked example: radio-plus

You are in `radio-plus` on 144.950. You send a direct message. `#bulletin` and a closed group use this same path.

1. This computer keys the message on the radio. It does not open the hub.
2. An `internet-radio` gateway on 144.950 hears the frame and sees that it may go to the internet.
3. The gateway sends it to the hub.
4. The hub delivers it to the internet station.
5. That station's receipt comes back to the gateway. The gateway keys the receipt on the radio if it has heard you on this dial.
6. Your line can show `[gw]` when the gateway accepted the frame, then `[ok]` when the receipt comes back on the radio. `[rl]` only means you heard your own frame on the radio again.

In `radio` mode the same air path runs. The gateway does not upload the frame, and there is no receipt from the internet station.

## Switching, the map, and a dead hub

Switching to `radio` asks for confirmation: it drops hub and LAN chat. Changing mode starts or stops the hub chat path and the radio path immediately. You do not need to restart the station. Switching between `radio` and `radio-plus` leaves both paths as they are: radio stays on, and this computer still does not open hub chat.

Clear `[telemetry] url` to stay off the map. An arc is drawn only between two station pins that addressed each other. A `#bulletin` or closed-group destination is not a callsign, so it does not draw a line to every other pin.

If the hub goes away while you are in `internet-radio`, radio keeps working. The status bar says **Internet down, radio only**. Messages that still needed the hub wait in an internet hold (`pending_inet`) and go out when the hub returns. That is separate from the RF ARQ hold queue used for retries.

## Weak-signal behaviour

On radio, `wcr` does not demodulate audio — [modem73](https://github.com/RFnexus/modem73) does that. What `wcr` can do is spend less time on air, retry, and pick a slower modem73 mode.

- Presets: `hf-good` (OFDM 8PSK 1/2 + postamble), `hf-poor` (RDM-600S), `hf-weak` (RDM-300S), `hf-deep` (MFSK-32R), `vox-safe` (MFSK-32R + long VOX lead).
- Unacked messages are retransmitted (`[rf] max_retries`, default 3). Each retry steps down the ladder: QPSK 1/2 → RDM-1200S → RDM-600S → RDM-300S → MFSK-32R (last step only if the frame fits in 55 bytes).
- ACKs carry the receiver's SNR so the sender can step back up on a good path.
- Emergency (`!!`) frames are sent twice, a fraction of a second apart.
- Group / oversized frames on HF are split into Reed–Solomon fragments. Any 2 of 3 reconstruct the message. The hub combines fragments heard by different gateways.

The status bar shows the current TX rung and retry count.

Versus JS8, FT8, Olivia, and APRS: [compare modes](https://weechatradio.com/docs/compare.html). Offline: `wcr help compare`.

## Busy channel

When several stations talk at once, frames would otherwise pile onto the KISS port and key over each other. Two layers stop that:

1. **modem73 CSMA** is on for every preset (VHF and HF). The modem itself waits for a quiet carrier before PTT.
2. **`wcr` air queue** serialises our own traffic. It waits while the channel is `rx`/`tx`, spreads retries and group ACKs so they do not land on the same slot, skips beacons when occupancy is high, and leaves a turnaround gap after each TX so an ACK can be heard. An ACK skips that queue. Once `emergency_max_defer_ms` (3 s) has elapsed it is keyed with modem carrier sense off for that frame only, including when this station's sense is idle. If the modem reports undecodable busy, carrier sense is turned off for the ACK immediately. It stays on while a frame is being received. Emergency chat still uses the 3 s cap inside the queue. Other chat and relays use `max_defer_ms` (15 s). The wait itself is a slot backoff, not a single sleep.

The status bar **PTT** field shows `wait` while the queue is holding a frame. **OCC** is occupancy 0–100 from the modem. **QUEUE** includes how many frames are in the air queue (`air N`).

```
[rf]
csma = true
slot_ms = 100
quiet_ms = 300
max_defer_ms = 15000          # chat and relays; then send anyway
emergency_max_defer_ms = 3000 # emergency and ACKs; then send anyway
turnaround_ms = 250
congested_pct = 60            # drop beacons / defer relays at or above this
ack_dither_ms = 800
beacon_jitter_s = 15
retry_jitter = true
```

Operator guide: [weak signals](https://weechatradio.com/docs/weak.html). Same text offline: `wcr help weak`.

## Gateway knobs

In `wcr.toml`:

```
[gateway]
rf_egress = true          # put internet traffic on the air
third_party = "deny"      # guest (~nick) traffic on RF: allow or deny
tower = false             # internet-radio only
```

In internet-radio the station panel has a **TOWER** switch (`/tower on` or `/tower off`). On draws that station as a small tower on the public map, with the dial frequency beside the icon. Other modes keep the station mark. The switch does not change what goes on the air.

Cross-frequency diagrams: [bridging](https://weechatradio.com/docs/bridging.html). Offline: `wcr help bridging`.

## How you know it worked

The status bar colour matches the table. A station with the tower switch on shows a tower and its frequency. Other stations on the map show the same colours.
