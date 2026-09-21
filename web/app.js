/* SPDX-License-Identifier: Apache-2.0 */
const API = window.WCR.API;

const MODE_COLOR = {
  internet: "#00e5ff",
  "internet-radio": "#39ff14",
  radio: "#ffbf00",
  "radio-plus": "#ff4dff",
};
/** Email traffic overlay. Not an operating mode. */
const MAIL_COLOR = "#ff7a3d";

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
const LINK_MS = 10 * 60 * 1000;
const markers = new Map();
const nodePos = new Map();
let arcCanvas = null;
let arcCtx = null;
const qsoLinks = new Map();

function nodeKey(call) {
  if (!call) return null;
  const u = String(call).toUpperCase();
  for (const k of nodePos.keys()) {
    if (k.toUpperCase() === u) return k;
  }
  return null;
}

function ensureArcLayer() {
  const host = document.getElementById("map");
  if (!host || arcCanvas) return;
  arcCanvas = document.createElement("canvas");
  arcCanvas.id = "map-arcs";
  host.style.position = "relative";
  host.appendChild(arcCanvas);
  arcCtx = arcCanvas.getContext("2d");
  syncArcCanvas();
  map.on("move", drawArcs);
  map.on("resize", () => {
    syncArcCanvas();
    drawArcs();
  });
}

