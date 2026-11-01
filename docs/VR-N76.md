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

## Bluetooth audio mode (VR-N76, VR-N7600)

The second way to use these radios: WeeChat Radio sends **modem73 audio** over the radio's Bluetooth link instead of using the 1200 bd TNC. Same no-cable setup, but you get the modem73 modes (faster, and the robustness ladder works).

1. On the radio: **squelch 0**, **KISS TNC off**, **Digital Mode off**, then **Menu → Pairing**. Close the HT phone app. Data bursts are short: at squelch 1 a VR-N76 chopped a Pi gateway's bursts and decoded none of them (QA #239). Set the radios at the other end to squelch 0 as well.
2. In setup choose **VR-N76 / UV-PRO / GA-5WB (Bluetooth audio, modem73)** or **VR-N7600 (Bluetooth audio, modem73)**, press **Find radio** (macOS: pair the radio in System Settings → Bluetooth first), then **Save and start**.
3. **RADIO** reads `VR-N76 linked` in green, or `lost, reconnecting…` in red; the app reconnects by itself every 5 s.

Presets: `vhf-fm` (default), `hf-poor`, `hf-weak` and `hf-deep`. `hf-good` is refused: the radio's Bluetooth audio codec (SBC) breaks 8PSK.

```toml
[modem]
backend = "modem73"
ptt = "bluetooth"       # modem73 keys the radio through wcr's Bluetooth bridge
preset = "vhf-fm"

[tnc]
bt_name = "VR-N76"      # or bt_addr = "38:D2:00:01:03:49"
```

This needs the modem73 that comes with WeeChat Radio (it adds network audio); a stock modem73 is refused with a clear message.

**Squelch 0 and sending.** At squelch 0 the radio streams its receiver hiss all the time, and the hiss is louder than a station's signal (QA #239: about -4 dBFS of hiss, -45 dBFS under a carrier). A busy test on audio level would never find the channel free, so WeeChat Radio starts modem73 with its sync busy test: only another station's data signal (modem73's sync or lead tone) counts as busy, not hiss. This is `csma_mode` in `[modem]`:

```toml
[modem]
csma_mode = "auto"     # auto (default) | sync | level
```

`auto` means `sync` on every preset, HF included: an HF receiver passes band noise all the time, which a level test reads as busy in the same way. `level` goes back to modem73's level test. VOX stations always keep the level test. With `sync`, signals modem73 cannot decode (voice, other data modes) do not count as busy, so listen before you leave a station running on a busy HF band. `modem73.log` shows the one in use at start: `CSMA: enabled (mode=sync, …)`.

**Transmit level.** modem73's audio already peaks at full scale, so WeeChat Radio cannot send it louder without clipping it: at +6 dB a quarter of each QPSK burst clipped and the far end's SNR went down (QA #239). The level that sets your deviation is the radio's own **Bluetooth mic gain** (0 to 7; in HTCommander: radio settings → Bluetooth Mic Gain). Raise it one step at a time and redo the transmit step of the quick signal check below; stop when the SNR stops improving. `bt_tx_level_db` in `[tnc]` can only lower the level (-20 to 0 dB), for a radio that distorts; higher values are treated as 0 and logged.

**Is the radio's audio arriving?** While the radio is linked, `/status` (and the station log every 10 s) shows `bt_rx_bytes` (about 11 KB/s even on a quiet channel, because the radio streams all the time), `bt_rx_frames` by frame type and `sbc_decode_errors`. Bytes stuck at 0 means nothing arrives over Bluetooth; bytes rising with decode errors rising too means the audio arrives but does not decode.

**Quick signal check** (with a second station, such as a [Pi gateway](PI-GATEWAY.md#quick-signal-check-after-setup), on the same frequency and preset):

1. **Receive:** when the other station sends, the station panel's audio in rises above the floor (not stuck at -80 dB) and its callsign shows in the heard list.
2. **Transmit:** your beacon or a bulletin shows on the other station with a comfortable SNR. Target: 10 dB or more with the radios in the same room. Under 6 dB, look at the Bluetooth mic gain (transmit) or the far radio's squelch and volume (its receive).
3. **Both ways:** a direct message of about 500 characters arrives whole, and you see `[ok rf]`.

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
