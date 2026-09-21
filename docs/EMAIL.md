# Email (wcr-gui v1)

Internet mail uses `callsign@mail.weechatradio.com` on the public hub (Resend). Ham-to-ham chat stays in Live Chat.

- **Gateway:** set **Email gateway** in Setup to an `internet-radio` callsign on your dial. Without it the Email tab stays hidden.
- **Pull only:** inbound mail waits at the hub until sync or RF **Check mail** (never auto-keyed onto RF).
- **Plain text, 4 KB** max; no attachments.
- **Send / Check mail** confirm modals list airtime when RF will key.
- **Copy-to** personal address is optional, off by default, confirmed by email code on the hub.
- **Live map:** Email is a traffic overlay (orange ring), not a fifth operating mode. Pins keep their real mode colour. Hub send/receive and RF mail emit `kind: mail` with **callsigns only** (no internet addresses). When a gateway carried the hop, the arc links operator ↔ gateway; the station card shows `email via G0ABC`.

See [email tab spec](https://weechatradio.com/docs/email.html) for product detail.