function syncArcCanvas() {
  const host = document.getElementById("map");
  if (!host || !arcCanvas || !arcCtx) return;
  const dpr = window.devicePixelRatio || 1;
  const w = host.clientWidth || 800;
  const h = host.clientHeight || 600;
  arcCanvas.style.width = `${w}px`;
  arcCanvas.style.height = `${h}px`;
  arcCanvas.width = Math.round(w * dpr);
  arcCanvas.height = Math.round(h * dpr);
  arcCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

const lastPulseAt = new Map();

function pulseMarker(callsign, kind) {
  const key = nodeKey(callsign);
  const m = key ? markers.get(key) : markers.get(callsign);
  if (!m) return;
  const now = Date.now();
  const id = key || callsign;
  if (now - (lastPulseAt.get(id) || 0) < 500) return;
  lastPulseAt.set(id, now);
  const el = m.getElement();
  el.classList.remove("pulse-tx", "pulse-rx");
  void el.offsetWidth;
  el.classList.add(kind === "rx" ? "pulse-rx" : "pulse-tx");
}

function isTrafficKind(kind, type) {
  const k = String(kind || type || "").toLowerCase();
  return (
    k === "tx" ||
    k === "rx" ||
    k === "msg" ||
    k === "relay" ||
    k === "hub_forward" ||
    k === "gateway_forward"
  );
}

function linkPairKey(a, b) {
  const [x, y] = [a.toUpperCase(), b.toUpperCase()].sort();
  return `${x}|${y}`;
}

function arcTargets(from, to) {
  const fromKey = nodeKey(from);
  if (!fromKey) return [];
  const toKey = nodeKey(to);
  if (toKey && toKey !== fromKey) return [toKey];
  const out = [];
  for (const call of nodePos.keys()) {
    if (call.toUpperCase() !== fromKey.toUpperCase()) out.push(call);
  }
  return out;
}

function rememberLink(from, to, inet, draw, mail) {
  const fromKey = nodeKey(from);
  if (!fromKey || !to) return;
  const now = Date.now();
  for (const dest of arcTargets(fromKey, to)) {
    qsoLinks.set(linkPairKey(fromKey, dest), {
      a: fromKey,
      b: dest,
      inet: !!inet,
      mail: !!mail,
      at: now,
    });
  }
  if (draw === false) return;
  ensureArcLayer();
  drawArcs();
}

function strokePinLink(s1, s2, color, dash) {
  const dx = s2.x - s1.x;
  const dy = s2.y - s1.y;
  const dist = Math.hypot(dx, dy) || 1;
  const bulge = Math.min(90, Math.max(28, dist * 0.22));
  const mx = (s1.x + s2.x) / 2;
  const my = (s1.y + s2.y) / 2 - bulge;
  arcCtx.beginPath();
  arcCtx.moveTo(s1.x, s1.y);
  arcCtx.quadraticCurveTo(mx, my, s2.x, s2.y);
  arcCtx.strokeStyle = color;
  arcCtx.setLineDash(dash);
  arcCtx.lineWidth = 2.5;
  arcCtx.lineCap = "round";
  arcCtx.stroke();
}

function drawArcs() {
  if (!arcCtx || !arcCanvas) return;
  const cssW = arcCanvas.clientWidth || 800;
  const cssH = arcCanvas.clientHeight || 600;
  arcCtx.clearRect(0, 0, cssW, cssH);
  const now = Date.now();
  for (const [k, link] of [...qsoLinks]) {
    if (now - link.at > LINK_MS) qsoLinks.delete(k);
  }
  for (const link of qsoLinks.values()) {
    const p1 = nodePos.get(link.a);
    const p2 = nodePos.get(link.b);
    if (!p1 || !p2) continue;
    const age = (now - link.at) / LINK_MS;
    const alpha = Math.max(0.35, 0.95 - age * 0.55);
    const s1 = map.project([p1.lon, p1.lat]);
    const s2 = map.project([p2.lon, p2.lat]);
    const color = link.mail
      ? `rgba(255,122,61,${alpha})`
      : link.inet
        ? `rgba(125,155,255,${alpha})`
        : `rgba(57,255,20,${alpha})`;
    const dash = link.mail ? [3, 3] : link.inet ? [] : [6, 4];
    strokePinLink(s1, s2, color, dash);
  }
}

function noteMailActivity(origin, dest) {
  const now = Date.now();
  const o = origin && String(origin).trim();
  if (o) mailRecent.set(o.toUpperCase(), now);
  const d = dest && String(dest).trim();
  if (d && !d.includes("@")) mailRecent.set(d.toUpperCase(), now);
}

function updateMailOverlays() {
  const now = Date.now();
  mailRecent.forEach((t, k) => {
    if (now - t > LINK_MS) mailRecent.delete(k);
  });
  markers.forEach((marker, call) => {
    marker.getElement().classList.toggle("mail-overlay", mailRecent.has(String(call).toUpperCase()));
  });
}

function mailOverlayCount() {
  const now = Date.now();
  let n = 0;
  mailRecent.forEach((t) => {
    if (now - t <= LINK_MS) n += 1;
  });
  return n;
}

function seedLinksFromEvents(events) {
  const cutoff = Date.now() / 1000 - LINK_MS / 1000;
  (events || []).forEach((x) => {
    if ((x.ts || 0) < cutoff) return;
    const kind = String(x.kind || x.type || "").toLowerCase();
    if (kind === "mail") {
      noteMailActivity(x.origin, x.dest);
      if (x.origin && x.dest) rememberLink(x.origin, x.dest, false, false, true);
      return;
    }
    if (!isTrafficKind(kind, x.type) || !x.origin || !x.dest) return;
    rememberLink(x.origin, x.dest, kind !== "relay", false, false);
  });
  updateMailOverlays();
  if (qsoLinks.size) {
    ensureArcLayer();
    drawArcs();
  }
}
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
let liveLogSeeded = false;
let playing = false;
let playTimer = 0;
let replayCache = [];
let replayLoadedAt = 0;
let nodeTrail = [];
let trailLoadedAt = 0;
const mailRecent = new Map();
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
    nodePos.set(n.callsign, { lat: n.lat, lon: n.lon });
  } else {
    m.setLngLat([n.lon, n.lat]);
    const el = m.getElement();
    if (el.dataset.mode !== mode) {
      el.dataset.mode = mode;
      el.innerHTML = modeMark(mode, "map-mark");
    }
    el.dataset.band = nodeBand(n);
    el.style.opacity = selectedBand && nodeBand(n) !== selectedBand ? "0.25" : "";
    nodePos.set(n.callsign, { lat: n.lat, lon: n.lon });
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
  const emailN = mailOverlayCount();
  const modeRows = Object.keys(MODE_COLOR).map((mode) => {
    const n = counts[mode] || 0;
    const pct = Math.round((n / total) * 100);
    return `<div class="mode-row">${modeMark(mode)}<span class="mode-name">${escapeHtml(mode)}</span><div class="mode-bar"><i style="width:${pct}%;background:${MODE_COLOR[mode]}"></i></div>${n}</div>`;
  }).join("");
  grid.innerHTML = modeRows + `<div class="mode-row mail-traffic-row"><span class="mail-logo">${WCR.markSvg(MAIL_COLOR, "mode-mark")}<span class="mail-e">e</span></span><span class="mode-name">email</span><div class="mode-bar"><i style="width:${emailN ? 100 : 0}%;background:${MAIL_COLOR}"></i></div>${emailN}</div>`;
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
  updateMailOverlays();
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

function eventLine(x) {
  const t = x.ts
    ? new Date(x.ts * 1000).toISOString().slice(11, 19)
    : new Date().toISOString().slice(11, 19);
  const origin = x.origin || x.callsign || "?";
  const kind = (x.kind || x.type || "").toLowerCase();
  if (kind === "mail") {
    const dest = x.dest && !String(x.dest).includes("@") ? ` -> ${x.dest}` : "";
    return `[${t}] MAIL ${origin}${dest}`;
  }
  const from = x.from_band || x.band || "";
  const dest = x.dest || "";
  const to = Array.isArray(x.to_bands) && x.to_bands.length
    ? ` -> ${x.to_bands.join(", ")}`
    : "";
  return `[${t}] ${origin}${from ? " " + from : ""} ${kind}${dest ? " -> " + dest : ""}${to}`.replace(/  +/g, " ");
}

function seedLiveLog() {
  if (!ticker || !liveMode) return;
  ticker.innerHTML = "";
  const rows = replayCache.slice().sort((a, b) => (a.ts || 0) - (b.ts || 0));
  rows.slice(-120).forEach((x) => tick(eventLine(x)));
  liveLogSeeded = true;
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
  if (!(Date.now() - replayLoadedAt < 20000 && replayCache.length)) {
    const since = Math.floor(Date.now() / 1000) - SPAN * 60;
    try {
      const ev = await (await fetch(`${API}/api/v1/events?since=${since}&limit=500`, { signal: AbortSignal.timeout(4000) })).json();
      replayCache = ev.events || [];
      replayLoadedAt = Date.now();
      drawSpark(replayCache);
      if (isLive()) seedLinksFromEvents(replayCache);
      if (liveMode && !liveLogSeeded) seedLiveLog();
    } catch (_) {}
  }
  await ensureTrail();
}

function replayCutoff() {
  const minutesAgo = SPAN - Number(scrub.value);
  return Math.floor(Date.now() / 1000) - minutesAgo * 60;
}

function dropMarkersExcept(nodes) {
  const keep = new Set((nodes || []).map((n) => String(n.callsign || "").toUpperCase()));
  for (const [call, marker] of [...markers]) {
    if (!keep.has(String(call).toUpperCase())) {
      marker.remove();
      markers.delete(call);
      nodePos.delete(call);
    }
  }
}

function nodesFromTrail(cutoff) {
  const start = cutoff - 1800;
  const latest = new Map();
  for (const row of nodeTrail) {
    const ts = row.ts || 0;
    if (ts < start || ts > cutoff) continue;
    const key = String(row.callsign || "").toUpperCase();
    const prev = latest.get(key);
    if (!prev || (prev.ts || 0) <= ts) latest.set(key, row);
  }
  return [...latest.values()];
}

function paintHistory(cutoff) {
  if (trailLoadedAt) {
    const nodes = nodesFromTrail(cutoff);
    dropMarkersExcept(nodes);
    renderStations(nodes);
  }
  qsoLinks.clear();
  mailRecent.clear();
  const start = cutoff - LINK_MS / 1000;
  (replayCache || []).forEach((x) => {
    const ts = x.ts || 0;
    if (ts < start || ts > cutoff) return;
    const kind = String(x.kind || x.type || "").toLowerCase();
    if (kind === "mail") {
      noteMailActivity(x.origin, x.dest);
      if (x.origin && x.dest) rememberLink(x.origin, x.dest, false, false, true);
      return;
    }
    if (!isTrafficKind(kind, x.type) || !x.origin || !x.dest) return;
    rememberLink(x.origin, x.dest, kind !== "relay", false, false);
  });
  ensureArcLayer();
  drawArcs();
  updateMailOverlays();
}

async function ensureTrail() {
  if (Date.now() - trailLoadedAt < 20000 && trailLoadedAt) return;
  const since = Math.floor(Date.now() / 1000) - SPAN * 60;
  try {
    const body = await (await fetch(`${API}/api/v1/nodes?since=${since}`, { signal: AbortSignal.timeout(4000) })).json();
    if (body.since == null) {
      nodeTrail = [];
      trailLoadedAt = 0;
      return;
    }
    nodeTrail = body.trail || [];
    trailLoadedAt = Date.now();
  } catch (_) {}
}

async function renderReplay() {
  syncTapeClock();
  updateDayNight();
  if (isLive()) {
    liveMode = true;
    return;
  }
  liveMode = false;
  liveLogSeeded = false;
  const cutoff = replayCutoff();
  const rows = replayCache.filter((x) => (x.ts || 0) <= cutoff).slice(0, 80);
  ticker.innerHTML = "";
  if (!rows.length) {
    tick("No traffic in this part of the last 24 hours.");
  } else {
    rows.forEach((x) => tick(eventLine(x)));
  }
  await ensureTrail();
  paintHistory(cutoff);
}

function goLive() {
  scrub.value = SPAN;
  liveMode = true;
  setPlaying(false);
  syncTapeClock();
  updateDayNight();
  seedLiveLog();
  refresh();
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
    seedLiveLog();
    refresh();
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
    if (isLive()) renderStations(nodes.nodes || []);
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
    if (isLive()) {
      qsoLinks.clear();
      mailRecent.clear();
      seedLinksFromEvents(replayCache);
    } else {
      paintHistory(replayCutoff());
    }
  } catch (e) {
    hubPanel.textContent = "hub unreachable — showing last data";
  }
  renderConn();
}

let refreshSoon = null;
function scheduleRefresh() {
  if (refreshSoon) return;
  refreshSoon = setTimeout(() => {
    refreshSoon = null;
    refresh();
  }, 2000);
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
      if (!liveMode) return;
      if (m.type === "node") {
        scheduleRefresh();
        return;
      }
      const origin = m.origin || m.callsign || "?";
      const kind = (m.kind || m.type || "").toLowerCase();
      if (kind === "mail") {
        noteMailActivity(origin, m.dest);
        if (m.dest && !String(m.dest).includes("@")) rememberLink(origin, m.dest, false, true, true);
        tick(eventLine(m));
        return;
      }
      if (kind === "tx" || kind === "msg" || m.type === "hub_forward") pulseMarker(origin, "tx");
      if (kind === "rx" || kind === "relay") pulseMarker(origin, "rx");
      if (m.dest && isTrafficKind(kind, m.type)) {
        rememberLink(origin, m.dest, kind !== "relay");
      }
      tick(eventLine(m));
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
