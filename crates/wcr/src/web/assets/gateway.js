// SPDX-License-Identifier: Apache-2.0
// Gateway view: one poll of /api/gateway every few seconds, drawn as text.
"use strict";

const POLL_MS = 3000;
const $ = (id) => document.getElementById(id);

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function ago(secs) {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  return `${Math.floor(secs / 86400)}d ${Math.floor((secs % 86400) / 3600)}h`;
}

function clock(ts) {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  if (d.toDateString() === new Date().toDateString()) return time;
  return `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
}

function light(label, state) {
  const word = state === true ? "up" : state === false ? "down" : "idle";
  const e = el("span", `light ${word}`, `${label} ${word === "idle" ? "—" : word}`);
  return e;
}

function fill(list, items, draw, none) {
  list.replaceChildren();
  if (!items.length) {
    list.append(el("li", "empty", none));
    return;
  }
  for (const item of items) list.append(draw(item));
}

function mailRow(m) {
  const li = el("li");
  const st = m.status || {};
  const chip = el("span", `chip ${st.key || "queued"}`, st.label || "?");
  chip.title = st.reason || st.line || "";
  chip.setAttribute("aria-label", `${st.mark || ""} ${st.label || ""}`.trim());
  const what = el("span", "what");
  what.append(el("span", "subject", m.subject || "(no subject)"), el("span", "who", `${m.from} → ${m.to}`));
  what.title = `${m.subject}\n${m.from} → ${m.to}${st.reason ? `\n${st.reason}` : ""}`;
  li.append(el("span", "when", clock(m.ts)), chip, what);
  return li;
}

function draw(d) {
  const s = d.status || {};
  const running = d.station.running;
  $("call").textContent = s.callsign || "";
  $("lights").replaceChildren(
    light("station", running),
    light("internet", d.internet),
    light("hub", running ? !!s.hub_ok : null),
    // Internet mode has no radio, so the light stays grey.
    light("radio", running && s.mode !== "internet" ? radioUp(s) : null),
  );
  const facts = [
    ["uptime", running ? ago(d.station.uptime_secs) : "stopped"],
    ["frequency", s.frequency ? `${s.frequency}${s.band ? ` (${s.band})` : ""}` : "—"],
    ["mode", s.mode || "—"],
    ["preset", s.preset ? `${s.preset}${s.tx_rung ? ` · ${s.tx_rung}` : ""}` : "—"],
    ["modem", s.tnc || (s.modem73 ? `modem73 ${s.modem73}${running && !radioUp(s) ? " (not running)" : ""}` : "—")],
    ["forwarded", s.relayed !== undefined ? `${s.relayed} frames (${s.relayed_unique} messages)` : "—"],
    ["restarts", String(d.station.restarts)],
  ];
  if (d.station.last_exit) facts.push(["last stop", d.station.last_exit]);
  const dl = $("facts");
  dl.replaceChildren();
  for (const [k, v] of facts) dl.append(el("dt", "", k), el("dd", "", v));
  setFreq = d.frequency || "";
  shown = { mode: s.mode, preset: s.preset };
  drawPickers(d.choices || {}, running, s);
  drawRadio(running, s);
  $("start").disabled = running;
  $("stop").disabled = !running;
  fill($("relay"), d.relay_mail, mailRow, running ? "No mail through this gateway yet." : "Station stopped.");
  fill($("outbox"), d.outbox, mailRow, "Empty.");
  const heard = (s.heard || []).slice().sort((a, b) => (b.last_heard || 0) - (a.last_heard || 0));
  fill($("heard"), heard, (h) => {
    const li = el("li");
    const age = h.last_heard ? ago(Math.max(0, d.now - h.last_heard)) : "";
    const what = el("span", "what");
    what.append(el("span", "subject", `${h.callsign}${h.gateway ? " (gateway)" : ""}`), el("span", "who", h.band || h.medium || ""));
    li.append(el("span", "when", age), what);
    if (h.snr !== null && h.snr !== undefined) {
      // 10 dB or more with the radios in one room; under 6 dB, check levels.
      li.append(el("span", `snr ${h.snr >= 10 ? "up" : h.snr >= 6 ? "amber" : "down"}`, `${h.snr.toFixed(0)} dB`));
    }
    return li;
  }, running ? "Nobody heard yet." : "Station stopped.");
  $("errors-box").hidden = !d.errors.length;
  $("errors").textContent = d.errors.join("\n");
  const log = $("log");
  const atEnd = log.scrollTop + log.clientHeight >= log.scrollHeight - 4;
  log.textContent = d.log.join("\n");
  if (atEnd) log.scrollTop = log.scrollHeight;
  $("updated").textContent = `Updated ${new Date().toLocaleTimeString()}${d.answering || !running ? "" : " · the station is not answering yet"}`;
}

// The node sets audio_label to "no modem" or "no audio" while modem73 is not
// running or has no sound card, and to a level (or "—") once it is attached.
// s.modem73 is only the installed version, not a sign that it runs.
function radioUp(s) {
  if (s.tnc) return !!s.tnc_ok;
  return !["", "no modem", "no audio"].includes(s.audio_label || "");
}

// Radio audio card (#241): the receive level as a meter with a target
// zone and a plain hint, the RX gain and TX level sliders, and the
// waterfall. Shown for a station with a sound-card or Bluetooth audio
// modem (not internet-only, not a radio with its own TNC).
const METER_MIN = -60;
const pctOf = (db) => Math.max(0, Math.min(100, ((db - METER_MIN) * 100) / -METER_MIN));
function drawRadio(running, s) {
  const want = !!(running && s.mode && s.mode !== "internet" && !s.tnc);
  $("radio-card").hidden = !want;
  wfOn = want && !!s.modem73;
  $("wf-box").hidden = !wfOn;
  if (!want) return;
  drawMeter(s);
  drawSliders(s);
}

// The green zone: -45..-12 dBFS on FM (sync busy test). With the level
// busy test (HF) idle noise must also sit 3 dB under the threshold; above
// that it is amber ("noisy": sends wait), above -12 red as on FM.
function zones(s) {
  if (s.csma_mode === "level" && typeof s.csma_threshold_db === "number") {
    return { low: -55, good: Math.min(-12, s.csma_threshold_db - 3), high: -12 };
  }
  return { low: -45, good: -12, high: -12 };
}

function drawMeter(s) {
  const z = zones(s);
  const [a, b, c] = [pctOf(z.low), pctOf(z.good), pctOf(z.high)];
  $("lv-zones").style.background = `linear-gradient(90deg, var(--meter-low) 0 ${a}%, var(--meter-good) ${a}% ${b}%, var(--meter-busy) ${b}% ${c}%, var(--meter-high) ${c}% 100%)`;
  const word = s.audio_in_word || "";
  const measured = word !== "" && !["", "no modem", "no audio", "—"].includes(s.audio_label || "");
  const db = s.audio_in_db;
  $("lv-bar").style.width = measured ? `${pctOf(db)}%` : "0";
  $("lv-bar").className = `meter-bar ${word}`;
  $("lv-meter").setAttribute("aria-valuenow", measured ? String(Math.round(db)) : "");
  const peak = s.audio_in_peak_db;
  $("lv-peak").hidden = !(measured && typeof peak === "number");
  if (typeof peak === "number") $("lv-peak").style.left = `${pctOf(peak)}%`;
  const cls = { good: "up", low: "idle", high: "amber", noisy: "amber", clipping: "down" }[word] || "idle";
  const audio = s.audio_label || "";
  $("lv-word").className = `light ${cls}`;
  if (measured) {
    $("lv-word").textContent = word;
    const nums = [`${Math.round(db)} dBFS`];
    if (typeof peak === "number") nums.push(`peak ${Math.round(peak)}`);
    if (typeof s.audio_in_clip_pct === "number") nums.push(`${s.audio_in_clip_pct.toFixed(1)}% clipped`);
    if (typeof s.audio_in_frame_db === "number") nums.push(`last signal ${Math.round(s.audio_in_frame_db)}`);
    $("lv-nums").textContent = nums.join(" · ");
    $("lv-hint").textContent = s.audio_in_hint || "";
  } else {
    $("lv-word").textContent = audio && audio !== "—" ? audio : "not measured";
    $("lv-nums").textContent = "";
    $("lv-hint").textContent = audio === "—" ? "This modem73 does not report the level live." : "";
  }
  $("lv-hint").className = `lv-hint small ${cls}`;
  $("lv-busy").textContent = s.csma_mode === "sync"
    ? "· busy test: data signals (FM)"
    : s.csma_mode === "level" ? `· busy test: level ${Math.round(s.csma_threshold_db)} dBFS` : "";
}

// A slider the operator touched is left alone until the station has
// answered with the new value (or 5 s pass).
const held = { rx: 0, tx: 0 };

function drawSliders(s) {
  const g = s.rx_gain;
  const rx = $("rx-gain");
  $("rx-row").hidden = !g;
  if (g) {
    rx.min = String(g.min_db);
    rx.max = String(g.max_db);
    if (Date.now() > held.rx) rx.value = String(g.db);
    $("rx-out").textContent = `${(+rx.value).toFixed(1)} dB`;
    $("rx-note").textContent = g.kind === "bluetooth"
      ? "wcr's gain on the radio's Bluetooth audio. The radio's volume knob does not change this audio."
      : `The sound card's ${g.control || "capture"} gain${g.agc === true ? " (automatic gain is on: moving the slider turns it off)" : ""}. If it is at its lowest and still clipping, turn the radio's volume down.`;
  } else {
    $("rx-note").textContent = s.rx_gain_note ? `RX gain: ${s.rx_gain_note}.` : "RX gain: use the radio's volume.";
  }
  const tx = $("tx-level");
  if (Date.now() > held.tx) tx.value = String(typeof s.tx_level_db === "number" ? s.tx_level_db : 0);
  $("tx-out").textContent = `${(+tx.value).toFixed(0)} dB`;
}

async function sendLevel(what, input, knob) {
  held[knob] = Date.now() + 5000;
  $("action").textContent = "";
  try {
    const r = await fetch("/api/station/setting", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ what, value: String(input.value) }),
    });
    const d = await r.json().catch(() => ({}));
    if (!r.ok) $("action").textContent = d.error || `could not set ${what}`;
  } finally {
    setTimeout(poll, 600);
  }
}

$("rx-gain").addEventListener("input", (e) => { held.rx = Date.now() + 5000; $("rx-out").textContent = `${(+e.target.value).toFixed(1)} dB`; });
$("tx-level").addEventListener("input", (e) => { held.tx = Date.now() + 5000; $("tx-out").textContent = `${(+e.target.value).toFixed(0)} dB`; });
$("rx-gain").addEventListener("change", (e) => sendLevel("rx_gain", e.target, "rx"));
$("tx-level").addEventListener("change", (e) => sendLevel("tx_level", e.target, "tx"));

// Waterfall: one row from /api/spectrum every WF_MS while the page is
// visible and the station runs a sound-card modem. Nothing is asked while
// the page is hidden or closed, so modem73 stops collecting audio for it.
const WF_MS = 250;
// Colours follow the noise floor (#241): every radio and every volume
// setting shows its noise as dark blue-purple and a signal as bright, the
// way WSJT-X and fldigi do. A fixed scale painted squelch-0 hiss solid
// white on a hot input and black on a quiet one. The floor is a low
// percentile of each row across the modem's passband, followed slowly
// upward (so a long frame stays bright) and quickly downward.
const WF_SPAN_DB = 36;     // floor - 6 dB is black, floor + 30 dB is white
const WF_RISE = 0.03, WF_FALL = 0.4;
let wfFloor = null;
// The canvas spans 0–3 kHz of the radio's audio, at true positions.
const WF_MAX_HZ = 3000;
// Our TX row is painted only where the modem sends: OFDM 1500 ± 1200 Hz.
const WF_TX_LO = 300, WF_TX_HI = 2700;
const WF_TX_RGB = [255, 122, 61];
let wfOn = false, wfBusy = false, wfNoAudio = 0;

// The row's noise floor: the 20th percentile of the passband bins.
function wfRowFloor(bins, hzPerBin) {
  const lo = Math.floor(WF_TX_LO / hzPerBin), hi = Math.min(bins.length - 1, Math.ceil(WF_TX_HI / hzPerBin));
  const band = bins.slice(lo, hi + 1).filter((v) => typeof v === "number").sort((a, b) => a - b);
  if (!band.length) return null;
  return band[Math.floor(band.length * 0.2)];
}

function wfTrack(bins, hzPerBin) {
  const f = wfRowFloor(bins, hzPerBin);
  if (f === null) return;
  if (wfFloor === null) wfFloor = f;
  else wfFloor += (f - wfFloor) * (f > wfFloor ? WF_RISE : WF_FALL);
}

function wfColor(db) {
  const base = (wfFloor === null ? -100 : wfFloor) - 6;
  const t = Math.max(0, Math.min(1, (db - base) / WF_SPAN_DB));
  // black → blue → purple → yellow → white
  const stops = [[0, 0, 0], [28, 42, 107], [140, 60, 200], [255, 225, 77], [255, 255, 255]];
  const x = t * (stops.length - 1), i = Math.min(stops.length - 2, Math.floor(x)), f = x - i;
  return stops[i].map((c, k) => Math.round(c + (stops[i + 1][k] - c) * f));
}

// One new row at the top. Column x shows the bin nearest its audio
// frequency, so 1000 Hz sits exactly a third of the way across.
function wfRow(bins, hzPerBin, tx) {
  const cv = $("wf");
  const w = Math.max(1, Math.round(cv.clientWidth * (window.devicePixelRatio || 1)));
  const h = cv.height;
  if (cv.width !== w) cv.width = w;
  const g = cv.getContext("2d");
  g.drawImage(cv, 0, 0, w, h - 1, 0, 1, w, h - 1);
  const row = g.createImageData(w, 1);
  for (let x = 0; x < w; x++) {
    const hz = ((x + 0.5) * WF_MAX_HZ) / w;
    let rgb;
    if (tx) {
      rgb = hz >= WF_TX_LO && hz <= WF_TX_HI ? WF_TX_RGB : [0, 0, 0];
    } else {
      rgb = wfColor(bins[Math.min(bins.length - 1, Math.round(hz / hzPerBin))]);
    }
    row.data.set([rgb[0], rgb[1], rgb[2], 255], x * 4);
  }
  g.putImageData(row, 0, 0);
}

async function wfTick() {
  if (!wfOn || wfBusy || document.hidden) return;
  wfBusy = true;
  try {
    const r = await fetch("/api/spectrum", { cache: "no-store" });
    if (r.status === 401) { location.href = "/login"; return; }
    const d = await r.json();
    const note = $("wf-note");
    wfNoAudio = d.why === "no audio" && !d.tx ? wfNoAudio + 1 : 0;
    if (d.tx) {
      wfRow([], 1, true);
      note.textContent = "· transmitting";
    } else if (d.bins.length) {
      const hz = d.hz_per_bin || WF_MAX_HZ / d.bins.length;
      wfTrack(d.bins, hz);
      wfRow(d.bins, hz, false);
      note.textContent = wfFloor === null ? "" : `· scaled to the noise floor (${Math.round(wfFloor)} dB)`;
    } else if (d.why === "no audio") {
      // No new audio since the last row: draw nothing rather than repeat
      // it. A short gap right after our TX is normal; say so after 1 s.
      if (wfNoAudio >= 4) note.textContent = "· no audio coming from the radio";
    } else {
      note.textContent = {
        "old modem": "· needs the modem73 from wcr 1.0.7 or later",
        "no modem": "· modem not answering",
        "stopped": "· station stopped",
      }[d.why] || "· starting…";
    }
  } catch (e) {
    // The main poll reports a lost connection.
  } finally {
    wfBusy = false;
  }
}

async function poll() {
  try {
    const r = await fetch("/api/gateway", { cache: "no-store" });
    if (r.status === 401) { location.href = "/login"; return; }
    draw(await r.json());
  } catch (e) {
    $("updated").textContent = `Cannot reach the station's page: ${e.message}`;
  }
}

