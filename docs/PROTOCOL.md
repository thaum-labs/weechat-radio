# Wire protocol

Version 1. Same envelope on radio (KISS) and internet (WebSocket).

## Header (31+ bytes before body)

| Field | Size | Notes |
|-------|------|-------|
| ver | 1 | `1` |
| type | 1 | MSG ACK BEACON HAVE WANT PING CHECKIN STATUS FORM FILE |
| flags | 2 | little-endian |
| msg_id | 8 | BLAKE3(origin, dest, seq, body) truncated |
| origin | 6 | packed callsign |
| dest | 6 | packed callsign or group name |
| hops_left | 1 | TTL, default 3 |
| ts | 4 | unix seconds, informational |
| seq | 4 | per-origin counter (clock-tolerant identity) |
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

`!!` at the start of a message is emergency. `!` is priority.

## Callsign packing

6 bits per character, 8 characters, alphabet:

` space A–Z 0–9 / - ~`

Guests: `~ALICE`. Amateur callsigns must look like a callsign (letter and digit).

## ACK

Type ACK. Body is the 8-byte `msg_id` of the original. Relayed with TTL.

## HAVE / WANT

Body is a comma-separated list of hex msg ids in a closed group window.

## Federation (reserved)

Hub-to-hub uses the same envelope and the same `msg_id` dedupe. Not enabled in v1.
