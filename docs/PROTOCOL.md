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

Type BEACON. Body is UTF-8 `B|<mode>|<khz>` (example `B|internet-radio|144950`). The kHz field is the origin's dial so gateways and the map know which band a station is on. Legacy `B|<mode>` is still decoded; frequency is unknown. Beacons heard on RF are also tagged with the receiving station's own dial. A station beacons about once a minute for its first three minutes, so its neighbours learn it even if one beacon is lost, then every 5 minutes on a quiet channel. It skips its beacon for 10 minutes after any traffic it keyed or heard, mail bursts and reports included, and beacons at least every 10 minutes. Its first beacon goes whatever it hears, but not after its own mail: keyed before the receiver's reply, it would leave the sender deaf to that reply. An idle beacon keyed while a line walks the chain can take a relay's copy at the station beside it. A line's retries and receipts run for about that long, and a beacon keyed between them can leave a relay deaf to the next retry.

## Callsign packing

6 bits per character, 8 characters, alphabet:

` space A–Z 0–9 / - ~`

Guests: `~ALICE`. Amateur callsigns must look like a callsign (letter and digit).

## Delivery marks

Marks on your own live-chat line only move forward: `[..]` waiting, `[tx]` sent, `[rl]` you heard your own frame on RF, `[gw]` a gateway accepted it onto the hub, `[ok]` delivered, `[all]` every member of a closed group acknowledged.