// The frequency from wcr.toml, known while the station is stopped too.
let setFreq = "";

async function act(what) {
  const ask = what === "start"
    ? `Start the station? It will transmit on ${setFreq || "its set frequency"} (beacons and gateway replies). You are the control operator.`
    : "Stop the station? The gateway stops until Start is pressed.";
  if (!confirm(ask)) return;
  $("action").textContent = what === "start" ? "Starting…" : "Stopping…";
  $("start").disabled = $("stop").disabled = true;
  try {
    await fetch(`/api/station/${what}`, { method: "POST" });
  } finally {
    $("action").textContent = "";
    poll();
  }
}

// Mode and preset pickers (#226): the same /mode and /preset the desktop
// app's pickers send, after a confirm. The list is redrawn only when it
// changes, and never while the operator has it open.
let pickBusy = false;

function fillSelect(sel, items, current) {
  const key = JSON.stringify(items.map((i) => i.value));
  if (sel.dataset.key !== key) {
    sel.replaceChildren(...items.map((i) => {
      const o = el("option", "", i.name || i.value);
      o.value = i.value;
      if (i.about) o.title = i.about;
      return o;
    }));
    sel.dataset.key = key;
  }
  if (document.activeElement !== sel && !pickBusy) sel.value = current || "";
}

function drawPickers(c, running, s) {
  const mode = $("mode-pick"), preset = $("preset-pick");
  fillSelect(mode, c.modes || [], s.mode);
  const presets = c.presets || [];
  $("preset-label").hidden = !presets.length;
  fillSelect(preset, presets, s.preset);
  mode.disabled = preset.disabled = !running || pickBusy;
}

