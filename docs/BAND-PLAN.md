# Band plan

Where WeeChat Radio sits on each band. Generated from `crates/wcr/src/band.rs` by `scripts/gen_band_plan.py`; the same spots as [calling frequencies](CALLING.md) and `wcr help calling`.

You are the control operator. The edges are the widest allocation the app knows; your country may allow less. Verify frequency, mode and power for your licence before you transmit.

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
