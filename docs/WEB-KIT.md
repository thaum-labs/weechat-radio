# Docs and guides page kit

How pages under `web/docs/` and `web/guides/` are built. Writing rules are in [STYLE.md](STYLE.md); this file covers layout, diagrams and the live diagram engine.

## Files

| File | What it is |
|---|---|
| `web/styles.css` | Site shell, colour tokens, `.dgm` diagram basics, `.dgm-key`, callouts (`.need` `.ok` `.note` `.warn-box`), `.steps`, code blocks and copy buttons |
| `web/docs.css` | The kit: cards, step titles, fix rows, flow chips, live diagrams, diagram glyphs, packets, Replay button |
| `web/live-dgm.js` | Live diagram engine: steps a message through an SVG, driven by JSON in the page |
| `web/<page>.css` | Optional page extras, only when the kit has nothing that fits (e.g. `mixed-band.css`) |

## Page head and scripts

```html
<link rel="stylesheet" href="/styles.css?v=34" />
<link rel="stylesheet" href="/docs.css?v=1" />
<!-- optional: <link rel="stylesheet" href="/<page>.css?v=1" /> -->
…
<script src="/copy.js?v=1"></script>
<script src="/shell.js?v=10"></script>
<script src="/live-dgm.js?v=1"></script>   <!-- only if the page has a live diagram -->
```

Bump `?v=` when a file changes.

## Page shape

```
crumb
h1  (weechat-radio:$ …)
p.lead
section.article-section.article-section--first   What you need → ul.cards
section.article-section                           a live or static diagram, with p.section-lead
section.article-section                           Steps → ol.steps with .step-title
div.ok                                            How you know it worked
section.article-section                           If it stays quiet → dl.fix-list
p.next
```

Reference pages (API, deploy, OS setup) keep their own order; use the pieces that fit.

## Pieces

**Section intro** — one or two sentences under an `h2`:
```html
<section class="article-section" aria-labelledby="setupH">
  <h2 id="setupH">Pick your setup</h2>
  <p class="section-lead">…</p>
```

**Cards** — short parallel items (what you need, choices). Title from the item's own first words:
```html
<ul class="cards">
  <li><svg class="card-ico" aria-hidden="true"><use href="#ico-laptop" /></svg>
      <strong>Two computers</strong><span>One person can sit them on the same desk.</span></li>
  <li class="is-optional">…</li>   <!-- dashed border -->
</ul>
```
Icon symbols go once per page, right after `</header>`:
```html
<svg width="0" height="0" style="position:absolute" aria-hidden="true" focusable="false">
  <symbol id="ico-laptop" viewBox="0 0 24 24"><rect x="4" y="5" width="16" height="11" /><path d="M2 19 H22" /></symbol>
  <symbol id="ico-app" viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" /><path d="M7 10 L10 12.5 L7 15 M12 15 H17" /></symbol>
  <symbol id="ico-radio" viewBox="0 0 24 24"><rect x="7" y="8" width="10" height="14" rx="1.5" /><path d="M15 8 V2 M10 15 H14 M10 18 H14" /></symbol>
  <symbol id="ico-cable" viewBox="0 0 24 24"><path d="M3 12 H9 M15 12 H21 M9 9 H15 V15 H9 Z" /></symbol>
  <symbol id="ico-globe" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M3 12 H21 M12 3 C8 7 8 17 12 21 C16 17 16 7 12 3" /></symbol>
  <symbol id="ico-key" viewBox="0 0 24 24"><circle cx="8" cy="12" r="4" /><path d="M12 12 H21 M18 12 V15" /></symbol>
</svg>
```

**Compact cards** — `ul.cards.cards--compact` puts the icon on the left and stacks the cards in one column: use it for a short list of choices beside a diagram.

**Step titles** — two to four words, verb first:
```html
<ol class="steps">
  <li><strong class="step-title">Run setup</strong><p>…</p><p>…</p></li>
```

**Fix rows** — symptom then fix. Loose tips go in `ul.fix-also` underneath:
```html
<dl class="fix-list">
  <div><dt>The radio light never comes on</dt><dd>Move the speaker closer…</dd></div>
</dl>
```

