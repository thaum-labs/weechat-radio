On the web: https://weechatradio.com/docs/protocol.html

# Wire protocol

Version 2 is the default on radio (KISS) and internet (WebSocket). Version 1 frames are still decoded during the transition.

## Version 2 header (variable, typically 16–22 bytes before body)

`msg_id` is not on the wire. The receiver recomputes `BLAKE3(origin, dest, seq, body)` (uncompressed body).

| Field | Size | Notes |
|-------|------|-------|
| ver | 1 | `2` |
| packed | 2 | type (high 4 bits) + flags (low 12 bits), little-endian |
| origin | 6 | packed callsign |
| dest | 1 or 6 | 1-byte well-known group index if `GROUP_IDX`, else packed callsign |
| hops_left | 1 | TTL, default 3 |
| ts | 2 | minutes since epoch mod 2^16; reconstructed against the receiver clock |
| seq | varint | per-origin counter (clock-tolerant identity) |
| body_len | varint | length of the (possibly compressed) body; max 300 uncompressed |
| body | n | UTF-8, optionally smaz-compressed |
| crc | 2 | CRC-16/IBM-3740 of everything before CRC |
| signature | 64 | optional Ed25519 if `SIGNED` |

Well-known group indices: `0` = `BULLETIN`, `1` = `BEACON`.

## Version 1 header (legacy, 35 bytes before body)

| Field | Size | Notes |
|-------|------|-------|
| ver | 1 | `1` |
| type | 1 | MSG ACK BEACON HAVE WANT PING CHECKIN STATUS FORM FILE FRAG |
| flags | 2 | little-endian |
| msg_id | 8 | BLAKE3(origin, dest, seq, body) truncated |
| origin | 6 | packed callsign |
| dest | 6 | packed callsign or group name |
| hops_left | 1 | TTL, default 3 |
| ts | 4 | unix seconds, informational |
| seq | 4 | per-origin counter |
| body_len | 2 | max 300 |
| body | n | UTF-8 |
| crc | 2 | CRC-16/IBM-3740 of everything before CRC |
| signature | 64 | optional Ed25519 if SIGNED |

## Flags

- 0 `INET_OK` — gateways may forward to the internet
- 1 `GROUP`
- 2 `SIGNED`
- 3 `REQ_ACK`
- 4 `THIRD_PARTY` — guest nick origin
- 5–6 priority: 0 routine, 1 priority, 2 emergency
- 7 `NO_INET` — never forward to the internet (`radio` mode)
- 8 `GROUP_IDX` — v2 wire only: dest is a 1-byte group index
- 9 `COMPRESSED` — v2 wire only: body is smaz-compressed (cleared after decode)

`!!` at the start of a message is emergency. `!` is priority.

Slow HF presets (`hf-poor`, `hf-weak`, `hf-deep`) send unsigned on RF so a chat line fits one PHY frame. Internet copies are still signed when the mode uses the hub.

## Callsign packing

6 bits per character, 8 characters, alphabet:

` space A–Z 0–9 / - ~`

Guests: `~ALICE`. Amateur callsigns must look like a callsign (letter and digit).

## ACK

Type ACK. Body is the 8-byte `msg_id` of the original, plus an optional 9th byte: receiver SNR in dB as a signed integer (`-32`…`31`). Relayed with TTL.

Unacked messages originated by this station are retransmitted up to `rf.max_retries` times (default 3), stepping the modem73 mode down the robustness ladder each try.

## HAVE / WANT

Body is a comma-separated list of hex msg ids in a closed group window.

## FRAG (erasure coding)

Type `FRAG` (10). Used for group messages on HF presets, or any payload larger than the current PHY MTU.

Each fragment body starts with:

| Field | Size | Notes |
|-------|------|-------|
| group_id | 4 | BLAKE3(origin, dest, seq) truncated |
| idx | 1 | shard index |
| k | 1 | data shards |
| m | 1 | parity shards |
| orig_kind | 1 | original message type |
| orig_len | 2 | original body length |
| shard | n | Reed–Solomon (GF(2^8)) shard |

Any `k` of `k+m` shards reconstruct the original body. Default `k=2`, `m=1` (`[rf] frag_k` / `frag_m`). Gateways forward fragments as they hear them; the hub reassembles from shards received via different gateways, then fans out the whole message.

## Federation (reserved)

Hub-to-hub uses the same envelope and the same `msg_id` dedupe. Not enabled in v1.
