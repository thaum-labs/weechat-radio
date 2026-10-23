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


DEFAULT = "home"
groups = []
steps = {}

# 1. Walkie-talkies calling a home gateway -----------------------------------
A_TIP, B_TIP, MAST = (70, 114), (160, 174), (390, 130)
body = "\n".join([
    '<rect class="lane-bg" x="10" y="92" width="232" height="266" />',
    '<text class="sub" x="20" y="108">out walking · 144.950 FM</text>',
    laptop(20, 14, "home-x", "G4XYZ · internet", [
        ("hx-hi", "scr", "&lt;G4ABC&gt; hi #walk"),
        ("hx-re", "scr", "&lt;G4XYZ&gt; hello"),
    ]),
    hub(),
    seg("home-hub-x", "net", "M250 48 H162"),
    '<text class="sub" x="206" y="42" text-anchor="middle">TLS</text>',
    house(280, 160, "home-gw", "M0GWY", "home gateway"),
    seg("home-gw-hub", "net", "M415 192 V72"),
    '<text class="sub" x="409" y="104" text-anchor="end">TLS</text>',
    seg("home-rf-a", "rf", wave(A_TIP[0] + 4, A_TIP[1] - 1, MAST[0] - 4, MAST[1] + 1)),
    seg("home-rf-b", "rf", wave(B_TIP[0] + 4, B_TIP[1] - 1, MAST[0] - 4, MAST[1] + 2)),
    seg("home-rf-ab", "rf", wave(A_TIP[0] + 2, A_TIP[1] + 4, B_TIP[0] - 2, B_TIP[1] - 4, amp=4, half=9)),
    handheld(30, 150, "home-a", "G4ABC", "on-rplus", "radio-plus"),
    handheld(120, 210, "home-b", "G4DEF", "on-rplus", "radio-plus"),
    '<text class="scr ok" data-show="ha-gw" x="16" y="288">[gw]</text>',
    '<text class="scr" data-show="ha-re" x="52" y="288">“hello”</text>',
    '<text class="scr" data-show="hb-hi" x="112" y="348">“hi”</text>',
    '<text class="scr" data-show="hb-re" x="146" y="348">“hello”</text>',
    '<text class="sub" x="355" y="298" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="355" y="312" text-anchor="middle">rf_egress = true</text>',
])
groups.append(group("home", body, DEFAULT))
steps["home"] = {"viewBox": "0 0 440 362", "steps": [
    {"say": "G4ABC says hi to #walk. Its computer keys the radio on 144.950 and does not open the hub.",
     "segs": ["home-rf-a", "home-rf-ab"], "on": ["home-a"], "ms": 1800},
    {"say": "G4DEF hears it on the air and shows the line.", "show": ["hb-hi"], "ms": 1200},
    {"say": "The gateway at home hears a radio-plus line and sends it to the hub. G4ABC’s line shows [gw].",
     "segs": ["home-gw-hub"], "on": ["home-gw"], "show": ["ha-gw"], "ms": 1800},
    {"say": "The hub delivers it to G4XYZ, who has no radio.", "segs": ["home-hub-x"], "show": ["hx-hi"]},
    {"say": "G4XYZ replies to #walk through the hub.", "segs": ["<home-hub-x"], "show": ["hx-re"]},
    {"say": "The hub hands the reply to the gateway, which heard #walk members on this dial in the last 10 minutes.",
     "segs": ["<home-gw-hub"], "ms": 1900},
    {"say": "With rf_egress = true, the gateway keys the reply on 144.950. Both walkers read it.",
     "segs": ["<home-rf-a", "<home-rf-b"], "on": ["home-gw"], "show": ["ha-re", "hb-re"], "ms": 2000},
], "done": "Everyone on the dial, and G4XYZ on the internet, reads the same #walk thread."}

