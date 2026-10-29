On the web: https://weechatradio.com/docs/api.html

OpenAPI: [openapi.yaml](./openapi.yaml) (also [on the site](https://weechatradio.com/docs/openapi.yaml)).

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
  "freq_khz": 144950,
  "band": "2m",
  "settings": { "frequency": "144.950 MHz", "band": "2m" },
  "events": [
    { "ts": 1710000000, "kind": "tx", "origin": "G4ABC", "dest": "NET", "hops": 3, "band": "2m" }
  ]
}
```

`ts` must be within 5 minutes of the hub clock.

## Read

- `GET /api/v1/nodes` — live stations and settings (`band`, `freq_khz`, `frequency`). `?heard_since=` keeps stations whose `last_seen` is at or after that unix time. The `nodes` table itself is not aged out; `node_seen` samples are kept for 7 days.
- `GET /api/v1/nodes?since=` — replay trail of position samples (not the live list)
- `GET /api/v1/events?since=&limit=` — TX/RX/relay events, kept for 7 days
- `GET /api/v1/links?since=&limit=` — one row per origin/dest pair, the newest traffic or mail event since `since` (map line history)
- `GET /api/v1/hubs` — hub/relay status, including `bands`
- `GET /api/v1/bands` — stations grouped by band (`band`, `freq_khz`, `gateways`, `stations`)
- `GET /api/v1/stats` — totals
- `GET /ws/live` — WebSocket firehose of the same events
- `GET /healthz` — `ok`
- `GET /version` — `{"version", "commit"}` of the running hub (`commit` is empty in a local build)

## How you know it worked

```
curl https://hub.weechatradio.com/api/v1/stats
```

returns JSON with `nodes_online`.
