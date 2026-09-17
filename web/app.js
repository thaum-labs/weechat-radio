/* SPDX-License-Identifier: Apache-2.0 */
const API = window.WCR.API;

const MODE_COLOR = {
  internet: "#00e5ff",
  "internet-radio": "#39ff14",
  radio: "#ffbf00",
  "radio-plus": "#ff4dff",
};

const CARTO_KEY = "cb1_3nxz_1_dba340f0a1c8450af09da179";
const MAP_STYLE_KEY = "wcr-map-style-v2";
const OSM_CARTO =
  '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>, &copy; <a href="https://carto.com/attributions">CARTO</a>';

const MAP_STYLES = {
  voyager: {
    tiles: [`https://basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}@2x.png?key=${CARTO_KEY}`],
    attribution: OSM_CARTO,
  },
  light: {
    tiles: [`https://basemaps.cartocdn.com/light_all/{z}/{x}/{y}@2x.png?key=${CARTO_KEY}`],
    attribution: OSM_CARTO,
  },
  dark: {
    tiles: [`https://basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png?key=${CARTO_KEY}`],
    attribution: OSM_CARTO,
  },
  terrain: {
    tiles: [
      "https://server.arcgisonline.com/ArcGIS/rest/services/World_Topo_Map/MapServer/tile/{z}/{y}/{x}",
    ],
    attribution:
      "Tiles &copy; <a href=\"https://www.esri.com/\">Esri</a> — Esri, USGS, NOAA",
  },
  satellite: {
    tiles: [
      "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
    ],
    attribution:
      "Tiles &copy; <a href=\"https://www.esri.com/\">Esri</a> — Esri, Maxar, Earthstar Geographics",
  },
};

function rasterStyle(id) {
  const spec = MAP_STYLES[id] || MAP_STYLES.satellite;
  return {
    version: 8,
    sources: {
      osm: {
        type: "raster",
        tiles: spec.tiles,
        tileSize: 256,
        attribution: spec.attribution,
      },
    },
    layers: [{ id: "osm", type: "raster", source: "osm" }],
  };
}

function preferredMapStyle() {
  const saved = localStorage.getItem(MAP_STYLE_KEY);
  return MAP_STYLES[saved] ? saved : "satellite";
}

const mapStyleSelect = document.getElementById("map-style");
const initialMapStyle = preferredMapStyle();
if (mapStyleSelect) mapStyleSelect.value = initialMapStyle;

const coarse = matchMedia("(pointer: coarse)").matches;
const map = new maplibregl.Map({
  container: "map",
  style: rasterStyle(initialMapStyle),
  center: [0, 20],
  zoom: 1.4,
  attributionControl: false,
  cooperativeGestures: coarse,
  dragRotate: !coarse,
  pitchWithRotate: !coarse,
  touchPitch: !coarse,
});
map.addControl(
  new maplibregl.AttributionControl({ compact: false }),
  "bottom-right"
);

function fitMap() {
  try { map.resize(); } catch (_) {}
}
map.on("load", fitMap);
window.addEventListener("resize", fitMap);
window.addEventListener("orientationchange", fitMap);

if (mapStyleSelect) {
  mapStyleSelect.addEventListener("change", () => {
    const id = MAP_STYLES[mapStyleSelect.value] ? mapStyleSelect.value : "satellite";
    localStorage.setItem(MAP_STYLE_KEY, id);
    map.setStyle(rasterStyle(id));
    map.once("idle", () => updateDayNight(true));
  });
}

const SPAN = 1440;
const markers = new Map();
const ticker = document.getElementById("ticker");
const stationList = document.getElementById("station-list");
const hubPanel = document.getElementById("hub-panel");
const card = document.getElementById("card");
const scrub = document.getElementById("scrub");
const playBtn = document.getElementById("play");
const clock = document.getElementById("tape-clock");
const connInfo = document.getElementById("conn-info");
const wsState = document.getElementById("ws-state");
const nightToggle = document.getElementById("map-night");
let selectedBand = null;
let lastNodes = [];
let lastBands = [];
const NIGHT_KEY = "wcr-map-night";
const NIGHT_SRC = "daynight";
const NIGHT_FILL = "daynight-fill";
const TWILIGHT_ALTS = [0, -2, -4, -6, -8, -10, -12, -14, -16, -18];
const TWILIGHT_OPACITY = [0.06, 0.11, 0.16, 0.21, 0.26, 0.31, 0.36, 0.41, 0.46];
let lastNightWall = 0;

