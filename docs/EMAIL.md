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
- **Radio-plus, every PTT:** Keying the burst is Outbox `[tx]`. When the gateway acks every chunk, the outbox shows `[gw]` (the gateway has the bytes). Sent is only the gateway's `CompleteAck`, after Resend accepted it. Until then the line stays in the outbox.
- Missing acks are retried after the keyed burst should have finished, then with backoff (8s, then 16s, 32s, …) up to `[rf] max_retries` (at least 3). The clock starts at the end of that airtime, not at key-up. A retry sends only the chunks the gateway has not reported. When a retry has heard nothing back and more than four chunks are still missing, it keys only the first chunk, the one that names this station, and the gateway answers with what it holds. The log labels each key `try 1/4` through `try 4/4` when `max_retries` is 3: the first send plus three retries. Then the row is Failed, with the reason: the gateway never acknowledged, or it has the message but it was not accepted.
- If the radio is not running, the row is Failed at once with `Radio is not running.`
- If the row is still `[tx]` five minutes after this attempt started (or three times the planned airtime plus a minute, when that is longer), and nothing has moved it on, it is Failed. VOX lead, tail, and the modem drain are unchanged.

## Send window

Pressing Send in the confirm box opens a send window with a progress bar. The rest of the app is out of reach until the send is done and OK is pressed. The bar follows the real stages, never a timer, except for the keying itself:

| Stage | Bar | Line |
| --- | --- | --- |
| Hub path, handing over | 30% | `Handing it to the hub…` |
| Radio+, waiting for the radio | 2% | `Waiting for the radio…` |
| Radio+, keying | 5–50%, by planned airtime | `On the air, about N s left…` |
| Radio+, gateway acks chunks | 50–80%, by chunks held | `The gateway has H of T chunks…` |
| Radio+, every chunk at the gateway | 80% | `The gateway has every chunk…` |
| Sent (Resend accepted it) | 90–99% | `Sent. Resend accepted it; checking delivery…` |
| Delivered | 100% | `Success: delivered.` |

After Sent the window waits up to 30 s for Resend's delivery report. Without one it closes as `Success: sent.` and the Sent folder shows the delivery later. A bounce says `Sent, but bounced.` A failure shows the reason, the same text as the Outbox, and keeps To, subject and body. If the station stops answering for 30 s the window says so and can be closed.

## Delivery

Sent means Resend accepted the mail. Delivery is Resend's own report after that. The station asks the hub (`POST /api/v1/mail/status` with its mail id), and the hub asks Resend (`GET /emails/{id}`, its `last_event`), so the Resend key stays on the hub. The hub keeps the answer and asks Resend at most every 10 s per mail; once the answer is final it stops asking.

