"""Build web/docs/scenarios.html (Common setups).

Run from the repo root:  python3 scripts/gen_scenarios.py

Each setup is drawn twice: a narrow drawing for phones (440 wide) and a wide one for
desktop (800 wide). Both use the same data-seg, data-node and data-show ids, so one list
of steps drives both. web/docs.css shows the drawing that fits the screen.
Edit this script and re-run it rather than editing the two SVGs in the page by hand.
"""
import json
import math
from pathlib import Path

OUT = Path(__file__).resolve().parents[1] / "web" / "docs" / "scenarios.html"


def f(v):
    return f"{v:.1f}".rstrip("0").rstrip(".")


def wave(x1, y1, x2, y2, amp=5, half=11):
    """Sine path from (x1,y1) to (x2,y2): quadratic half-waves along the line."""
    dx, dy = x2 - x1, y2 - y1
    length = math.hypot(dx, dy)
    n = max(2, round(length / half))
    ux, uy = dx / n, dy / n                      # one half-wave along the line
    px, py = -dy / length, dx / length           # unit normal
    d = [f"M{f(x1)} {f(y1)}"]
    cx, cy = ux / 2 + px * amp * 1.1, uy / 2 + py * amp * 1.1
    d.append(f"q{f(cx)} {f(cy)} {f(ux)} {f(uy)}")
    d += [f"t{f(ux)} {f(uy)}"] * (n - 1)
    return " ".join(d)


def seg(sid, kind, d, setups=None):
    draw = {"rf": "rf", "net": "net", "sound": "wire"}[kind]
    attrs = f' data-setups="{setups}"' if setups else ""
    return (f'<g class="seg seg-{kind}" data-seg="{sid}"{attrs}><path class="{draw}" d="{d}" />'
            f'<path class="pkt" d="{d}" /></g>')


def handheld(x, y, node, call, mode_cls, mode_label, body="rplus", w=48, h=88, label_dy=18):
    ax = x + w - 8
    return f'''<g data-node="{node}">
  <line class="antenna" x1="{ax}" y1="{y}" x2="{ax}" y2="{y - 34}" />
  <circle class="antenna-tip" cx="{ax}" cy="{y - 36}" r="2.5" />
  <rect class="box {body}" x="{x}" y="{y}" width="{w}" height="{h}" rx="5" />
  <circle class="led" cx="{x + w - 12}" cy="{y + 12}" r="3.5" />
  <path class="grille" d="M{x + 10} {y + h * 0.55:.0f} H{x + w - 10} M{x + 10} {y + h * 0.55 + 7:.0f} H{x + w - 10} M{x + 10} {y + h * 0.55 + 14:.0f} H{x + w - 10}" />
</g>
<text class="{mode_cls}" x="{x + w / 2:g}" y="{y + h + label_dy}" text-anchor="middle">{call}</text>
<text class="sub" x="{x + w / 2:g}" y="{y + h + label_dy + 14}" text-anchor="middle">{mode_label}</text>'''


def house(x, y, node, call, role, body="gw", text_cls="on-gw"):
    return f'''<g data-node="{node}">
  <path class="house" d="M{x} {y + 40} L{x + 75} {y} L{x + 150} {y + 40} V{y + 120} H{x} Z" />
  <path class="mast" d="M{x + 110} {y + 18} V{y - 30} M{x + 102} {y - 22} H{x + 118} M{x + 104} {y - 12} H{x + 116}" />
  <rect class="box {body}" x="{x + 20}" y="{y + 74}" width="70" height="34" rx="3" />
  <circle class="led" cx="{x + 30}" cy="{y + 84}" r="3.5" />
  <path class="grille" d="M{x + 44} {y + 84} H{x + 82} M{x + 44} {y + 91} H{x + 82} M{x + 44} {y + 98} H{x + 82}" />
  <path class="mast" d="M{x + 90} {y + 82} H{x + 110} V{y + 18}" />
  <text class="{text_cls}" x="{x + 56}" y="{y + 56}" text-anchor="middle">{call}</text>
  <text class="sub" x="{x + 56}" y="{y + 69}" text-anchor="middle">{role}</text>
</g>'''


def laptop(x, y, node, title, lines, w=140, h=70):
    out = [f'<g data-node="{node}">',
           f'  <rect class="box accent" x="{x}" y="{y}" width="{w}" height="{h}" />',
           f'  <path class="laptop-base" d="M{x - 12} {y + h} H{x + w + 12} L{x + w + 20} {y + h + 10} H{x - 20} Z" />',
           f'  <text class="sub" x="{x + 10}" y="{y + 17}">{title}</text>']
    for i, (show, cls, text) in enumerate(lines):
        out.append(f'  <text class="{cls}" data-show="{show}" x="{x + 10}" y="{y + 38 + i * 16}">{text}</text>')
    out.append("</g>")
    return "\n".join(out)


def hub(x=250, y=26, w=170, h=44, sub="wss · TLS"):
    return f'''<g data-node="hub">
  <rect class="box hub" x="{x}" y="{y}" width="{w}" height="{h}" />
  <text class="on-hub" x="{x + w / 2:g}" y="{y + 19}" text-anchor="middle">hub</text>
  <text class="sub" x="{x + w / 2:g}" y="{y + 35}" text-anchor="middle">{sub}</text>
</g>'''


def group(name, body, default):
    cls = "" if name == default else ' class="is-off"'
    return f'<g data-setups="{name}"{cls}>\n{body}\n</g>'


def slot(items, x, y, cls="scr", anchor="middle"):
    """Several lines in one place; docs.css shows only the latest one shown."""
    texts = "".join(f'<text class="{c or cls}" data-show="{show}" x="{x}" y="{y}" text-anchor="{anchor}">{text}</text>'
                    for show, text, c in [(i + (None,))[:3] for i in items])
    return f'<g class="slot">{texts}</g>'


def laptop_slot(x, y, node, title, items, w=140, h=70):
    """Laptop whose screen shows the latest line."""
    return "\n".join([
        f'<g data-node="{node}">',
        f'  <rect class="box accent" x="{x}" y="{y}" width="{w}" height="{h}" />',
        f'  <path class="laptop-base" d="M{x - 12} {y + h} H{x + w + 12} L{x + w + 20} {y + h + 10} H{x - 20} Z" />',
        f'  <text class="sub" x="{x + 10}" y="{y + 17}">{title}</text>',
        "  " + slot(items, x + 10, y + 42, anchor="start"),
        f'  <text class="scr-input" x="{x + 10}" y="{y + 62}">&gt; _</text>',
        "</g>",
    ])


DEFAULT = "home"
groups, wide_groups = [], []
steps = {}