if (nightToggle) {
  const savedNight = localStorage.getItem(NIGHT_KEY);
  nightToggle.checked = savedNight !== "0";
}

function wrapDeg(x, span) {
  return ((x % span) + span) % span;
}

function julianDay(date) {
  return date.getTime() / 86400000 + 2440587.5;
}

function gmstHours(jd) {
  const d = jd - 2451545.0;
  return wrapDeg(18.697374558 + 24.06570982441908 * d, 24);
}

function sunEquatorial(jd) {
  const n = jd - 2451545.0;
  const L = wrapDeg(280.46 + 0.9856474 * n, 360);
  const g = wrapDeg(357.528 + 0.9856003 * n, 360) * (Math.PI / 180);
  const lambda = wrapDeg(L + 1.915 * Math.sin(g) + 0.02 * Math.sin(2 * g), 360) * (Math.PI / 180);
  const eps = (23.4393 - 3.563e-7 * n) * (Math.PI / 180);
  return {
    alpha: Math.atan2(Math.cos(eps) * Math.sin(lambda), Math.cos(lambda)),
    delta: Math.asin(Math.sin(eps) * Math.sin(lambda)),
  };
}

function sunAltitude(latDeg, sun, ha) {
  const lat = latDeg * (Math.PI / 180);
  const s = Math.sin(lat) * Math.sin(sun.delta) + Math.cos(lat) * Math.cos(sun.delta) * Math.cos(ha);
  return Math.asin(Math.max(-1, Math.min(1, s))) * (180 / Math.PI);
}

function terminatorLat(lng, sun, gst, altDeg) {
  const ha = (gst + lng / 15) * 15 * (Math.PI / 180) - sun.alpha;
  const target = altDeg || 0;
  const nightPole = sun.delta < 0 ? 89.9 : -89.9;
  let day = -nightPole;
  let night = nightPole;
  for (let i = 0; i < 22; i++) {
    const mid = (day + night) / 2;
    if (sunAltitude(mid, sun, ha) > target) day = mid;
    else night = mid;
  }
  return Math.max(-89.9, Math.min(89.9, (day + night) / 2));
}

function shiftRing(ring, dLng) {
  return ring.map(([lng, lat]) => [lng + dLng, lat]);
}

function worldPolys(ring) {
  return [[ring], [shiftRing(ring, -360)], [shiftRing(ring, 360)]];
}

function closeBand(outer, inner) {
  const ring = outer.concat(inner.slice().reverse());
  ring.push(ring[0]);
  return ring;
}

function nightGeoJSON(date) {
  const jd = julianDay(date);
  const gst = gmstHours(jd);
  const sun = sunEquatorial(jd);
  const step = 2;
  const poleLat = sun.delta < 0 ? 89.9 : -89.9;
  const lines = TWILIGHT_ALTS.map((alt) => {
    const line = [];
    for (let lng = -180; lng <= 180; lng += step) {
      line.push([lng, terminatorLat(lng, sun, gst, alt)]);
    }
    return line;
  });

  const features = lines.slice(0, -1).map((outer, i) => ({
    type: "Feature",
    properties: { kind: `twilight-${i}` },
    geometry: {
      type: "MultiPolygon",
      coordinates: worldPolys(closeBand(outer, lines[i + 1])),
    },
  }));

  const coreLine = lines[lines.length - 1];
  const core = [[-180, poleLat], ...coreLine, [180, poleLat], [-180, poleLat]];
  features.push({
    type: "Feature",
    properties: { kind: "night" },
    geometry: { type: "MultiPolygon", coordinates: worldPolys(core) },
  });
  return { type: "FeatureCollection", features };
}

function overlayDate() {
  if (!scrub || Number(scrub.value) >= SPAN) return new Date();
  return new Date(Date.now() - (SPAN - Number(scrub.value)) * 60000);
}

