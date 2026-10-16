# Email

Email sits beside Live Chat. It does not use the chat air queue, `send_chat`, or chat delivery.

The address is `{callsign}@mail.weechatradio.com`. There is no domain setting. Guests (`~NICK`) do not get the Email tab.

## Paths

| Mode | How mail leaves |
| --- | --- |
| internet | Hub, when the hub is up. No radio. |
| internet-radio | This station is a gateway. Its own mail goes to the hub when the hub is up. It also accepts radio-plus mail and posts that to the hub. |
| radio-plus | Always RF to the callsign in `[mail] gateway`. No shortcut to the hub. |
| radio | The tab stays grey. There is no path to Resend. |

`[gateway] third_party = deny` blocks chat relay only. It does not block a gateway from posting mail.

## Sent

- **Hub:** Sent after the hub says Resend accepted the message. A rejection is Failed.
- **Radio-plus, every PTT:** Keying the burst is Outbox `[tx]`. A chunk ack from the gateway is `[gw]` (the gateway has the bytes). Sent is only the gateway's `CompleteAck`, after Resend accepted it. Until then the line stays in the outbox.
- Missing acks are retried after the keyed burst should have finished, then with backoff (8s, then 16s, 32s, …) up to `[rf] max_retries` (at least 3). The clock starts at the end of that airtime, not at key-up. A retry sends only the chunks the gateway has not reported. The log labels each key `try 1/4` through `try 4/4` when `max_retries` is 3: the first send plus three retries. Then the row is Failed, with the reason: the gateway never acknowledged, or it has the message but it was not accepted.
- If the radio is not running, the row is Failed at once with `Radio is not running.`
- If the row is still `[tx]` five minutes after this attempt started (or three times the planned airtime plus a minute, when that is longer), and nothing has moved it on, it is Failed. VOX lead, tail, and the modem drain are unchanged.

A toast timer does not mark Sent. The compose hint and the send confirm both show the estimated airtime for this message, counted from the WCRM frames the send will key (header included, same split as the body), so one short mail says `1 burst` and a long one matches the chunk count. `POST /api/v1/mail/compose` returns that same hint as `airtime`. A long message says it is not Sent until the gateway accepts it.

## Log

The station log records `mail keyed`, `mail gateway ack`, `mail at gateway`, `mail accepted`, `mail failed`, and `mail retry`. The gateway records `mail received`, `mail have` (indexes it already holds), `mail posted … resend`, `mail rejected`, and `already accepted; repeating the ack` when the same mail arrives again. Check mail with nothing waiting logs `mail check: no new mail` and the Email tab says the same.

## On the air

Mail frames are `WCRM` text, not chat envelopes. A message wider than the rung is split into chunks. Every try uses that same chunk count, including a resend of the missing indexes. A slower rung may slice one of those chunks as `WCRP`; it does not split the message again. The gateway keeps one set per mail id. The header total is not a separate bucket, and an index past the smallest total already stored is ignored. It waits out a quiet gap after the last chunk, longer than one frame. While the burst's last index has not arrived, it also waits for the time the remaining chunks would take at the spacing heard (up to 3 minutes), so a sender held up mid-burst is not keyed over. A complete set is then one `CompleteAck`. A set that is still short is one `ChunkAck` whose body is `b:<total>:<hex>`, a bitmap of the indexes already stored. A progress ack that arrives while the burst is still keying is stored and does not start another send. The next try, after the burst has finished, sends only the indexes still missing. A second complete copy gets the `CompleteAck` again and is not posted twice. A list request is split the same way, and an empty inbox is answered with `[]`.

The rung starts at the preset's `ladder_start` and only steps more robust. On an HF or VOX preset that means it never moves onto QPSK or OFDM. The rung is chosen from the frames that will actually be keyed.

The mail worker pauses the chat queue, waits until the modem's `tx_frame_count` is stable and its transmitter is idle, sends the burst with modem CSMA off, waits until those frames have left, then restores the chat modem config (including `csma_enabled`) and resumes chat. Upstream modem73 does not count transmitted frames, so "left" also means the modem has stopped keying and its `tx_queue` is empty. The wait is three times the burst's airtime plus 15 s, the same patience chat uses, so a 48-chunk mail keeps the radio for its whole burst. A chat ACK does not key during that time: it waits for the burst to finish, and a burst waits for an ACK that is already keying. An ACK switches the modem config for itself and then restores chat's, which mid-burst turned CSMA back on under the rest of the mail. A resend is planned when the lease starts, from the newest have-report, so it keys only what the gateway still lacks. A have-report is stored as soon as it is decoded, even while this station is keying. The gateway holds its reply while a frame is still being received, a few times at most.

One VOX lead covers the whole burst.

## On the map

The pin keeps its mode colour. An e beside that logo means mail moved in the last few minutes. If it went through a gateway, an orange arc links the two callsigns. The ticker shows `MAIL` and callsigns only.

## Check mail

Internet and internet-radio pull waiting mail from the hub as themselves. Radio-plus asks the gateway over RF (list, then get). The gateway pulls that station's inbox, and only when the station signed the request. Nothing arrives on RF unless this station asked. If the list comes back empty, the tab says `no new mail`. If the gateway does not answer, it says `No reply from the gateway.`

## Copy-to

Off by default. A personal address is confirmed with a code sent to that inbox. The code goes to the station store and to the hub, so a confirm needs the hub up; the hub is the side that copies mail that arrives. Outbound mail can BCC it. Inbound copies use `X-WCR-Copy`. Copy-to is never put on RF.

The hub resolves copy-to from the callsign in `From`, so a relaying gateway cannot attach its own address to third-party mail or ask for one. A station's own `bcc` is honoured only when it signs for the author.

## Hub

Signed routes:

- `POST /api/v1/mail/send`
- `POST /api/v1/mail/inbox`
- `POST /api/v1/mail/fetch`
- `POST /api/v1/mail/copy`
- `POST /api/v1/mail/copy/confirm`
- `POST /api/v1/mail/webhook/resend` for Resend `email.received`

Resend signs that post with Svix (`svix-id`, `svix-timestamp`, `svix-signature`). `RESEND_WEBHOOK_SECRET` is the webhook signing secret from Resend (`whsec_…`). The hub then fetches the message body from Resend. `RESEND_API_KEY` and `RESEND_WEBHOOK_SECRET` live in the hub environment (`/opt/wcr/.env` on the droplet). They are not in the app and not in git.

## What a green test is

Unit tests check that chat's send, pacing, rung config, and fragment code still match v0.1.73, and that mail frames fit 512, 170, and 55 byte rungs. They do not show that a message crossed the air. That takes two stations.