# 2. HF to HF, no internet ----------------------------------------------------
body = "\n".join([
    '<text class="sub on-rf" x="255" y="34" text-anchor="middle">14.070 USB · 20m</text>',
    seg("hf-rf", "rf", wave(134, 58, 376, 58, amp=6, half=11)),
    house(20, 88, "hf-a", "G4ABC", "radio", body="radio", text_cls="on-radio"),
    house(270, 88, "hf-b", "EA4XYZ", "radio", body="radio", text_cls="on-radio"),
    '<text class="sub" x="95" y="228" text-anchor="middle">preset hf-poor</text>',
    '<text class="sub" x="345" y="228" text-anchor="middle">preset hf-poor</text>',
    '<text class="scr ok" data-show="hfa-ok" x="95" y="248" text-anchor="middle">&lt;G4ABC&gt; hi [ok]</text>',
    '<text class="scr" data-show="hfb-hi" x="345" y="248" text-anchor="middle">&lt;G4ABC&gt; hi</text>',
    '<text class="sub" x="220" y="186" text-anchor="middle">no hub</text>',
    '<text class="sub" x="220" y="200" text-anchor="middle">no gateway</text>',
])
groups.append(group("hf", body, DEFAULT))
steps["hf"] = {"viewBox": "0 0 440 262", "steps": [
    {"say": "G4ABC keys a line on 14.070. In radio mode it stays on the air: nothing goes to the hub.",
     "segs": ["hf-rf"], "on": ["hf-a"], "ms": 1900},
    {"say": "EA4XYZ decodes it and shows the line.", "show": ["hfb-hi"], "ms": 1300},
    {"say": "EA4XYZ sends its receipt back on the same frequency.", "segs": ["<hf-rf"], "on": ["hf-b"], "ms": 1900},
    {"say": "G4ABC shows [ok]. A gateway that heard it would leave it on the air.", "show": ["hfa-ok"]},
], "done": "Two stations, one frequency, no internet anywhere. G4ABC shows [ok]."}

