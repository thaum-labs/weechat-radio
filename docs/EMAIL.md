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

- **Hub:** Sent after the hub says Resend accepted the message.
- **VOX:** Sent after the gateway's `CompleteAck`. Finishing the local transmit is still Outbox.
- **Other PTT** (digirig, CM108, rigctl, TNC): Sent when this station's mail burst has finished.

A toast timer does not mark Sent.

## On the air

Mail frames are `WCRM` text, not chat envelopes. If a frame is wider than the rung, it is sliced as `WCRP` and rebuilt before decode.

The rung starts at the preset's `ladder_start` and only steps more robust. On an HF or VOX preset that means it never moves onto QPSK or OFDM. The rung is chosen from the frames that will actually be keyed.

The mail worker pauses the chat queue, waits until the modem's `tx_frame_count` is stable, sends the burst with modem CSMA off, waits until those frames have left, then restores the chat modem config (including `csma_enabled`) and resumes chat.

One VOX lead covers the whole burst.

## Check mail

Internet and internet-radio pull waiting mail from the hub. Radio-plus asks the gateway over RF (list, then get). Nothing arrives on RF unless this station asked.

## Copy-to

Off by default. A personal address is confirmed with a code sent to that inbox. Outbound mail can BCC it. Inbound copies use `X-WCR-Copy`. Copy-to is never put on RF.

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
