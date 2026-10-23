#!/usr/bin/env python3
"""Build web/docs/band-plan.html and docs/BAND-PLAN.md from crates/wcr/src/band.rs.

The band edges come from band_for_khz() and the spots from CALLING_FREQS, so the page
always shows what the app suggests. Run after changing band.rs:

    python3 scripts/gen_band_plan.py
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = (ROOT / "crates/wcr/src/band.rs").read_text()

# --- read band.rs -------------------------------------------------------------
# Licence-free first: `if (26330..=27992).contains(&khz) || (...) { return Some("CB"); }`
FREE = {}
for ranges, name in re.findall(r"if ((?:\(\d+\.\.=\d+\)\.contains\(&khz\)(?: \|\| )?)+) \{\s*return Some\(\"([^\"]+)\"\)", SRC):
    FREE[name] = [(int(a), int(b)) for a, b in re.findall(r"\((\d+)\.\.=(\d+)\)", ranges)]
HAM = {name: [(int(a), int(b))] for a, b, name in re.findall(r"(\d+)\.\.=(\d+) => \"([^\"]+)\"", SRC)}

SPOTS = []
for fn, args in re.findall(r"\b(cf|cf_note)\((\"[^)]*)\)", SRC.split("const CALLING_FREQS")[1].split("];")[0]):
    parts = [p.strip() for p in re.findall(r"\"[^\"]*\"|[\d_]+", args)]
    region, khz, mode, preset = parts[0].strip('"'), int(parts[1].replace("_", "")), parts[2].strip('"'), parts[3].strip('"')
    note = parts[4].strip('"') if fn == "cf_note" else ""
    SPOTS.append(dict(region=region, khz=khz, mode=mode, preset=preset, note=note))
assert len(SPOTS) > 40 and "CB" in FREE and "2m" in HAM, "band.rs changed shape; update this script"

# The app also says: keep chat off APRS (calling_card_text in band.rs).
AVOID = [dict(region="UK/EU", khz=144_800, label="APRS"), dict(region="US", khz=144_390, label="APRS")]

# Region filter groups. WW is in every group.
GROUPS = {"eu": "UK & Europe", "na": "US & Canada", "anz": "Australia & NZ"}
REGION_GROUPS = {"WW": "eu na anz", "UK/EU": "eu", "UK": "eu", "EU": "eu", "DE": "eu",
                 "US": "na", "US/CA": "na", "US/AU": "na anz", "AU": "anz", "NZ": "anz"}
for s in SPOTS + AVOID:
    assert s["region"] in REGION_GROUPS, f"new region {s['region']}: add it to REGION_GROUPS"

SECTIONS = [
    ("hf", "HF", "1.8 to 30 MHz. Long-distance bands, plus CB just below 10m.",
     ["160m", "80m", "60m", "40m", "30m", "20m", "17m", "15m", "12m", "CB", "10m"]),
    ("vhf", "VHF", "30 to 300 MHz. Mostly line of sight, the home of handhelds and FM.",
     ["6m", "4m", "2m", "MURS", "1.25m"]),
    ("uhf", "UHF and up", "300 MHz and above. Short range, small antennas, and licence-free PMR446 and FRS.",
     ["70cm", "PMR446", "FRS", "23cm"]),
]
# Why each spot is where it is, from the published band plans (IARU Region 1, RSGB, ARRL)
# and the CB rules for each country. Keep these in step with CALLING_FREQS.
WHY = {
    "160m": "Inside the narrow data segment at the bottom of the band, just below FT8 on 1.840. Data is USB here, though voice on this band is LSB.",
    "80m": "In the data segment (3.570–3.600 in Region 1), a few kHz above FT8 on 3.573. Data is USB here, though voice on this band is LSB.",
    "60m": "60m is a small shared allocation (5.3515–5.3665 worldwide) and 5.357 is where data gathers, FT8 included, so keep overs short. Channels and power limits differ a lot by country.",
    "40m": "7.045 is in Region 1’s data segment (7.040–7.050). The Americas and Australia keep data higher (7.080–7.125 in the ARRL plan), so 7.090 there. Both stay clear of FT8 on 7.074. Data is USB, though voice on this band is LSB.",
    "30m": "30m is for CW and data only. 10.140 is in the data segment (10.140–10.150), just above FT8 on 10.136.",
    "20m": "14.070 is the traditional start of the data segment, home of PSK31, just below FT8 on 14.074.",
    "17m": "In the data segment. 18.100 is also the FT8 dial, so expect busy neighbours.",
    "15m": "21.070 is the PSK31 spot in the data segment, just below FT8 on 21.074.",
    "12m": "24.920 is the PSK31 spot in the data segment, just above FT8 on 24.915.",
    "CB": "Licence-free. Calling channels differ by country, so each country has its own picks. The full bar covers every regional plan the app knows: NZ 26 MHz, the German extra channels below channel 1, and the UK 27/81 band above.",
    "10m": "28.120 is in the data area low in the band, clear of FT8 on 28.074. 29.250 is in the segment for data and unattended stations (29.200–29.300), for FM sets.",
    "6m": "50.230 is USB data low in the band, away from MSK144 on 50.260 and FT8 on 50.313. 50.620 is in the digital communications segment (50.500–50.700 in the UK plan), for FM sets.",
    "4m": "70.4875 is the UK 4m packet channel. 4m exists only in some countries, mostly in Europe.",
    "2m": "144.950 is in Region 1’s digital communications segment, clear of APRS on 144.800. 145.010 is in the US packet segment (144.900–145.100). 145.530 is in the US miscellaneous and experimental segment, away from the 146.520 calling channel. 146.550 is an FM simplex pick for Australia. Keep chat off the APRS channels.",
    "MURS": "Channel 1 of 5, which every MURS radio has. Licence-free in the US.",
    "1.25m": "In the FM simplex range (223.400–223.520), below the 223.500 calling channel. US only.",
    "70cm": "433.625 is the first of Region 1’s digital communications channels (433.625–433.775). 439.950 and 441.000 are FM picks for Australia and the US; 70cm use varies a lot, so check your local plan.",
    "PMR446": "Channel 1, which every PMR446 radio has. Licence-free in the UK and Europe.",
    "FRS": "Channel 1, which every FRS radio has. Licence-free in the US and Canada.",
    "23cm": "Check this one: in the IARU Region 1 plan, 1296.800–1296.994 is for beacons only. In Region 1, pick an FM simplex channel from your national plan instead.",
}
WHY_REGION = {
    "UK": "Channel 19 is the calling channel on both the CEPT band and the older UK 27/81 band. SSB sets use channel 15, away from it.",
    "EU": "Channel 19 is the common FM calling and road channel. SSB sets use channel 15, away from it.",
    "DE": "Germany has 80 channels. Channel 1 is a common FM calling channel for German speakers; channel 41 starts the extra channels below it.",
    "US/CA": "Channel 19 is the road channel almost every AM set listens to. Sideband sets use 36 (USB) and 38 (LSB, the usual SSB calling channel).",
    "AU": "Channels 11 (AM) and 16 (SSB) are the official calling channels. Channel 35 is where SSB DX contacts gather.",
    "NZ": "Channel 11 AM and channel 35 SSB match Australia. NZ also has a 26 MHz band below the common channels, with its own channel 35.",
}
RULES = [
    "Inside the data or digital part of each band plan, so long data bursts stay out of the voice and CW segments.",
    "Next to the busy automated spots (FT8, JS8, APRS) where the plan allows, not on them. 60m and 17m are too small to avoid FT8.",
    "One spot per band and region, so stations find each other without agreeing a frequency first.",
    "On licence-free bands and CB, channel 1 or the national calling channel: every set can tune it, and people already listen there.",
    "Mode follows the radio: USB data on HF and SSB CB, FM on VHF, UHF and FM CB sets, AM where typical CB sets are AM only.",
]



def mhz(khz):
    return f"{khz / 1000:.3f}"


def band_range(name, spots):
    ranges = FREE.get(name) or HAM[name]
    if len(ranges) == 1:
        return ranges[0]
    # A split allocation (MURS, FRS): draw the part that holds the spots.
    return next(r for r in ranges if any(r[0] <= s["khz"] <= r[1] for s in spots))


def mode_cls(mode):
    return {"USB": "usb", "LSB": "usb", "FM": "fm", "AM": "am"}[mode]


# CB: the common 40-channel block gets a close-up, and spots are grouped by country.
ZOOM = {"CB": (26_965, 27_405, "ch 1–40", "Channels 1 to 40, used in most countries")}
BY_REGION = {"CB": ["UK", "EU", "DE", "US/CA", "AU", "NZ"]}


def ticks_html(spots, avoid, lo, hi, labels=False):
    """Marks for one bar. A frequency used in two modes gets a mark per mode, so the
    region filter shows the right colour."""
    pos = lambda khz: (khz - lo) / (hi - lo) * 100
    out, seen, freqs, last, lifted = [], set(), set(), -99.0, False
    for s in spots:
        key = (s["khz"], mode_cls(s["mode"]))
        if key in seen or not lo <= s["khz"] <= hi:
            continue
        seen.add(key)
        same = [x for x in spots if (x["khz"], mode_cls(x["mode"])) == key]
        g = " ".join(sorted({g for x in same for g in REGION_GROUPS[x["region"]].split()}))
        title = "; ".join(f'{mhz(x["khz"])} {x["mode"]} {x["region"]}' for x in same)
        label = ""
        if labels:
            ch = next((m.group(1) for x in same for m in [re.search(r"(?:^|CEPT )ch (\d+)", x["note"])] if m), "")
            if s["khz"] not in freqs:
                # Lift a label that would touch the one before it.
                lifted = pos(s["khz"]) - last < 6 and not lifted
                last = pos(s["khz"])
            label = f'<em{" class=\"bp-tick--hi\"" if lifted else ""}>{ch}</em>' if ch else ""
        freqs.add(s["khz"])
        out.append(f'<span class="bp-tick bp-{mode_cls(s["mode"])}" style="left:{pos(s["khz"]):.2f}%" '
                   f'data-groups="{g}" title="{title}">{label}</span>')
    for a in avoid:
        out.append(f'<span class="bp-tick bp-avoid" style="left:{pos(a["khz"]):.2f}%" data-groups="{REGION_GROUPS[a["region"]]}" '
                   f'title="{mhz(a["khz"])} {a["label"]} {a["region"]}: keep chat off"></span>')
    return "".join(out), len(freqs)


def chip(s, where=True):
    note = f'<span class="bp-note">{s["note"]}</span>' if s["note"] else '<span class="bp-note"></span>'
    region = f' <span class="bp-where">{s["region"]}</span>' if where else ""
    return (f'<li class="bp-{mode_cls(s["mode"])}" data-groups="{REGION_GROUPS[s["region"]]}">'
            f'<b>{mhz(s["khz"])}</b> <span class="bp-mode">{s["mode"]}</span> {note}{region} <code>{s["preset"]}</code></li>')


def band_html(name):
    spots = [s for s in SPOTS if s["band"] == name]
    avoid = [a for a in AVOID if a["band"] == name]
    lo, hi = band_range(name, spots)
    groups = " ".join(sorted({g for s in spots for g in REGION_GROUPS[s["region"]].split()}))
    ticks, count = ticks_html(spots, avoid, lo, hi)

    seg = edge_label = zoom = ""
    if name in ZOOM:
        zlo, zhi, short, long = ZOOM[name]
        left, width = (zlo - lo) / (hi - lo) * 100, (zhi - zlo) / (hi - lo) * 100
        seg = f'<span class="bp-seg" style="left:{left:.2f}%;width:{width:.2f}%"></span>'
        edge_label = f'<span class="bp-seg-label" style="left:{left + width / 2:.2f}%">{short}</span>'
        zticks, _ = ticks_html(spots, [], zlo, zhi, labels=True)
        zoom = f'''
            <p class="bp-zoom-title">Close up: {long.lower()}, {mhz(zlo)}–{mhz(zhi)} MHz</p>
            <div class="bp-bar bp-bar--zoom" role="img" aria-label="{long}, {mhz(zlo)} to {mhz(zhi)} MHz, with channel numbers">
              {zticks}
            </div>
            <p class="bp-edges" aria-hidden="true"><span>{mhz(zlo)}</span><span>{mhz(zhi)}</span></p>'''

    if name in BY_REGION:
        order = BY_REGION[name]
        assert {s["region"] for s in spots} <= set(order), "new CB region: add it to BY_REGION"
        rows = "".join(
            f'<div data-groups="{REGION_GROUPS[r]}"><span class="bp-where">{r}</span>'
            f'<ul class="bp-spots">{"".join(chip(s, where=False) for s in spots if s["region"] == r)}</ul>'
            f'<p class="bp-why bp-why--region">{WHY_REGION[r]}</p></div>'
            for r in order if any(s["region"] == r for s in spots))
        chips_html = f'<div class="bp-regions">{rows}</div>'
    else:
        chips = [chip(s) for s in spots] + [
            f'<li class="bp-avoid" data-groups="{REGION_GROUPS[a["region"]]}"><b>{mhz(a["khz"])}</b> '
            f'{a["label"]} <span class="bp-where">{a["region"]}</span> <span class="bp-note">keep chat off</span></li>'
            for a in avoid]
        chips_html = f'<ul class="bp-spots">{"".join(chips)}</ul>'

    kind = "free" if name in FREE else "ham"
    label = "" if name in BY_REGION else "<b>Why here</b> "
    note_html = f'\n            <p class="bp-why">{label}{WHY[name]}</p>'
    return f'''          <div class="bp-band bp-band--{kind}" id="band-{name.lower().replace(".", "-")}" data-groups="{groups}">
            <p class="bp-band__name"><b>{name}</b><span>{mhz(lo)}–{mhz(hi)} MHz</span></p>
            <div class="bp-bar" role="img" aria-label="{name}, {mhz(lo)} to {mhz(hi)} MHz, with {count} suggested spot{"s" if count != 1 else ""}">
              {seg}{ticks}
            </div>
            <p class="bp-edges" aria-hidden="true"><span>{mhz(lo)}</span>{edge_label}<span>{mhz(hi)}</span></p>{zoom}
            {chips_html}{note_html}
          </div>'''


def band_of(khz):
    for name, ranges in FREE.items():
        if any(a <= khz <= b for a, b in ranges):
            return name
    for name, ((a, b),) in HAM.items():
        if a <= khz <= b:
            return name
    raise ValueError(khz)


for s in SPOTS + AVOID:
    s["band"] = band_of(s["khz"])
listed = {b for _, _, _, bands in SECTIONS for b in bands}
assert {s["band"] for s in SPOTS} <= set(WHY), "a band has no WHY entry"
assert {s["region"] for s in SPOTS if s["band"] in BY_REGION} <= set(WHY_REGION), "a CB region has no WHY_REGION entry"
assert {s["band"] for s in SPOTS} <= listed, "a spot sits in a band no section lists"

sections_html = []
for sid, title, lead, bands in SECTIONS:
    body = "\n".join(band_html(b) for b in bands if any(s["band"] == b for s in SPOTS))
    sections_html.append(f'''      <section class="article-section" aria-labelledby="{sid}H">
        <h2 id="{sid}H">{title}</h2>
        <p class="section-lead">{lead}</p>
        <div class="bp-list">
{body}
        </div>
      </section>''')

buttons = '<button type="button" data-bp-region="all" aria-pressed="true">All regions</button>' + "".join(
    f'<button type="button" data-bp-region="{k}" aria-pressed="false">{v}</button>' for k, v in GROUPS.items())

HTML = f'''<!doctype html>
<!-- Generated by scripts/gen_band_plan.py from crates/wcr/src/band.rs. Do not edit by hand. -->
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <meta name="theme-color" content="#07070a" />
  <meta name="apple-mobile-web-app-capable" content="yes" />
  <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
  <meta name="mobile-web-app-capable" content="yes" />
  <title>Band plan — WeeChat Radio</title>
  <meta name="description" content="Where WeeChat Radio sits on each band, from 160m to 23cm, CB, PMR446, FRS and MURS. Every band drawn to scale with the app's suggested spots." />
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content="WeeChat Radio" />
  <meta property="og:title" content="Band plan — WeeChat Radio" />
  <meta property="og:description" content="Where WeeChat Radio sits on each band, from 160m to 23cm, CB, PMR446, FRS and MURS." />
  <meta property="og:url" content="https://weechatradio.com/docs/band-plan.html" />
  <meta property="og:image" content="https://weechatradio.com/og.png" />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta property="og:image:alt" content="WeeChat Radio. Live messaging chat powered by radio." />
  <meta name="twitter:card" content="summary_large_image" />
  <meta name="twitter:image" content="https://weechatradio.com/og.png" />
  <link rel="icon" type="image/svg+xml" href="/icons/mark-accent.svg" />
  <link rel="icon" type="image/png" href="/favicon.png" />
  <link rel="apple-touch-icon" href="/apple-touch-icon.png" />
  <link rel="stylesheet" href="/styles.css?v=34" />
  <link rel="stylesheet" href="/docs.css?v=4" />
</head>
<body class="page-doc">
  <div id="scan"></div>
  <div id="boot_screen" hidden><pre id="boot_log"></pre></div>
  <header class="topbar">
    <div class="prompt"><span class="who">weechat-radio</span><span class="sym">:$</span></div>
    <nav>
      <a href="/map.html">map</a>
      <a href="/downloads.html">downloads</a>
      <a href="/guides.html">guides</a>
      <a href="/docs.html">docs</a>
      <a href="/why.html">why</a>
      <a href="/status.html">hub</a>
      <a href="/changelog.html">changelog</a>
    </nav>
    <div class="clock-mod">
      <div id="clock-utc">UTC --:--:--</div>
      <div class="sub" id="clock-local">LCL --:--</div>
    </div>
  </header>
  <div class="page-wrap">
    <main class="page-panel">
      <p class="crumb"><a href="/docs.html">docs</a> / band plan</p>
      <h1><span class="who">weechat-radio</span><span class="sym">:$</span> band plan</h1>
      <p class="lead">Where WeeChat Radio sits on each band. Every bar is one band, edge to edge, and the marks are the spots the app suggests: the same list as <a href="/docs/calling.html">calling frequencies</a> and <code>wcr help calling</code>.</p>

      <div class="warn-box">
        <h2>Check your licence first</h2>
        <p>You are the control operator. The edges here are the widest allocation the app knows, and your country may allow less. Verify the frequency, mode and power for your licence before you transmit.</p>
      </div>

      <section class="article-section" aria-labelledby="whyH">
        <h2 id="whyH">How the spots were picked</h2>
        <ul class="bp-rules">
          {"".join(f"<li>{r}</li>" for r in RULES)}
        </ul>
        <p class="bp-why">Each band below says why its spot sits where it does.</p>
      </section>

      <div class="bp-key" aria-label="Key">
        <span class="bp-usb">USB or LSB data</span><span class="bp-fm">FM</span><span class="bp-am">AM</span><span class="bp-avoid">keep chat off</span>
        <span class="bp-band--free-key">licence-free band</span><span class="bp-key__plain">WW = worldwide</span>
      </div>

      <div class="setup-switch setup-switch--fill bp-switch" role="group" aria-label="Show spots for" hidden>{buttons}</div>
      <div class="bp-plan" data-bp-region="all">
{chr(10).join(sections_html)}
      </div>

      <section class="article-section" aria-labelledby="moreH">
        <h2 id="moreH">More</h2>
        <p>The preset under each spot is where to start. On a rough HF path use <code>hf-weak</code> or <code>hf-deep</code> instead of <code>hf-poor</code>. See <a href="/docs/weak.html">weak signals</a>.</p>
        <p>Stations on different bands still chat through a gateway and the hub. See <a href="/docs/bridging.html">cross-frequency chat</a> and <a href="/docs/mixed-band.html">one group across PMR, CB, 2m and internet</a>.</p>
      </section>

      <p class="next">The same spots as a table: <a href="/docs/calling.html">calling frequencies</a>.</p>
    </main>
  </div>
  <script src="/copy.js?v=1"></script>
  <script src="/shell.js?v=10"></script>
  <script>
    // Region filter: hides spots (and bands) with nothing for the chosen region.
    (() => {{
      const sw = document.querySelector(".bp-switch");
      const plan = document.querySelector(".bp-plan");
      if (!sw || !plan) return;
      sw.hidden = false;
      sw.addEventListener("click", (e) => {{
        const btn = e.target.closest("[data-bp-region]");
        if (!btn) return;
        plan.dataset.bpRegion = btn.dataset.bpRegion;
        sw.querySelectorAll("button").forEach((b) => b.setAttribute("aria-pressed", String(b === btn)));
      }});
    }})();
  </script>
</body>
</html>
'''
(ROOT / "web/docs/band-plan.html").write_text(HTML)

# --- markdown twin ---------------------------------------------------------------
md = ["# Band plan", "",
      "Where WeeChat Radio sits on each band. Generated from `crates/wcr/src/band.rs` by "
      "`scripts/gen_band_plan.py`; the same spots as [calling frequencies](CALLING.md) and `wcr help calling`.", "",
      "You are the control operator. The edges are the widest allocation the app knows; your country may allow less. "
      "Verify frequency, mode and power for your licence before you transmit.", ""]
md += ["## How the spots were picked", ""] + [f"- {r}" for r in RULES] + [""]
for sid, title, lead, bands in SECTIONS:
    md += [f"## {title}", "", lead, "", "| Band | Edges (MHz) | Spot | Mode | Where | Preset |", "|---|---|---|---|---|---|"]
    for b in bands:
        spots = [s for s in SPOTS if s["band"] == b]
        if not spots:
            continue
        lo, hi = band_range(b, spots)
        for i, s in enumerate(spots):
            note = f' ({s["note"]})' if s["note"] else ""
            md.append(f'| {b if i == 0 else ""} | {f"{mhz(lo)}–{mhz(hi)}" if i == 0 else ""} | {mhz(s["khz"])}{note} | {s["mode"]} | {s["region"]} | `{s["preset"]}` |')
        for a in AVOID:
            if a["band"] == b:
                md.append(f'|  |  | {mhz(a["khz"])} | {a["label"]}: keep chat off | {a["region"]} |  |')
    md.append("")
    md += ["Why here:", ""] + [f"- **{b}**: {WHY[b]}" for b in bands if any(s["band"] == b for s in SPOTS)]
    if any(b in BY_REGION for b in bands):
        md += [f"  - {r}: {WHY_REGION[r]}" for r in BY_REGION["CB"]]
    md.append("")
(ROOT / "docs/BAND-PLAN.md").write_text("\n".join(md))
print("wrote web/docs/band-plan.html and docs/BAND-PLAN.md:", len(SPOTS), "spots")
