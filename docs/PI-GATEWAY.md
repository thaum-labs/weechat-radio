# Raspberry Pi gateway

A Raspberry Pi can run a WeeChat Radio station on its own, for example an always-on email gateway with a radio left on one channel. You check on it from a phone or laptop on the same network, in a browser.

## What you need

- A Raspberry Pi 3, 4 or 5 with **64-bit** Raspberry Pi OS (Lite is enough). The Pi 3 is the smallest model we test: it has 1 GB of memory and a slower CPU, so use it for a gateway on a robust preset. A Pi 4 or 5 has more headroom.
- A power supply rated for your Pi, and a microSD card (16 GB or more).
- A USB sound card interface to the radio, such as a Digirig (audio, plus serial PTT on the same cable), or another USB sound card with VOX or a CM108 PTT line.
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

   Choose the preset for the band, the Digirig or sound card, the PTT, and the dial frequency the radio stays on. Setup puts a station with both a radio and the internet in `internet-radio` mode by itself; that is the mode a gateway needs. Without a desktop it ends by pointing you at the next two steps.

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

- **HTTPS only.** On first start the Pi makes its own certificate. Browsers do not know it, so they warn once: check that the address is your Pi's, then continue. To use your own certificate, set `cert` and `key` (PEM file paths) in `[web]` in `wcr.toml`.
- **Password.** Log in with the password from `wcr web password`. A login lasts 12 hours of use.
- **Local network only.** The page answers only addresses on your own network (192.168.x.x, 10.x.x.x, 172.16–31.x.x, and IPv6 local addresses). Do not forward port 8443 on your router. If you really want it reachable from elsewhere, set `allow_public = true` in `[web]` and protect it yourself. That is your decision as the control operator.

The **gateway view** shows:

- whether the station, internet, hub and radio are up
- uptime, dial frequency, mode, preset and modem
- frames forwarded for other stations
- mail through this gateway, with its delivery status, and this station's own outbox
- stations heard
- recent errors (newest first) and a log tail

The radio light is up only while modem73 is running and has its sound card. If it is down, the Errors box says why.

**Stop** and **Start** stop and start the station. The page stays up while the station is stopped. Start asks first, because the station then transmits: beacons and gateway replies.

`[web]` settings, all optional:

| Key | Default | Meaning |
| --- | --- | --- |
| `bind` | `0.0.0.0:8443` | Address and port the page listens on |
| `password_hash` | (empty) | Set by `wcr web password`. Empty means `wcr web` will not start |
| `allow_public` | `false` | Answer browsers outside the local network |
| `cert`, `key` | (empty) | Your own PEM certificate and key, instead of the self-signed pair |

## The radio on a fixed channel

- Set the radio to the frequency you chose in setup. Turn off scanning, dual watch, power saving and VOX on the radio itself (use the interface's PTT).
- Set the radio's audio levels before you leave it unattended: the radio light on the web page should be up, and frames from other stations should show in the log.
- Use a preset the path supports. On HF, robust presets are slower but get through. A Pi 3 is fine on them.
- Make sure your licence allows unattended or remote operation of a station on that band, and that you can stop it: the Stop button, `sudo systemctl stop wcr-web`, or switching the radio off.

## Without a monitor

Raspberry Pi Imager can set the hostname, your user, Wi-Fi and SSH before the first boot. Do the steps above over SSH. A first-run setup page in the browser, so SSH is not needed at all, is planned next.

## Updating

Run `wcr update`, then `sudo systemctl restart wcr-web`.
