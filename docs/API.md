# Public API

Base: `https://hub.weechatradio.com`

CORS is open. Reads need no key. Writes are signed.

## Write — `POST /api/v1/report`

Headers:

- `X-Radio-Callsign`
- `X-Radio-Pubkey` — hex Ed25519 public key
- `X-Radio-Signature` — hex signature of the **raw body**

First time a public key is seen, it is bound to that callsign. Later posts must use the same key.

Body is JSON metadata only. Never send message text.

```json
{
  "callsign": "G4ABC",
  "ts": 1710000000,
  "grid": "IO91wm",
  "mode": "internet-radio",
  "ptt": "digirig",
  "preset": "vhf-fm",
  "snr": 12.4,
  "ber": 0.0,
  "queue": 0,
  "hub_ok": true,
  "settings": { "frequency": "144.950" },
  "events": [
    { "ts": 1710000000, "kind": "tx", "origin": "G4ABC", "dest": "NET", "hops": 3 }
  ]
}
```

`ts` must be within 5 minutes of the hub clock.

## Read

- `GET /api/v1/nodes` — live stations and settings
- `GET /api/v1/events?since=&limit=` — TX/RX/relay events
- `GET /api/v1/hubs` — hub/relay status
- `GET /api/v1/stats` — totals
- `GET /ws/live` — WebSocket firehose of the same events
- `GET /healthz` — `ok`

## How you know it worked

```
curl https://hub.weechatradio.com/api/v1/stats
```

returns JSON with `nodes_online`.
