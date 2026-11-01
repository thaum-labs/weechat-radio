# Raspberry Pi gateway

A Raspberry Pi can run a WeeChat Radio station on its own, for example an always-on email gateway with a radio left on one channel. You check on it from a phone or laptop on the same network, in a browser.

## What you need

- A Raspberry Pi 3, 4 or 5 with **64-bit** Raspberry Pi OS (Lite is enough). The Pi 3 is the smallest model we test: it has 1 GB of memory and a slower CPU, so use it for a gateway on a robust preset. A Pi 4 or 5 has more headroom.
- The official Raspberry Pi power supply (or one with the same rating), and a microSD card (16 GB or more). A weak supply makes the Pi drop USB devices: in a hardware test the Pi could not see the Digirig at all until it moved to the official supply. `vcgencmd get_throttled` should say `0x0`, and `dmesg` should show no "Undervoltage detected".
- A USB sound card interface to the radio, such as a Digirig (audio, plus serial PTT on the same cable), or another USB sound card with VOX or a CM108 PTT line.
- For a Digirig, a USB-C cable that carries data. A charge-only cable powers it but shows nothing in `lsusb`.
- A transceiver left on a fixed frequency, with its cable to the interface.
- The Pi on your network (Ethernet is the most reliable; Wi-Fi works).

**A gateway must be able to transmit.** It acknowledges each mail chunk, confirms that the hub took the mail, answers inbox requests and keys receipts. A receive-only radio cannot be a gateway.

## Install

On the Pi, logged in as your own user (not root):

1. Install WeeChat Radio:

   ```
   curl -fsSL https://weechatradio.com/install.sh | sh
   ```

   On a 64-bit Pi this fetches the `linux-aarch64` build of `wcr` and `modem73` into `~/.local/bin`. It also installs, with apt, the few libraries `modem73` needs (`libncurses6`, `libhidapi-hidraw0`, `libhamlib4t64`) and WeeChat. `wcr` itself needs no desktop libraries, so Raspberry Pi OS Lite is enough. If anything is still missing, the installer stops with an error that names it.

2. Let your user reach the serial port and sound card:

   ```
   sudo usermod -aG dialout,audio $USER
   ```

   Log out and back in.

3. Set the station up:

   ```
   wcr setup
   ```

   Choose the preset for the band, the Digirig or sound card, the PTT, and the dial frequency the radio stays on. With "Handheld + Digirig", setup starts the serial and audio pickers on the Digirig (`plughw:CARD=Device,DEV=0` for audio), so Enter is right; the Pi's own `Headphones` card cannot hear the radio. Setup puts a station with both a radio and the internet in `internet-radio` mode by itself; that is the mode a gateway needs. Without a desktop it ends by pointing you at the next two steps.

   Running `wcr setup` again on a working gateway is safe: it keeps the web page password and the settings it does not ask about, and starts each picker on the current value. While the station runs, it holds the Digirig, so setup lists it as "in use"; stop the station first (`sudo systemctl stop wcr-web`) if you want to test the card. To try setup without touching the live file, give it another one: `wcr setup -c /tmp/test/wcr.toml`.

4. Choose a password for the web page (at least 8 characters):

   ```
   wcr web password
   ```

5. Start it at boot as a service. It restarts by itself after a crash:

   ```
   sudo ~/.local/bin/wcr web install
   ```

   This writes `/etc/systemd/system/wcr-web.service`, which runs as your user, and starts it. Check it with `systemctl status wcr-web` and read its log with `journalctl -u wcr-web -f`. The journal has the station's own log too, including modem73 problems (modem73's full log is `~/.local/share/wcr/modem73.log`).

## The web page

Open `https://<pi-name>.local:8443` from a phone or laptop on the same network (for example `https://raspberrypi.local:8443`), or use the Pi's IP address.

Right after the first boot, `raspberrypi.local` may not resolve yet. Use the Pi's IP address instead, for SSH and for the page (for example `ssh you@192.168.1.158` and `https://192.168.1.158:8443`). Your router's list of devices shows it, and so does `hostname -I` on the Pi. The `.local` name usually starts working a few minutes later.