**Long fixes** — when a fix holds paragraphs, a list or a command, use `dl.fix-list.fix-list--long` (symptom on top, fix at full width). Rows can carry an `id` for links; the targeted row gets an orange border.

**Mark list** — short codes and what they mean:
```html
<dl class="mark-list"><div><dt><code>[ok]</code></dt><dd>delivered</dd></div></dl>
```

**Cards beside a diagram** — `<div class="pair"><ul class="cards">…</ul><figure class="diagram">…</figure></div>`; stacks on phones.

**Flow** — two settings and the condition to move between them:
```html
<div class="flow" aria-hidden="true">
  <div class="flow-chip is-start"><svg viewBox="0 0 78 16"><path d="…" /></svg><code>hf-poor</code><span>slow, steady · start here</span></div>
  <div class="flow-step"><span>hi got through both ways</span><span class="flow-step__arrow">→</span><span class="flow-step__back">still quiet? back to hf-poor</span></div>
  <div class="flow-chip">…</div>
</div>
```
The flow is a summary; keep the sentences it summarises.

## Diagrams

Colours carry meaning; keep them consistent with the live map:

| Colour | Class | Meaning |
|---|---|---|
| orange | `.rf`, `.box.orange`, `.on-orange` | on the air (RF), radios |
| cyan | `.net`, `.box.hub`, `.on-hub` | hub / internet over TLS |
| blue | `.wire`, `.box.accent`, `.on-accent` | computer, sound or cable |
| green | `.box.gw`, `.on-gw` | `internet-radio` gateway |
| magenta | `.box.rplus`, `.on-rplus` | `radio-plus` station |
| amber | `.box.radio`, `.on-radio` | `radio` mode station |

Text: `.sub` for small labels, `.tag` only where an existing diagram already uses it. Sentence case; no all-caps labels.

Size: draw for a phone. A `viewBox` about 420–440 wide with 11–12px text stays readable at 360px. Give such a static diagram `class="dgm fit"` so it drops the shared 620px scroll width. Wide static diagrams may keep it; live diagrams always fit (the kit removes the min-width inside `.live-dgm`).

Every SVG has `role="img"`, a `<title>` and a `<desc>` that says in words what the picture shows.

### Glyphs

Laptop, top-left of screen at (X, Y):
```html
<g data-node="pc-a">
  <rect class="box accent" x="X" y="Y" width="140" height="84" />
  <path class="laptop-base" d="M X-12 Y+84 H X+152 L X+160 Y+94 H X-20 Z" />
  <text class="sub" x="X+10" y="Y+17">computer A</text>
  <text class="scr" data-show="a-hi" x="X+10" y="Y+44">&lt;~alex&gt; hi</text>
  <text class="scr-input" x="X+10" y="Y+72">&gt; _</text>
</g>
```

Handheld radio, body top-left at (X, Y), 48×88:
```html
<g data-node="radio-a">
  <line class="antenna" x1="X+40" y1="Y" x2="X+40" y2="Y-34" />
  <circle class="antenna-tip" cx="X+40" cy="Y-36" r="2.5" />
  <rect class="box orange" x="X" y="Y" width="48" height="88" rx="5" />
  <circle class="led" cx="X+36" cy="Y+12" r="3.5" />          <!-- lights when the node is on -->
  <path class="grille" d="M X+10 Y+48 H X+38 M X+10 Y+55 H X+38 M X+10 Y+62 H X+38" />
  <text class="on-orange" x="X+24" y="Y+106" text-anchor="middle">radio A</text>
</g>
```

Base station at home, house top-left at (X, Y), 150×120:
```html
<g data-node="home">
  <path class="house" d="M X Y+40 L X+75 Y L X+150 Y+40 V Y+120 H X Z" />
  <path class="mast" d="M X+110 Y+18 V Y-30 M X+102 Y-22 H X+118 M X+104 Y-12 H X+116" />
  <rect class="box orange" x="X+20" y="Y+72" width="70" height="34" rx="3" />
  <circle class="led" cx="X+30" cy="Y+82" r="3.5" />
  <path class="grille" d="M X+44 Y+82 H X+82 M X+44 Y+89 H X+82 M X+44 Y+96 H X+82" />
  <path class="mast" d="M X+90 Y+80 H X+110 V Y+18" />
</g>
```

