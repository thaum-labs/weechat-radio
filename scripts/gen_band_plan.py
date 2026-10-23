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
LICENCE_FREE_NOTE = {
    "CB": "Licence-free. The edges cover every regional CB plan the app knows (NZ 26 MHz, German extra channels, UK 27/81). Check your national CB plan.",
    "MURS": "Licence-free in the US.",
    "PMR446": "Licence-free in the UK and Europe.",
    "FRS": "Licence-free in the US and Canada.",
}
BAND_NOTE = {
    "160m": "Data is USB here, though voice on this band is LSB.",
    "80m": "Data is USB here, though voice on this band is LSB.",
    "40m": "Data is USB here, though voice on this band is LSB. The UK and Europe stop at 7.200.",
    "60m": "Channels and power limits differ a lot by country.",
    "2m": "Keep chat off the APRS channels.",
}


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


def band_html(name):
    spots = [s for s in SPOTS if s["band"] == name]
    avoid = [a for a in AVOID if a["band"] == name]
    lo, hi = band_range(name, spots)
    groups = " ".join(sorted({g for s in spots for g in REGION_GROUPS[s["region"]].split()}))
    pos = lambda khz: f"{(khz - lo) / (hi - lo) * 100:.2f}%"

    ticks, seen = [], set()
    for s in spots:
        if s["khz"] in seen:
            continue
        seen.add(s["khz"])
        same = [x for x in spots if x["khz"] == s["khz"]]
        g = " ".join(sorted({g for x in same for g in REGION_GROUPS[x["region"]].split()}))
        title = "; ".join(f'{mhz(x["khz"])} {x["mode"]} {x["region"]}' for x in same)
        ticks.append(f'<span class="bp-tick bp-{mode_cls(s["mode"])}" style="left:{pos(s["khz"])}" data-groups="{g}" title="{title}"></span>')
    for a in avoid:
        ticks.append(f'<span class="bp-tick bp-avoid" style="left:{pos(a["khz"])}" data-groups="{REGION_GROUPS[a["region"]]}" '
                     f'title="{mhz(a["khz"])} {a["label"]} {a["region"]}: keep chat off"></span>')

    chips = []
    for s in spots:
        note = f' <span class="bp-note">{s["note"]}</span>' if s["note"] else ""
        chips.append(f'<li class="bp-{mode_cls(s["mode"])}" data-groups="{REGION_GROUPS[s["region"]]}">'
                     f'<b>{mhz(s["khz"])}</b> {s["mode"]}{note} <span class="bp-where">{s["region"]}</span> '
                     f'<code>{s["preset"]}</code></li>')
    for a in avoid:
        chips.append(f'<li class="bp-avoid" data-groups="{REGION_GROUPS[a["region"]]}"><b>{mhz(a["khz"])}</b> '
                     f'{a["label"]} <span class="bp-where">{a["region"]}</span> <span class="bp-note">keep chat off</span></li>')

    kind = "free" if name in FREE else "ham"
    note = LICENCE_FREE_NOTE.get(name) or BAND_NOTE.get(name, "")
    note_html = f'\n            <p class="bp-band__note">{note}</p>' if note else ""
    return f'''          <div class="bp-band bp-band--{kind}" id="band-{name.lower().replace(".", "-")}" data-groups="{groups}">
            <p class="bp-band__name"><b>{name}</b><span>{mhz(lo)}–{mhz(hi)} MHz</span></p>
            <div class="bp-bar" role="img" aria-label="{name}, {mhz(lo)} to {mhz(hi)} MHz, with {len(seen)} suggested spot{"s" if len(seen) != 1 else ""}">
              {"".join(ticks)}
            </div>
            <p class="bp-edges" aria-hidden="true"><span>{mhz(lo)}</span><span>{mhz(hi)}</span></p>
            <ul class="bp-spots">
              {"".join(chips)}
            </ul>{note_html}
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

      <div class="bp-key" aria-label="Key">
        <span class="bp-usb">USB or LSB data</span><span class="bp-fm">FM</span><span class="bp-am">AM</span><span class="bp-avoid">keep chat off</span>
        <span class="bp-band--free-key">licence-free band</span><span class="bp-key__plain">WW = worldwide</span>
      </div>

      <div class="setup-switch bp-switch" role="group" aria-label="Show spots for" hidden>{buttons}</div>
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
(ROOT / "docs/BAND-PLAN.md").write_text("\n".join(md))
print("wrote web/docs/band-plan.html and docs/BAND-PLAN.md:", len(SPOTS), "spots")
