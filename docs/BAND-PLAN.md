# Band plan

Where WeeChat Radio sits on each band. Generated from `crates/wcr/src/band.rs` by `scripts/gen_band_plan.py`; the same spots as [calling frequencies](CALLING.md) and `wcr help calling`.

You are the control operator. The edges are the widest allocation the app knows; your country may allow less. Verify frequency, mode and power for your licence before you transmit.

## How the spots were picked

- Inside the data or digital part of each band plan, so long data bursts stay out of the voice and CW segments.
- Next to the busy automated spots (FT8, JS8, APRS) where the plan allows, not on them. 60m and 17m are too small to avoid FT8.
- One spot per band and region, so stations find each other without agreeing a frequency first.
- On licence-free bands and CB, channel 1 or the national calling channel: every set can tune it, and people already listen there.
- Mode follows the radio: USB data on HF and SSB CB, FM on VHF, UHF and FM CB sets, AM where typical CB sets are AM only.

## HF

1.8 to 30 MHz. Long-distance bands, plus CB just below 10m.

| Band | Edges (MHz) | Spot | Mode | Where | Preset |
|---|---|---|---|---|---|
| 160m | 1.800–2.000 | 1.838 | USB | WW | `hf-poor` |
| 80m | 3.500–4.000 | 3.580 | USB | WW | `hf-poor` |
| 60m | 5.250–5.450 | 5.357 | USB | WW | `hf-poor` |
| 40m | 7.000–7.300 | 7.045 | USB | UK/EU | `hf-poor` |
|  |  | 7.090 | USB | US/AU | `hf-poor` |
| 30m | 10.100–10.150 | 10.140 | USB | WW | `hf-poor` |
| 20m | 14.000–14.350 | 14.070 | USB | WW | `hf-poor` |
| 17m | 18.068–18.168 | 18.100 | USB | WW | `hf-poor` |
| 15m | 21.000–21.450 | 21.070 | USB | WW | `hf-poor` |
| 12m | 24.890–24.990 | 24.920 | USB | WW | `hf-poor` |
| CB | 26.330–27.992 | 26.565 (ch 41) | FM | DE | `vhf-fm` |
|  |  | 26.720 (26ch 35) | LSB | NZ | `hf-poor` |
|  |  | 26.965 (ch 1) | FM | DE | `vhf-fm` |
|  |  | 27.085 (ch 11) | AM | AU | `vox-safe` |
|  |  | 27.085 (ch 11) | AM | NZ | `vox-safe` |
|  |  | 27.135 (ch 15) | USB | EU | `hf-poor` |
|  |  | 27.135 (CEPT ch 15) | USB | UK | `hf-poor` |
|  |  | 27.155 (ch 16) | LSB | AU | `hf-poor` |
|  |  | 27.185 (ch 19) | AM | US/CA | `vox-safe` |
|  |  | 27.185 (ch 19) | FM | EU | `vhf-fm` |
|  |  | 27.185 (CEPT ch 19) | FM | UK | `vhf-fm` |
|  |  | 27.355 (ch 35) | LSB | AU | `hf-poor` |
|  |  | 27.355 (ch 35) | USB | AU | `hf-poor` |
|  |  | 27.355 (ch 35) | USB | NZ | `hf-poor` |
|  |  | 27.365 (ch 36) | USB | US/CA | `hf-poor` |
|  |  | 27.385 (ch 38) | LSB | US/CA | `hf-poor` |
|  |  | 27.781 (UK ch 19) | FM | UK | `vhf-fm` |
| 10m | 28.000–29.700 | 28.120 | USB | WW | `hf-poor` |
|  |  | 29.250 | FM | WW | `vhf-fm` |

Why here:

