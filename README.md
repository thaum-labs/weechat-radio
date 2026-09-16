# WeeChat Radio

Chat that keeps working when the internet does not.

WeeChat Radio is a small program (`wcr`) that talks to [WeeChat](https://weechat.org/) like an IRC server, and talks to [modem73](https://github.com/RFnexus/modem73) like a TNC. You can use the internet, HF/VHF radio, or both. Messages store-and-forward so nobody has to leave a radio on all night.

Live map: [weechatradio.com](https://weechatradio.com)

WeeChat is a trademark of Sébastien Helleu. This project is independent.

## 5-minute start

**Linux / macOS**
```
curl -fsSL https://weechatradio.com/install.sh | sh
wcr setup
wcr tui
```

**Windows (PowerShell)**
```
irm https://weechatradio.com/install.ps1 | iex
wcr setup
wcr tui
```

How you know it worked: the status bar shows your callsign. Send a line in `#bulletin`. A `✓` means it went out.

Guides: [handheld + Digirig](docs/RADIO-SETUP.md), [VOX cable](docs/RADIO-SETUP.md), [HF + CAT](docs/RADIO-SETUP.md).

## Modes

| Command | What it does |
|---------|----------------|
| `/radio mode internet` | Internet only |
| `/radio mode internet-radio` | Radio first, internet fills gaps (gateway) |
| `/radio mode radio` | Radio only — never uses the internet |
| `/radio mode radio-plus` | Radio only, but a gateway may forward you |

If the hub dies, radio keeps working. The bar says **Internet down, radio only**.

## Build from source

```
cargo install --path crates/wcr
```

You need a Rust toolchain. On Windows, the MSVC build tools are required for the bundled SQLite.

## Licence

Apache-2.0 for the daemon, hub, and website. The WeeChat script `weechat/radio.py` is GPL-3.0-or-later (it runs inside WeeChat).

See `LICENSE`, `NOTICE`, and `weechat/LICENSE`.