Hub: `<rect class="box hub" …>` with `<text class="on-hub">hub</text>`.
Group of stations on one dial: `<rect class="lane-bg" …>`.
Sound through air instead of a cable: add `air` to the path class (`class="wire air"`).

### Paths and packets

A path that a message can travel is a segment: the drawn line plus a copy with class `pkt` that carries the moving packet.
```html
<g class="seg seg-rf" data-seg="rf"><path class="rf" d="…" /><path class="pkt" d="…" /></g>
```
`seg-rf` (orange packet), `seg-net` (cyan), `seg-sound` (blue). Draw each path in the direction a message usually travels; a step can run it backwards.

RF waves: a sine path, e.g. from x 132 to 308 at y 172:
`M132 172 q5.5 -5 11 0 t11 0 …` (16 half-waves of 11). Generate with a short script rather than by hand.

### Live diagram

Use one when the page explains how a message travels. One per page is usually enough.

```html
<figure class="diagram live-dgm" data-live-dgm data-setup="cable">
  <div class="setup-switch" role="group" aria-label="Setup" data-setup-switch hidden>   <!-- only with 2+ setups -->
    <button type="button" data-setup-btn="cable" aria-pressed="true">radios + cable</button>
    <button type="button" data-setup-btn="noradio" aria-pressed="false">no radio</button>
  </div>
  <div class="live-dgm__body">                     <!-- add live-dgm--wide on the figure to stack text under the picture -->
    <div class="live-dgm__pic">
      <div class="dgm-wrap"><svg class="dgm" viewBox="0 0 440 322" role="img" aria-labelledby="…">…</svg></div>
      <div class="live-status">
        <p class="live-status__text" data-live-status aria-live="polite"></p>
        <button type="button" class="btn-replay" data-live-replay hidden>Replay</button>
      </div>
    </div>
    <div class="live-dgm__text">
      <div class="setup-panel" data-setup-panel="cable"><h3>…</h3><p>…</p><p class="setup-note">…</p></div>
      <div class="setup-panel" data-setup-panel="noradio">…</div>
    </div>
  </div>
  <script type="application/json" data-live-steps>
  { "cable": { "viewBox": "0 0 440 322", "steps": [
      { "say": "Computer A plays “hi” as sound into radio A’s microphone.", "segs": ["a-out"], "show": ["a-hi"] },
      { "say": "Radio A transmits. Its light comes on.", "segs": ["rf"], "on": ["radio-a"], "ms": 1700 },
      { "say": "Radio B transmits the [ok] back.", "segs": ["<rf"], "on": ["radio-b"] }
  ] },
    "noradio": { "viewBox": "0 0 440 146", "steps": [ … ] } }
  </script>
</figure>
```

Step fields: `say` (status line, one plain sentence), `segs` (segments to run, `"<id"` runs backwards), `chain` (`true` runs the segs one after another instead of together; give the step enough `ms`), `on` (nodes lit for that step), `show` (`data-show` lines that appear and stay), `ms` (step length, default 1500).

Parts that only belong to some setups get `data-setups="a b"`; also give them `class="is-off"` when they are not in the default setup, so the page reads right without script. Several elements may share a `data-seg` id (e.g. a cable lead and an air lead); all of them run.

The engine plays once when the diagram scrolls into view, and again on Replay or a setup change. Reduced motion and no-script show the finished state.

## Checks before a page is done

- Desktop (1280) and phone (390) screenshots: nothing clipped, no sideways scroll, no console errors.
- Reduced motion shows the finished state with a sensible status line.
- Every link, anchor id and copy button from the old page still works.
- Copy: unchanged except new headings, step titles, section intros, switch labels and status lines. Facts come from the page itself, `docs/*.md` or the source — never invented.
