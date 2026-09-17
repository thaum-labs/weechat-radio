On the web: https://weechatradio.com/docs/modes.html

# Modes

Four modes. Switch with `/radio mode <name>` or `F2` in the TUI.

| Mode | Radio | Internet | On the map |
|------|-------|----------|------------|
| `internet` | off | hub only | cyan |
| `internet-radio` | on, primary | fills gaps; this node is a gateway | green |
| `radio` | on | none. Frames are never put on the internet | amber |
| `radio-plus` | on | none locally; a gateway that hears you may forward | magenta |

Switching to `radio` asks for confirmation: it drops the internet and map upload.

If the hub goes away while you are in `internet-radio`, radio keeps working. The status bar says **Internet down, radio only**. Messages that needed the internet wait in the hold queue and go out when the hub returns.

## Weak-signal behaviour

On radio, `wcr` does not demodulate audio — [modem73](https://github.com/RFnexus/modem73) does that. What `wcr` can do is spend less time on air, retry, and pick a slower modem73 mode.

- Presets: `hf-good` (OFDM 8PSK 1/2 + postamble), `hf-poor` (RDM-600S), `hf-weak` (RDM-300S), `hf-deep` (MFSK-32R).
- Unacked messages are retransmitted (`[rf] max_retries`, default 3). Each retry steps down the ladder: QPSK 1/2 → RDM-1200S → RDM-600S → RDM-300S → MFSK-32R (last step only if the frame fits in 55 bytes).
- ACKs carry the receiver's SNR so the sender can step back up on a good path.
- Emergency (`!!`) frames are sent twice, a fraction of a second apart.
- Group / oversized frames on HF are split into Reed–Solomon fragments. Any 2 of 3 reconstruct the message. The hub combines fragments heard by different gateways.

The status bar shows the current TX rung and retry count.

## Busy channel

When several stations talk at once, frames would otherwise pile onto the KISS port and key over each other. Two layers stop that:

1. **modem73 CSMA** is on for every preset (VHF and HF). The modem itself waits for a quiet carrier before PTT.
2. **`wcr` air queue** serialises our own traffic. It waits while the channel is `rx`/`tx`, spreads retries and group ACKs so they do not land on the same slot, skips beacons when occupancy is high, and leaves a turnaround gap after each TX so an ACK can be heard.

The status bar **PTT** field shows `wait` while the queue is holding a frame. **OCC** is occupancy 0–100 from the modem. **QUEUE** includes how many frames are in the air queue (`air N`).

```
[rf]
csma = true
slot_ms = 100
quiet_ms = 300
max_defer_ms = 15000          # then send anyway; modem73 CSMA still gates PTT
emergency_max_defer_ms = 3000
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
```

Cross-frequency diagrams: [bridging](https://weechatradio.com/docs/bridging.html). Offline: `wcr help bridging`.

## How you know it worked

The status bar colour matches the table. Other stations on the map show the same colours.
