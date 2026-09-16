/* SPDX-License-Identifier: Apache-2.0 */
const API = (window.WCR_API || (location.hostname === "localhost"
  ? "http://127.0.0.1:7373"
  : `${location.protocol}//hub.${location.hostname.replace(/^www\./, "")}`));

const MODE_COLOR = {
  internet: "#00e5ff",
  "internet-radio": "#39ff14",
  radio: "#ffbf00",
  "radio-plus": "#ff4dff",
};

const CARTO_KEY = "cb1_3nxz_1_dba340f0a1c8450af09da179";

const map = new maplibregl.Map({
  container: "map",
  style: {
    version: 8,
    sources: {
      osm: {
        type: "raster",
        tiles: [
          `https://basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png?key=${CARTO_KEY}`,
        ],
        tileSize: 256,
        attribution:
          '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>, &copy; <a href="https://carto.com/attributions">CARTO</a>',
      },
    },
    layers: [{ id: "osm", type: "raster", source: "osm" }],
  },
  center: [0, 20],
  zoom: 1.4,
  attributionControl: true,
});

const SPAN = 1440;
const markers = new Map();
const ticker = document.getElementById("ticker");
const stationList = document.getElementById("station-list");
const hubPanel = document.getElementById("hub-panel");
const card = document.getElementById("card");
const scrub = document.getElementById("scrub");
const playBtn = document.getElementById("play");
const clock = document.getElementById("tape-clock");

let liveMode = true;
let playing = false;
let playTimer = 0;
let replayCache = [];
let replayLoadedAt = 0;

function upsertNode(n) {
  if (n.lat == null || n.lon == null) return;
  const color = MODE_COLOR[n.mode] || "#39ff14";
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
  card.hidden = false;
  card.innerHTML = `<b>${n.callsign}</b><br>mode ${n.mode}<br>ptt ${n.ptt || "—"}<br>preset ${n.preset || "—"}<br>grid ${n.grid || "—"}<br>snr ${n.snr ?? "—"}`;
}

function renderStations(nodes) {
  stationList.innerHTML = "";
  nodes.forEach((n) => {
    const d = document.createElement("div");
    d.className = "station";
    d.innerHTML = `<span class="dot" style="background:${MODE_COLOR[n.mode] || "#39ff14"}"></span><b>${n.callsign}</b>${n.mode || ""} ${n.preset || ""}`;
    d.onclick = () => {
      showCard(n);
      if (n.lat != null) map.flyTo({ center: [n.lon, n.lat], zoom: 6 });
    };
    stationList.appendChild(d);
    upsertNode(n);
  });
  document.getElementById("n-online").textContent = nodes.length;
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

async function ensureReplay() {
  if (Date.now() - replayLoadedAt < 20000 && replayCache.length) return;
  const since = Math.floor(Date.now() / 1000) - SPAN * 60;
  try {
    const ev = await (await fetch(`${API}/api/v1/events?since=${since}&limit=500`)).json();
    replayCache = ev.events || [];
    replayLoadedAt = Date.now();
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

playBtn.addEventListener("click", async () => {
  if (playing) {
    setPlaying(false);
    clock.textContent = clockLabel();
    return;
  }
  await ensureReplay();
  if (isLive()) scrub.value = "0";
  liveMode = false;
  setPlaying(true);
  renderReplay();
  playTimer = setTimeout(playStep, 50);
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
      hubPanel.innerHTML = `<b>${h.id}</b><br>connected ${h.connected_nodes}<br>forwarded ${h.forwarded}<br>uptime ${h.uptime_secs}s`;
    }
    const stats = await (await fetch(`${API}/api/v1/stats`)).json();
    document.getElementById("n-tx").textContent = stats.forwarded ?? "—";
  } catch (e) {
    hubPanel.textContent = "hub unreachable — showing last data";
  }
}

function connectLive() {
  const proto = API.startsWith("https") ? "wss" : "ws";
  const host = API.replace(/^https?:\/\//, "");
  const ws = new WebSocket(`${proto}://${host}/ws/live`);
  ws.onmessage = (ev) => {
    try {
      const m = JSON.parse(ev.data);
      if (m.type === "node") refresh();
      if (!liveMode) return;
      const t = new Date().toISOString().slice(11, 19);
      tick(`[${t}] ${m.origin || m.callsign || "?"} ${m.kind || m.type || ""} -> ${m.dest || ""}`);
    } catch (_) {}
  };
  ws.onclose = () => setTimeout(connectLive, 4000);
}

refresh();
setInterval(refresh, 15000);
connectLive();