`[tx]` is this station keying the message. `[rl]` is that same message heard again with your callsign still the origin: a speaker echo (common on VOX) or someone relaying your frame on the radio. It is shown once for that message. It is not the other station confirming receipt, and it can be skipped. In `radio` mode the frame carries `NO_INET`, so `[rl]` is as far as that copy goes: a gateway logs that the bulletin stays on RF and does not upload it. `[gw]` is a gateway telling you it forwarded the frame to the hub (`forwarded to hub` in that gateway's log, distinct from `relayed`). `[gw]` does not stop retries. `[ok]` is their ACK: origin is their callsign, body names your `msg_id`. If that ACK arrives before any echo, the line goes from `[tx]` to `[ok]`.

A direct message paints `[ok]` once. A later ACK for the same line does not repeat `delivered`, and it cancels any retry that was still queued or keying. A closed group paints `[ok]` on the first member ACK and keeps retrying until every member stored on this station has ACKed (`[all]`, painted once), or the retries are used up. One member's ACK does not paint `[no]` and does not cancel the remaining copies or a gateway's RF relay to the other members. When the retries are used up and some members have ACKed but not all of them, the line stays `[ok]` and stays open for a late ACK to become `[all]`. Retries after `[rl]`, `[gw]` or `[ok]` go out without a new mark, and an echo heard after `[ok]` does not paint `[rl]`, so a mark never moves back. A station that sends on a closed group, or answers one of its lines, is added to the member list on every member station that hears it, so a station whose roster was lost still counts toward `[all]`. `#bulletin` is public: there is no member list, so the first ACK from anyone paints `[ok]` and the line does not go on to `[all]`. The sender stops there, or when it hears another station relay the bulletin on: someone has it. A relayed bulletin with no ACK keeps `[rl]` and does not go to `[no]`. A relay of that bulletin keeps sending until it hears a station further along, so one neighbour ACK does not end the copy for the rest of a chain. Extra bulletin ACKs are quiet. `[no]` means the send was not accepted (the hub refused the callsign, for example) or the retries are used up with nobody having ACKed. `[no]` and `[ok]` stay put when a later copy of your own frame is heard. A real ACK can still move `[no]` to `[ok]`. `[gw]` can move on to `[ok]`, `[all]`, or `[no]`. A `[gw]` notice that arrives after `[ok]`, `[all]`, or `[no]` is ignored. A group ACK starts with 6 hops, the same as the group message, and the member retries it on the same schedule until a station further along relays it, so a receipt can walk back along a five-station line.

Diagram: https://weechatradio.com/docs/protocol.html#delivery-marks

## ACK

Type ACK. This is what paints `[ok]` on the sender. Body is the 8-byte `msg_id` of the original, then the receiver SNR in dB as a signed byte (`-32`…`31`, or `-128` for none), then the hops left on the original when this station heard it. ACKs from before v0.1.90 stop after the id or the SNR byte. Relayed with TTL.

Unacked messages originated by this station are retransmitted up to `rf.max_retries` times (default 3), stepping the modem73 mode down the robustness ladder each try. That includes a line sitting on `[rl]`: hearing your own frame is not an ACK, so the retry timer keeps running until an ACK arrives or the retry count is used up. When the count is used up the line waits two more full intervals and is then marked `[no]` if no ACK arrived: on a long or busy chain an ACK takes as long to walk back as the line took to walk out. A later ACK can still move it to `[ok]`. `[xN]` is how many times that line was keyed, counted until the line shows `[rl]`, `[gw]` or `[ok]`. The first send is `[x1]`. With `max_retries` 3 the last copy is `[x4]` (the `retry-3/3` transmission). `[no]` is not sent in that same second.

Retry hold before each extra copy, counted from when the previous copy was keyed: routine 45s, then 90s, then 180s; priority 20s, 40s, 80s; emergency 8s, 16s, 32s. After the last copy the station waits that schedule's next step twice (routine 360s, priority 160s, emergency 64s) and does not key again. Each wait is multiplied by a random factor from 0.75 to 1.25 when `[rf] retry_jitter` is on, so the routine gaps can look like about 34–56s, 68–113s, 135–225s, and then 540–900s before `[no]`. A line the station heard a relay carry on (its own line back with fewer hops) is not marked `[no]` then: it keeps `[rl]` until its receipt arrives or it reaches `outbound_max_age_secs`, because on a long or busy chain the receipt can come back after the last wait. A busy channel can add more time before a copy actually keys, because modem CSMA holds the frame; those longer gaps are channel access, not a different timer. The jitter keeps stations that collided from retrying in lockstep. Once an ACK is in, the hold is cleared and a copy that was already keying does not schedule the next try. Own unacked rows older than `outbound_max_age_secs` (default 3600) are marked `[no]` instead of being keyed, including across a restart. `/radio flush` does that immediately and also drops relay holds.

Group ACKs wait a random whole-frame slot (up to 6 for routine) plus 0–`ack_dither_ms` (default 800 ms) before keying, so every member of a net does not ACK at once and two members on either side of the sender do not land on it together. Direct (point-to-point) ACKs are not delayed.

If the channel stays busy, an ACK does not wait behind the rest of the air queue or out the full chat defer. It is keyed after `emergency_max_defer_ms` (default 3 s). That key turns modem carrier sense off for the ACK alone and restores it afterwards, including when this station's own sense still says idle, because a steady noise floor often produces no decoded frame while modem73's detector is busy. The same carrier-sense-off send happens immediately when the modem reports undecodable busy (not `rx`). While the modem is demodulating a frame, or this station's sense is `rx`, carrier sense stays on so the ACK does not key over a station we can hear. Ordinary chat and relays use `max_defer_ms` (default 15 s), and the modem may still refuse to key those over a carrier it hears. A station does not relay a frame whose destination is itself. A direct message that already has an ACK is not relayed again. A `#bulletin` relay keeps going until a station further down the chain is heard. A closed group is still relayed until `[all]`, so one member's ACK does not stop the RF copy for the members who have not heard it, and each relay keys it again until a station further along is heard. A relay also stops repeating a line, and stops passing on the originator's retries, once every station it hears directly has the line: the originator, a member whose receipt it holds, or, when it hears only one station and got the line relayed, that station. At the end of a chain that is one relay and no repeats. A station that hears only the originator keeps relaying, up to the cap below: a station beyond it whose beacons were lost would look the same. A relay keys one line at most 3 times in all (its relay, a repeat, and an answer to a retry); the receipts it carries are not capped. Its copies of one line go at least the line's first retry interval apart (45 s routine): a modem whose carrier sense sticks keys what is queued behind a held frame in the same burst, so two copies handed over together would share one collision. A station learns its neighbours from beacons and from frames heard with no relay between, and uses that list from three minutes after start. An invite to a closed group, and its ACK, carry the group's 6 hops. An ACK travels back toward the originator only: a station carries a receipt only if it heard the line with more hops left than the receipt's sender did. An older ACK without that byte counts as coming from the originator's side when it arrived before this station's own relay of the line went out. A relay keys a receipt again until a station closer to the originator is heard carrying it. Next to the originator a receipt goes once, and the originator's retry asks for it again. A `#bulletin` is ACKed only by the stations that heard it from its originator. Relays and group receipts key in a random whole-frame slot, so two neighbours answering the same frame do not land on top of each other at the station between them. The hub keeps an offline station's direct messages for 10 minutes (32 per callsign) and delivers them on the next hello. It does not keep groups or `#bulletin`, and it does not key a radio. Hearing a later copy of an unacked frame (a retry) is answered again, and a middle station forwards that retry, so a lost receipt does not end at `[rl]`.

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
