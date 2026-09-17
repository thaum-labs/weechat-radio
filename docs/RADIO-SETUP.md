On the web (with pictures): https://weechatradio.com/guides/radio.html

# Radio setup

Three paths. Pick the one that matches your station. All of them work **without the internet**.

Legal note: you are the control operator. Stay in your licence privileges. Suggested calling frequencies are only suggestions — check your band plan before you transmit.

---

## Path A — Handheld + Digirig (5 minutes)

On the web: https://weechatradio.com/guides/handheld.html

### What you need

- A VHF/UHF handheld (Baofeng, UV-K6, or similar)
- A [Digirig](https://digirig.net/) (or AIOC) USB cable
- The WeeChat Radio wizard already run (`wcr setup`)

### Steps

1. Plug the Digirig into the computer. Plug the audio/PTT cable into the radio's speaker-mic jack.
2. Turn the radio on. Set **FM**, simplex (one frequency, not a repeater), low power. Disable voice scramble and VOX on the radio itself.
3. Note the serial port:
   - Windows: Device Manager → Ports (COM & LPT), for example `COM5`
   - Linux: `ls /dev/ttyUSB* /dev/ttyACM*`
   - macOS: `ls /dev/cu.usb*`
4. Run `wcr setup` and choose **Handheld + Digirig**. Enter that port.
5. Start with `wcr tui`. Watch the status bar: audio should read `good`, not `low` or `hot`.
6. Ask a second station on the same frequency to send a short test. You should see their callsign in the heard list.

### How you know it worked

The channel state flickers `rx` when they transmit, and SNR is a positive number. Your `✓` appears after you send.

---

## Path B — Any radio with VOX (5 minutes)

On the web: https://weechatradio.com/guides/vox.html

VOX means the radio transmits when it hears sound.

### What you need

- Any radio that can key on VOX (voice-operated transmit)
- A 3.5 mm audio cable from the computer sound card to the radio mic/speaker
- Volume set so the radio just keys on data tones, not on room noise

### Steps

1. Connect computer headphone-out to radio mic-in, radio speaker-out to computer mic-in.
2. On the radio, turn **VOX on**. Start with a medium VOX delay.
3. Run `wcr setup` and choose **Any radio with a plain audio cable (VOX)**. This selects the `vox-safe` preset, which waits a moment so the radio is fully keyed before data starts.
4. Start `wcr tui`. Send a short test to `#bulletin`.
5. If the other station hears a clipped first character, raise `modem.vox_lead_ms` in `wcr.toml` (try 700).

### How you know it worked

The radio's TX LED lights for each message. Delivery ticks still work; they just take a little longer because of VOX delay.

---

## Path C — HF rig with CAT (5 minutes)

On the web: https://weechatradio.com/guides/hf.html

CAT means the computer can talk to the radio over USB (change frequency, press transmit).

### What you need

- An HF transceiver with USB CAT (IC-7300, FT-891, Xiegu, and most modern rigs)
- [Hamlib](https://github.com/Hamlib/Hamlib/releases) so `rigctld` is on your PATH

### Steps

1. Plug the radio USB into the computer. Leave the radio in USB, 2400 Hz filter if you have one.
2. Find your Hamlib model number: `rigctl -l` (or `rigctl.exe -l` on Windows).
3. Start CAT:

   ```
   rigctld -m MODEL -r COM3 -s 19200
   ```

   Replace MODEL, COM port, and baud with yours.

4. Run `wcr setup` and choose **HF rig with CAT**. Keep the default `127.0.0.1:4532`.
5. Pick a preset: `hf-good` on a clear band, `hf-poor` for NVIS, `hf-weak` when signals are faint, `hf-deep` as a last-resort MFSK backup. The node will step down automatically on retries if ACKs do not come back.
6. To change frequency: `/radio qsy 7.045` — this never happens automatically.

### How you know it worked

The status bar shows the frequency the radio reports. A decoded frame shows SNR. If CAT fails, run `/radio ptt vox` and use Path B until CAT is sorted.

---

## Audio too low / too hot / good

The status bar reads the receive level from modem73.

- **low** — turn radio volume up, or raise the sound-card input.
- **hot** — turn it down. Distortion kills data.
- **good** — leave it.

---

## Suggested calling frequencies

See `wcr help calling` or https://weechatradio.com/docs/calling.html. Always verify the band plan for your country before you transmit.
