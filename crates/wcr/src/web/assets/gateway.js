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
    ["audio in", running && s.mode !== "internet" ? audioIn(s) : "—"],
    ["forwarded", s.relayed !== undefined ? `${s.relayed} frames (${s.relayed_unique} messages)` : "—"],
    ["restarts", String(d.station.restarts)],
  ];
  if (d.station.last_exit) facts.push(["last stop", d.station.last_exit]);
  const dl = $("facts");
  dl.replaceChildren();
  for (const [k, v] of facts) dl.append(el("dt", "", k), el("dd", "", v));
  setFreq = d.frequency || "";
  wfShow(running, s);
  $("start").disabled = running;
  $("stop").disabled = !running;
  fill($("relay"), d.relay_mail, mailRow, running ? "No mail through this gateway yet." : "Station stopped.");
  fill($("outbox"), d.outbox, mailRow, "Empty.");
  const heard = (s.heard || []).slice().sort((a, b) => (b.last_heard || 0) - (a.last_heard || 0));
  fill($("heard"), heard, (h) => {
    const li = el("li");
    const snr = h.snr === null || h.snr === undefined ? "" : ` ${h.snr.toFixed(0)} dB`;
    const age = h.last_heard ? ago(Math.max(0, d.now - h.last_heard)) : "";
    li.append(
      el("span", "when", age),
      el("span", "what", `${h.callsign}${h.gateway ? " (gateway)" : ""}  ${h.band || h.medium || ""}${snr}`),
    );
    return li;
  }, running ? "Nobody heard yet." : "Station stopped.");
  $("errors").textContent = d.errors.length ? d.errors.join("\n") : "None.";
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

// Capture level from the radio. modem73 holds the sound card, so the level
// comes from modem73; "—" means nothing measures it (an older modem73).
function audioIn(s) {
  if (s.tnc) return "—";
  const label = s.audio_label || "";
  if (["", "no modem", "no audio"].includes(label)) return label || "—";
  if (label === "—") return "not measured";
  return `${Math.round(s.audio_in_db)} dBFS (${label})`;
}

// Waterfall: one row from /api/spectrum every WF_MS while the page is
// visible and the station runs a sound-card modem. Nothing is asked while
// the page is hidden or closed, so modem73 stops collecting audio for it.
const WF_MS = 250;
// Fixed scale (dB below full scale per bin), so a squelched radio stays
// near black, open-squelch noise is purple and a signal is bright. No
// auto-gain on purpose. A squelched BF-1909 on a Digirig reads about
// -67 dBFS overall (QA #222) and about -99 dB per bin on the Pi 3
// (QA #225); a keyed call reads about -22 dB per bin.
const WF_FLOOR = -100, WF_TOP = -30;
// The canvas spans 0–3 kHz of the radio's audio, at true positions.
const WF_MAX_HZ = 3000;
// Our TX row is painted only where the modem sends: OFDM 1500 ± 1200 Hz.
const WF_TX_LO = 300, WF_TX_HI = 2700;
const WF_TX_RGB = [255, 122, 61];
let wfOn = false, wfBusy = false, wfNoAudio = 0;

function wfColor(db) {
  const t = Math.max(0, Math.min(1, (db - WF_FLOOR) / (WF_TOP - WF_FLOOR)));
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
            wfRow(d.bins, d.hz_per_bin || WF_MAX_HZ / d.bins.length, false);
      note.textContent = "";
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

// Shown for a station with a sound-card modem (not internet-only, not a TNC).
function wfShow(running, s) {
  const want = running && s.mode && s.mode !== "internet" && !s.tnc;
  $("wf-card").hidden = !want;
  wfOn = want;
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

$("start").addEventListener("click", () => act("start"));
$("stop").addEventListener("click", () => act("stop"));
poll();
setInterval(poll, POLL_MS);
setInterval(wfTick, WF_MS);
