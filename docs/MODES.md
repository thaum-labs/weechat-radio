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

## Gateway knobs

In `wcr.toml`:

```
[gateway]
rf_egress = true          # put internet traffic on the air
third_party = "deny"      # guest (~nick) traffic on RF: allow or deny
```

## How you know it worked

The status bar colour matches the table. Other stations on the map show the same colours.