# Shared wide layout: radios on the left, the home gateway, the hub, the internet end.
W_HOUSE = (270, 120)
W_MAST = (W_HOUSE[0] + 110, W_HOUSE[1] - 30)
W_HUB = dict(x=480, y=158, w=130, h=44)
GW_TO_HUB, HUB_TO_GW = "M420 180 H480", "M480 180 H420"
HUB_TO_END, END_TO_HUB = "M610 180 H640", "M640 180 H610"
TLS_GW = '<text class="sub" x="450" y="172" text-anchor="middle">TLS</text>'
TLS_END = '<text class="sub" x="625" y="172" text-anchor="middle">TLS</text>'
W_TIP = (110, 114)                          # handheld(70, 150) antenna tip
MAST = (390, 130)                           # narrow house(280, 160) mast top
N_TIP = (100, 154)                          # narrow handheld(60, 190) antenna tip

# 1. Home gateway: one walker with a handheld, a base station at home ----------
HOME_X = [("x1", "at the trailhead"), ("x2", "windy up there?"),
          ("x3", "a bit. back by 4"), ("x4", "kettle on at 4")]
groups.append(group("home", "\n".join([
    laptop_slot(20, 14, "home-x", "G4XYZ · #walk", HOME_X),
    hub(),
    seg("home-hub-x", "net", "M250 48 H162"),
    '<text class="sub" x="206" y="42" text-anchor="middle">TLS</text>',
    house(280, 160, "home-gw", "M0GWY", "home gateway"),
    seg("home-gw-hub", "net", "M415 192 V72"),
    '<text class="sub" x="409" y="104" text-anchor="end">TLS</text>',
    '<rect class="lane-bg" x="10" y="116" width="200" height="214" />',
    '<text class="sub" x="20" y="134">out walking · 144.950 FM</text>',
    seg("home-rf-a", "rf", wave(N_TIP[0] + 4, N_TIP[1] - 1, MAST[0] - 4, MAST[1] + 1)),
    handheld(60, 190, "home-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="sub" x="355" y="298" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="355" y="312" text-anchor="middle">rf_egress = true</text>',
]), DEFAULT))
wide_groups.append(group("home", "\n".join([
    '<rect class="lane-bg" x="16" y="60" width="200" height="230" />',
    '<text class="sub" x="26" y="78">out walking · 144.950 FM</text>',
    house(*W_HOUSE, "home-gw", "M0GWY", "home gateway"),
    hub(**W_HUB),
    seg("home-gw-hub", "net", GW_TO_HUB),
    seg("home-hub-x", "net", HUB_TO_END),
    TLS_GW, TLS_END,
    laptop_slot(640, 145, "home-x", "G4XYZ · #walk", HOME_X, w=130),
    seg("home-rf-a", "rf", wave(W_TIP[0] + 4, W_TIP[1] - 1, W_MAST[0] - 4, W_MAST[1] + 1)),
    handheld(70, 150, "home-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="sub" x="345" y="262" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="345" y="276" text-anchor="middle">rf_egress = true</text>',
]), DEFAULT))
steps["home"] = {"viewBox": "0 0 440 340", "viewBoxWide": "0 52 800 248", "steps": [
    {"say": "G4ABC writes to #walk. Its computer keys the handheld on 144.950 and does not open the hub.",
     "segs": ["home-rf-a"], "on": ["home-a"], "show": ["h1"], "ms": 1900},
    {"say": "The home gateway hears a radio-plus line and sends it to the hub. G4ABC’s line shows [gw].",
     "segs": ["home-gw-hub"], "on": ["home-gw"], "show": ["h1gw"], "ms": 1900},
    {"say": "The hub delivers it to G4XYZ, who has no radio.",
     "segs": ["home-hub-x"], "on": ["home-x"], "show": ["x1"], "ms": 1500},
    {"say": "G4XYZ replies through the hub.",
     "segs": ["<home-hub-x"], "on": ["home-x"], "show": ["x2"], "ms": 1400},
    {"say": "The hub hands it to the gateway, which heard a #walk member on 144.950 in the last 10 minutes.",
     "segs": ["<home-gw-hub"], "on": ["home-gw"], "ms": 2000},
    {"say": "With rf_egress = true, the gateway keys it on 144.950, and G4ABC reads it.",
     "segs": ["<home-rf-a"], "on": ["home-gw"], "show": ["h2"], "ms": 1900},
    {"say": "G4ABC answers on the air.", "segs": ["home-rf-a"], "on": ["home-a"], "show": ["h3"], "ms": 1400},
    {"say": "Same path as before: the gateway uploads it and the hub delivers it.",
     "segs": ["home-gw-hub", "home-hub-x"], "chain": True, "on": ["home-gw"], "show": ["h3gw", "x3"], "ms": 2000},
    {"say": "G4XYZ’s reply goes to the hub…", "segs": ["<home-hub-x"], "on": ["home-x"], "show": ["x4"], "ms": 1300},
    {"say": "…to the gateway, and onto the air for G4ABC.",
     "segs": ["<home-gw-hub", "<home-rf-a"], "chain": True, "on": ["home-gw"], "ms": 2000},
    {"say": "G4ABC reads it.", "show": ["h4"], "on": ["home-a"], "ms": 1200},
], "done": "One walker with a handheld and one person at a laptop share the #walk thread through a home gateway."}

# 2. HF to HF, no internet ----------------------------------------------------
HF_B = [("b1", "&lt;G4ABC&gt; good evening"), ("b2", "&lt;EA4XYZ&gt; hola, copy fine"),
        ("b3", "&lt;G4ABC&gt; 73, same time tmrw?"), ("b4", "&lt;EA4XYZ&gt; si, 73")]
groups.append(group("hf", "\n".join([
    '<text class="sub on-rf" x="255" y="34" text-anchor="middle">14.070 USB · 20m</text>',
    seg("hf-rf", "rf", wave(134, 58, 376, 58, amp=6, half=11)),
    house(20, 88, "hf-a", "G4ABC", "radio", body="radio", text_cls="on-radio"),
    house(270, 88, "hf-b", "EA4XYZ", "radio", body="radio", text_cls="on-radio"),
    '<text class="sub" x="95" y="228" text-anchor="middle">preset hf-poor</text>',
    '<text class="sub" x="345" y="228" text-anchor="middle">preset hf-poor</text>',
    slot(HF_B, 345, 250),
    '<text class="sub" x="220" y="186" text-anchor="middle">no hub</text>',
    '<text class="sub" x="220" y="200" text-anchor="middle">no gateway</text>',
]), DEFAULT))
wide_groups.append(group("hf", "\n".join([
    '<text class="sub on-rf" x="435" y="46" text-anchor="middle">14.070 USB · 20m</text>',
    house(60, 100, "hf-a", "G4ABC", "radio", body="radio", text_cls="on-radio"),
    house(590, 100, "hf-b", "EA4XYZ", "radio", body="radio", text_cls="on-radio"),
    seg("hf-rf", "rf", wave(174, 70, 696, 70, amp=6, half=11)),
    '<text class="sub" x="435" y="168" text-anchor="middle">no hub</text>',
    '<text class="sub" x="435" y="184" text-anchor="middle">no gateway</text>',
    '<text class="sub" x="135" y="248" text-anchor="middle">preset hf-poor</text>',
    '<text class="sub" x="665" y="248" text-anchor="middle">preset hf-poor</text>',
    slot(HF_B, 665, 270),
]), DEFAULT))
steps["hf"] = {"viewBox": "0 0 440 262", "viewBoxWide": "0 28 800 256", "steps": [
    {"say": "G4ABC keys a line on 14.070. In radio mode it stays on the air: nothing goes to the hub.",
     "segs": ["hf-rf"], "on": ["hf-a"], "show": ["f1"], "ms": 1900},
    {"say": "EA4XYZ decodes it and sends a receipt on the same frequency.",
     "segs": ["<hf-rf"], "on": ["hf-b"], "show": ["b1"], "ms": 1900},
    {"say": "The receipt arrives: G4ABC’s line shows [ok].", "show": ["f1ok"], "on": ["hf-a"], "ms": 1200},
    {"say": "EA4XYZ replies.", "segs": ["<hf-rf"], "on": ["hf-b"], "show": ["b2"], "ms": 1800},
    {"say": "G4ABC decodes it, and its station sends the receipt back by itself.",
     "segs": ["hf-rf"], "on": ["hf-a"], "show": ["f2"], "ms": 1900},
    {"say": "G4ABC answers.", "segs": ["hf-rf"], "on": ["hf-a"], "show": ["f3"], "ms": 1700},
    {"say": "EA4XYZ reads it and acks; G4ABC’s line shows [ok].",
     "segs": ["<hf-rf"], "on": ["hf-b"], "show": ["b3", "f3ok"], "ms": 1900},
    {"say": "EA4XYZ signs off.", "segs": ["<hf-rf"], "on": ["hf-b"], "show": ["b4"], "ms": 1700},
    {"say": "G4ABC reads it and acks. No hub, no gateway, no internet the whole time.",
     "segs": ["hf-rf"], "on": ["hf-a"], "show": ["f4"], "ms": 2000},
], "done": "Two stations, one frequency, no internet anywhere. Every line got its receipt on the air."}

# 3. Internet guest via a gateway ---------------------------------------------
GUEST_A = [("a1", "&lt;~sam&gt; on the hill yet?"), ("a2", "&lt;G4ABC&gt; yes, at the top"),
           ("a3", "&lt;~sam&gt; photo later?"), ("a4", "&lt;G4ABC&gt; will email it")]
GUEST_S = [("s1", "on the hill yet?"), ("s2", "yes, at the top"), ("s3", "photo later?"), ("s4", "will email it")]
groups.append(group("guest", "\n".join([
    laptop_slot(20, 14, "g-sam", "~sam · G4ABC", GUEST_S),
    hub(sub="picks the gateway"),
    seg("g-sam-hub", "net", "M162 48 H250"),
    '<text class="sub" x="206" y="42" text-anchor="middle">TLS</text>',
    house(280, 160, "g-gw", "M0GWY", "home gateway"),
    seg("g-hub-gw", "net", "M415 72 V192"),
    '<text class="sub" x="409" y="104" text-anchor="end">TLS</text>',
    seg("g-gw-a", "rf", wave(MAST[0] - 4, MAST[1] + 1, N_TIP[0] + 4, N_TIP[1] - 1)),
    '<text class="sub on-rf" x="236" y="128" text-anchor="middle">144.950</text>',
    handheld(60, 190, "g-a", "G4ABC", "on-rplus", "radio-plus"),
    slot(GUEST_A, 92, 330),
    '<text class="scr ok" data-show="g-chk" x="355" y="298" text-anchor="middle">rf_egress = true</text>',
    '<text class="scr ok" data-show="g-chk" x="355" y="314" text-anchor="middle">G4ABC heard here</text>',
    '<text class="scr ok" data-show="g-chk" x="355" y="330" text-anchor="middle">third_party = "allow"</text>',
]), DEFAULT))
wide_groups.append(group("guest", "\n".join([
    house(*W_HOUSE, "g-gw", "M0GWY", "home gateway"),
    hub(**W_HUB, sub="picks the gateway"),
    seg("g-hub-gw", "net", HUB_TO_GW),
    seg("g-sam-hub", "net", END_TO_HUB),
    TLS_GW, TLS_END,
    laptop_slot(640, 145, "g-sam", "~sam · G4ABC", GUEST_S, w=130),
    seg("g-gw-a", "rf", wave(W_MAST[0] - 4, W_MAST[1] + 1, W_TIP[0] + 4, W_TIP[1] - 1)),
    '<text class="sub on-rf" x="245" y="86" text-anchor="middle">144.950</text>',
    handheld(70, 150, "g-a", "G4ABC", "on-rplus", "radio-plus"),
    slot(GUEST_A, 102, 292),
    '<text class="scr ok" data-show="g-chk" x="345" y="262" text-anchor="middle">rf_egress = true</text>',
    '<text class="scr ok" data-show="g-chk" x="345" y="278" text-anchor="middle">G4ABC heard here</text>',
    '<text class="scr ok" data-show="g-chk" x="345" y="294" text-anchor="middle">third_party = "allow"</text>',
]), DEFAULT))
steps["guest"] = {"viewBox": "0 0 440 342", "viewBoxWide": "0 64 800 242", "steps": [
    {"say": "~sam has no radio and no callsign. They message G4ABC through the hub.",
     "segs": ["g-sam-hub"], "on": ["g-sam"], "show": ["s1"], "ms": 1600},
    {"say": "The hub picks the gateway that has heard G4ABC: M0GWY at home.", "segs": ["g-hub-gw"], "ms": 1500},
    {"say": "M0GWY checks: rf_egress is on, G4ABC was heard on this dial in the last 10 minutes, and guests are allowed.",
     "on": ["g-gw"], "show": ["g-chk"], "ms": 2500},
    {"say": "All three pass, so M0GWY keys the line on 144.950. G4ABC reads it.",
     "segs": ["g-gw-a"], "on": ["g-gw"], "show": ["a1"], "ms": 1900},
    {"say": "G4ABC replies on the air in radio-plus.", "segs": ["<g-gw-a"], "on": ["g-a"], "show": ["a2"], "ms": 1700},
    {"say": "The gateway hears a radio-plus line and sends it to the hub…", "segs": ["<g-hub-gw"], "on": ["g-gw"], "ms": 1300},
    {"say": "…which delivers it to ~sam.", "segs": ["<g-sam-hub"], "on": ["g-sam"], "show": ["s2"], "ms": 1300},
    {"say": "~sam asks again. The same checks pass, so it goes straight out.",
     "segs": ["g-sam-hub", "g-hub-gw"], "chain": True, "on": ["g-sam"], "show": ["s3"], "ms": 2100},
    {"say": "G4ABC reads it.", "segs": ["g-gw-a"], "on": ["g-gw"], "show": ["a3"], "ms": 1500},
    {"say": "G4ABC answers, and the gateway carries it back.",
     "segs": ["<g-gw-a", "<g-hub-gw"], "chain": True, "on": ["g-a"], "show": ["a4"], "ms": 2200},
    {"say": "~sam reads it.", "segs": ["<g-sam-hub"], "on": ["g-sam"], "show": ["s4"], "ms": 1300},
], "done": "~sam and G4ABC chat through the gateway. With third_party = \"deny\", the default, the gateway would stay quiet."}

# 4. Relay with hops ----------------------------------------------------------
calls = ["G4ABC", "G4DEF", "G4JKL", "G4MNO"]
nodes = ["hp-a", "hp-b", "hp-c", "hp-d"]
HOP_SLOTS = [  # one line under each station: the message out, then the reply back
    [("p1", "3 hops left"), ("pok", "[ok]", "scr ok"), ("r0", ""), ("r4", "reads it")],
    [("p2", "2 left"), ("r0", ""), ("r3", "1 left")],
    [("p3", "1 left"), ("r0", ""), ("r2", "2 left")],
    [("p4", "reads it"), ("r0", ""), ("r1", "3 hops left")],
]


def hops_body(xs, tip_y, radius, hand, label):
    tips = [(x + hand["tip_dx"], tip_y) for x in xs]
    parts = [
        f'<text class="sub on-rf" x="{label[0]}" y="22" text-anchor="middle">144.950 FM · one dial</text>',
        f'<circle class="lane-bg" cx="{tips[0][0]}" cy="{tips[0][1]}" r="{radius}" />',
        f'<text class="sub" x="{tips[0][0] + label[1]}" y="44">G4ABC’s reach</text>',
    ]
    for i in range(3):
        (x1, y1), (x2, y2) = tips[i], tips[i + 1]
        parts.append(seg(f"hp-{i + 1}", "rf", wave(x1 + 6, y1, x2 - 6, y2, amp=5, half=10.5)))
    for x, call, node, items in zip(xs, calls, nodes, HOP_SLOTS):
        parts.append(handheld(x - hand["dx"], hand["y"], node, call, "on-radio", "radio", body="radio",
                              w=hand["w"], h=hand["h"], label_dy=16))
        parts.append(slot(items, x, hand["slot_y"]))
    return "\n".join(parts)


groups.append(group("hops", hops_body([50, 165, 280, 395], 110, 150,
                                      dict(dx=18, y=140, w=36, h=64, tip_dx=10, slot_y=252), (220, 6)), DEFAULT))
wide_groups.append(group("hops", hops_body([100, 300, 500, 700], 94, 260,
                                           dict(dx=20, y=130, w=40, h=72, tip_dx=12, slot_y=256), (400, 12)), DEFAULT))
steps["hops"] = {"viewBox": "0 0 440 266", "viewBoxWide": "0 0 800 270", "steps": [
    {"say": "G4ABC sends G4MNO a direct message with 3 hops left. Only G4DEF is in reach.",
     "segs": ["hp-1"], "on": ["hp-a"], "show": ["p1", "m1"], "ms": 1900},
    {"say": "G4DEF hears it for the first time, takes one off and sends it on with 2 left. G4ABC hears that too: [rl].",
     "segs": ["hp-2"], "on": ["hp-b"], "show": ["p2", "m1rl"], "ms": 2300},
    {"say": "G4JKL does the same with 1 left.", "segs": ["hp-3"], "on": ["hp-c"], "show": ["p3"], "ms": 1700},
    {"say": "G4MNO reads it. Hops left is now 0, so it does not send it on.", "on": ["hp-d"], "show": ["p4"], "ms": 1600},
    {"say": "G4MNO’s receipt comes back the same way, relayed by G4JKL and G4DEF.",
     "segs": ["<hp-3", "<hp-2", "<hp-1"], "chain": True, "ms": 2900},
    {"say": "G4ABC shows [ok]. It never heard G4MNO directly.", "on": ["hp-a"], "show": ["pok", "m1ok"], "ms": 1500},
    {"say": "G4MNO replies with 3 hops left.", "segs": ["<hp-3"], "on": ["hp-d"], "show": ["r0", "r1"], "ms": 1800},
    {"say": "G4JKL and G4DEF relay it, each taking one off.",
     "segs": ["<hp-2", "<hp-1"], "chain": True, "show": ["r2", "r3"], "ms": 2200},
    {"say": "G4ABC reads it with 0 left, and its receipt goes back through the same two relays.",
     "segs": ["hp-1", "hp-2", "hp-3"], "chain": True, "on": ["hp-a"], "show": ["r4", "m2"], "ms": 3000},
], "done": "G4ABC and G4MNO never hear each other, yet they hold a conversation through two relays on one dial."}

# 5. Email over radio ---------------------------------------------------------
EMAIL_INBOX = [("e-inbox", "inbox: 1 waiting"), ("e-ch1", "getting: 1 of 2"), ("e-ch2", "getting: 2 of 2"),
               ("e-got", "inbox: Re: Meeting point")]
groups.append(group("email", "\n".join([
    '<g data-node="e-mail">',
    '  <rect class="box accent" x="20" y="26" width="140" height="44" />',
    '  <path class="wire" d="M30 36 H52 V54 H30 Z M30 36 L41 46 L52 36" />',
    '  <text class="on-accent" x="62" y="45">Resend</text>',
    '  <text class="sub" x="62" y="61">email service</text>',
    '</g>',
    slot([("e-acc", "accepted", "scr ok"), ("e-re", "reply from alex")], 90, 88),
    '<text class="sub" x="90" y="104" text-anchor="middle">alex@example.com</text>',
    hub(),
    seg("e-hub-mail", "net", "M250 48 H162"),
    house(280, 160, "e-gw", "M0GWY", "mail gateway"),
    seg("e-gw-hub", "net", "M415 192 V72"),
    '<text class="sub" x="409" y="104" text-anchor="end">TLS</text>',
    seg("e-a-gw", "rf", wave(N_TIP[0] + 4, N_TIP[1] - 1, MAST[0] - 4, MAST[1] + 1)),
    '<text class="sub on-rf" x="236" y="128" text-anchor="middle">144.950</text>',
    handheld(60, 190, "e-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="scr" data-show="e-tx" x="26" y="330">[tx]</text>',
    '<text class="scr ok" data-show="e-gw" x="62" y="330">[gw]</text>',
    '<text class="scr ok" data-show="e-sent" x="98" y="330">Sent</text>',
    slot(EMAIL_INBOX, 26, 346, anchor="start"),
    '<text class="sub" x="355" y="298" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="355" y="312" text-anchor="middle">named in [mail] gateway</text>',
]), DEFAULT))
wide_groups.append(group("email", "\n".join([
    house(*W_HOUSE, "e-gw", "M0GWY", "mail gateway"),
    hub(**W_HUB),
    seg("e-gw-hub", "net", GW_TO_HUB),
    seg("e-hub-mail", "net", HUB_TO_END),
    TLS_GW,
    '<g data-node="e-mail">',
    '  <rect class="box accent" x="640" y="158" width="140" height="44" />',
    '  <path class="wire" d="M650 168 H672 V186 H650 Z M650 168 L661 178 L672 168" />',
    '  <text class="on-accent" x="682" y="177">Resend</text>',
    '  <text class="sub" x="682" y="193">email service</text>',
    '</g>',
    slot([("e-acc", "accepted", "scr ok"), ("e-re", "reply from alex")], 710, 222),
    '<text class="sub" x="710" y="238" text-anchor="middle">alex@example.com</text>',
    seg("e-a-gw", "rf", wave(W_TIP[0] + 4, W_TIP[1] - 1, W_MAST[0] - 4, W_MAST[1] + 1)),
    '<text class="sub on-rf" x="245" y="86" text-anchor="middle">144.950</text>',
    handheld(70, 150, "e-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="scr" data-show="e-tx" x="44" y="292">[tx]</text>',
    '<text class="scr ok" data-show="e-gw" x="80" y="292">[gw]</text>',
    '<text class="scr ok" data-show="e-sent" x="116" y="292">Sent</text>',
    slot(EMAIL_INBOX, 44, 308, anchor="start"),
    '<text class="sub" x="345" y="262" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="345" y="276" text-anchor="middle">named in [mail] gateway</text>',
]), DEFAULT))
steps["email"] = {"viewBox": "0 0 440 356", "viewBoxWide": "0 64 800 256", "steps": [
    {"say": "G4ABC writes to alex@example.com and presses Send. A confirm shows the size and airtime first.",
     "on": ["e-a"], "show": ["e-compose", "e-dlg-send"], "ms": 2800},
    {"say": "G4ABC confirms. The mail goes out as a burst on 144.950, and the Outbox shows [tx].",
     "segs": ["e-a-gw"], "on": ["e-a"], "show": ["e-tx"], "ms": 2000},
    {"say": "The gateway has every chunk and acks on the air. The Outbox shows [gw].",
     "segs": ["<e-a-gw"], "on": ["e-gw"], "show": ["e-gw"], "ms": 1900},
    {"say": "The gateway posts the mail to the hub.", "segs": ["e-gw-hub"], "on": ["e-gw"], "ms": 1300},
    {"say": "The hub hands it to Resend, the email service, which accepts it.",
     "segs": ["e-hub-mail"], "on": ["e-mail"], "show": ["e-acc"], "ms": 1600},
    {"say": "The gateway keys a CompleteAck. Only now is the mail Sent, and the form clears.",
     "segs": ["<e-gw-hub", "<e-a-gw"], "chain": True, "on": ["e-gw"], "show": ["e-sent"], "ms": 2600},
    {"say": "Alex replies. Resend passes it to the hub, which keeps it. Nothing goes on the air unasked.",
     "segs": ["<e-hub-mail"], "on": ["e-mail"], "show": ["e-re"], "ms": 2400},
    {"say": "Later, G4ABC presses Check mail.", "segs": ["e-a-gw"], "on": ["e-a"], "show": ["e-check"], "ms": 1700},
    {"say": "The gateway pulls G4ABC’s list from the hub and keys it back: one mail waiting, headers only.",
     "segs": ["e-gw-hub", "<e-gw-hub", "<e-a-gw"], "chain": True, "on": ["e-gw"], "show": ["e-inbox"], "ms": 3000},
    {"say": "G4ABC clicks the waiting line. Get mail? shows the size and airtime.",
     "on": ["e-a"], "show": ["e-dlg-get"], "ms": 2600},
    {"say": "G4ABC presses Get, and the request goes on the air.",
     "segs": ["e-a-gw"], "on": ["e-a"], "show": ["e-dlg-ask"], "ms": 1800},
    {"say": "The gateway fetches the mail from the hub.",
     "segs": ["e-gw-hub", "<e-gw-hub"], "chain": True, "on": ["e-gw"], "ms": 2000},
    {"say": "It keys the mail back in chunks.", "segs": ["<e-a-gw"], "on": ["e-gw"], "show": ["e-ch1"], "ms": 1700},
    {"say": "The last chunk lands.", "segs": ["<e-a-gw"], "on": ["e-gw"], "show": ["e-ch2"], "ms": 1500},
    {"say": "Fetched. The mail is in G4ABC’s Inbox.", "on": ["e-a"], "show": ["e-got"], "ms": 2000},
    {"say": "G4ABC presses OK and opens it. The reply reads in full, with no internet on this computer.",
     "on": ["e-a"], "show": ["e-open"], "ms": 3400},
], "done": "Mail goes out and comes back through the gateway named in [mail] gateway. Radio-plus never opens the hub itself."}


# --- App window: the chat (or Email) part of the app, filled in as the steps play ---
# Lines and marks carry the same data-show names as the drawings, so the steps that
# light the diagram also add lines here. A row of marks shows only the latest one.
MODE_CLASS = {"internet": "mode-internet", "internet-radio": "mode-internet-radio",
              "radio": "mode-radio", "radio-plus": "mode-radio-plus"}


def chat_line(show, time, nick, text, me=False, via="", ticks=()):
    via_html = ""
    if via:
        label = "net" if via == "inet" else via
        via_html = f' <span class="via via-{via}">[{label}]</span>'
    tick_html = ""
    if ticks:
        spans = "".join(
            f'<span data-show="{name}"' + (' class="tick-ok"' if mark == "[ok]" else "") + f">{mark}</span>"
            for name, mark in ticks)
        tick_html = f' <span class="ticks">{spans}</span>'
    nick_cls = "nick me" if me else "nick"
    return (f'<li data-show="{show}"><time>{time}</time> <span class="{nick_cls}">[{nick}]</span> '
            f"{text}{via_html}{tick_html}</li>")


def app_window(setup, me, mode, dial, side, lines, stations=(),
               hint="message in this channel, or /join /invite /part", buttons=None):
    off = "" if setup == DEFAULT else " is-off"
    who = f"{me} · {mode}" + (f" · {dial}" if dial else "")
    side_items = "".join(("<li class=\"is-active\">" if active else "<li>") + f"{name}</li>"
                         for name, active in side)
    if stations:
        side_items += '<li class="app-win__label">stations</li>' + "".join(f"<li>{st}</li>" for st in stations)
    body = "\n".join(f"                    {line}" for line in lines)
    return "\n".join([
        f'              <div class="app-win {MODE_CLASS[mode]}{off}" data-setups="{setup}">',
        f'                <p class="app-win__bar"><span class="app-win__dot" aria-hidden="true"></span><b>{who}</b><span>WeeChat Radio</span></p>',
        '                <div class="app-win__body">',
        f'                  <ul class="app-win__side" aria-hidden="true">{side_items}</ul>',
        f'                  <ol class="app-win__lines" aria-label="{me}’s chat">',
        body,
        '                  </ol>',
        (f'                  <p class="app-win__input" aria-hidden="true">{buttons}</p>' if buttons else
         f'                  <p class="app-win__input" aria-hidden="true"><span class="app-win__sym">$</span><span class="app-win__field">{hint}</span><span class="app-win__send">send</span></p>'),
        '                </div>',
        '              </div>',
    ])




# --- Email window: the app's Email tab for the email setup ---
MAIL_OUT = "Meet at the car park at 4?"
MAIL_IN = "Car park at 4 works. I will bring the map and a flask. If it rains, the cafe instead. Alex"


def mail_window():
    """The Email tab: folder tabs, a notice, the mail list, an opened mail, compose, and dialogs.
    Each part is a .slot, so it shows the latest state the steps have reached."""
    def tabs(on, waiting):
        names = "".join(f'<span class="app-mail__tab{" is-cur" if n == on else ""}">{n}</span>'
                        for n in ("Inbox", "Outbox", "Sent", "Drafts"))
        return (f'{names}<span class="app-mail__dim">waiting {waiting}</span>'
                '<span class="app-win__send app-win__send--alt">Check mail</span>')

    def buttons(*names):
        btns = "".join(f'<span class="app-win__send{" app-win__send--alt" if i == 0 else ""}">{n}</span>'
                       for i, n in enumerate(names))
        return f'<span class="app-mail__btns">{btns}</span>'

    def progress(pct, label, title="Getting email", done=False):
        return (f'<b>{title}</b><span class="app-mail__acc">alex@example.com  Re: Meeting point</span>'
                f'<span class="app-mail__bar"><i style="width:{pct}%"></i><em>{pct}%</em></span>'
                f'<span>{label}</span>' + buttons("OK" if done else "Hide"))

    def compose(to, subject, body):
        return (f'<span class="app-mail__lbl">To</span><span class="app-mail__field">{to}&nbsp;</span>'
                f'<span class="app-mail__lbl">Subject</span><span class="app-mail__field">{subject}&nbsp;</span>'
                f'<span class="app-mail__field app-mail__body">{body}&nbsp;</span>' + buttons("Send"))

    def out_row(show, mark, tail=""):
        return f'<p data-show="{show}">19:20:05  {mark}  alex@example.com  Meeting point{tail}</p>'

    in_row = "19:48:10        alex@example.com  Re: Meeting point"
    off = "" if DEFAULT == "email" else " is-off"
    return "\n".join([
        f'              <div class="app-win app-win--mail mode-radio-plus{off}" data-setups="email">',
        '                <p class="app-win__bar"><span class="app-win__dot" aria-hidden="true"></span>'
        '<b>G4ABC · radio-plus · 144.950</b><span>WeeChat Radio</span></p>',
        '                <div class="app-mail" role="group" aria-label="G4ABC’s Email tab">',
        '                  <p class="app-mail__head"><b>EMAIL</b>  G4ABC@mail.weechatradio.com</p>',
        '                  <p class="app-mail__tabs slot">'
        f'<span data-show="e-compose">{tabs("Outbox", 0)}</span>'
        f'<span data-show="e-sent">{tabs("Sent", 0)}</span>'
        f'<span data-show="e-inbox">{tabs("Inbox", 1)}</span>'
        f'<span data-show="e-got">{tabs("Inbox", 0)}</span></p>',
        '                  <p class="app-mail__toast slot">'
        '<span data-show="e-check">Checking…</span>'
        '<span data-show="e-inbox">1 mail waiting at the gateway. Click a line to Get it.</span>'
        '<span data-show="e-got"></span></p>',
        '                  <div class="app-mail__rows slot">'
        + out_row("e-tx", "[tx]")
        + out_row("e-gw", "[gw]", "  at gateway")
        + out_row("e-sent", '<span class="tick-ok">[ok]</span>', "  with Resend")
        + f'<p data-show="e-inbox"><span class="app-mail__dim">waiting</span>  alex@example.com  Re: Meeting point  {len(MAIL_IN)} B</p>'
        + f'<p data-show="e-got">{in_row}</p>'
        + f'<p data-show="e-open" class="is-cur">{in_row}</p></div>',
        '                  <div class="app-mail__open">'
        '<div data-show="e-open"><span class="app-mail__acc">19:48:10  alex@example.com → G4ABC@mail.weechatradio.com</span>'
        f'<span class="app-mail__acc">Re: Meeting point</span><span>{MAIL_IN}</span></div></div>',
        '                  <div class="app-mail__compose slot">'
        f'<div data-show="e-compose">{compose("alex@example.com", "Meeting point", MAIL_OUT)}</div>'
        f'<div data-show="e-sent">{compose("", "", "")}</div></div>',
        '                  <div class="app-mail__dlg slot">'
        '<div data-show="e-dlg-send"><b>Send on the radio?</b><span>To  alex@example.com</span><span>Subject  Meeting point</span>'
        f'<span class="app-mail__dim">{len(MAIL_OUT)} / 4096 B</span><span>radio-plus  vhf-fm  144.950</span>'
        '<span>About 4.2 s on the air (1 burst @ vhf-fm). Not Sent until the gateway accepts it.</span>'
        + buttons("Send", "Cancel") + '</div>'
        '<div data-show="e-tx"></div>'
        '<div data-show="e-dlg-get"><b>Get mail?</b><span class="app-mail__acc">alex@example.com  Re: Meeting point</span>'
        f'<span class="app-mail__dim">{len(MAIL_IN)} / 4096 B</span><span>radio-plus  vhf-fm  144.950</span>'
        '<span>About 9 s on the air (2 bursts @ vhf-fm).</span>'
        + buttons("Get", "Cancel") + '</div>'
        f'<div data-show="e-dlg-ask">{progress(10, "Asking the gateway…")}</div>'
        f'<div data-show="e-ch1">{progress(52, "Chunks received 1 of 2")}</div>'
        f'<div data-show="e-ch2">{progress(95, "Chunks received 2 of 2")}</div>'
        f'<div data-show="e-got">{progress(100, "Fetched. It is in your Inbox.", title="Fetched", done=True)}</div>'
        '<div data-show="e-open"></div></div>',
        '                </div>',
        '              </div>',
    ])


app_windows = "\n".join([
    app_window("home", "G4ABC", "radio-plus", "144.950",
               [("#bulletin", False), ("#walk", True)], [
                   chat_line("h1", "10:02:15", "G4ABC", "at the trailhead", me=True,
                             ticks=[("h1", "[tx]"), ("h1gw", "[gw]")]),
                   chat_line("h2", "10:03:40", "G4XYZ", "windy up there?", via="rf"),
                   chat_line("h3", "10:04:02", "G4ABC", "a bit. back by 4", me=True,
                             ticks=[("h3", "[tx]"), ("h3gw", "[gw]")]),
                   chat_line("h4", "10:05:31", "G4XYZ", "kettle on at 4", via="rf"),
               ], stations=["M0GWY [2m]"]),
    app_window("hf", "G4ABC", "radio", "14.070",
               [("#bulletin", False), ("EA4XYZ", True)], [
                   chat_line("f1", "19:02:11", "G4ABC", "good evening", me=True,
                             ticks=[("f1", "[tx]"), ("f1ok", "[ok]")]),
                   chat_line("f2", "19:03:05", "EA4XYZ", "hola, copy fine", via="rf"),
                   chat_line("f3", "19:03:48", "G4ABC", "73, same time tmrw?", me=True,
                             ticks=[("f3", "[tx]"), ("f3ok", "[ok]")]),
                   chat_line("f4", "19:04:30", "EA4XYZ", "si, 73", via="rf"),
               ], stations=["EA4XYZ [20m]"]),
    app_window("guest", "~sam", "internet", "",
               [("#bulletin", False), ("G4ABC", True)], [
                   chat_line("s1", "11:30:02", "~sam", "on the hill yet?", me=True),
                   chat_line("s2", "11:30:41", "G4ABC", "yes, at the top", via="inet"),
                   chat_line("s3", "11:31:10", "~sam", "photo later?", me=True),
                   chat_line("s4", "11:31:52", "G4ABC", "will email it", via="inet"),
               ]),
    app_window("hops", "G4ABC", "radio", "144.950",
               [("#bulletin", False), ("G4MNO", True)], [
                   chat_line("m1", "12:00:03", "G4ABC", "hi G4MNO, copy?", me=True,
                             ticks=[("m1", "[tx]"), ("m1rl", "[rl]"), ("m1ok", "[rl] [ok]")]),
                   chat_line("m2", "12:01:20", "G4MNO", "copy, via two relays", via="rf"),
               ], stations=["G4DEF [2m]"]),
    mail_window(),
])


svg_body = "\n".join(groups)
svg_body_wide = "\n".join(wide_groups)
steps_json = json.dumps(steps, ensure_ascii=False, indent=1)

PANELS = """
              <div class="setup-panel" data-setup-panel="home">
                <h3>A walkie-talkie calling a home gateway</h3>
                <div class="setup-panel__main">
                  <p>G4ABC is out walking with a 2m handheld on 144.950, linked to a computer running WeeChat Radio in <code>radio-plus</code>. At home, a base station on a roof antenna runs <code>internet-radio</code> on the same dial.</p>
                  <p>The home gateway carries G4ABC’s lines to the hub for people with no radio, like G4XYZ, and puts their replies back on the air. Anyone else on 144.950 hears the chat too.</p>
                  <p class="setup-note">The gateway only puts a hub line on the air when the station, or a member of the group, was heard on this dial in the last 10 minutes. A base station with an outdoor antenna hears a handheld much further away than another handheld would.</p>
                </div>
                <dl class="who-list">
                  <div class="mode-radio-plus"><dt>G4ABC<small>radio-plus</small></dt><dd>Handheld on 144.950 FM. The computer never opens the hub.</dd></div>
                  <div class="mode-internet-radio"><dt>M0GWY<small>internet-radio</small></dt><dd>Base station at home on 144.950, with <code>rf_egress = true</code>.</dd></div>
                  <div class="mode-internet"><dt>G4XYZ<small>internet</small></dt><dd>No radio. Chats through the hub.</dd></div>
                </dl>
              </div>
              <div class="setup-panel" data-setup-panel="hf">
                <h3>HF to HF, no internet</h3>
                <div class="setup-panel__main">
                  <p>Two stations far apart, both on 14.070 MHz USB in <code>radio</code> mode. The line goes out on the air and the receipt comes back on the air. No hub, no gateway and no internet connection are involved.</p>
                  <p class="setup-note">In <code>radio</code> mode the frame says do not forward, so a gateway that hears it leaves it on the air. Use <code>radio-plus</code> instead if a gateway may carry it to the internet.</p>
                </div>
                <dl class="who-list">
                  <div class="mode-radio"><dt>G4ABC<small>radio</small></dt><dd>14.070 USB, 20m. Preset <code>hf-poor</code> on a rough path.</dd></div>
                  <div class="mode-radio"><dt>EA4XYZ<small>radio</small></dt><dd>Same dial, same preset.</dd></div>
                </dl>
              </div>
              <div class="setup-panel" data-setup-panel="guest">
                <h3>An internet guest through a gateway</h3>
                <div class="setup-panel__main">
                  <p>~sam has no radio and no licence, and joins with a guest name. They can still talk to G4ABC on the air, because a gateway on G4ABC’s dial puts hub lines on the air.</p>
                  <p class="setup-note">Guest names starting with <code>~</code> need <code>third_party = "allow"</code> on the gateway. The default is <code>deny</code>, and then the gateway stays quiet.</p>
                </div>
                <dl class="who-list">
                  <div class="mode-internet"><dt>~sam<small>internet</small></dt><dd>Guest name. Chats through the hub.</dd></div>
                  <div class="mode-internet-radio"><dt>M0GWY<small>internet-radio</small></dt><dd>144.950, <code>rf_egress = true</code>, <code>third_party = "allow"</code>.</dd></div>
                  <div class="mode-radio-plus"><dt>G4ABC<small>radio-plus</small></dt><dd>144.950. Heard by the gateway in the last 10 minutes.</dd></div>
                </dl>
              </div>
              <div class="setup-panel" data-setup-panel="hops">
                <h3>Relay with hops</h3>
                <div class="setup-panel__main">
                  <p>Four stations on one dial, each only in reach of its neighbours. A station that hears a message for the first time shows it, takes one off the hop count, and sends it on while hops remain. The receipt comes back the same way.</p>
                  <p class="setup-note">A direct message starts with 3 hops left. Start a line with <code>!</code> for 4, or <code>!!</code> in an emergency for 5. <code>#bulletin</code> and groups start with 6.</p>
                </div>
                <dl class="who-list">
                  <div class="mode-radio"><dt>G4ABC … G4MNO<small>radio</small></dt><dd>All on 144.950 FM. No setting to turn on.</dd></div>
                </dl>
              </div>
              <div class="setup-panel" data-setup-panel="email">
                <h3>Email over radio</h3>
                <div class="setup-panel__main">
                  <p>G4ABC has no internet on its computer, but can still send and fetch email as <code>G4ABC@mail.weechatradio.com</code>. The mail goes on the air to the gateway named in <code>[mail] gateway</code>, and that station posts it.</p>
                  <p class="setup-note">Sent means the email service accepted it, not that the gateway heard it. Guests (<code>~</code> names) and <code>radio</code> mode have no email.</p>
                </div>
                <dl class="who-list">
                  <div class="mode-radio-plus"><dt>G4ABC<small>radio-plus</small></dt><dd><code>[mail] gateway</code> set to M0GWY. Uses the Email tab.</dd></div>
                  <div class="mode-internet-radio"><dt>M0GWY<small>internet-radio</small></dt><dd>Posts radio-plus mail it hears to the hub.</dd></div>
                </dl>
              </div>"""

HTML = f"""<!doctype html>
<!-- Generated by scripts/gen_scenarios.py. Edit that script and re-run it; do not edit this file by hand. -->
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <meta name="theme-color" content="#07070a" />
  <meta name="apple-mobile-web-app-capable" content="yes" />
  <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
  <meta name="mobile-web-app-capable" content="yes" />
  <title>Common setups — WeeChat Radio</title>
  <meta name="description" content="Five everyday WeeChat Radio setups, animated: a walkie-talkie calling a home gateway, HF to HF with no internet, an internet guest, relays with hops, and email over radio." />
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content="WeeChat Radio" />
  <meta property="og:title" content="Common setups — WeeChat Radio" />
  <meta property="og:description" content="Five everyday setups, animated: a home gateway, HF to HF, an internet guest, relays with hops, and email over radio." />
  <meta property="og:url" content="https://weechatradio.com/docs/scenarios.html" />
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
  <link rel="stylesheet" href="/docs.css?v=3" />
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
      <a href="/docs.html" aria-current="page">docs</a>
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
      <p class="crumb"><a href="/docs.html">docs</a> / common setups</p>
      <h1><span class="who">weechat-radio</span><span class="sym">:$</span> common setups</h1>
      <p class="lead">Five ways people use WeeChat Radio day to day. Pick one to watch a message travel, then see what each station is set to.</p>

      <section class="article-section article-section--first" aria-labelledby="setupsH">
        <h2 id="setupsH" class="sr-only">Setups</h2>
        <figure class="diagram live-dgm live-dgm--wide" data-live-dgm data-setup="{DEFAULT}">
          <div class="setup-switch setup-switch--fill" role="group" aria-label="Setup" data-setup-switch hidden>
            <button type="button" data-setup-btn="home" aria-pressed="true">home gateway</button>
            <button type="button" data-setup-btn="hf" aria-pressed="false">HF to HF</button>
            <button type="button" data-setup-btn="guest" aria-pressed="false">internet guest</button>
            <button type="button" data-setup-btn="hops" aria-pressed="false">relay with hops</button>
            <button type="button" data-setup-btn="email" aria-pressed="false">email</button>
          </div>
          <div class="live-dgm__body">
            <div class="live-dgm__pic">
              <div class="dgm-wrap">
                <svg class="dgm" data-layout="wide" viewBox="{steps[DEFAULT]['viewBoxWide']}" role="img" aria-labelledby="scTw scDw">
                  <title id="scTw">Common WeeChat Radio setups</title>
                  <desc id="scDw">One picture per setup. Walkie-talkies on 144.950 in radio-plus mode reach a gateway at home that links them to the hub and an internet station. Two HF stations on 14.070 talk with no internet. An internet guest reaches a radio station through a gateway. Four stations relay a message hop by hop. A radio-plus station sends email through a mail gateway.</desc>
{svg_body_wide}
                </svg>
                <svg class="dgm" data-layout="narrow" viewBox="{steps[DEFAULT]['viewBox']}" role="img" aria-labelledby="scT scD">
                  <title id="scT">Common WeeChat Radio setups</title>
                  <desc id="scD">One picture per setup. Walkie-talkies on 144.950 in radio-plus mode reach a gateway at home that links them to the hub and an internet station. Two HF stations on 14.070 talk with no internet. An internet guest reaches a radio station through a gateway. Four stations relay a message hop by hop. A radio-plus station sends email through a mail gateway.</desc>
{svg_body}
                </svg>
              </div>
              <div class="live-status">
                <p class="live-status__text" data-live-status aria-live="polite"></p>
                <button type="button" class="btn-replay" data-live-replay hidden>Replay</button>
              </div>
{app_windows}
            </div>
            <div class="live-dgm__text">{PANELS}
            </div>
          </div>
          <script type="application/json" data-live-steps>
{steps_json}
          </script>
        </figure>
      </section>

      <section class="article-section" aria-labelledby="pickH">
        <h2 id="pickH">Which one fits</h2>
        <ul class="cards">
          <li><strong>You have handhelds and one home station</strong><span>Home gateway. Walkers stay in <code>radio-plus</code>; the home station runs <code>internet-radio</code>.</span></li>
          <li><strong>You only want the air</strong><span>HF to HF, or any two radios on one dial in <code>radio</code> mode.</span></li>
          <li><strong>A friend has no radio</strong><span>Internet guest, through a gateway that allows guests.</span></li>
          <li><strong>Stations are out of reach of each other</strong><span>Relay with hops on one dial.</span></li>
          <li><strong>You need email with no internet</strong><span>Email over radio, through a mail gateway.</span></li>
        </ul>
      </section>

      <p class="next">More detail: <a href="/docs/modes.html">modes</a>, <a href="/docs/bridging.html">cross-frequency chat</a>, <a href="/docs/hops.html">hops</a>, <a href="/docs/email.html">email</a>. Worked examples: <a href="/docs/mixed-band.html">one group across PMR, CB, 2m and internet</a>, <a href="/docs/try.html">try it with two computers</a>.</p>
    </main>
  </div>
  <script src="/copy.js?v=1"></script>
  <script src="/shell.js?v=10"></script>
  <script src="/live-dgm.js?v=3"></script>
</body>
</html>
"""

with open(OUT, "w", encoding="utf-8") as fh:
    fh.write(HTML)
print("wrote", OUT, len(HTML))