**`wcr webui` finds it for you.** On a laptop with WeeChat Radio installed (Mac, Windows or Linux), run `wcr webui`. It looks for the page in this order: the address it found last time, then by name on the network (mDNS, including `raspberrypi.local`), then by trying port 8443 on every address of your network. It prints the address and opens it in your browser. With several gateways it lists them (callsign, name, IP) and asks which one; the choice is saved as `gateway` in `[web]`. Options: `--host <address>` skips the search, `--no-open` only prints, `--qr` also prints a QR code. It never opens a port, changes a service or sees your password. If nothing answers within a few seconds, it says so.

On the Pi itself, `wcr webui` prints the page's addresses, whether the `wcr-web` service runs (and how to start it), and a QR code a phone can scan.

- **HTTPS only.** On first start the Pi makes its own certificate. Browsers do not know it, so they warn once: check that the address is your Pi's, then continue. To use your own certificate, set `cert` and `key` (PEM file paths) in `[web]` in `wcr.toml`.
- **Password.** Log in with the password from `wcr web password`. A login lasts 12 hours of use.
- **Local network only.** The page answers only addresses on your own network (192.168.x.x, 10.x.x.x, 172.16–31.x.x, and IPv6 local addresses). Do not forward port 8443 on your router. If you really want it reachable from elsewhere, set `allow_public = true` in `[web]` and protect it yourself. That is your decision as the control operator.

The **gateway view** shows:

- whether the station, internet, hub and radio are up
- uptime, dial frequency, mode, preset and modem
- the audio level from the radio (audio in), as modem73 measures it
- frames forwarded for other stations
- mail through this gateway, with its delivery status, and this station's own outbox
- stations heard
- recent errors (newest first) and a log tail

The radio light is up only while modem73 is running and has its sound card. If it is down, the Errors box says why.

**Stop** and **Start** stop and start the station. The page stays up while the station is stopped. Start asks first, because the station then transmits: beacons and gateway replies.

## The App view: messages in the browser

**App** at the top of the page (or `https://<pi>:8443/app`) turns the Pi into a field computer you use from a phone or laptop on the same network, without the desktop app. **Gateway** goes back to the gateway view.

- **Chat** (Live Chat): your channels and direct conversations, with history from the station. Type and press Send (or Enter). New lines appear by themselves. Type `#name` or a callsign under the list and press Open to start one.
- Your own lines carry the same marks as in the desktop app: `[..]` waiting to go out, `[tx]` sent on air, `[x2]` keyed twice, `[rl]` heard relayed, `[gw]` put on the hub by a gateway, `[ok]` delivered, `[all]` every member got it, `[no]` failed. Lines from others show how they reached you (`[rf]`, `[net]`, `[lan]`) and the SNR of a radio line. **marks** next to the conversation name explains them.
- **Mail**: Inbox, Outbox and Sent, with the same status chips as the desktop app (sent, delivered, failed; there is no read or opened status). New mail, Reply, and Retry on a failed mail. A Radio+ station also has Check mail and Get.
- **Stations**: who the station has heard, newest first. **Message** opens a direct conversation.

The browser sends through the station exactly as the desktop app does, so the same on-air, hub and gateway rules apply. Slash commands, creating closed channels and invites stay in the desktop app for now.

modem73's own ports (KISS on 8001, control on 8073) listen only on the Pi itself (127.0.0.1), so nobody on the network can key the radio through them. To open both to the network on purpose, set `host = "0.0.0.0"` in `[modem]`.

Two things are on the network. The password-protected page on 8443. And port 7373 (TCP and UDP), where WeeChat Radio stations on the same network find each other and pass chat lines without the hub. A station on your network can hand the gateway a line there, and the gateway treats it like a line from the hub: it goes on the air only under the same gateway rules (`rf_egress`, the addressee must have been heard on this frequency lately, guests only with `third_party = "allow"`). Nobody can key the radio or change settings through 7373. If you do not want local stations at all, set `discovery = false` in `[lan]`; the port then closes.

`[web]` settings, all optional:

| Key | Default | Meaning |
| --- | --- | --- |
| `bind` | `0.0.0.0:8443` | Address and port the page listens on |
| `password_hash` | (empty) | Set by `wcr web password`. Empty means `wcr web` will not start |
| `allow_public` | `false` | Answer browsers outside the local network |
| `cert`, `key` | (empty) | Your own PEM certificate and key, instead of the self-signed pair |
| `gateway` | (empty) | On a laptop: the gateway page `wcr webui` opened last, checked first next time |

## The radio on a fixed channel

