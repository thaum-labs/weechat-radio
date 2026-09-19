# Isolated tests (no live radio)

Offline: `wcr help e2e` (also `wcr help test`). Your normal `wcr.toml` is not touched.

| Command | Machines | What it is |
|---------|----------|------------|
| `wcr e2e lan` | Two, same Wi-Fi | Internet path, private hub, map |
| `wcr e2e radio` | One | Simulated RF: air queue, CSMA, KISS. No transmitter |

## LAN (`wcr e2e lan`)

Same command on both computers. They chat over Wi-Fi, elect a **private hub** (not the public map), and plot two pins at different grid squares even though they sit on the same LAN.

### What you need

- Two computers on the **same Wi-Fi** (Windows, macOS, or Linux)
- The same `wcr` build on both (`wcr e2e lan` from 0.1.25; map linger from 0.1.27; map server from 0.1.29)
- No radio. No WeeChat.

### Steps

1. On **both** computers:

   ```
   wcr e2e lan
   ```

   Optional: `wcr e2e lan --timeout 60 --port 7375 --hub-port 7376`

   Ports **7375** (LAN) and **7376** (hub) stay off 6667 / 7373 so a live station can keep running.

2. If Windows asks, allow UDP/TCP. Set the Wi-Fi profile to **Private** if discovery hangs.

3. Leave both terminals running. After PASS, `wcr` starts the map page and prints a `map  http://127.0.0.1:…` URL (it also tries to open a browser). The hub dies when you Ctrl-C.

PASS looks like:

```
PASS local=~A1B2C3D grid=FN20XR peer=~Z9Y8X7W peer_grid=IO91WM lan_peers=1 hub_ok=true
```

It never dials `hub.weechatradio.com`.

## Radio (`wcr e2e radio`)

One computer. Two stations share a fake modem73 and a loss-free in-process “air”. Mode is **radio**. There is no audio, no PTT, and no RF leaving the PC.

```
wcr e2e radio
```

Optional: `wcr e2e radio --timeout 60`

Uses loopback KISS **18001 / 18002** and IRC **16668 / 16669**.

PASS looks like:

```
PASS A=~AAAAAAA B=~BBBBBBB peer_ok=true hub_ok=false queue_air=0 retries=0 heard_peer=true
```

That means `#bulletin` crossed the **air queue and KISS mock**, not the internet hub.

This does **not** replace an on-air test with you as control operator.

## If LAN fails

- Same Wi-Fi, not a guest/AP isolation network
- Firewall allow **7375** and **7376**
- Same `wcr` version (`wcr --version`)
- Do not point the test at the public hub

## If radio fails

- Ports 18001–18002 and 16668–16669 free
- `wcr --version` 0.1.28 or later