function nightOn() {
  return !nightToggle || nightToggle.checked;
}

function twilightLayerId(i) {
  return `daynight-twilight-${i}`;
}

function ensureDayNightLayers() {
  if (!map.getSource(NIGHT_SRC)) {
    map.addSource(NIGHT_SRC, { type: "geojson", data: nightGeoJSON(overlayDate()) });
  }
  TWILIGHT_OPACITY.forEach((opacity, i) => {
    const id = twilightLayerId(i);
    if (map.getLayer(id)) return;
    map.addLayer({
      id,
      type: "fill",
      source: NIGHT_SRC,
      filter: ["==", ["get", "kind"], `twilight-${i}`],
      paint: {
        "fill-color": "#020617",
        "fill-opacity": opacity,
        "fill-antialias": true,
      },
    });
  });
  if (!map.getLayer(NIGHT_FILL)) {
    map.addLayer({
      id: NIGHT_FILL,
      type: "fill",
      source: NIGHT_SRC,
      filter: ["==", ["get", "kind"], "night"],
      paint: {
        "fill-color": "#020617",
        "fill-opacity": 0.5,
        "fill-antialias": true,
      },
    });
  }
  const vis = nightOn() ? "visible" : "none";
  TWILIGHT_OPACITY.forEach((_, i) => {
    map.setLayoutProperty(twilightLayerId(i), "visibility", vis);
  });
  map.setLayoutProperty(NIGHT_FILL, "visibility", vis);
}

function updateDayNight(force) {
  const now = Date.now();
  if (!force && now - lastNightWall < 200 && map.getLayer(NIGHT_FILL)) return;
  lastNightWall = now;
  try {
    ensureDayNightLayers();
    const src = map.getSource(NIGHT_SRC);
    if (src) src.setData(nightGeoJSON(overlayDate()));
  } catch (_) {}
}

function onMapReadyForNight() {
  updateDayNight(true);
}
map.on("load", onMapReadyForNight);
map.on("style.load", onMapReadyForNight);
if (map.loaded()) onMapReadyForNight();
setInterval(() => {
  if (nightOn() && (!scrub || Number(scrub.value) >= SPAN)) updateDayNight();
}, 60000);

if (nightToggle) {
  nightToggle.addEventListener("change", () => {
    localStorage.setItem(NIGHT_KEY, nightToggle.checked ? "1" : "0");
    updateDayNight(true);
  });
}

let liveMode = true;
let playing = false;
let playTimer = 0;
let replayCache = [];
let replayLoadedAt = 0;
let lastMsgAt = 0;
let wsLabel = "IDLE";

function nodeBand(n) {
  const band = (n && n.band) || "";
  if (band) return band;
  const khz = n && n.freq_khz;
  if (!khz) return "inet";
  return "?";
}

function nodeFreq(n) {
  if (n && n.frequency) return String(n.frequency).replace(/ MHz$/i, "");
  const khz = n && n.freq_khz;
  if (!khz) return "";
  return (Number(khz) / 1000).toFixed(3);
}

function kv(rows) {
  return `<div class="kv">${rows.map(([k, v]) => `<span>${escapeHtml(k)}</span><b>${escapeHtml(v)}</b>`).join("")}</div>`;
}