- Set the radio to the frequency you chose in setup. Turn off scanning, dual watch, power saving and VOX on the radio itself (use the interface's PTT).
- Set squelch to **0** on the gateway's radio and on the handhelds that talk to it. A data burst is short: at squelch 1 a VR-N76 only just opened on the Pi's beacon and cut out twice during it, and an RT-900 did not open at all; at squelch 0 both heard the whole burst. Squelch 0 hisses on the speaker, so turn the volume down rather than the squelch up. The hiss does not stop the gateway sending: on VHF/UHF FM modem73 counts only another station's data signal as busy, not hiss (`csma_mode`, see [RADIO-SETUP.md](RADIO-SETUP.md)).
- Set the audio levels before you leave it unattended (see below): the radio light on the web page should be up, and frames from other stations should show in the log.
- Use a preset the path supports. On HF, robust presets are slower but get through. A Pi 3 is fine on them.
- Make sure your licence allows unattended or remote operation of a station on that band, and that you can stop it: the Stop button, `sudo systemctl stop wcr-web`, or switching the radio off.

## Digirig audio levels

**Receive:** use the **RX gain** slider on the web page's **Radio audio** card. It sets the Digirig's own `Mic` capture gain (0 to +23.8 dB) and turns its Auto Gain Control off, saves the value in wcr.toml (`[modem] rx_gain_db`), and sets it again at every start, so `alsamixer` and `alsactl store` are not needed for it. At squelch 0, set it so the radio's hiss sits in the meter's green band (−45 to −12 dBFS) with nothing clipped; the hint under the meter says which way to go. The QA Pi (BF-1909, Mic at 12 of 16, about +18 dB) read −1.5 dBFS of hiss with a quarter of its samples clipped: about 0 dB of RX gain brings that into the green. If RX gain at its lowest still clips, turn the radio's volume down.

**Transmit:** the Digirig **Speaker** playback level is the audio into the radio's microphone input, so it sets the deviation. Set it once with `alsamixer -c Device` (F3 for playback; it starts at 44 of 151), then `sudo alsactl store`. Fine-tune with the **TX level** slider (it only turns down) and the far end's SNR, as in [RADIO-SETUP.md](RADIO-SETUP.md#audio-too-low--too-hot--good). Too much audio over-deviates and distorts the burst, and that costs more SNR than a weak signal.

The waterfall on the same card scales its colours to the noise floor, so squelch-0 hiss shows dark blue and a data signal bright, whatever the level.

## Quick signal check after setup

Do this once with a second station (a laptop or another Pi) a few metres away, both on the gateway's frequency and preset, squelch 0 on both radios:

1. **The far station hears the gateway.** Wait for the gateway's beacon (about a minute after start). The far station's heard list shows the gateway's callsign with an SNR. Listen on a handheld too: the burst should sound clean and steady, not clipped at the start or cut in the middle.
2. **The gateway hears the far station.** Send a bulletin from the far station. The gateway's web page shows it in the log, the Radio audio card shows its level as "last signal" (it should not read clipping), and the far station's heard list gets the gateway's reply.
3. **SNR is comfortable.** Target: 10 dB or more with the radios in the same room. A few dB means a level is wrong: the receive side clipping (the meter says so), or the transmit side overdriven or too weak (try TX level 0, −6, −12 and −18 dB and keep the best), or the squelch is chopping the burst.
4. **A long message gets through.** Send a direct message of about 500 characters each way. It should arrive whole, and the sender should see `[ok rf]`.

If step 1 fails and step 2 works, it is the transmit side (Speaker level, TX level, PTT); if step 2 fails, it is the receive side (RX gain, radio volume, squelch).

## Troubleshooting

- **The Digirig does not show up** (`lsusb` has no `10c4:ea60 Silicon Labs CP210x UART Bridge` and no `0d8c:013c C-Media Electronics, Inc. CM108 Audio Controller`; the IDs are what matters, the names vary): try a USB-C cable that carries data, and the official power supply. `vcgencmd get_throttled` must say `0x0`.
- **`raspberrypi.local` does not answer:** run `wcr webui` on your laptop, or use the Pi's IP address (see The web page).
- **Audio in says "not measured":** the modem73 next to `wcr` is an older one. Run `wcr update`.

## Without a monitor

Raspberry Pi Imager can set the hostname, your user, Wi-Fi and SSH before the first boot. Do the steps above over SSH. A first-run setup page in the browser, so SSH is not needed at all, is planned next.

## Updating

Run `wcr update`, then `sudo systemctl restart wcr-web`.
