On the web (with pictures): https://weechatradio.com/guides/radio.html

# Radio setup

Four paths. Pick the one that matches your station. All of them work **without the internet**.

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
2. Turn the radio on. Set **FM**, simplex (one frequency, not a repeater), low power, **squelch 0**. Disable voice scramble and VOX on the radio itself.
3. Note the serial port:
   - Windows: Device Manager → Ports (COM & LPT), for example `COM5`
   - Linux: `ls /dev/ttyUSB* /dev/ttyACM*`
   - macOS: `ls /dev/cu.usb*`
4. Run `wcr setup` and choose **Handheld + Digirig**. Enter that port.
5. Start with `wcr tui`. Watch the status bar: audio should read `good`, not `low` or `hot`. Tell the app the frequency on the dial: `/radio freq 144.950`.
6. Ask a second station on the same frequency to send a short test. You should see their callsign in the heard list.

### How you know it worked

The channel state flickers `rx` when they transmit, and SNR is a positive number. Your line gets `[tx]` after you send, then `[ok]` when it is delivered.

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
3. Run `wcr setup` and choose **Any radio with a plain audio cable (VOX)**. This selects the `vox-safe` preset: RDM-600S (robust over a speaker and mic) and a 900 ms lead so the radio is fully keyed before data starts.
4. Start `wcr tui`. Send a short test to `#bulletin`. Tell the app the frequency on the dial: `/radio freq 144.950`. Keep radio VOX hang on the long side.
5. If the other station still hears a clipped start, raise `modem.vox_lead_ms` in `wcr.toml` (try 1200).

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

## Path D — VR-N76 / UV-PRO / GA-5WB over Bluetooth (3 minutes)

On the web: https://weechatradio.com/guides/vr-n76.html · offline: `wcr help vr-n76`

These handhelds have a built-in 1200 bd packet TNC that speaks KISS over Bluetooth. No cable, no sound card, no modem73.

### Steps

1. On the radio: **General Settings → KISS TNC → Enable**, **Digital Mode → off**, then **Menu → Pairing**. Close the HT phone app.
2. In the app choose **VR-N76 / UV-PRO (Bluetooth)** and press **Find radio**. It pairs (PIN 0000) and saves the address. Terminal: `wcr tnc find`.
3. **Save and start**. The station panel shows `RADIO VR-N76 linked`.

### How you know it worked

The radio shows its Bluetooth-data icon; its TX light comes on for ~2 s when you send. `wcr tnc test --tx` sends one identified test frame and prints what it hears.

Also in setup: **KISS TNC on a serial port** for Mobilinkd, `/dev/rfcomm0`, or a Bluetooth COM port.

---

## Audio too low / too hot / good

The Pi page (**Radio audio** card) and the desktop app (**IN** meter) show the receive level against a green target band, with a word and a hint that says what to change. `/level` prints the same in any client. Every radio and sound card is different, so set yours by the meter, not by a number from someone else's setup.

| Word | Means | Do |
|---|---|---|
| **good** | Idle hiss or band noise from −45 to −12 dBFS RMS (HF: from −55), nothing at full scale | Leave it. |
| **low** | Under −45 dBFS (HF: −55). modem73 stops finding frames when all it hears is under about −58 | Radio volume up, or raise **RX gain**. |
| **high** | Over −12 dBFS, or peaks within 1 dB of full scale: a loud signal will clip | A little down. |
| **clipping** | 0.5 % or more of the samples at full scale | Down until the meter is green. |
| **noisy** | HF only: idle noise above the busy threshold, so sends wait | Down until idle noise reads 3 dB under the threshold. |

At squelch 0 on FM the hiss is the loudest thing the radio passes, so set the level by the hiss. Anywhere in the green band decodes as well as anywhere else: in our test with two modem73s, hiss from −50 to −4 dBFS gave the same 35 dB SNR, as long as the signal did not clip. Clipping costs SNR: 4 % of samples at full scale cost 12 dB.

**RX gain** (Pi page slider, desktop app **RX GAIN**, `/level rx <dB>`):

- **Digirig or another USB sound card on Linux** (the Pi): wcr sets the card's own capture gain (`Mic` on a Digirig, 0 to +23.8 dB) and turns its automatic gain off. It is saved as `[modem] rx_gain_db` and set again at every start, with no sudo (the station's user is in the `audio` group). If RX gain is at its lowest and the meter still says clipping, turn the radio's volume down.
- **VR-N76 / UV-PRO Bluetooth audio:** wcr's own gain on the radio's audio, −30 to +12 dB, saved as `[tnc] bt_rx_level_db`. The radio's volume knob does not change this audio.
- **Elsewhere** (Windows, Mac sound cards): the radio's volume and the computer's input level.

**TX level** (Pi page slider, desktop app **TX LVL**, `/level tx <dB>`, `[modem] tx_level_db`): −26 to 0 dB, and it only turns down, because modem73 already sends at full scale. Too much audio overdrives the radio's microphone input, and that distortion costs more than a weak signal: in our test, audio 6 dB into clipping cost 20 dB of SNR.

How to set TX without test gear, with someone at the far end:

1. Set the radio's own mic gain to the middle (on a VR-N76 the Bluetooth mic gain, in the radio's menu or its app).
2. Send a line at TX level 0, then −6, −12 and −18 dB. The far end reads the SNR of each from its heard-stations list.
3. Keep the level where the SNR stops getting better. Two radios in one room should reach 10 dB or more; under 6 dB, check levels at both ends.

**Busy test before sending** (`[modem] csma_mode`, default `auto`):

- **VHF/UHF FM:** at squelch 0 an FM radio hisses louder than any signal, so modem73 counts only another station's data signal as busy, not hiss (`sync`). Voice on the channel does not count as busy either, so listen to the frequency before you leave a station running.
- **HF (below 30 MHz):** the band is shared with SSB voice, FT8 and other data users, so modem73 waits while any audio is above its threshold (`level`). Set the receive audio so the idle band noise sits below that threshold (see the input target below), or the station waits for a quiet moment that never comes.
- `csma_mode = "sync"` or `"level"` sets the test by hand. VOX stations always use the level test.

---

## When the path is weak

If the other station is faint, or HF is fading, do not turn the audio up into **hot**. Pick a slower preset and let the program retry.

- `hf-poor` is the usual HF calling preset. `hf-weak` and `hf-deep` are slower.
- The status bar **TX** and **RETRY** fields show when the program has stepped down on its own.

Guide: https://weechatradio.com/docs/weak.html (offline: `wcr help weak`).

---

## Suggested calling frequencies

See `wcr help calling` or https://weechatradio.com/docs/calling.html. Always verify the band plan for your country before you transmit.
