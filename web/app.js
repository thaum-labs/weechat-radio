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

const markers = new Map();
const ticker = document.getElementById("ticker");
const stationList = document.getElementById("station-list");
const hubPanel = document.getElementById("hub-panel");
const card = document.getElementById("card");

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
      const t = new Date().toISOString().slice(11, 19);
      tick(`[${t}] ${m.origin || m.callsign || "?"} ${m.kind || m.type || ""} -> ${m.dest || ""}`);
    } catch (_) {}
  };
  ws.onclose = () => setTimeout(connectLive, 4000);
}

document.getElementById("scrub").addEventListener("input", async (e) => {
  const minutesAgo = 1440 - Number(e.target.value);
  const since = Math.floor(Date.now() / 1000) - minutesAgo * 60;
  const ev = await (await fetch(`${API}/api/v1/events?since=${since}&limit=200`)).json();
  ticker.innerHTML = "";
  (ev.events || []).slice(0, 12).forEach((x) => {
    tick(`[replay] ${x.origin || "?"} ${x.kind} -> ${x.dest || ""}`);
  });
});

refresh();
setInterval(refresh, 15000);
connectLive();