# 3. Internet guest via a gateway ---------------------------------------------
G_TIP = (100, 154)
body = "\n".join([
    laptop(20, 14, "g-sam", "~sam · internet", [
        ("gs-out", "scr", "&lt;~sam&gt; hi G4ABC"),
        ("gs-re", "scr", "&lt;G4ABC&gt; hi ~sam"),
    ]),
    hub(sub="picks the gateway"),
    seg("g-sam-hub", "net", "M162 48 H250"),
    '<text class="sub" x="206" y="42" text-anchor="middle">TLS</text>',
    house(280, 160, "g-gw", "M0GWY", "home gateway"),
    seg("g-hub-gw", "net", "M415 72 V192"),
    '<text class="sub" x="409" y="104" text-anchor="end">TLS</text>',
    seg("g-gw-a", "rf", wave(MAST[0] - 4, MAST[1] + 1, G_TIP[0] + 4, G_TIP[1] - 1)),
    '<text class="sub on-rf" x="236" y="128" text-anchor="middle">144.950</text>',
    handheld(60, 190, "g-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="scr" data-show="ga-hi" x="84" y="330" text-anchor="middle">&lt;~sam&gt; hi G4ABC</text>',
    '<text class="scr ok" data-show="g-chk" x="355" y="298" text-anchor="middle">rf_egress = true</text>',
    '<text class="scr ok" data-show="g-chk" x="355" y="314" text-anchor="middle">G4ABC heard here</text>',
    '<text class="scr ok" data-show="g-chk" x="355" y="330" text-anchor="middle">third_party = "allow"</text>',
])
groups.append(group("guest", body, DEFAULT))
steps["guest"] = {"viewBox": "0 0 440 342", "steps": [
    {"say": "~sam has no radio and no callsign. They message G4ABC through the hub.",
     "segs": ["g-sam-hub"], "show": ["gs-out"]},
    {"say": "The hub picks the gateway that has heard G4ABC: M0GWY at home.", "segs": ["g-hub-gw"]},
    {"say": "M0GWY checks: rf_egress is on, G4ABC was heard on this dial in the last 10 minutes, and guests are allowed.",
     "on": ["g-gw"], "show": ["g-chk"], "ms": 2400},
    {"say": "All three pass, so M0GWY keys the line on 144.950. G4ABC reads it.",
     "segs": ["g-gw-a"], "on": ["g-gw"], "show": ["ga-hi"], "ms": 1900},
    {"say": "G4ABC replies on the air in radio-plus.", "segs": ["<g-gw-a"], "on": ["g-a"], "ms": 1900},
    {"say": "The gateway hears a radio-plus line and sends it to the hub.", "segs": ["<g-hub-gw"]},
    {"say": "The hub delivers the reply to ~sam.", "segs": ["<g-sam-hub"], "show": ["gs-re"]},
], "done": "~sam and G4ABC chat through the gateway. With third_party = \"deny\", the default, the gateway would stay quiet."}

# 4. Relay with hops ----------------------------------------------------------
xs = [50, 165, 280, 395]
calls = ["G4ABC", "G4DEF", "G4JKL", "G4MNO"]
nodes = ["hp-a", "hp-b", "hp-c", "hp-d"]
tips = [(x + 12, 110) for x in xs]
parts = [
    '<text class="sub on-rf" x="220" y="22" text-anchor="middle">144.950 FM · one dial</text>',
    f'<circle class="lane-bg" cx="{tips[0][0]}" cy="{tips[0][1]}" r="150" />',
    f'<text class="sub" x="{tips[0][0] + 6}" y="44">G4ABC’s reach</text>',
]
for i in range(3):
    (x1, y1), (x2, y2) = tips[i], tips[i + 1]
    parts.append(seg(f"hp-{i + 1}", "rf", wave(x1 + 6, y1, x2 - 6, y2, amp=5, half=10.5)))
for x, call, node in zip(xs, calls, nodes):
    parts.append(handheld(x - 18, 140, node, call, "on-radio", "radio", body="radio", w=36, h=64, label_dy=16))
parts += [
    '<text class="scr" data-show="hp-a3" x="50" y="252" text-anchor="middle">3 hops left</text>',
    '<text class="scr ok" data-show="hp-aok" x="50" y="268" text-anchor="middle">[ok]</text>',
    '<text class="scr" data-show="hp-b2" x="165" y="252" text-anchor="middle">2 left</text>',
    '<text class="scr" data-show="hp-c1" x="280" y="252" text-anchor="middle">1 left</text>',
    '<text class="scr" data-show="hp-d0" x="395" y="252" text-anchor="middle">0 left</text>',
    '<text class="scr" data-show="hp-d0" x="395" y="268" text-anchor="middle">reads it</text>',
]
groups.append(group("hops", "\n".join(parts), DEFAULT))
steps["hops"] = {"viewBox": "0 0 440 280", "steps": [
    {"say": "G4ABC sends G4MNO a direct message with 3 hops left. Only G4DEF is in reach.",
     "segs": ["hp-1"], "on": ["hp-a"], "show": ["hp-a3"], "ms": 1800},
    {"say": "G4DEF hears it for the first time: it shows the line, takes one off, and sends it on with 2 left.",
     "segs": ["hp-2"], "on": ["hp-b"], "show": ["hp-b2"], "ms": 2000},
    {"say": "G4JKL does the same and sends it on with 1 left.", "segs": ["hp-3"], "on": ["hp-c"], "show": ["hp-c1"], "ms": 1800},
    {"say": "G4MNO reads it. Hops left is now 0, so it does not send it on.", "show": ["hp-d0"], "ms": 1500},
    {"say": "G4MNO’s receipt goes back with the same hop count.", "segs": ["<hp-3"], "on": ["hp-d"], "ms": 1700},
    {"say": "G4JKL relays the receipt.", "segs": ["<hp-2"], "on": ["hp-c"], "ms": 1600},
    {"say": "G4DEF relays it to G4ABC.", "segs": ["<hp-1"], "on": ["hp-b"], "ms": 1600},
    {"say": "G4ABC shows [ok]. It never heard G4MNO directly.", "show": ["hp-aok"]},
], "done": "G4ABC shows [ok]. It never heard G4MNO directly: two stations in between relayed it."}

# 5. Email over radio ---------------------------------------------------------
body = "\n".join([
    '<g data-node="e-mail">',
    '  <rect class="box accent" x="20" y="26" width="140" height="44" />',
    '  <path class="wire" d="M30 36 H52 V54 H30 Z M30 36 L41 46 L52 36" />',
    '  <text class="on-accent" x="62" y="45">Resend</text>',
    '  <text class="sub" x="62" y="61">email service</text>',
    '</g>',
    '<text class="scr ok" data-show="e-acc" x="90" y="88" text-anchor="middle">accepted</text>',
    '<text class="sub" x="90" y="104" text-anchor="middle">to alex@example.com</text>',
    hub(),
    seg("e-hub-mail", "net", "M250 48 H162"),
    house(280, 160, "e-gw", "M0GWY", "mail gateway"),
    seg("e-gw-hub", "net", "M415 192 V72"),
    '<text class="sub" x="409" y="104" text-anchor="end">TLS</text>',
    seg("e-a-gw", "rf", wave(G_TIP[0] + 4, G_TIP[1] - 1, MAST[0] - 4, MAST[1] + 1)),
    '<text class="sub on-rf" x="236" y="128" text-anchor="middle">144.950</text>',
    handheld(60, 190, "e-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="scr" data-show="e-tx" x="26" y="330">[tx]</text>',
    '<text class="scr ok" data-show="e-gw" x="62" y="330">[gw]</text>',
    '<text class="scr ok" data-show="e-sent" x="98" y="330">Sent</text>',
    '<text class="scr" data-show="e-inbox" x="26" y="346">inbox: 1 new mail</text>',
    '<text class="sub" x="355" y="298" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="355" y="312" text-anchor="middle">named in [mail] gateway</text>',
])
groups.append(group("email", body, DEFAULT))
steps["email"] = {"viewBox": "0 0 440 356", "steps": [
    {"say": "G4ABC writes to alex@example.com and keys the mail as a burst on 144.950. The outbox shows [tx].",
     "segs": ["e-a-gw"], "on": ["e-a"], "show": ["e-tx"], "ms": 1900},
    {"say": "The gateway has every chunk and acks on the air. The outbox shows [gw].",
     "segs": ["<e-a-gw"], "on": ["e-gw"], "show": ["e-gw"], "ms": 1900},
    {"say": "The gateway posts the mail to the hub.", "segs": ["e-gw-hub"]},
    {"say": "The hub hands it to Resend, the email service, which accepts it.", "segs": ["e-hub-mail"], "show": ["e-acc"]},
    {"say": "The gateway keys a CompleteAck. Only now does G4ABC’s mail show Sent.",
     "segs": ["<e-gw-hub", "<e-a-gw"], "chain": True, "on": ["e-gw"], "show": ["e-sent"], "ms": 2400},
    {"say": "Later, G4ABC presses Check mail. Nothing comes over the air unless it asks.",
     "segs": ["e-a-gw"], "on": ["e-a"], "ms": 1900},
    {"say": "The gateway pulls G4ABC’s inbox from the hub and sends the list back on the air.",
     "segs": ["e-gw-hub", "<e-gw-hub", "<e-a-gw"], "chain": True, "on": ["e-gw"], "show": ["e-inbox"], "ms": 3200},
], "done": "Mail goes out and comes in through the gateway named in [mail] gateway. Radio-plus never opens the hub itself."}


# --- Wide drawings (desktop) -------------------------------------------------
# Same ids as the narrow drawings, laid out left to right in an 800-wide box:
# radios on the left, the gateway at home, the hub, then the internet end on the right.
W_HOUSE = (270, 120)
W_MAST = (W_HOUSE[0] + 110, W_HOUSE[1] - 30)   # top of the house mast
W_HUB = dict(x=480, y=158, w=130, h=44)
GW_TO_HUB, HUB_TO_GW = "M420 180 H480", "M480 180 H420"
HUB_TO_END, END_TO_HUB = "M610 180 H640", "M640 180 H610"
TLS_GW = '<text class="sub" x="450" y="172" text-anchor="middle">TLS</text>'
TLS_END = '<text class="sub" x="625" y="172" text-anchor="middle">TLS</text>'
W_TIP = (110, 114)                              # handheld(70, 150) antenna tip
wide_groups = []

# 1. home gateway
WA_TIP, WB_TIP = (76, 68), (172, 114)
wide_groups.append(group("home", "\n".join([
    '<rect class="lane-bg" x="16" y="40" width="214" height="262" />',
    '<text class="sub" x="26" y="58">out walking · 144.950 FM</text>',
    house(*W_HOUSE, "home-gw", "M0GWY", "home gateway"),
    hub(**W_HUB),
    seg("home-gw-hub", "net", GW_TO_HUB),
    seg("home-hub-x", "net", HUB_TO_END),
    TLS_GW, TLS_END,
    laptop(640, 145, "home-x", "G4XYZ · internet", [
        ("hx-hi", "scr", "&lt;G4ABC&gt; hi #walk"),
        ("hx-re", "scr", "&lt;G4XYZ&gt; hello"),
    ], w=130),
    seg("home-rf-a", "rf", wave(WA_TIP[0] + 4, WA_TIP[1] - 1, W_MAST[0] - 4, W_MAST[1] + 1)),
    seg("home-rf-b", "rf", wave(WB_TIP[0] + 4, WB_TIP[1] - 1, W_MAST[0] - 4, W_MAST[1] + 2)),
    seg("home-rf-ab", "rf", wave(WA_TIP[0] + 2, WA_TIP[1] + 4, WB_TIP[0] - 2, WB_TIP[1] - 4, amp=4, half=9)),
    handheld(36, 104, "home-a", "G4ABC", "on-rplus", "radio-plus"),
    handheld(132, 150, "home-b", "G4DEF", "on-rplus", "radio-plus"),
    '<text class="scr ok" data-show="ha-gw" x="24" y="246">[gw]</text>',
    '<text class="scr" data-show="ha-re" x="60" y="246">“hello”</text>',
    '<text class="scr" data-show="hb-hi" x="120" y="292">“hi”</text>',
    '<text class="scr" data-show="hb-re" x="154" y="292">“hello”</text>',
    '<text class="sub" x="345" y="262" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="345" y="276" text-anchor="middle">rf_egress = true</text>',
]), DEFAULT))
steps["home"]["viewBoxWide"] = "0 24 800 290"

# 2. HF to HF
wide_groups.append(group("hf", "\n".join([
    '<text class="sub on-rf" x="435" y="46" text-anchor="middle">14.070 USB · 20m</text>',
    house(60, 100, "hf-a", "G4ABC", "radio", body="radio", text_cls="on-radio"),
    house(590, 100, "hf-b", "EA4XYZ", "radio", body="radio", text_cls="on-radio"),
    seg("hf-rf", "rf", wave(174, 70, 696, 70, amp=6, half=11)),
    '<text class="sub" x="435" y="168" text-anchor="middle">no hub</text>',
    '<text class="sub" x="435" y="184" text-anchor="middle">no gateway</text>',
    '<text class="sub" x="135" y="248" text-anchor="middle">preset hf-poor</text>',
    '<text class="sub" x="665" y="248" text-anchor="middle">preset hf-poor</text>',
    '<text class="scr ok" data-show="hfa-ok" x="135" y="268" text-anchor="middle">&lt;G4ABC&gt; hi [ok]</text>',
    '<text class="scr" data-show="hfb-hi" x="665" y="268" text-anchor="middle">&lt;G4ABC&gt; hi</text>',
]), DEFAULT))
steps["hf"]["viewBoxWide"] = "0 28 800 256"

# 3. internet guest
wide_groups.append(group("guest", "\n".join([
    house(*W_HOUSE, "g-gw", "M0GWY", "home gateway"),
    hub(**W_HUB, sub="picks the gateway"),
    seg("g-hub-gw", "net", HUB_TO_GW),
    seg("g-sam-hub", "net", END_TO_HUB),
    TLS_GW, TLS_END,
    laptop(640, 145, "g-sam", "~sam · internet", [
        ("gs-out", "scr", "&lt;~sam&gt; hi G4ABC"),
        ("gs-re", "scr", "&lt;G4ABC&gt; hi ~sam"),
    ], w=130),
    seg("g-gw-a", "rf", wave(W_MAST[0] - 4, W_MAST[1] + 1, W_TIP[0] + 4, W_TIP[1] - 1)),
    '<text class="sub on-rf" x="245" y="86" text-anchor="middle">144.950</text>',
    handheld(70, 150, "g-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="scr" data-show="ga-hi" x="94" y="292" text-anchor="middle">&lt;~sam&gt; hi G4ABC</text>',
    '<text class="scr ok" data-show="g-chk" x="345" y="262" text-anchor="middle">rf_egress = true</text>',
    '<text class="scr ok" data-show="g-chk" x="345" y="278" text-anchor="middle">G4ABC heard here</text>',
    '<text class="scr ok" data-show="g-chk" x="345" y="294" text-anchor="middle">third_party = "allow"</text>',
]), DEFAULT))
steps["guest"]["viewBoxWide"] = "0 64 800 242"

# 4. relay with hops
wxs = [100, 300, 500, 700]
wtips = [(x + 12, 94) for x in wxs]
parts = [
    '<text class="sub on-rf" x="400" y="22" text-anchor="middle">144.950 FM · one dial</text>',
    f'<circle class="lane-bg" cx="{wtips[0][0]}" cy="{wtips[0][1]}" r="260" />',
    f'<text class="sub" x="{wtips[0][0] + 12}" y="44">G4ABC’s reach</text>',
]
for i in range(3):
    (x1, y1), (x2, y2) = wtips[i], wtips[i + 1]
    parts.append(seg(f"hp-{i + 1}", "rf", wave(x1 + 6, y1, x2 - 6, y2, amp=5, half=10.5)))
for x, call, node in zip(wxs, calls, nodes):
    parts.append(handheld(x - 20, 130, node, call, "on-radio", "radio", body="radio", w=40, h=72, label_dy=16))
parts += [
    '<text class="scr" data-show="hp-a3" x="100" y="256" text-anchor="middle">3 hops left</text>',
    '<text class="scr ok" data-show="hp-aok" x="100" y="272" text-anchor="middle">[ok]</text>',
    '<text class="scr" data-show="hp-b2" x="300" y="256" text-anchor="middle">2 left</text>',
    '<text class="scr" data-show="hp-c1" x="500" y="256" text-anchor="middle">1 left</text>',
    '<text class="scr" data-show="hp-d0" x="700" y="256" text-anchor="middle">0 left</text>',
    '<text class="scr" data-show="hp-d0" x="700" y="272" text-anchor="middle">reads it</text>',
]
wide_groups.append(group("hops", "\n".join(parts), DEFAULT))
steps["hops"]["viewBoxWide"] = "0 0 800 284"

# 5. email over radio
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
    '<text class="scr ok" data-show="e-acc" x="710" y="222" text-anchor="middle">accepted</text>',
    '<text class="sub" x="710" y="238" text-anchor="middle">to alex@example.com</text>',
    seg("e-a-gw", "rf", wave(W_TIP[0] + 4, W_TIP[1] - 1, W_MAST[0] - 4, W_MAST[1] + 1)),
    '<text class="sub on-rf" x="245" y="86" text-anchor="middle">144.950</text>',
    handheld(70, 150, "e-a", "G4ABC", "on-rplus", "radio-plus"),
    '<text class="scr" data-show="e-tx" x="44" y="292">[tx]</text>',
    '<text class="scr ok" data-show="e-gw" x="80" y="292">[gw]</text>',
    '<text class="scr ok" data-show="e-sent" x="116" y="292">Sent</text>',
    '<text class="scr" data-show="e-inbox" x="44" y="308">inbox: 1 new mail</text>',
    '<text class="sub" x="345" y="262" text-anchor="middle">internet-radio · 144.950</text>',
    '<text class="sub" x="345" y="276" text-anchor="middle">named in [mail] gateway</text>',
]), DEFAULT))
steps["email"]["viewBoxWide"] = "0 64 800 256"

