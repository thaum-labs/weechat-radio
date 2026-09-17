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

const map = new maplibregl.Map({
  container: "map",
  style: rasterStyle(initialMapStyle),
  center: [0, 20],
  zoom: 1.4,
  attributionControl: true,
});

if (mapStyleSelect) {
  mapStyleSelect.addEventListener("change", () => {
    const id = MAP_STYLES[mapStyleSelect.value] ? mapStyleSelect.value : "satellite";
    localStorage.setItem(MAP_STYLE_KEY, id);
    map.setStyle(rasterStyle(id));
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

let liveMode = true;
let playing = false;
let playTimer = 0;
let replayCache = [];
let replayLoadedAt = 0;
let lastMsgAt = 0;
let wsLabel = "IDLE";

function kv(rows) {
  return `<div class="kv">${rows.map(([k, v]) => `<span>${k}</span><b>${v}</b>`).join("")}</div>`;
}

function upsertNode(n) {
  if (n.lat == null || n.lon == null) return;
  const color = MODE_COLOR[n.mode] || "#aacfd1";
  let m = markers.get(n.callsign);
  if (!m) {
    const el = document.createElement("div");
    el.className = "dot";
    el.style.background = color;
    el.style.boxShadow = `0 0 12px ${color}`;
    el.style.width = "10px";
    el.style.height = "10px";
    m = new maplibregl.Marker({ element: el }).setLngLat([n.lon, n.lat]).addTo(map);
    el.addEventListener("click", () => showCard(n));
    markers.set(n.callsign, m);
  } else {
    m.setLngLat([n.lon, n.lat]);
  }
}

function showCard(n) {
  card.innerHTML = kv([
    ["call", n.callsign || "—"],
    ["mode", n.mode || "—"],
    ["ptt", n.ptt || "—"],
    ["preset", n.preset || "—"],
    ["grid", n.grid || "—"],
    ["snr", n.snr ?? "—"],
  ]);
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
    return `<div class="mode-row"><span class="dot" style="background:${MODE_COLOR[mode]}"></span>${mode}<div class="mode-bar"><i style="width:${pct}%;background:${MODE_COLOR[mode]}"></i></div>${n}</div>`;
  }).join("");
}

function renderStations(nodes) {
  stationList.innerHTML = "";
  nodes.forEach((n) => {
    const d = document.createElement("div");
    d.className = "station";
    d.innerHTML = `<span class="dot" style="background:${MODE_COLOR[n.mode] || "#aacfd1"}"></span><b>${n.callsign}</b>${n.mode || ""} ${n.preset || ""}`;
    d.onclick = () => {
      showCard(n);
      if (n.lat != null) map.flyTo({ center: [n.lon, n.lat], zoom: 6 });
    };
    stationList.appendChild(d);
    upsertNode(n);
  });
  document.getElementById("n-online").textContent = nodes.length;
  const sc = document.getElementById("station-count");
  if (sc) sc.textContent = String(nodes.length);
  renderModes(nodes);
}

function tick(line) {
  const p = document.createElement("div");
  p.textContent = line;
  ticker.prepend(p);
  while (ticker.children.length > 8) ticker.removeChild(ticker.lastChild);
}

function isLive() {
  return Number(scrub.value) >= SPAN;
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
  clock.textContent = clockLabel();
  if (isLive()) {
    liveMode = true;
    return;
  }
  liveMode = false;
  const minutesAgo = SPAN - Number(scrub.value);
  const cutoff = Math.floor(Date.now() / 1000) - minutesAgo * 60;
  const rows = replayCache.filter((x) => (x.ts || 0) <= cutoff).slice(0, 12);
  ticker.innerHTML = "";
  if (!rows.length) {
    tick("No traffic in this part of the last 24 hours.");
    return;
  }
  rows.forEach((x) => {
    const t = x.ts ? new Date(x.ts * 1000).toISOString().slice(11, 19) : "--:--:--";
    tick(`[${t}] ${x.origin || "?"} ${x.kind || ""} -> ${x.dest || ""}`);
  });
}

function goLive() {
  scrub.value = SPAN;
  liveMode = true;
  setPlaying(false);
  clock.textContent = clockLabel();
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
  clock.textContent = clockLabel();
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
    clock.textContent = clockLabel();
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
      tick(`[${t}] ${m.origin || m.callsign || "?"} ${m.kind || m.type || ""} -> ${m.dest || ""}`);
    } catch (_) {}
  };
  ws.onclose = () => {
    wsLabel = "RETRY";
    renderConn();
    setTimeout(connectLive, 4000);
  };
}

setInterval(renderConn, 1000);
refresh();
setInterval(refresh, 15000);
connectLive();
