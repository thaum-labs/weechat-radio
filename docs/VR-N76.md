On the web (with pictures): https://weechatradio.com/guides/vr-n76.html

# VR-N76 / UV-PRO / GA-5WB over Bluetooth

The Vero VR-N76, BTECH UV-PRO, Radioddity GA-5WB and VR-N7500 share one platform with a **built-in 1200 bd packet TNC** that speaks KISS over Bluetooth. No sound card, no Digirig, no modem73: WeeChat Radio talks to the radio directly and the radio does the modulation.

Legal note: you are the control operator. Stay within your licence. Every frame carries your callsign in a standard AX.25 header, so anyone with a packet decoder can identify you.

---

## What you need

- A VR-N76, UV-PRO, GA-5WB or VR-N7500 with firmware **0.7.11 or newer** (KISS mode arrived in the October 2024 update; update with the HT app if the menu below is missing)
- A computer with Bluetooth (built-in or a USB dongle)
- WeeChat Radio 0.1.9 or newer

## On the radio (once)

1. **Menu → General Settings → KISS TNC → Enable.**
2. **General Settings → Digital Mode → off.** The radio's own APRS/digipeater logic otherwise fights the KISS stream.
3. Pick a simplex FM frequency for packet, low power. Do not use the APRS frequency (144.800 / 144.390) for chat.
4. **Menu → Pairing** so the radio is visible.
5. Close the **HT phone app**. The radio accepts one Bluetooth client at a time; if the phone is connected, the computer cannot be.

## In the app

1. Open WeeChat Radio and choose **VR-N76 / UV-PRO (Bluetooth)** in setup (toolbar → **setup** if you are already configured).
2. Press **Find radio**. The app looks for a paired radio, otherwise scans for ~8 s and pairs it for you (the fixed PIN is `0000`; numeric-comparison prompts are accepted).
3. Press **Save and start**. The station panel shows **RADIO … linked** in green and the radio shows its Bluetooth-data (phone) icon next to the power level.

Command line: `wcr tnc find` does the same and writes `wcr.toml`. `wcr tnc scan --inquiry` lists what your computer can see. `wcr tnc test --tx` sends one identified test frame and prints anything heard for 20 s.

## How you know it worked

- **RADIO** in the station panel reads `VR-N76 linked` (green). Red text tells you what is wrong: not paired, radio off, HT app still connected.
- Send a line in `#bulletin`: the radio's TX indicator lights for about two seconds.
- Another station on the frequency shows up in the heard list; **PTT** flickers `RX` when they transmit.
- Other packet stations see you as `M7TJF>WCR UI` — a normal AX.25 frame.

## Settings (wcr.toml)

```toml
[modem]
backend = "bluetooth"   # or "serial" for a COM port / /dev/rfcomm0

[tnc]
bt_name = "VR-N76"      # what Find radio matched
bt_addr = "38:D2:00:01:03:49"
txdelay_ms = 600        # below ~600 the radio clips the start of a frame
persist = 63
slot_ms = 100
frame_gap_ms = 400      # pause between frames; the TNC keys once per frame
ax25 = true             # wrap frames as SRC>WCR UI so packet stations can read the header
ax25_dest = "WCR"
```

The preset is fixed at `afsk-1200` (the radio modulates); the robustness ladder does not apply.

## Known quirks of the built-in TNC

- **One frame per PTT.** The radio drops the carrier between consecutive frames and re-sends its preamble each time. WeeChat Radio paces frames (`frame_gap_ms`) so nothing is lost; group messages therefore take a little longer than on modem73.
- **Back-to-back RX.** The radio decodes one frame at a time; a burst from another station may lose frames. ARQ retries cover this.
- **TXDELAY matters.** Users running Winlink found 600 ms the sweet spot; that is the default here.
- **Frame size.** Stay under ~200 bytes of payload. The app fragments longer messages automatically.
- **macOS** creates `/dev/cu.VR-N76` on pairing but some versions never deliver data on it. Use `backend = "serial"` with that path, or a Linux/Windows machine.

## Linux

Pair once, then the app connects by address:

```
bluetoothctl
  scan on          # note the address, 38:D2:00:xx:xx:xx
  pair 38:D2:00:xx:xx:xx
  trust 38:D2:00:xx:xx:xx
```

If the desktop grabs the radio as a headset, disable that profile in `/etc/bluetooth/input.conf` (`[General] Disable=Headset`) and restart `bluetooth.service`. Then set `tnc.bt_addr` in `wcr.toml`, or bind a port with `rfcomm bind /dev/rfcomm0 <addr> 1` and use `backend = "serial"`, `serial = "/dev/rfcomm0"`.

## Other KISS TNCs

Anything that speaks KISS on a serial port works with **KISS TNC on a serial port** in setup: Mobilinkd TNC3/TNC4, a Bluetooth COM port that Windows created, `/dev/rfcomm0`, a USB TNC. The same `afsk-1200` preset and AX.25 wrapping apply.