svg_body = "\n".join(groups)
svg_body_wide = "\n".join(wide_groups)
steps_json = json.dumps(steps, ensure_ascii=False, indent=1)

PANELS = """
              <div class="setup-panel" data-setup-panel="home">
                <h3>Walkie-talkies calling a home gateway</h3>
                <div class="setup-panel__main">
                  <p>A few people out walking each carry a 2m radio on one calling spot, linked to a computer running WeeChat Radio in <code>radio-plus</code>. At home, one station runs <code>internet-radio</code> on the same dial: a base station on a roof antenna, or a handheld by a window.</p>
                  <p>Everyone on 144.950 hears the chat on the air. The home gateway also carries it to the hub for people with no radio, and puts their replies back on the air.</p>
                  <p class="setup-note">The gateway only puts a hub line on the air when the station, or a member of the group, was heard on this dial in the last 10 minutes. A base station with an outdoor antenna hears much further than a handheld indoors.</p>
                </div>
                <dl class="who-list">
                  <div class="mode-radio-plus"><dt>G4ABC, G4DEF<small>radio-plus</small></dt><dd>144.950 FM. Their computers never open the hub.</dd></div>
                  <div class="mode-internet-radio"><dt>M0GWY<small>internet-radio</small></dt><dd>At home on 144.950, with <code>rf_egress = true</code>.</dd></div>
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
  <meta name="description" content="Five everyday WeeChat Radio setups, animated: walkie-talkies calling a home gateway, HF to HF with no internet, an internet guest, relays with hops, and email over radio." />
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
  <link rel="stylesheet" href="/docs.css?v=2" />
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
  <script src="/live-dgm.js?v=2"></script>
</body>
</html>
"""

with open(OUT, "w", encoding="utf-8") as fh:
    fh.write(HTML)
print("wrote", OUT, len(HTML))
