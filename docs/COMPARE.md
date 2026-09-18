On the web (sortable table): https://weechatradio.com/docs/compare.html

# Compare with other ham text modes

Noise resistance is almost entirely the modem waveform, not whether the payload is “text.” WeeChat Radio sends a short binary frame with CRC-16. Weak-signal performance comes from [modem73](https://github.com/RFnexus/modem73) (OFDM / RDM / MFSK), then ACKs, retries that step to a slower mode, and Reed–Solomon pieces on HF group traffic.

Scores are **1 (poor) to 5 (strong)** for that column only. They are operator ratings, not a lab bake-off. SNR figures for WSPR, FT8, JS8, Olivia, PSK31, RTTY, and APRS are typical published ham values. modem73 has no side-by-side SNR table in this project.

The two WeeChat Radio HF chat presets are **`hf-weak` / `hf-poor`** (normal noisy HF) and **`hf-deep`** (last resort). `vhf-fm` / handheld KISS is a different path: 1200 baud packet, like APRS.

## Ratings

| Mode | Job | Weak SNR | Fading | QRM | Speed | Auto recover | Open | Quiet VHF FM |
|------|-----|----------|--------|-----|-------|--------------|------|--------------|
| **WeeChat Radio hf-weak / hf-poor** | Normal HF chat | 3 | 4 | 3 | 3 | 5 | 4 | 1 |
| **WeeChat Radio hf-deep** | Last-resort HF | 4 | 4 | 4 | 1 | 5 | 4 | 1 |
| WeeChat Radio vhf-fm / handheld | Local VHF/UHF FM | 1 | 2 | 2 | 4 | 4 | 4 | 5 |
| WSPR | Beacon / prop map — not chat | 5 | 4 | 5 | 1 | 1 | 5 | 1 |
| FT8 | 77-bit QSO — not free text | 5 | 4 | 4 | 1 | 2 | 5 | 1 |
| FT4 | Faster FT8 QSO — not free text | 4 | 3 | 4 | 2 | 2 | 5 | 1 |
| JS8Call | Slow HF chat (FT8-family) | 5 | 4 | 4 | 1 | 3 | 5 | 1 |
| Olivia 8/250 | HF typed chat | 4 | 4 | 4 | 1 | 2 | 5 | 1 |
| VARA HF | Winlink / files | 4 | 4 | 3 | 4 | 5 | 1 | 1 |
| PSK31 | Live keyboard HF | 3 | 1 | 1 | 3 | 1 | 5 | 1 |
| RTTY 45 | Contest / bulletin | 2 | 1 | 1 | 3 | 1 | 5 | 1 |
| APRS / AX.25 1200 AFSK | VHF short text | 1 | 1 | 1 | 4 | 2 | 5 | 5 |

## What the columns mean

| Trait | 5 | 1 |
|-------|---|---|
| Weak SNR | WSPR (~−31 dB) then FT8/JS8 (~−21 dB) | AFSK 1200 / FM quieting required |
| Fading | Long coded symbols, interleave, or retries across a fade | One dip prints garbage (PSK31, RTTY, APRS CRC) |
| QRM | Narrow or heavily coded | No FEC; one hit corrupts the line |
| Speed | 1200 baud packet or adaptive OFDM (VARA) | WSPR 2 min beacon; FT8/FT4 tiny payloads; JS8 / hf-deep chat |
| Auto recover | ACK + retries + mode step-down or ARQ | Beacon only (WSPR) or print-and-pray (PSK31, RTTY) |
| Open | Documented amateur mode | Closed (VARA). wcr envelope is open; modem73 is a separate modem |
| Quiet VHF FM | Handheld KISS / APRS on a clean FM channel | HF-only waveforms |

## How to read WeeChat Radio

- **`hf-weak` / `hf-poor`:** mid SNR (3), strong fading recovery and retries (5), usable speed (3). This is the usual noisy-HF chat preset (`hf-poor` calling, `hf-weak` when they are faint).
- **`hf-deep`:** climbs toward Olivia on SNR (4), below FT8/WSPR (5). Speed is 1. A short 1:1 line is about **2–5 seconds** of PTT. The same line to `#bulletin` is about **10–15 seconds** because HF groups send three Reed–Solomon pieces.
- **`vhf-fm` / handheld:** same air as APRS (low SNR/fade/QRM). Auto recover is 4 because of ACKs and retries. Use this on a quiet FM channel, not as an HF weak-signal mode.

WSPR, FT8, and FT4 are weak-signal benchmarks. They do not carry a typed chat line.

Operator steps when the path is faint: `wcr help weak` or https://weechatradio.com/docs/weak.html
