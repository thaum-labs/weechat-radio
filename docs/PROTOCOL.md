On the web: https://weechatradio.com/docs/protocol.html

# Wire protocol

Version 2 is the default on radio (KISS) and internet (WebSocket). Version 1 frames are still decoded during the transition.

This hub build (17 Sep 2026, commit `ce4a108`) speaks v2. Operator guide: https://weechatradio.com/docs/weak.html

## Version 2 header (variable, typically 16–22 bytes before body)

`msg_id` is not on the wire. The receiver recomputes `BLAKE3(origin || dest || seq || body)` (uncompressed body), first 8 bytes. The timestamp is not part of the id. Two messages in the same second get different ids when the destination, the text, or the per-origin `seq` differs. `seq` is one counter per callsign and advances on every send, including two at once.

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

## BEACON

Type BEACON. Body is UTF-8 `B|<mode>|<khz>` (example `B|internet-radio|144950`). The kHz field is the origin's dial so gateways and the map know which band a station is on. Legacy `B|<mode>` is still decoded; frequency is unknown. Beacons heard on RF are also tagged with the receiving station's own dial.

## Callsign packing

6 bits per character, 8 characters, alphabet:

` space A–Z 0–9 / - ~`

Guests: `~ALICE`. Amateur callsigns must look like a callsign (letter and digit).

## Delivery marks

Marks on your own live-chat line only move forward: `[..]` waiting, `[tx]` sent, `[rl]` you heard your own frame come back, `[ok]` delivered, `[all]` every member of a group acknowledged.

`[tx]` is this station keying the message. `[rl]` is that same message heard again with your callsign still the origin: a speaker echo (common on VOX) or someone relaying your frame. It is not the other station confirming receipt, and it can be skipped. `[ok]` is their ACK: origin is their callsign, body names your `msg_id`. If that ACK arrives before any echo, the line goes from `[tx]` to `[ok]`.

A direct message paints `[ok]` once. A later ACK for the same line does not repeat `delivered`, and it cancels any retry that was still queued or keying. A closed group paints `[ok]` on the first member ACK and `[all]` when every member stored on this station has ACKed. `#bulletin` is public: there is no member list, so the first ACK from anyone paints `[ok]` and the line does not go on to `[all]`. Extra bulletin ACKs are quiet. `[no]` means the send was not accepted (the hub refused the callsign, for example) or the retries are used up. `[no]` and `[ok]` stay put when a later copy of your own frame is heard. A real ACK can still move `[no]` to `[ok]`.

Diagram: https://weechatradio.com/docs/protocol.html#delivery-marks

## ACK

Type ACK. This is what paints `[ok]` on the sender. Body is the 8-byte `msg_id` of the original, plus an optional 9th byte: receiver SNR in dB as a signed integer (`-32`…`31`). Relayed with TTL.

Unacked messages originated by this station are retransmitted up to `rf.max_retries` times (default 3), stepping the modem73 mode down the robustness ladder each try. That includes a line sitting on `[rl]`: hearing your own frame is not an ACK, so the retry timer keeps running until an ACK arrives or the retry count is used up. When the count is used up the line waits one more full interval and is then marked `[no]` if no ACK arrived. A later ACK can still move it to `[ok]`. `[xN]` is how many times that line was keyed. The first send is `[x1]`. With `max_retries` 3 the last copy is `[x4]` (the `retry-3/3` transmission). `[no]` is not sent in that same second.

Retry hold before each extra copy, counted from when the previous copy was keyed: routine 45s, then 90s, then 180s; priority 20s, 40s, 80s; emergency 8s, 16s, 32s. After the last copy the same schedule takes one more step (routine 360s, priority 160s, emergency 64s) and that wait does not key again. Each wait is multiplied by a random factor from 0.75 to 1.25 when `[rf] retry_jitter` is on, so the routine gaps can look like about 34–56s, 68–113s, 135–225s, and then 270–450s before `[no]`. A busy channel can add more time before a copy actually keys, because modem CSMA holds the frame; those longer gaps are channel access, not a different timer. The jitter keeps stations that collided from retrying in lockstep. Once an ACK is in, the hold is cleared and a copy that was already keying does not schedule the next try. Own unacked rows older than `outbound_max_age_secs` (default 3600) are marked `[no]` instead of being keyed, including across a restart. `/radio flush` does that immediately and also drops relay holds.

Group ACKs wait a random 0–`ack_dither_ms` (default 800 ms) before keying so every member of a net does not ACK at once. Direct (point-to-point) ACKs are not dithered.

If the channel stays busy, an ACK does not wait behind the rest of the air queue or out the full chat defer. It is keyed after `emergency_max_defer_ms` (default 3 s). That key turns modem carrier sense off for the ACK alone and restores it afterwards, including when this station's own sense still says idle, because a steady noise floor often produces no decoded frame while modem73's detector is busy. The same carrier-sense-off send happens immediately when the modem reports undecodable busy (not `rx`). While the modem is demodulating a frame, or this station's sense is `rx`, carrier sense stays on so the ACK does not key over a station we can hear. Ordinary chat and relays use `max_defer_ms` (default 15 s), and the modem may still refuse to key those over a carrier it hears. A station does not relay a frame whose destination is itself, and it does not relay a message that already has an ACK. An ACK toward another station is still relayed. Hearing a later copy of an unacked frame (a retry) is answered again, and a middle station forwards that retry, so a lost receipt does not end at `[rl]`.

A closed group on the hub is delivered to sessions that registered that group name (from their local member list) and to callsigns listed as members on the hub. It is not sent to every connected station. The group name is sent to the hub when the group is created or joined, so the first message does not wait for the periodic membership refresh. `#bulletin` still goes to every connected station.

## HAVE / WANT

Body is a comma-separated list of hex msg ids in a closed group window.

## FRAG (erasure coding)

Type `FRAG` (10). Used when the encoded frame is larger than the current PHY MTU.

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

Any `k` of `k+m` shards reconstruct the original body. `k` starts at `[rf] frag_k` (default 2, with `frag_m` 1 parity shard) and grows until every fragment fits the PHY MTU, because a modem drops an oversized frame instead of keying it. Gateways forward fragments as they hear them; the hub reassembles from shards received via different gateways, then fans out the whole message.

Fragments carry the original flags with `SIGNED` cleared, since a signature covers the whole body rather than one shard. `REQ_ACK` is preserved so the reassembled message still asks for a single ACK; a fragment never acknowledges on its own. The reassembled `msg_id` recomputes to the original, so that ACK matches what the sender queued.

The TX mode is chosen from the largest fragment, not the pre-fragment envelope. Picking it from the whole envelope forces the fastest rung, which a weak or VOX audio path cannot decode.

## Federation (reserved)

Hub-to-hub uses the same envelope and the same `msg_id` dedupe. Not enabled in v1.