The station checks each Sent mail every 10 s for its first two minutes, every minute up to 30 minutes, every 15 minutes up to a day, then stops. It also stops once the answer is final. The Sent list and an opened sent mail show it as a chip (see [Status in the Email tab](#status-in-the-email-tab)). Radio+ asks the hub over this computer's internet, as copy-to does. Without internet the mail says `delivery unknown (offline)`. The station log records `mail delivery <id>: <event>` when it changes.

Every station and gateway sends its mail id with `POST /api/v1/mail/send` (`mail_id`). A gateway sends the id it computed from the RF mail, which is the Radio+ station's own id. The hub lets the station that posted the mail, or the station that wrote it, read its status.

A toast timer does not mark Sent. The compose hint and the send confirm both show the estimated airtime for this message, counted from the WCRM frames the send will key (header included, same split as the body), so one short mail says `1 burst` and a long one matches the chunk count. `POST /api/v1/mail/compose` returns that same hint as `airtime`. A long message says it is not Sent until the gateway accepts it.

## Status in the Email tab

Every mail in the list has one coloured chip, and **status key** under the folder tabs explains them:

| Chip | Mark | Meaning |
| --- | --- | --- |
| Queued | `[..]` | Waiting for the radio or the hub |
| On air | `[tx]` | Keyed to the gateway (Radio+) |
| At gateway | `[gw]` | The gateway has every chunk and is handing it to the hub |
| Sent | `[ok]` | Resend accepted it. The word after the subject says `with Resend`, `no delivery report` (no final answer a day after Sent), or `delivery unknown (offline)` (the last delivery check could not reach the hub) |
| Delivered | `[ok]` | Resend says the recipient's mail server took it |
| Delayed | `[ok]` | Resend is still trying |
| Bounced | `[ok]` | The recipient's mail server refused it |
| Spam | `[ok]` | The recipient marked it as spam |
| Failed | `[no]` | Not sent (the reason is in the opened mail), or Resend could not send it |
| No record | `[ok]` | The hub has no delivery record for this mail |

Inbox mail shows Received, with an orange dot and brighter text while unread. Radio+ headers not fetched yet show Waiting and the size, in the same layout. Screen readers hear the mark with the chip, for example `[ok] Delivered`.

Times are local, with the zone in the status key and in the opened mail. The opened mail shows the chip, the full reason when something went wrong, and when it reached each hop: created, on air, at gateway, accepted by Resend, then delivered, bounced or another final answer. Inbound mail shows when the sender sent it and when this station fetched it. Mail from before this version shows only the times it reaches from now on.

Failed reasons say where the mail stopped: the gateway never acknowledged it, the gateway has it but the hub refused it (with the hub's reason), the hub refused it, or the hub could not be reached. A hub-path mail that the hub refuses, or that finds the hub down, is Failed at once rather than left Queued.

- **Retry** (Failed mail): the mail goes back to the outbox and is sent again from the start, on the same path and RF pacing as a new send, and the send window follows it.
- **Re-send** (Bounced, or Resend failed or canceled it): compose opens with the same To, subject and body. Fix the address, then Send. A Radio+ re-send with nothing changed has the same mail id, and the gateway treats it as the mail it already posted.

**No read status.** Mail goes out as plain text with no tracking, so nobody can tell when it is opened, and the Email tab never says "read". Resend's `opened` and `clicked` events count as Delivered.

The local mail API (`/mail/list` and `/mail/open`) returns each row with `status` (`key`, `label`, `mark`, `word`, `reason`, `line` such as `[ok] delivered`, `retry`, `resend`) and `hops` (`stage`, `ts`), so the WeeChat script and other clients can show the same thing. `POST /mail/retry` with `{"id": …}` retries a Failed mail.

## Log

The station log records `mail keyed`, `mail gateway ack`, `mail at gateway`, `mail accepted`, `mail failed`, `mail retry`, and `mail retry by hand` when Retry is pressed. The gateway records `mail received`, `mail have` (indexes it already holds), `mail posted … resend`, `mail rejected`, and `already accepted; repeating the ack` when the same mail arrives again. Check mail with nothing waiting logs `mail check: no new mail` and the Email tab says the same.

## On the air

Mail frames are `WCRM` text, not chat envelopes. A message wider than the rung is split into chunks. Every try uses that same chunk count, including a resend of the missing indexes. A slower rung may slice one of those chunks as `WCRP`; it does not split the message again. The gateway keeps one set per mail id. The header total is not a separate bucket, and an index past the smallest total already stored is ignored. It waits out a quiet gap after the last chunk, longer than one frame. While the burst's last index has not arrived, it also waits for the time the remaining chunks would take at the spacing heard (up to 3 minutes), so a sender held up mid-burst is not keyed over. A complete set is then one `CompleteAck`. A set that is still short is one `ChunkAck` whose body is `b:<total>:<hex>`, a bitmap of the indexes already stored. A progress ack that arrives while the burst is still keying is stored and does not start another send. The next try, after the burst has finished, sends only the indexes still missing. A second complete copy gets the `CompleteAck` again and is not posted twice. A list request is split the same way, and an empty inbox is answered with `[]`.

The rung starts at the preset's `ladder_start` and only steps more robust. On an HF or VOX preset that means it never moves onto QPSK or OFDM. The rung is chosen from the frames that will actually be keyed.

The mail worker pauses the chat queue, waits until the modem's `tx_frame_count` is stable and its transmitter is idle, sends the burst with modem CSMA off, waits until those frames have left, then restores the chat modem config (including `csma_enabled`) and resumes chat. Upstream modem73 does not count transmitted frames, so "left" also means the modem has stopped keying and its `tx_queue` is empty. The wait is three times the burst's airtime plus 15 s, the same patience chat uses, so a 48-chunk mail keeps the radio for its whole burst. A chat ACK does not key during that time: it waits for the burst to finish, and a burst waits for an ACK that is already keying. A chat frame already taken off the queue, still waiting out carrier sense, goes to the modem before the burst: it sets chat's modem config just before it goes, which mid-burst turned CSMA back on under the rest of the mail. An ACK switches the modem config for itself and then restores chat's, which mid-burst turned CSMA back on under the rest of the mail. A resend is planned when the lease starts, from the newest have-report, so it keys only what the gateway still lacks. A have-report is stored as soon as it is decoded, even while this station is keying. The gateway holds its reply while a frame is still being received, a few times at most.

One VOX lead covers the whole burst.

## On the map

The pin keeps its mode colour. An e beside that logo means mail moved in the last few minutes. If it went through a gateway, an orange arc links the two callsigns. The ticker shows `MAIL` and callsigns only.

## Check mail

Internet and internet-radio pull waiting mail from the hub as themselves. Radio-plus asks the gateway over RF (list, then get). The list gives headers only. They show in the Inbox as `waiting` lines until Get brings the mail in, which removes the line. Delete on a waiting line hides it on this station only; the hub keeps the mail until it is fetched, so the next Check mail lists it again. The gateway pulls that station's inbox, and only when the station signed the request. Nothing arrives on RF unless this station asked. If the list comes back empty, the tab says `no new mail`. If the gateway does not answer, it says `No reply from the gateway.`

## Copy-to

Off by default. Type a personal address in the Copy-to box and press save. The hub emails a six-digit code to that inbox; type it in the code box and press confirm. The hub is the side that copies mail, so save, confirm, and off all need the hub to answer, and the station changes nothing until it has. Outbound mail can BCC it. Inbound copies use `X-WCR-Copy`. Copy-to is never put on RF.

- **Confirm with the hub down:** the code matches here but is kept, and the state is `hub not confirmed`. Press confirm again with the same code when the hub is up.
- **Radio+:** save and confirm are one signed HTTPS call to the hub over this computer's internet, nothing on the air. Without internet the station says so. The hub then copies the station's mail by its callsign, including mail a gateway posts for it.
- **Off:** save an empty address. The hub and the station both drop it.
- The code may be typed with spaces or a trailing full stop.
- The Email tab shows the state when it opens: `off`, `code sent`, `hub not confirmed`, or `on`. `GET /mail/copy` returns the same as `state`.

The hub resolves copy-to from the callsign in `From`, so a relaying gateway cannot attach its own address to third-party mail or ask for one. A station's own `bcc` is honoured only when it signs for the author.

## Hub

Signed routes:

- `POST /api/v1/mail/send`
- `POST /api/v1/mail/status`
- `POST /api/v1/mail/inbox`
- `POST /api/v1/mail/fetch`
- `POST /api/v1/mail/copy`
- `POST /api/v1/mail/copy/confirm`
- `POST /api/v1/mail/webhook/resend` for Resend `email.received`

Resend signs that post with Svix (`svix-id`, `svix-timestamp`, `svix-signature`). `RESEND_WEBHOOK_SECRET` is the webhook signing secret from Resend (`whsec_…`). The hub then fetches the message body from Resend. `RESEND_API_KEY` and `RESEND_WEBHOOK_SECRET` live in the hub environment (`/opt/wcr/.env` on the droplet). They are not in the app and not in git.

## What a green test is

Unit tests check that chat's send, pacing, rung config, and fragment code still match v0.1.73, and that mail frames fit 512, 170, and 55 byte rungs. They do not show that a message crossed the air. That takes two stations.
