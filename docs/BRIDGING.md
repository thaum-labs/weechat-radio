On the web: https://weechatradio.com/docs/bridging.html

WeeChat Radio — cross-frequency chat
A station talks on one dial at a time. Same-frequency chat is radio. Different-frequency chat uses a gateway and the hub.

Same frequency (no hub)

      .--------------- direct RF ---------------.
      |                                         v
   [ you ]   --RF-->   [ relay ]   --RF-->   [ them ]
   144.950             same dial             144.950
   2m radio            hops 3 -> 2           2m radio

Dial the same calling spot. Nearby stations hear you on the air. Any other station on that frequency can store-and-forward for you (TTL / hops_left, default 3), so you do not have to hear each other directly.

Different dials hear nothing

   [ you: 144.950 2m ]   - - -  X  - - -   [ them: 7.045 40m ]

There is no radio path between two frequencies, however strong the signals are. Someone has to bridge the two bands.

Different frequencies (hub bridge)

   (1) [ you ]          2m    144.950    radio
          |  RF
          v
   (2) [ gateway A ]    2m    internet-radio
          |  internet
          v
   (3) [ hub ]          routes by band: picks the gateway that
          |  internet   has recently heard them (or is them)
          v
   (4) [ gateway B ]    40m   internet-radio
          |  RF
          v
   (5) [ them ]         40m   7.045      radio

Each gateway is in internet-radio mode and stays on its own dial. The map ticker shows this as `2m -> 40m`.

Radio-plus (you stay offline)

   .....................................
   :   [ you ]  radio-plus, 2m         :   no internet here
   :......|............................:
          |  RF
          v
     [ gateway ]  --internet-->  [ hub ]
     hears you on 2m                |
                        .-----------+-----------.
                        v                       v
              [ internet station ]      [ another band ]

   never happens:   [ you ]  --X-->  [ hub ]

`radio` and `radio-plus` both post map telemetry when `[telemetry] url` is set, and that post never contains message text. Neither mode sends chat to the hub or the LAN from this computer. The only difference is whether an internet-radio gateway that hears you may forward your messages (and email) to the internet.

You never upload chat from this computer. A gateway that hears a radio-plus frame forwards it to the hub for any destination: a direct message, a closed group, and `#bulletin`. The frame says it may go to the internet. `radio` mode says do not forward, so a gateway keeps that frame on the radio.

`[gateway] rf_egress` is the other direction. It puts hub traffic on the air. It does not decide the upload.

The sender's line shows `[gw]` when that gateway has accepted the frame onto the hub. `[rl]` is only “I heard my own frame on the radio again”. In `radio` mode `[rl]` is as far as the frame goes: a gateway will not upload it. Both marks can appear, and neither one is an ACK.

What a gateway will and will not put on the air

   hub gives this node a frame
        |
        v
   gateway with rf_egress = true?  -- no -->  stay quiet
        |  yes
        v
   dest heard on this dial in the
   last 10 minutes?                -- no -->  stay quiet
        |  yes
        v
   TX on this dial

A group counts as heard if any member was heard on this dial. Guest names starting with `~` also need `third_party = "allow"`.

Heard means **on RF on this frequency**, not a map pin or a LAN/hub sighting. A group flag by itself is not enough; at least one listed member must have been heard here in the last 10 minutes. `#bulletin` keys only if *someone* was heard on this dial over RF, so a silent gateway stays quiet.

What the program never does
- It never changes your radio frequency by itself. `/radio qsy` (or the FREQ list) is always your action.
- `radio` and `radio-plus` never put chat frames on the hub from this computer. Map data (callsign, grid, tx/rx ids — never message text) still POSTs if `[telemetry] url` is set. A gateway may forward `radio-plus` only.
- Internet traffic is not put on the air unless this node is a gateway, `rf_egress = true`, and the dest was recently heard on this frequency (or it is a group with members heard here).

Beacons carry `B|<mode>|<khz>` so peers and the map know your dial. Tell the app the dial if you have no CAT: `/radio freq 144.950`.

LAN: nodes on the same local network can also pass frames without the public hub. Gateway rules still apply.

Offline direct messages

The hub keeps a direct message for a station that is not connected, for 10 minutes, up to 32 messages per callsign. The oldest is dropped when that fills. It is delivered when that station next says hello. Closed groups and `#bulletin` are not held: someone has to be connected to receive them. The hub does not key a radio. A station that is only on the air still depends on the RF retry while both ends can hear the path.

How you know it worked
- Nicklist: `[2m] G4ABC`, `[40m] M7TJF`, `[inet] ~ALICE`
- Map BANDS panel and ticker: `G4ABC 2m TX -> #net -> 40m, inet`
- Status bar shows your own `144.950 2m`

Related: wcr help modes · wcr help calling
Guide: https://weechatradio.com/docs/bridging.html
