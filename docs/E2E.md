# Paired LAN test (two computers, no radio)

Same command on both machines. They chat over your Wi-Fi, elect a **private hub** (not the public map), and plot two pins at different grid squares even though they sit on the same LAN.

Offline: `wcr help e2e` (also `wcr help test`).

## What you need

- Two computers on the **same Wi-Fi** (Windows, macOS, or Linux)
- The same `wcr` build on both (`wcr e2e lan` from 0.1.25; `wcr help e2e` from 0.1.26)
- No radio. No WeeChat. Your normal `wcr.toml` is not touched.

## Steps

1. On **both** computers, in a terminal:

   ```
   wcr e2e lan
   ```

   Optional flags:

   ```
   wcr e2e lan --timeout 60 --port 7375 --hub-port 7376
   ```

   Ports **7375** (LAN) and **7376** (hub) are chosen so a live station on 6667 / 7373 can stay running.

2. If Windows asks, allow UDP/TCP for those ports. Set the Wi-Fi profile to **Private** if discovery hangs.

3. Wait. One machine becomes the private hub; the other joins it. Each prints a guest callsign (`~` plus seven characters) and a hashed Maidenhead grid.

4. When both print `PASS`, open the map against that hub. On either computer, from the repo `web/` folder:

   ```
   python -m http.server 5173
   ```

   Then open the URL the test printed, of the form:

   ```
   http://127.0.0.1:5173/?api=http://<hub-lan-ip>:7376
   ```

   `?api=` points the live map at the private hub. It does not use weechatradio.com.

## What it checks

- `#bulletin` token from the other machine (real local IRC path)
- `hub_ok` on the private hub
- `GET /api/v1/nodes` shows **both** callsigns with **different** grids and coordinates

It never dials `hub.weechatradio.com`. Identity keys live under a temp `WCR_HOME`, not your normal config folder.

## How you know it worked

Each terminal prints a line like:

```
PASS local=~A1B2C3D grid=FN20XR peer=~Z9Y8X7W peer_grid=IO91WM lan_peers=1 hub_ok=true
```

The map page shows two marks, not one stacked pin. `FAIL` dumps `lan_peers`, `hub_ok`, and the heard list.

## If it fails

- Same Wi-Fi, not a guest/AP isolation network
- Firewall allow **7375** and **7376**
- Same `wcr` version on both machines (`wcr --version`)
- Do not point this test at the public hub