async function pick(what, sel, current) {
  const value = sel.value;
  if (!value || value === current) return;
  const opt = sel.selectedOptions[0];
  const ask = what === "mode"
    ? value === "radio"
      ? "Switch to Radio? Hub chat stops and messages go on RF only. You are the control operator."
      : `Switch the station to ${opt ? opt.textContent : value}?`
    : `Change the preset to ${value}?${opt && opt.title ? `\n${opt.title}` : ""}`;
  if (!confirm(ask)) { sel.value = current || ""; return; }
  pickBusy = true;
  $("action").textContent = `Setting ${what}…`;
  try {
    const r = await fetch("/api/station/setting", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ what, value }),
    });
    const d = await r.json().catch(() => ({}));
    $("action").textContent = r.ok ? "" : d.error || `could not set ${what}`;
  } finally {
    pickBusy = false;
    setTimeout(poll, 500);
  }
}

let shown = {};
$("mode-pick").addEventListener("change", (e) => pick("mode", e.target, shown.mode));
$("preset-pick").addEventListener("change", (e) => pick("preset", e.target, shown.preset));

// Health card (#231): /api/health every HEALTH_MS while the page is
// visible. Each reading is optional; what the system lacks is left out.
const HEALTH_MS = 5000;
let healthBusy = false;

