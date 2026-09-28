On the web: https://weechatradio.com/docs/hops.html

WeeChat Radio — hops
A group on one frequency can all take part, even when some stations cannot hear some of the others. A station that hears a message keeps a copy and sends it again. That extra send is a hop (a relay).

The maps on the web page are radio only. The hub is not in the path. The program does not change the dial. Two different bands use a gateway and the hub: `wcr help bridging`.

What the program does

A normal message goes out with 3 hops left. A priority message (a leading `!`) starts with 4. An emergency message (`!!`) starts with 5.

The first time a station hears the message, it shows the line locally and subtracts one from hops left. If any hops remain, and this station did not write the message, it waits a short random moment and sends the message again.

Hearing the same message a second time does not send it again. If this station was waiting to relay and then hears someone else relay it, it cancels its own send. When hops left hits 0, the station that has the message can still read it, and it does not transmit it.

A normal message can cross three radio legs. The program does not measure kilometres. It relays a frame it decoded, on the same frequency, while hops remain.

20m, across Europe

All four stations are on 14.070 MHz USB. On a rough path the preset is `hf-poor`.

On 20m a station often hears others hundreds of kilometres away, and sometimes well over a thousand. It does not hear every other station in Europe at the same time, and the distance changes through the day. This example uses a reach of about 1,300 km.

```
  Lisbon --hops 3--> Barcelona --hops 2--> Munich --hops 1--> Bucharest
  ~1,000 km           ~1,050 km            ~1,200 km
```

Lisbon does not hear Munich (~2,000 km) or Bucharest (~3,000 km). Barcelona does not hear Bucharest (~2,000 km).

1. Lisbon transmits with 3 hops left.
2. Barcelona hears it and sends it on with 2 hops left.
3. Munich hears Barcelona and sends it on with 1 hop left.
4. Bucharest hears Munich. Hops left is now 0, so Bucharest reads the line and does not send it on.

Barcelona and Munich have the line as well. The four of them are one group. Lisbon never heard Bucharest directly.

2m, around Kyiv

Same rules. 2m is mostly line of sight (the signal travels until a building, a hill, or the horizon blocks it). This example uses a reach of about 25 km: the next town, not the far side of the city.

All four stations are on 144.950 MHz FM.

```
  Bucha --hops 3--> Kyiv --hops 2--> Brovary --hops 1--> Boryspil
  ~25 km           ~20 km           ~21 km
```

Kyiv does not hear Boryspil (~32 km). Bucha does not hear Brovary (~41 km) or Boryspil (~57 km).

Vyshhorod, Boyarka, and Vasylkiv are nearby towns. They are not part of this relay. The river through the city is the Dnipro.

1. Bucha transmits with 3 hops left.
2. Kyiv hears it and sends it on with 2 hops left.
3. Brovary hears Kyiv and sends it on with 1 hop left.
4. Boryspil hears Brovary. Hops left is now 0, so Boryspil reads the line and does not send it on.

Kyiv and Brovary have the line too. Bucha and Boryspil never hear each other. The group still shares the message on one dial.

Same 20m map, with the hub

Lisbon switches to `internet-radio`. Bucharest switches to `internet`. Barcelona and Munich stay on `radio`. The distances are the ones above. Bucharest's radio is off, so Bucharest has no reach.

Internet-radio still transmits on the dial. It also uses the hub when the station it is calling has not been heard on that dial. Internet has no radio. It reads and sends only through the hub, and it does not take part in an air hop.

Lisbon calls Bucharest. This is one station calling another. Lisbon has not heard Bucharest on 14.070, so the call goes to the hub as well as onto the air.

```
  air:  Lisbon --hops 3--> Barcelona --hops 2--> Munich --hops 1--> (Bucharest radio off)
  hub:  Lisbon -----------------------------------------------> Bucharest
```

1. Lisbon transmits on 14.070 with 3 hops left, and sends the same line to the hub.
2. Barcelona hears Lisbon on the air and sends it on with 2 hops left. Barcelona does not talk to the hub.
3. Munich hears Barcelona and sends it on with 1 hop left. That would reach Bucharest, about 1,200 km. Bucharest's radio is off, so it is not heard.
4. Bucharest reads the line from the hub and does not transmit.

Barcelona and Munich have the copy from the air. Bucharest has the copy from the hub. Lisbon and Bucharest still do not hear each other on 20m.

In `radio` mode the line is marked to stay off the internet. The air hops would run the same way and stop in the same place, and the hub would not have a copy for Bucharest.

What a hop is not

A hop stays on the frequency you are already using. It is not a voice repeater you have to set up, and it does not move a message from 2m to 20m. On one dial, an internet-radio station can also hand the same line to the hub when the other station has not been heard on the air.

Suggested dials: `wcr help calling`.
