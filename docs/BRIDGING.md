On the web: https://weechatradio.com/docs/bridging.html

WeeChat Radio — cross-frequency chat
A station talks on one dial at a time. Same-frequency chat is radio. Different-frequency chat uses a gateway and the hub.

Same frequency (no hub)

  you [2m]  --RF-->  them [2m]

Dial the same calling spot. Nearby stations hear you on the air. Other stations on that frequency can store-and-forward (TTL / hops_left, default 3).

Different frequencies (hub bridge)

  you [2m] --RF--> gateway A [2m] --hub--> gateway B [40m] --RF--> them [40m]

Each gateway is in internet-radio mode, on its own band. The hub copies the message to the gateway that has recently heard the destination (or is the destination). The map ticker shows this as `2m -> 40m`.

Radio-plus (you stay offline)

  you [radio-plus, 2m] --RF--> gateway [internet-radio] --hub--> internet station

You never upload. A gateway that hears you may forward if the message allows internet (INET_OK) and its `[gateway] rf_egress` is on.

What the program never does
- It never changes your radio frequency by itself. `/radio qsy` (or the FREQ list) is always your action.
- A radio-only station is never put on the hub.
- Internet traffic is not put on the air unless this node is a gateway, `rf_egress = true`, and the dest was recently heard on this frequency (or it is a group with members heard here).

Beacons carry `B|<mode>|<khz>` so peers and the map know your dial. Tell the app the dial if you have no CAT: `/radio freq 144.950`.

LAN: nodes on the same local network can also pass frames without the public hub. Gateway rules still apply.

How you know it worked
- Nicklist: `[2m] G4ABC`, `[40m] M7TJF`, `[inet] ~ALICE`
- Map BANDS panel and ticker: `G4ABC 2m TX -> #net -> 40m, inet`
- Status bar shows your own `144.950 2m`

Related: wcr help modes · wcr help calling
Guide: https://weechatradio.com/docs/bridging.html