function escapeHtml(value) {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function modeMark(mode, cls) {
  return WCR.markSvg(MODE_COLOR[mode] || "#7d9bff", cls || "mode-mark");
}

function upsertNode(n) {
  if (n.lat == null || n.lon == null) return;
  const mode = n.mode || "";
  let m = markers.get(n.callsign);
  if (!m) {
    const el = document.createElement("div");
    el.className = "map-mark-wrap";
    el.dataset.mode = mode;
    el.dataset.band = nodeBand(n);
    el.style.opacity = selectedBand && nodeBand(n) !== selectedBand ? "0.25" : "";
    el.innerHTML = modeMark(mode, "map-mark");
    m = new maplibregl.Marker({ element: el }).setLngLat([n.lon, n.lat]).addTo(map);
    el.addEventListener("click", () => showCard(n));
    markers.set(n.callsign, m);
  } else {
    m.setLngLat([n.lon, n.lat]);
    const el = m.getElement();
    if (el.dataset.mode !== mode) {
      el.dataset.mode = mode;
      el.innerHTML = modeMark(mode, "map-mark");
    }
    el.dataset.band = nodeBand(n);
    el.style.opacity = selectedBand && nodeBand(n) !== selectedBand ? "0.25" : "";
  }
}

function showCard(n) {
  if (!card) return;
  card.hidden = false;
  card.innerHTML = kv([
    ["call", n.callsign || "—"],
    ["mode", n.mode || "—"],
    ["ptt", n.ptt || "—"],
    ["preset", n.preset || "—"],
    ["band", nodeBand(n)],
    ["freq", nodeFreq(n) || "—"],
    ["grid", n.grid || "—"],
    ["snr", n.snr ?? "—"],
  ]);
  if (stationList) {
    stationList.querySelectorAll(".station").forEach((el) => {
      el.classList.toggle("on", el.dataset.call === (n.callsign || ""));
    });
  }
}

function renderModes(nodes) {
  const counts = {};
  nodes.forEach((n) => {
    const m = n.mode || "unknown";
    counts[m] = (counts[m] || 0) + 1;
  });
  const total = nodes.length || 1;
  const grid = document.getElementById("mode-grid");
  const meta = document.getElementById("mode-meta");
  if (meta) meta.textContent = String(nodes.length);
  if (!grid) return;
  grid.innerHTML = Object.keys(MODE_COLOR).map((mode) => {
    const n = counts[mode] || 0;
    const pct = Math.round((n / total) * 100);
    return `<div class="mode-row">${modeMark(mode)}<span class="mode-name">${escapeHtml(mode)}</span><div class="mode-bar"><i style="width:${pct}%;background:${MODE_COLOR[mode]}"></i></div>${n}</div>`;
  }).join("");
}

function renderBands(bands) {
  lastBands = bands || [];
  const grid = document.getElementById("band-grid");
  const meta = document.getElementById("band-meta");
  if (meta) meta.textContent = String(lastBands.length);
  if (!grid) return;
  if (!lastBands.length) {
    grid.innerHTML = `<div class="mode-row"><span class="mode-name">no RF yet</span></div>`;
    return;
  }
  grid.innerHTML = lastBands.map((b) => {
    const name = b.band || "inet";
    const freq = b.frequency || (b.freq_khz ? (b.freq_khz / 1000).toFixed(3) : "");
    const on = selectedBand === name ? " on" : "";
    return `<div class="mode-row band-row${on}" data-band="${escapeHtml(name)}"><span class="mode-name">${escapeHtml(name)}</span><span class="band-freq">${escapeHtml(freq)}</span><span class="band-n">${escapeHtml(b.stations ?? 0)}</span></div>`;
  }).join("");
  grid.querySelectorAll(".band-row").forEach((el) => {
    el.onclick = () => {
      const band = el.dataset.band;
      selectedBand = selectedBand === band ? null : band;
      renderBands(lastBands);
      renderStations(lastNodes);
    };
  });
}

function renderStations(nodes) {
  lastNodes = nodes || [];
  const shown = selectedBand
    ? lastNodes.filter((n) => nodeBand(n) === selectedBand)
    : lastNodes;
  stationList.innerHTML = "";
  shown.forEach((n) => {
    const d = document.createElement("div");
    d.className = "station";
    d.dataset.call = n.callsign || "";
    const band = nodeBand(n);
    const freq = nodeFreq(n);
    d.innerHTML = `${modeMark(n.mode)}<span><b>${escapeHtml(n.callsign)}</b>${escapeHtml(n.mode || "")} ${escapeHtml(band)}${freq ? " " + escapeHtml(freq) : ""}</span>`;
    d.onclick = () => {
      showCard(n);
      if (n.lat != null) map.flyTo({ center: [n.lon, n.lat], zoom: 6 });
    };
    stationList.appendChild(d);
    upsertNode(n);
  });
  lastNodes.forEach((n) => upsertNode(n));
  document.getElementById("n-online").textContent = lastNodes.length;
  const sc = document.getElementById("station-count");
  if (sc) sc.textContent = String(lastNodes.length);
  renderModes(lastNodes);
}

function tick(line) {
  if (!ticker) return;
  const p = document.createElement("div");
  p.className = "reel-line";
  p.textContent = line;
  ticker.prepend(p);
  const cap = ticker.classList.contains("reel") ? 120 : 8;
  while (ticker.children.length > cap) ticker.removeChild(ticker.lastChild);
}

function isLive() {
  return Number(scrub.value) >= SPAN;
}

function syncTapeClock() {
  if (clock) clock.textContent = clockLabel();
  const meta = document.getElementById("tape-clock-meta");
  if (meta) meta.textContent = isLive() ? "LIVE" : playing ? "PLAY" : "PAUSE";
}

function clockLabel() {
  if (isLive()) {
    return playing
      ? "Reaching live…"
      : "LIVE · drag to rewind, or press Play to replay the day";
  }
  const ago = SPAN - Number(scrub.value);
  const h = Math.floor(ago / 60);
  const m = ago % 60;
  const when = new Date(Date.now() - ago * 60000);
  const hh = String(when.getHours()).padStart(2, "0");
  const mm = String(when.getMinutes()).padStart(2, "0");
  const rel = h ? `${h}h ${String(m).padStart(2, "0")}m ago` : `${m}m ago`;
  return `${playing ? "Playing" : "Paused"} ${rel}  (${hh}:${mm})`;
}

function setPlaying(on) {
  playing = on;
  playBtn.textContent = on ? "Pause" : "Play";
  playBtn.setAttribute("aria-pressed", on ? "true" : "false");
  playBtn.setAttribute(
    "aria-label",
    on ? "Pause replay" : "Play last 24 hours of traffic",
  );
  if (!on && playTimer) {
    clearTimeout(playTimer);
    playTimer = 0;
  }
}

function drawSpark(events) {
  const c = document.getElementById("spark");
  if (!c) return;
  const ctx = c.getContext("2d");
  const w = c.width;
  const h = c.height;
  ctx.clearRect(0, 0, w, h);
  const bins = 48;
  const now = Date.now() / 1000;
  const span = SPAN * 60;
  const counts = new Array(bins).fill(0);
  (events || []).forEach((x) => {
    const ts = x.ts || 0;
    const age = now - ts;
    if (age < 0 || age > span) return;
    const i = Math.min(bins - 1, Math.floor(((span - age) / span) * bins));
    counts[i] += 1;
  });
  const max = Math.max(1, ...counts);
  ctx.strokeStyle = "#7d9bff";
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  counts.forEach((n, i) => {
    const x = (i / (bins - 1)) * w;
    const y = h - (n / max) * (h - 4) - 2;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
}

async function ensureReplay() {
  if (Date.now() - replayLoadedAt < 20000 && replayCache.length) return;
  const since = Math.floor(Date.now() / 1000) - SPAN * 60;
  try {
    const ev = await (await fetch(`${API}/api/v1/events?since=${since}&limit=500`, { signal: AbortSignal.timeout(4000) })).json();
    replayCache = ev.events || [];
    replayLoadedAt = Date.now();
    drawSpark(replayCache);
  } catch (_) {
    replayCache = [];
  }
}

function renderReplay() {
  syncTapeClock();
  updateDayNight();
  if (isLive()) {
    liveMode = true;
    return;
  }
  liveMode = false;
  const minutesAgo = SPAN - Number(scrub.value);
  const cutoff = Math.floor(Date.now() / 1000) - minutesAgo * 60;
  const rows = replayCache.filter((x) => (x.ts || 0) <= cutoff).slice(0, 80);
  ticker.innerHTML = "";
  if (!rows.length) {
    tick("No traffic in this part of the last 24 hours.");
    return;
  }
  rows.forEach((x) => {
    const t = x.ts ? new Date(x.ts * 1000).toISOString().slice(11, 19) : "--:--:--";
    tick(`[${t}] ${x.origin || "?"} ${x.band || ""} ${x.kind || ""} -> ${x.dest || ""}`.replace(/  +/g, " "));
  });
}

function goLive() {
  scrub.value = SPAN;
  liveMode = true;
  setPlaying(false);
  syncTapeClock();
  updateDayNight();
}

function playStep() {
  if (!playing) return;
  const next = Number(scrub.value) + 5;
  if (next >= SPAN) {
    goLive();
    tick("Caught up. Showing live traffic.");
    return;
  }
  scrub.value = String(next);
  renderReplay();
  playTimer = setTimeout(playStep, 50);
}

async function applyScrub() {
  syncTapeClock();
  updateDayNight();
  if (isLive()) {
    liveMode = true;
    return;
  }
  await ensureReplay();
  renderReplay();
}

function ageLabel() {
  if (!lastMsgAt) return "—";
  const s = Math.max(0, Math.round((Date.now() - lastMsgAt) / 1000));
  if (s < 60) return `${s}s ago`;
  return `${Math.floor(s / 60)}m ago`;
}

function renderConn() {
  if (!connInfo) return;
  connInfo.innerHTML = kv([
    ["api", API.replace(/^https?:\/\//, "")],
    ["ws", wsLabel],
    ["last", ageLabel()],
  ]);
  if (wsState) wsState.textContent = wsLabel;
}

playBtn.addEventListener("click", () => {
  if (playing) {
    setPlaying(false);
    syncTapeClock();
    return;
  }
  if (isLive()) scrub.value = "0";
  liveMode = false;
  setPlaying(true);
  renderReplay();
  playTimer = setTimeout(playStep, 50);
  ensureReplay().then(() => {
    if (!isLive()) renderReplay();
  });
});

scrub.addEventListener("input", () => {
  setPlaying(false);
  applyScrub();
});

async function refresh() {
  try {
    const nodes = await (await fetch(`${API}/api/v1/nodes`)).json();
    renderStations(nodes.nodes || []);
    try {
      const bands = await (await fetch(`${API}/api/v1/bands`)).json();
      renderBands(bands.bands || []);
    } catch (_) {
      renderBands([]);
    }
    const hubs = await (await fetch(`${API}/api/v1/hubs`)).json();
    const h = (hubs.hubs || [])[0];
    if (h) {
      hubPanel.innerHTML = kv([
        ["id", h.id || "—"],
        ["nodes", h.connected_nodes ?? "—"],
        ["fwd", h.forwarded ?? "—"],
        ["up", `${h.uptime_secs ?? "—"}s`],
      ]);
      const hm = document.getElementById("hub-meta");
      if (hm) hm.textContent = h.id || "online";
    }
    const stats = await (await fetch(`${API}/api/v1/stats`)).json();
    document.getElementById("n-tx").textContent = stats.forwarded ?? "—";
    await ensureReplay();
  } catch (e) {
    hubPanel.textContent = "hub unreachable — showing last data";
  }
  renderConn();
}

function connectLive() {
  const proto = API.startsWith("https") ? "wss" : "ws";
  const host = API.replace(/^https?:\/\//, "");
  wsLabel = "CONNECT";
  renderConn();
  const ws = new WebSocket(`${proto}://${host}/ws/live`);
  ws.onopen = () => { wsLabel = "LIVE"; lastMsgAt = Date.now(); renderConn(); };
  ws.onmessage = (ev) => {
    try {
      const m = JSON.parse(ev.data);
      lastMsgAt = Date.now();
      if (m.type === "node") refresh();
      if (!liveMode) return;
      const t = new Date().toISOString().slice(11, 19);
      const origin = m.origin || m.callsign || "?";
      const from = m.from_band || m.band || "";
      const dest = m.dest || "";
      const kind = m.kind || m.type || "";
      const to = Array.isArray(m.to_bands) && m.to_bands.length
        ? ` -> ${m.to_bands.join(", ")}`
        : "";
      tick(`[${t}] ${origin}${from ? " " + from : ""} ${kind}${dest ? " -> " + dest : ""}${to}`);
    } catch (_) {}
  };
  ws.onclose = () => {
    wsLabel = "RETRY";
    renderConn();
    setTimeout(connectLive, 4000);
  };
}

setInterval(renderConn, 1000);
renderModes([]);
refresh();
setInterval(refresh, 15000);
connectLive();