- **160m**: Inside the narrow data segment at the bottom of the band, just below FT8 on 1.840. Data is USB here, though voice on this band is LSB.
- **80m**: In the data segment (3.570–3.600 in Region 1), a few kHz above FT8 on 3.573. Data is USB here, though voice on this band is LSB.
- **60m**: 60m is a small shared allocation (5.3515–5.3665 worldwide) and 5.357 is where data gathers, FT8 included, so keep overs short. Channels and power limits differ a lot by country.
- **40m**: 7.045 is in Region 1’s data segment (7.040–7.050). The Americas and Australia keep data higher (7.080–7.125 in the ARRL plan), so 7.090 there. Both stay clear of FT8 on 7.074. Data is USB, though voice on this band is LSB.
- **30m**: 30m is for CW and data only. 10.140 is in the data segment (10.140–10.150), just above FT8 on 10.136.
- **20m**: 14.070 is the traditional start of the data segment, home of PSK31, just below FT8 on 14.074.
- **17m**: In the data segment. 18.100 is also the FT8 dial, so expect busy neighbours.
- **15m**: 21.070 is the PSK31 spot in the data segment, just below FT8 on 21.074.
- **12m**: 24.920 is the PSK31 spot in the data segment, just above FT8 on 24.915.
- **CB**: Licence-free. Calling channels differ by country, so each country has its own picks. The full bar covers every regional plan the app knows: NZ 26 MHz, the German extra channels below channel 1, and the UK 27/81 band above.
- **10m**: 28.120 is in the data area low in the band, clear of FT8 on 28.074. 29.250 is in the segment for data and unattended stations (29.200–29.300), for FM sets.
  - UK: Channel 19 is the calling channel on both the CEPT band and the older UK 27/81 band. SSB sets use channel 15, away from it.
  - EU: Channel 19 is the common FM calling and road channel. SSB sets use channel 15, away from it.
  - DE: Germany has 80 channels. Channel 1 is a common FM calling channel for German speakers; channel 41 starts the extra channels below it.
  - US/CA: Channel 19 is the road channel almost every AM set listens to. Sideband sets use 36 (USB) and 38 (LSB, the usual SSB calling channel).
  - AU: Channels 11 (AM) and 16 (SSB) are the official calling channels. Channel 35 is where SSB DX contacts gather.
  - NZ: Channel 11 AM and channel 35 SSB match Australia. NZ also has a 26 MHz band below the common channels, with its own channel 35.

## VHF

30 to 300 MHz. Mostly line of sight, the home of handhelds and FM.

| Band | Edges (MHz) | Spot | Mode | Where | Preset |
|---|---|---|---|---|---|
| 6m | 50.000–54.000 | 50.230 | USB | WW | `hf-poor` |
|  |  | 50.620 | FM | UK/EU | `vhf-fm` |
| 4m | 70.000–70.500 | 70.488 | FM | UK/EU | `vhf-fm` |
| 2m | 144.000–148.000 | 144.950 | FM | UK/EU | `vhf-fm` |
|  |  | 145.010 | FM | US | `vhf-fm` |
|  |  | 145.530 | FM | US | `vhf-fm` |
|  |  | 146.550 | FM | AU | `vhf-fm` |
|  |  | 144.800 | APRS: keep chat off | UK/EU |  |
|  |  | 144.390 | APRS: keep chat off | US |  |
| MURS | 151.800–151.950 | 151.820 | FM | US | `vhf-fm` |
| 1.25m | 222.000–225.000 | 223.400 | FM | US | `vhf-fm` |

Why here:

- **6m**: 50.230 is USB data low in the band, away from MSK144 on 50.260 and FT8 on 50.313. 50.620 is in the digital communications segment (50.500–50.700 in the UK plan), for FM sets.
- **4m**: 70.4875 is the UK 4m packet channel. 4m exists only in some countries, mostly in Europe.
- **2m**: 144.950 is in Region 1’s digital communications segment, clear of APRS on 144.800. 145.010 is in the US packet segment (144.900–145.100). 145.530 is in the US miscellaneous and experimental segment, away from the 146.520 calling channel. 146.550 is an FM simplex pick for Australia. Keep chat off the APRS channels.
- **MURS**: Channel 1 of 5, which every MURS radio has. Licence-free in the US.
- **1.25m**: In the FM simplex range (223.400–223.520), below the 223.500 calling channel. US only.

## UHF and up

300 MHz and above. Short range, small antennas, and licence-free PMR446 and FRS.

| Band | Edges (MHz) | Spot | Mode | Where | Preset |
|---|---|---|---|---|---|
| 70cm | 420.000–450.000 | 433.625 | FM | UK/EU | `vhf-fm` |
|  |  | 439.950 | FM | AU | `vhf-fm` |
|  |  | 441.000 | FM | US | `vhf-fm` |
| PMR446 | 446.000–446.200 | 446.006 | FM | UK/EU | `vhf-fm` |
| FRS | 462.550–462.725 | 462.562 | FM | US | `vhf-fm` |
| 23cm | 1240.000–1300.000 | 1296.800 | FM | WW | `vhf-fm` |

Why here:

- **70cm**: 433.625 is the first of Region 1’s digital communications channels (433.625–433.775). 439.950 and 441.000 are FM picks for Australia and the US; 70cm use varies a lot, so check your local plan.
- **PMR446**: Channel 1, which every PMR446 radio has. Licence-free in the UK and Europe.
- **FRS**: Channel 1, which every FRS radio has. Licence-free in the US and Canada.
- **23cm**: Check this one: in the IARU Region 1 plan, 1296.800–1296.994 is for beacons only. In Region 1, pick an FM simplex channel from your national plan instead.