function bytes(n) {
  if (n === null || n === undefined) return "—";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < u.length - 1) { n /= 1024; i++; }
  return `${n >= 10 || i === 0 ? Math.round(n) : n.toFixed(1)} ${u[i]}`;
}

function chip(text, state, title) {
  // state: "ok" green, "warn" amber, "bad" red, else grey.
  const cls = { ok: "up", warn: "amber", bad: "down" }[state] || "idle";
  const e = el("span", `light ${cls}`, text);
  if (title) e.title = title;
  return e;
}

function drawHealth(h) {
  const strip = [], facts = [];
  const cpu = h.cpu || {};
  if (cpu.pct !== null && cpu.pct !== undefined) {
    strip.push(chip(`CPU ${Math.round(cpu.pct)}%`, cpu.pct >= 90 ? "warn" : "ok"));
    const cores = (cpu.per_core || []).map((p) => (p === null ? "—" : `${Math.round(p)}%`)).join(" ");
    facts.push(["CPU", `${Math.round(cpu.pct)}%${cores ? ` (cores ${cores})` : ""}${cpu.mhz ? ` · ${cpu.mhz} MHz` : ""}`]);
  }
  if (cpu.load) {
    const busy = cpu.cores && cpu.load[0] > cpu.cores;
    facts.push(["load", `${cpu.load.map((l) => l.toFixed(2)).join(" ")}${busy ? " (busy)" : ""}`]);
  }
  const m = h.memory;
  if (m && m.total) {
    const used = m.total - (m.available || 0);
    const pct = Math.round((used * 100) / m.total);
    const low = m.available !== null && m.available * 10 < m.total;
    strip.push(chip(`RAM ${pct}%`, low ? "warn" : "ok", `${bytes(m.available)} available`));
    facts.push(["memory", `${bytes(used)} used of ${bytes(m.total)}, ${bytes(m.available)} available${low ? " (low)" : ""}`]);
    if (m.swap_total) facts.push(["swap", `${bytes(m.swap_used)} used of ${bytes(m.swap_total)}`]);
    const rss = Object.entries(m.rss || {}).filter(([, v]) => v).map(([k, v]) => `${k} ${bytes(v)}`);
    if (rss.length) facts.push(["station memory", rss.join(" · ")]);
  }
  if (h.temperature !== null && h.temperature !== undefined) {
    const t = h.temperature;
    const word = h.temperature_word || "ok";
    strip.push(chip(`${t.toFixed(0)} °C ${word}`, word === "hot" ? "bad" : word === "warm" ? "warn" : "ok"));
    facts.push(["temperature", `${t.toFixed(1)} °C (${word}; a Pi slows down at 80 °C)`]);
  }
  const p = h.power;
  if (p) {
    const state = p.now.length ? "bad" : p.since_boot.length ? "warn" : "ok";
    const text = p.now.length ? p.now.join(", ") : p.since_boot.length ? "power: was low" : "power ok";
    strip.push(chip(text, state, `vcgencmd get_throttled = ${p.bits}`));
    facts.push(["power", `${p.now.length ? `now: ${p.now.join(", ")}` : "OK now"}${p.since_boot.length ? `; since boot: ${p.since_boot.join(", ")}` : ""} (${p.bits})`]);
  }
  for (const d of h.disks || []) {
    const usedPct = d.pct_used;
    strip.push(chip(`disk ${usedPct}%${d.label === "/" ? "" : ` (${d.label})`}`, d.word === "very low" ? "bad" : d.word === "low" ? "warn" : "ok", `${bytes(d.free)} free`));
    facts.push([`disk ${d.label}`, `${bytes(d.free)} free of ${bytes(d.size)} (${usedPct}% used${d.word === "ok" ? "" : `, ${d.word}`})${d.label === "/" ? "" : ` · ${d.path}`}`]);
  }
  const net = h.network || {};
  for (const w of net.wifi || []) {
    strip.push(chip(`Wi-Fi ${Math.round(w.dbm)} dBm`, w.word === "weak" ? "warn" : "ok", `${w.word}, quality ${w.quality_pct}%`));
    facts.push([`Wi-Fi ${w.name}`, `${Math.round(w.dbm)} dBm, quality ${w.quality_pct}% (${w.word})`]);
  }
  const ifs = (net.interfaces || []).map((i) => `${i.name} ${i.ip}${i.serving ? " (this page)" : ""}`);
  if (ifs.length) facts.push(["network", ifs.join(" · ")]);
  if (h.clock_synced === true || h.clock_synced === false) {
    if (!h.clock_synced) strip.push(chip("clock not synced", "warn"));
    facts.push(["clock", h.clock_synced ? "synced (NTP)" : "not synced: check the internet or NTP"]);
  }
  if (h.uptime_secs) facts.push(["machine up", ago(h.uptime_secs)]);
  const sys = h.system || {};
  const about = [sys.model, sys.os, sys.kernel && `kernel ${sys.kernel}`].filter(Boolean);
  if (about.length) facts.push(["system", about.join(" · ")]);
  $("health-sys").textContent = sys.model ? `· ${sys.model}` : "";
  $("health-strip").replaceChildren(...strip);
  const dl = $("health-facts");
  dl.replaceChildren();
  for (const [k, v] of facts) dl.append(el("dt", "", k), el("dd", "", v));
  $("health-card").hidden = !strip.length && !facts.length;
}

async function healthTick() {
  if (healthBusy || document.hidden) return;
  healthBusy = true;
  try {
    const r = await fetch("/api/health", { cache: "no-store" });
    if (r.status === 401) { location.href = "/login"; return; }
    drawHealth(await r.json());
  } catch (e) {
    // The main poll reports a lost connection.
  } finally {
    healthBusy = false;
  }
}

// The log's open or closed state is remembered on this browser.
try {
  if (localStorage.getItem("wcr.log") === "closed") $("log-box").open = false;
} catch (e) { /* private window: open */ }
$("log-box").addEventListener("toggle", () => {
  try { localStorage.setItem("wcr.log", $("log-box").open ? "open" : "closed"); } catch (e) { /* ignore */ }
});

$("start").addEventListener("click", () => act("start"));
$("stop").addEventListener("click", () => act("stop"));
poll();
setInterval(poll, POLL_MS);
setInterval(wfTick, WF_MS);
healthTick();
setInterval(healthTick, HEALTH_MS);
document.addEventListener("visibilitychange", () => { if (!document.hidden) healthTick(); });
