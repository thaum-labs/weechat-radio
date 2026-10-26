/* SPDX-License-Identifier: Apache-2.0 */
(function () {
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const host = location.hostname;
  const local = host === "localhost" || host === "127.0.0.1";
  const PATH_API = local ? "http://127.0.0.1:7380" : "";

  const CARTO_KEY = "cb1_3nxz_1_dba340f0a1c8450af09da179";
  const MAP_STYLE_KEY = "wcr-map-style-v2";
  const NIGHT_KEY = "wcr-map-night";
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
      attribution: 'Tiles &copy; <a href="https://www.esri.com/">Esri</a> — Esri, USGS, NOAA',
    },
    satellite: {
      tiles: [
        "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}",
      ],
      attribution:
        'Tiles &copy; <a href="https://www.esri.com/">Esri</a> — Esri, Maxar, Earthstar Geographics',
    },
  };

  function rasterStyle(id) {
    const spec = MAP_STYLES[id] || MAP_STYLES.satellite;
    return {
      version: 8,
      sources: {
        osm: { type: "raster", tiles: spec.tiles, tileSize: 256, attribution: spec.attribution },
      },
      layers: [{ id: "osm", type: "raster", source: "osm" }],
    };
  }

  function preferredMapStyle() {
    const saved = localStorage.getItem(MAP_STYLE_KEY);
    return MAP_STYLES[saved] ? saved : "satellite";
  }

  const mapStyleSelect = document.getElementById("map-style");
  const nightToggle = document.getElementById("map-night");
  const initialMapStyle = preferredMapStyle();
  if (mapStyleSelect) mapStyleSelect.value = initialMapStyle;
  if (nightToggle) nightToggle.checked = localStorage.getItem(NIGHT_KEY) !== "0";

  const coarse = matchMedia("(pointer: coarse)").matches;
  const map = new maplibregl.Map({
    container: "map",
    style: rasterStyle(initialMapStyle),
    center: [0, 20],
    zoom: 1.6,
    attributionControl: false,
    cooperativeGestures: coarse,
    dragRotate: !coarse,
    pitchWithRotate: !coarse,
    touchPitch: !coarse,
  });
  map.addControl(new maplibregl.AttributionControl({ compact: false }), "bottom-right");
  map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");

  const hintEl = document.getElementById("band-hint");
  const waitEl = document.getElementById("band-wait");
  const emptyEl = document.getElementById("band-empty");
  const cardEl = document.getElementById("band-card");
  const searchEl = document.getElementById("band-search");
  const placesEl = document.getElementById("band-places");
  const locateBtn = document.getElementById("band-locate");
  const onAirBtn = document.getElementById("band-onair");
  const hourEl = document.getElementById("band-hour");
  const hourLabel = document.getElementById("band-hour-label");
  const txAntEl = document.getElementById("band-tx-ant");
  const rxAntEl = document.getElementById("band-rx-ant");
  const powerEl = document.getElementById("band-power");
  const profileEl = document.getElementById("band-profile");

  let tx = null;
  let rx = null;
  let txMarker = null;
  let rxMarker = null;
  let abort = null;
  let pathFailed = false;
  let arcCanvas = null;
  let arcCtx = null;
  let answer = null;

  function hourOffset() {
    return hourEl ? Number(hourEl.value) || 0 : 0;
  }
  function planDate() {
    return new Date(Date.now() + hourOffset() * 3600000);
  }
  function planHourUtc() {
    return planDate().getUTCHours();
  }
  // Each end has a preset list plus "other…", which shows a metres box.
  function antennaM(sel) {
    if (!sel) return 10;
    const box = sel.parentElement.querySelector("input");
    const v = Number(sel.value === "other" ? box && box.value : sel.value);
    return Number.isFinite(v) && v >= 0.5 && v <= 100 ? v : 10;
  }
  function txAntM() {
    return antennaM(txAntEl);
  }
  function rxAntM() {
    return antennaM(rxAntEl);
  }
  function setAntenna(sel, m) {
    if (!sel || !Number.isFinite(m) || m < 0.5 || m > 100) return;
    const hit = [...sel.options].find((o) => Number(o.value) === m);
    sel.value = hit ? hit.value : "other";
    if (!hit) sel.parentElement.querySelector("input").value = String(m);
    showAntennaBox(sel);
  }
  function showAntennaBox(sel) {
    const other = sel.value === "other";
    sel.parentElement.querySelectorAll("input, i").forEach((el) => (el.hidden = !other));
  }
  function powerW() {
    const v = powerEl ? Number(powerEl.value) : 100;
    return Number.isFinite(v) && v > 0 ? v : 100;
  }
  function showHourLabel() {
    if (!hourLabel) return;
    const off = hourOffset();
    const hh = String(planHourUtc()).padStart(2, "0");
    hourLabel.textContent = off ? `${hh}:00 UTC · +${off} h` : "now";
  }
  function markEl(color, kind) {
    const wrap = document.createElement("div");
    wrap.className = "map-mark-wrap";
    wrap.innerHTML = (window.WCR && WCR.markSvg)
      ? WCR.markSvg(color, "mode-mark")
      : `<span style="color:${color}">●</span>`;
    const label = document.createElement("span");
    label.className = "band-pin-label";
    label.textContent = kind;
    wrap.appendChild(label);
    return wrap;
  }

  function pulse(el, kind) {
    if (!el || reduceMotion) return;
    el.classList.remove("pulse-tx", "pulse-rx");
    void el.offsetWidth;
    el.classList.add(kind === "rx" ? "pulse-rx" : "pulse-tx");
  }

  function setHint(_step, text) {
    if (hintEl) hintEl.textContent = text;
  }

  const NIGHT_SRC = "daynight";
  const NIGHT_FILL = "daynight-fill";
  const TWILIGHT_ALTS = [0, -2, -4, -6, -8, -10, -12, -14, -16, -18];
  const TWILIGHT_OPACITY = [0.06, 0.11, 0.16, 0.21, 0.26, 0.31, 0.36, 0.41, 0.46];
  let lastNightWall = 0;

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
      for (let lng = -180; lng <= 180; lng += step) line.push([lng, terminatorLat(lng, sun, gst, alt)]);
      return line;
    });
    const features = lines.slice(0, -1).map((outer, i) => ({
      type: "Feature",
      properties: { kind: `twilight-${i}` },
      geometry: { type: "MultiPolygon", coordinates: worldPolys(closeBand(outer, lines[i + 1])) },
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
  function nightOn() {
    return !nightToggle || nightToggle.checked;
  }
  function twilightLayerId(i) {
    return `daynight-twilight-${i}`;
  }
  function ensureDayNightLayers() {
    if (!map.getSource(NIGHT_SRC)) {
      map.addSource(NIGHT_SRC, { type: "geojson", data: nightGeoJSON(planDate()) });
    }
    TWILIGHT_OPACITY.forEach((opacity, i) => {
      const id = twilightLayerId(i);
      if (map.getLayer(id)) return;
      map.addLayer({
        id,
        type: "fill",
        source: NIGHT_SRC,
        filter: ["==", ["get", "kind"], `twilight-${i}`],
        paint: { "fill-color": "#020617", "fill-opacity": opacity, "fill-antialias": true },
      });
    });
    if (!map.getLayer(NIGHT_FILL)) {
      map.addLayer({
        id: NIGHT_FILL,
        type: "fill",
        source: NIGHT_SRC,
        filter: ["==", ["get", "kind"], "night"],
        paint: { "fill-color": "#020617", "fill-opacity": 0.5, "fill-antialias": true },
      });
    }
    const vis = nightOn() ? "visible" : "none";
    TWILIGHT_OPACITY.forEach((_, i) => map.setLayoutProperty(twilightLayerId(i), "visibility", vis));
    map.setLayoutProperty(NIGHT_FILL, "visibility", vis);
  }
  function updateDayNight(force) {
    const now = Date.now();
    if (!force && now - lastNightWall < 200 && map.getLayer(NIGHT_FILL)) return;
    lastNightWall = now;
    try {
      ensureDayNightLayers();
      const src = map.getSource(NIGHT_SRC);
      if (src) src.setData(nightGeoJSON(planDate()));
    } catch (_) {}
  }

  // Copernicus DEM via Open-Meteo. CARTO's path-profile tool needs a
  // BigQuery warehouse, so it is not used here. TX and RX antenna heights
  // from the page, 4/3 earth.
  const K_EARTH_KM = 6371 * (4 / 3);
  let terrain = null;
  let elevation = null;

  function distKm(a, b) {
    const R = 6371;
    const φ1 = (a.lat * Math.PI) / 180;
    const φ2 = (b.lat * Math.PI) / 180;
    const dφ = ((b.lat - a.lat) * Math.PI) / 180;
    const dλ = ((b.lon - a.lon) * Math.PI) / 180;
    const h = Math.sin(dφ / 2) ** 2 + Math.cos(φ1) * Math.cos(φ2) * Math.sin(dλ / 2) ** 2;
    return 2 * R * Math.asin(Math.min(1, Math.sqrt(h)));
  }

  function along(a, b, f) {
    const φ1 = (a.lat * Math.PI) / 180;
    const λ1 = (a.lon * Math.PI) / 180;
    const φ2 = (b.lat * Math.PI) / 180;
    const λ2 = (b.lon * Math.PI) / 180;
    const d = 2 * Math.asin(Math.sqrt(
      Math.sin((φ2 - φ1) / 2) ** 2 + Math.cos(φ1) * Math.cos(φ2) * Math.sin((λ2 - λ1) / 2) ** 2
    ));
    if (d < 1e-9) return { lat: a.lat, lon: a.lon };
    const A = Math.sin((1 - f) * d) / Math.sin(d);
    const B = Math.sin(f * d) / Math.sin(d);
    const x = A * Math.cos(φ1) * Math.cos(λ1) + B * Math.cos(φ2) * Math.cos(λ2);
    const y = A * Math.cos(φ1) * Math.sin(λ1) + B * Math.cos(φ2) * Math.sin(λ2);
    const z = A * Math.sin(φ1) + B * Math.sin(φ2);
    return {
      lat: (Math.atan2(z, Math.hypot(x, y)) * 180) / Math.PI,
      lon: (Math.atan2(y, x) * 180) / Math.PI,
    };
  }

  function setLosNote(text) {
    const el = document.getElementById("los-note");
    if (el) el.textContent = text;
  }

  // A sample every 200 m so narrow ridges are not missed, 100 points per
  // Open-Meteo call. Past 200 km the spacing grows to keep it to 10 calls;
  // such paths are beyond VHF line of sight unless both ends are on peaks.
  const SAMPLE_KM = 0.2;
  const MAX_SAMPLES = 1000;
  const PER_CALL = 100;

  async function fetchElevation(a, b, signal) {
    const total = distKm(a, b);
    const n = Math.min(MAX_SAMPLES - 1, Math.max(8, Math.ceil(total / SAMPLE_KM)));
    const pts = [];
    for (let i = 0; i <= n; i++) pts.push(along(a, b, i / n));
    const chunks = [];
    for (let i = 0; i < pts.length; i += PER_CALL) chunks.push(pts.slice(i, i + PER_CALL));
    const parts = await Promise.all(
      chunks.map(async (part) => {
        const url =
          "https://api.open-meteo.com/v1/elevation?latitude=" +
          part.map((p) => p.lat.toFixed(5)).join(",") +
          "&longitude=" +
          part.map((p) => p.lon.toFixed(5)).join(",");
        const res = await fetch(url, { signal });
        if (!res.ok) return null;
        const body = await res.json();
        const elev = body && body.elevation;
        return elev && elev.length === part.length ? elev : null;
      })
    );
    if (parts.some((p) => !p)) return null;
    const elev = parts.flat();
    let known = 0;
    let d = 0;
    const samples = pts.map((p, i) => {
      if (i) d += distKm(pts[i - 1], p);
      const e = elev[i];
      if (typeof e === "number" && Number.isFinite(e)) known += 1;
      return { lat: p.lat, lon: p.lon, dKm: d, elev: typeof e === "number" ? e : null };
    });
    if (known < pts.length / 2) return null;
    return samples;
  }

  // Straight radio line from the TX antenna top to the RX antenna top
  // against ground plus the 4/3-earth bulge, judged with the first Fresnel
  // zone (F1) at `mhz`. Each sample is "clear" when 60% of F1 is free,
  // "blocked" when the ground stands more than 0.6 x F1 above the line
  // (knife-edge loss well past 10 dB), and "marginal" in between: a near
  // miss that may work with more height or power. `byCurve` means a smooth
  // earth at the lower pin's ground height would already block the line (a
  // long path, often over sea), so the path is beyond line of sight whatever
  // the hills do. `byTerrain` means the earth's curve alone leaves the line
  // clear and the ground is in the way. `worst` is the tightest sample.
  function lineOfSight(raw, txAnt, rxAnt, mhz) {
    const samples = raw.map((s) => ({ ...s }));
    const D = samples[samples.length - 1].dKm;
    const y0 = (samples[0].elev || 0) + txAnt;
    const y1 = (samples[samples.length - 1].elev || 0) + rxAnt;
    const floor = Math.min(samples[0].elev || 0, samples[samples.length - 1].elev || 0);
    const lambdaM = 299.792458 / (mhz || 145);
    let blocked = false;
    let marginal = false;
    let byCurve = false;
    let worst = null;
    for (const s of samples) {
      const d1 = s.dKm;
      const d2 = Math.max(0, D - d1);
      s.bulge = (d1 * d2 * 1000) / (2 * K_EARTH_KM);
      s.line = D < 1e-6 ? y0 : y0 + (y1 - y0) * (d1 / D);
      s.state = "clear";
      if (s.elev == null) continue;
      const inside = d1 > 0.05 && d2 > 0.05;
      if (!inside) continue;
      s.clearance = s.line - (s.elev + s.bulge);
      s.f1 = Math.sqrt((lambdaM * d1 * d2 * 1000) / D);
      s.margin = s.clearance / s.f1;
      if (s.margin <= -0.6) s.state = "blocked";
      else if (s.margin < 0.6) s.state = "marginal";
      if (s.state === "blocked") blocked = true;
      if (s.state === "marginal") marginal = true;
      if (!worst || s.margin < worst.margin) worst = s;
      if (s.line < floor + s.bulge) byCurve = true;
    }
    const byTerrain = blocked && !byCurve;
    marginal = marginal && !blocked;
    return { samples, blocked, marginal, byTerrain, worst, y0, y1, D, txAnt, rxAnt };
  }

  // "just clips a hill at 12 km" when the ground pokes above the line,
  // "skims the ground at 12 km" when it only crowds the Fresnel zone.
  function marginalText() {
    const w = terrain && terrain.worst;
    if (!w) return "";
    const km = w.dKm >= 10 ? Math.round(w.dKm) : w.dKm.toFixed(1);
    return `${w.clearance < 0 ? "just clips a hill" : "skims the ground"} at ${km} km, may work with more height or power`;
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
    map.on("move", paintPath);
    map.on("resize", () => {
      syncArcCanvas();
      paintPath();
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

  // Same screen-space arc as the live map (strokePinLink in app.js).
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

  // Blue clear, yellow marginal, red blocked: same colours as the profile.
  const TRACK_COLORS = {
    clear: "rgba(125,155,255,0.9)",
    marginal: "rgba(255,214,10,0.95)",
    blocked: "rgba(255,59,107,0.95)",
  };
  const STATE_RANK = { clear: 0, marginal: 1, blocked: 2 };
  function segmentState(a, b) {
    return STATE_RANK[a.state] >= STATE_RANK[b.state] ? a.state : b.state;
  }

  function paintPath() {
    ensureArcLayer();
    if (!arcCtx || !arcCanvas) return;
    syncArcCanvas();
    const cssW = arcCanvas.clientWidth || 800;
    const cssH = arcCanvas.clientHeight || 600;
    arcCtx.clearRect(0, 0, cssW, cssH);
    if (!tx || !rx) return;
    const s1 = map.project([tx.lon, tx.lat]);
    const s2 = map.project([rx.lon, rx.lat]);
    const color = pathFailed ? "rgba(122,115,136,0.9)" : "rgba(57,255,20,0.95)";
    const dash = pathFailed ? [5, 5] : [6, 4];
    if (!pathFailed && !reduceMotion && waitEl && !waitEl.hidden) {
      arcCtx.shadowColor = color;
      arcCtx.shadowBlur = 10;
    }
    strokePinLink(s1, s2, color, dash);
    arcCtx.shadowBlur = 0;
    if (!terrain || !terrain.samples) return;
    arcCtx.lineWidth = 4;
    arcCtx.lineCap = "round";
    for (let i = 1; i < terrain.samples.length; i++) {
      const a = terrain.samples[i - 1];
      const b = terrain.samples[i];
      const state = segmentState(a, b);
      const p1 = map.project([a.lon, a.lat]);
      const p2 = map.project([b.lon, b.lat]);
      arcCtx.beginPath();
      arcCtx.moveTo(p1.x, p1.y);
      arcCtx.lineTo(p2.x, p2.y);
      arcCtx.strokeStyle = TRACK_COLORS[state];
      arcCtx.setLineDash(state === "clear" ? [6, 4] : []);
      arcCtx.stroke();
    }
  }

  function placeMarker(kind, lngLat) {
    const color = kind === "tx" ? "#39ff14" : "#00e5ff";
    const el = markEl(color, kind.toUpperCase());
    const marker = new maplibregl.Marker({ element: el, draggable: true })
      .setLngLat(lngLat)
      .addTo(map);
    pulse(el, kind);
    marker.on("dragend", () => {
      const ll = marker.getLngLat();
      if (kind === "tx") tx = { lat: ll.lat, lon: ll.lng };
      else rx = { lat: ll.lat, lon: ll.lng };
      terrain = null;
      if (tx && rx) requestCard();
      else {
        paintPath();
        saveUrl();
      }
    });
    return marker;
  }

  function setTx(ll) {
    tx = { lat: ll.lat, lon: ll.lng };
    if (txMarker) txMarker.remove();
    txMarker = placeMarker("tx", ll);
    terrain = null;
    if (!rx) {
      setHint("RX", "Click the other station");
      saveUrl();
    } else requestCard();
  }

  function setRx(ll) {
    rx = { lat: ll.lat, lon: ll.lng };
    if (rxMarker) rxMarker.remove();
    rxMarker = placeMarker("rx", ll);
    requestCard();
  }

  function fitPins() {
    if (!tx || !rx) return;
    const b = new maplibregl.LngLatBounds();
    b.extend([tx.lon, tx.lat]);
    b.extend([rx.lon, rx.lat]);
    map.fitBounds(b, { padding: 80, maxZoom: 8, duration: reduceMotion ? 0 : 600 });
  }

  async function requestCard(opts) {
    if (!tx || !rx) return;
    const refit = !opts || opts.refit !== false;
    pathFailed = false;
    if (abort) abort.abort();
    abort = new AbortController();
    const signal = abort.signal;
    if (emptyEl) emptyEl.hidden = true;
    waitEl.hidden = false;
    cardEl.classList.remove("has-answer");
    setHint("…", "Working out the path");
    if (txMarker) pulse(txMarker.getElement(), "tx");
    paintPath();
    if (refit) fitPins();
    saveUrl();
    try {
      const res = await fetch(`${PATH_API}/api/path`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          tx_lat: tx.lat,
          tx_lon: tx.lon,
          rx_lat: rx.lat,
          rx_lon: rx.lon,
          hour_utc: planHourUtc(),
          power_w: powerW(),
          tx_antenna_m: txAntM(),
          rx_antenna_m: rxAntM(),
        }),
        signal,
      });
      if (!res.ok) throw new Error(await res.text());
      answer = await res.json();
      pathFailed = false;
      paintPath();
      waitEl.hidden = true;
      if (emptyEl) emptyEl.hidden = true;
      showAnswer();
      if (rxMarker) pulse(rxMarker.getElement(), "rx");
      await loadTerrain(signal);
    } catch (e) {
      if (e.name === "AbortError") return;
      pathFailed = true;
      answer = null;
      paintPath();
      waitEl.hidden = true;
      if (emptyEl) emptyEl.hidden = true;
      cardEl.classList.add("has-answer");
      const old = cardEl.querySelector(".band-answer");
      if (old) old.remove();
      cardEl.insertAdjacentHTML(
        "beforeend",
        `<div class="band-answer"><p class="band-err">${escapeHtml(e.message || "request failed")}</p></div>`
      );
      setHint("ERR", "Try again");
    }
  }

  function pathKey() {
    return tx && rx ? `${tx.lat},${tx.lon}|${rx.lat},${rx.lon}` : "";
  }

  // Elevation is fetched once per pin pair; antenna or time changes reuse it.
  // The Fresnel zone uses the FM card's frequency, else 2m calling.
  async function loadTerrain(signal) {
    const key = pathKey();
    if (!elevation || elevation.key !== key) {
      setLosNote("checking terrain…");
      terrain = null;
      drawProfile();
      try {
        const samples = await fetchElevation(tx, rx, signal);
        if (signal.aborted) return;
        elevation = { key, samples };
      } catch (e) {
        if (e.name === "AbortError") return;
        elevation = { key, samples: null };
      }
    }
    judgeTerrain();
  }

  function fresnelMhz() {
    const f = answer && answer.kind === "fm" ? Number(answer.frequency_mhz) : NaN;
    return Number.isFinite(f) && f > 30 ? f : 145;
  }

  function judgeTerrain() {
    terrain = elevation && elevation.key === pathKey() && elevation.samples
      ? lineOfSight(elevation.samples, txAntM(), rxAntM(), fresnelMhz())
      : null;
    paintPath();
    drawProfile();
    if (!terrain) setLosNote("no terrain data for this path");
    else if (terrain.byTerrain) setLosNote("terrain blocks VHF and UHF");
    else if (terrain.blocked) setLosNote("beyond line of sight for VHF and UHF");
    else if (terrain.marginal) setLosNote(`marginal: ${marginalText()}`);
    else setLosNote("clear line of sight for VHF and UHF");
    showAnswer();
  }

  // The server answers on distance alone. When it says 2m FM and the ground
  // or the earth's curve is in the way, switch to the HF card it sent along.
  function showAnswer() {
    if (!answer) return;
    let card = { ...answer };
    if (terrain && terrain.blocked) {
      const reason = terrain.byTerrain ? "Terrain blocks VHF and UHF." : "Beyond line of sight for VHF and UHF.";
      if (card.kind === "fm" && card.hf) {
        card = { ...card.hf, why: `${reason} ${card.hf.why || ""}`.trim() };
      }
      card.note = `${reason} ${card.note || ""}`.trim();
    } else if (terrain && terrain.marginal && card.kind === "fm") {
      const m = marginalText();
      card.note = `Marginal for VHF and UHF: ${m.charAt(0).toUpperCase()}${m.slice(1)}. ${card.note || ""}`.trim();
    } else if (terrain && card.kind === "fm") {
      card.note = `Clear line of sight for VHF and UHF. ${card.note || ""}`.trim();
    }
    renderCard(card);
    setHint("OK", card.confidence || "Ready");
  }

  // Terrain under the path, drawn on the curved earth: the dashed curve is
  // sea level bent by the 4/3-earth bulge, the ground sits on top of it,
  // and the straight line joins the two antenna tops.
  function drawProfile() {
    if (!profileEl) return;
    if (!terrain) {
      profileEl.hidden = true;
      profileEl.innerHTML = "";
      return;
    }
    profileEl.hidden = false;
    const W = Math.max(240, profileEl.clientWidth || 600);
    const H = 104;
    const pad = { l: 6, r: 6, t: 16, b: 14 };
    const pts = terrain.samples;
    const ground = (s) => (s.elev == null ? s.bulge : s.elev + s.bulge);
    let lo = 0;
    let hi = Math.max(terrain.y0, terrain.y1);
    for (const s of pts) {
      lo = Math.min(lo, s.elev == null ? 0 : s.elev);
      hi = Math.max(hi, ground(s), s.bulge);
    }
    if (hi - lo < 1) hi = lo + 1;
    const D = terrain.D || 1;
    const x = (d) => pad.l + (d / D) * (W - pad.l - pad.r);
    const y = (v) => H - pad.b - ((v - lo) / (hi - lo)) * (H - pad.t - pad.b);
    const f = (n) => n.toFixed(1);
    const groundPts = pts.map((s) => `${f(x(s.dKm))},${f(y(ground(s)))}`);
    const fill = `M${f(x(0))},${f(y(lo))} L${groundPts.join(" L")} L${f(x(D))},${f(y(lo))} Z`;
    const curve = pts.map((s) => `${f(x(s.dKm))},${f(y(s.bulge))}`).join(" L");
    const hits = { marginal: "", blocked: "" };
    for (let i = 1; i < pts.length; i++) {
      const state = segmentState(pts[i - 1], pts[i]);
      if (state === "clear") continue;
      hits[state] += `M${groundPts[i - 1]} L${groundPts[i]} `;
    }
    const km = D >= 100 ? Math.round(D) : D.toFixed(1);
    const end = (s, ant) => `${s.elev == null ? "?" : `${Math.round(s.elev)} m`} +${ant} m`;
    profileEl.innerHTML = `<svg viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Path profile">
        <path class="pf-ground" d="${fill}"/>
        <path class="pf-curve" d="M${curve}"/>
        ${hits.marginal ? `<path class="pf-marginal" d="${hits.marginal}"/>` : ""}
        ${hits.blocked ? `<path class="pf-block" d="${hits.blocked}"/>` : ""}
        <line class="pf-line" x1="${f(x(0))}" y1="${f(y(terrain.y0))}" x2="${f(x(D))}" y2="${f(y(terrain.y1))}"/>
        <text x="${pad.l}" y="11">TX ${end(pts[0], terrain.txAnt)}</text>
        <text x="${W / 2}" y="11" text-anchor="middle">${W < 560 ? `${km} km` : `path profile · ${km} km · 4/3 earth`}</text>
        <text x="${W - pad.r}" y="11" text-anchor="end">RX ${end(pts[pts.length - 1], terrain.rxAnt)}</text>
        <text x="${pad.l}" y="${H - 2}">ground</text>
        <text x="${W / 2}" y="${H - 2}" text-anchor="middle" class="pf-key-line">radio line</text>
        <text x="${W - pad.r}" y="${H - 2}" text-anchor="end" class="pf-key-curve">earth curve</text>
      </svg>`;
  }

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  function copyButton(label, text) {
    return `<button type="button" class="copy-btn band-copy" data-copy="${escapeHtml(text)}">${escapeHtml(label)}</button>`;
  }

  function renderCard(c) {
    const power = c.power_w != null ? `${c.power_w} W` : "over 100 W";
    const ssb = c.ssb ? "on" : "off";
    const voice = c.voice_note ? `<p class="band-voice">${escapeHtml(c.voice_note)}</p>` : "";
    cardEl.classList.add("has-answer");
    const old = cardEl.querySelector(".band-answer");
    if (old) old.remove();
    cardEl.insertAdjacentHTML(
      "beforeend",
      `<div class="band-answer">
        <div class="band-freq-block">
          <p class="band-freq">${escapeHtml(c.frequency_mhz)}<span> MHz</span></p>
          <p class="band-band">${escapeHtml(c.band)}</p>
        </div>
        <ul class="band-stats">
          <li><span>SSB</span><b>${ssb}</b></li>
          <li><span>mode</span><b>${escapeHtml(c.mode)}</b></li>
          <li><span>power</span><b>${escapeHtml(power)}</b></li>
          <li><span>preset</span><b>${escapeHtml(c.preset)}</b></li>
          <li><span>path</span><b>${c.distance_km} km</b></li>
        </ul>
        <div class="band-aside">
          <p class="band-conf">${escapeHtml(c.confidence || "")}</p>
          ${c.why ? `<p class="band-why">${escapeHtml(c.why)}</p>` : ""}
          ${voice}
          <p class="band-note">${escapeHtml(c.note || "")}</p>
          <div class="band-cmds">
            ${copyButton(c.qsy, c.qsy)}
            ${copyButton(c.preset_cmd, c.preset_cmd)}
          </div>
          <p class="band-foot">${escapeHtml(c.footnote || "")}</p>
        </div>
      </div>`
    );
  }

  cardEl.addEventListener("click", async (e) => {
    const btn = e.target.closest(".band-copy");
    if (!btn) return;
    const text = btn.getAttribute("data-copy") || "";
    try {
      await navigator.clipboard.writeText(text);
      const prev = btn.textContent;
      btn.textContent = "copied";
      setTimeout(() => {
        btn.textContent = prev;
      }, 1200);
    } catch (_) {
      btn.textContent = "copy failed";
    }
  });

  if (mapStyleSelect) {
    mapStyleSelect.addEventListener("change", () => {
      const id = MAP_STYLES[mapStyleSelect.value] ? mapStyleSelect.value : "satellite";
      localStorage.setItem(MAP_STYLE_KEY, id);
      map.setStyle(rasterStyle(id));
    });
  }
  if (nightToggle) {
    nightToggle.addEventListener("change", () => {
      localStorage.setItem(NIGHT_KEY, nightToggle.checked ? "1" : "0");
      updateDayNight(true);
    });
  }

  function onStyleReady() {
    updateDayNight(true);
    paintPath();
    try {
      map.resize();
    } catch (_) {}
  }
  map.on("load", onStyleReady);
  map.on("style.load", onStyleReady);
  setInterval(() => {
    if (nightOn()) updateDayNight();
  }, 60000);
  window.addEventListener("resize", () => {
    try {
      map.resize();
    } catch (_) {}
  });

  // Shareable link: ?tx=lat,lon&rx=lat,lon, plus txant=, rxant= and w= when
  // not the defaults. An old ant= sets both ends. Other query keys (e.g.
  // api=) are kept.
  function fmtLL(p) {
    return `${p.lat.toFixed(4)},${p.lon.toFixed(4)}`;
  }
  function parseLL(v) {
    const m = String(v || "").split(",").map(Number);
    if (m.length !== 2 || !m.every(Number.isFinite)) return null;
    if (Math.abs(m[0]) > 90 || Math.abs(m[1]) > 180) return null;
    return { lat: m[0], lng: m[1] };
  }
  function saveUrl() {
    const q = new URLSearchParams(location.search);
    const set = (k, v) => (v == null ? q.delete(k) : q.set(k, v));
    set("tx", tx ? fmtLL(tx) : null);
    set("rx", rx ? fmtLL(rx) : null);
    set("ant", null);
    set("txant", txAntM() !== 10 ? String(txAntM()) : null);
    set("rxant", rxAntM() !== 10 ? String(rxAntM()) : null);
    set("w", powerW() !== 100 ? String(powerW()) : null);
    const qs = q.toString().replace(/%2C/g, ",");
    try {
      history.replaceState(null, "", `${location.pathname}${qs ? "?" + qs : ""}${location.hash}`);
    } catch (_) {}
  }
  function pickOption(sel, value) {
    if (!sel || value == null) return;
    const hit = [...sel.options].find((o) => Number(o.value) === Number(value));
    if (hit) sel.value = hit.value;
  }
  function restoreUrl() {
    const q = new URLSearchParams(location.search);
    const both = q.get("ant");
    setAntenna(txAntEl, Number(q.get("txant") || both));
    setAntenna(rxAntEl, Number(q.get("rxant") || both));
    pickOption(powerEl, q.get("w"));
    const t = parseLL(q.get("tx"));
    const r = parseLL(q.get("rx"));
    if (t) setTx(t);
    if (t && r) setRx(r);
    else if (t) map.jumpTo({ center: [t.lng, t.lat], zoom: 6 });
  }

  let hourTimer = null;
  if (hourEl) {
    hourEl.addEventListener("input", () => {
      showHourLabel();
      updateDayNight(true);
      clearTimeout(hourTimer);
      hourTimer = setTimeout(() => requestCard({ refit: false }), 250);
    });
  }
  let antTimer = null;
  for (const sel of [txAntEl, rxAntEl]) {
    if (!sel) continue;
    const box = sel.parentElement.querySelector("input");
    const changed = () => {
      judgeTerrain();
      requestCard({ refit: false });
    };
    sel.addEventListener("change", () => {
      showAntennaBox(sel);
      if (sel.value === "other") {
        if (!box.value) box.value = "10";
        box.focus();
      }
      changed();
    });
    box.addEventListener("input", () => {
      clearTimeout(antTimer);
      antTimer = setTimeout(changed, 300);
    });
  }
  if (powerEl) powerEl.addEventListener("change", () => requestCard({ refit: false }));
  window.addEventListener("resize", () => drawProfile());
  // Keep "now" honest as the clock rolls over an hour.
  setInterval(showHourLabel, 60000);

  map.on("click", (e) => {
    if (!tx) setTx(e.lngLat);
    else setRx(e.lngLat);
  });

  locateBtn.addEventListener("click", () => {
    if (!navigator.geolocation) {
      setHint("ERR", "Location not available");
      return;
    }
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        const ll = { lng: pos.coords.longitude, lat: pos.coords.latitude };
        map.flyTo({ center: [ll.lng, ll.lat], zoom: 7, duration: reduceMotion ? 0 : 800 });
        setTx(ll);
      },
      () => setHint("ERR", "Could not read location"),
      { enableHighAccuracy: true, timeout: 10000 }
    );
  });

  // "other station": on-air stations from the live map first, then places.
  // Enter takes the first hit, and looks the place up right away if the
  // suggestions have not arrived yet.
  const ON_AIR_MS = 48 * 60 * 60 * 1000;
  let searchTimer = null;
  let searchSeq = 0;
  let hits = [];
  let hitsFor = null;
  let onAir = [];
  let onAirAt = 0;

  async function loadOnAir() {
    if (Date.now() - onAirAt < 60000) return onAir;
    const api = (window.WCR && WCR.API) || "";
    const since = Math.floor((Date.now() - ON_AIR_MS) / 1000);
    try {
      const res = await fetch(`${api}/api/v1/nodes?heard_since=${since}`, { signal: AbortSignal.timeout(4000) });
      const body = await res.json();
      onAir = (body.nodes || [])
        .filter((n) => n.callsign && Number.isFinite(n.lat) && Number.isFinite(n.lon))
        .sort((a, b) => (b.last_seen || 0) - (a.last_seen || 0))
        .map((n) => ({
          kind: "station",
          call: String(n.callsign),
          label: [n.callsign, n.grid, n.band].filter(Boolean).join(" · "),
          lat: n.lat,
          lon: n.lon,
        }));
      onAirAt = Date.now();
    } catch (_) {}
    return onAir;
  }

  function stationHits(q) {
    const k = q.trim().toUpperCase();
    return onAir.filter((s) => !k || s.call.toUpperCase().startsWith(k));
  }

  async function lookupPlaces(q) {
    try {
      const res = await fetch(`${PATH_API}/api/path/places?q=${encodeURIComponent(q)}`);
      if (!res.ok) return [];
      const data = await res.json();
      return (data.places || []).map((p) => ({ kind: "place", ...p }));
    } catch (_) {
      return [];
    }
  }

  function hidePlaces() {
    hits = [];
    hitsFor = null;
    if (!placesEl) return;
    placesEl.hidden = true;
    placesEl.innerHTML = "";
  }

  function choosePlace(p) {
    searchSeq += 1;
    clearTimeout(searchTimer);
    hidePlaces();
    searchEl.value = p.kind === "station" ? p.call : p.label || "";
    map.flyTo({
      center: [p.lon, p.lat],
      zoom: 7,
      duration: reduceMotion ? 0 : 700,
    });
    setRx({ lng: p.lon, lat: p.lat });
  }

  function showPlaces(items, q) {
    hits = items || [];
    hitsFor = q;
    if (!placesEl) return;
    placesEl.innerHTML = "";
    if (!hits.length) {
      placesEl.hidden = true;
      return;
    }
    hits.forEach((p) => {
      const li = document.createElement("li");
      const btn = document.createElement("button");
      btn.type = "button";
      if (p.kind === "station") {
        btn.className = "band-onair-hit";
        btn.innerHTML = `${escapeHtml(p.label)}<span>on air</span>`;
      } else {
        btn.textContent = p.label || `${p.lat.toFixed(2)}, ${p.lon.toFixed(2)}`;
      }
      btn.addEventListener("click", () => choosePlace(p));
      li.appendChild(btn);
      placesEl.appendChild(li);
    });
    placesEl.hidden = false;
  }

  async function showOnAir() {
    const seq = ++searchSeq;
    await loadOnAir();
    if (seq !== searchSeq) return;
    if (!onAir.length) {
      showPlaces([{ kind: "none", label: "No stations on air in the last 48 h" }], "");
      const b = placesEl && placesEl.querySelector("button");
      if (b) b.disabled = true;
      hits = [];
      return;
    }
    showPlaces(stationHits(searchEl.value), searchEl.value.trim());
  }

  searchEl.addEventListener("focus", () => {
    if (!searchEl.value.trim()) showOnAir();
  });

  searchEl.addEventListener("input", () => {
    clearTimeout(searchTimer);
    const seq = ++searchSeq;
    const q = searchEl.value.trim();
    if (!q) {
      showOnAir();
      return;
    }
    const stations = stationHits(q);
    if (q.length < 2) {
      if (stations.length) showPlaces(stations, q);
      else hidePlaces();
      return;
    }
    if (stations.length) showPlaces(stations, null);
    searchTimer = setTimeout(async () => {
      const places = await lookupPlaces(q);
      if (seq !== searchSeq || searchEl.value.trim() !== q) return;
      showPlaces(stationHits(q).concat(places), q);
    }, 250);
  });

  searchEl.addEventListener("keydown", async (e) => {
    if (e.key === "Escape") {
      hidePlaces();
      return;
    }
    if (e.key !== "Enter") return;
    e.preventDefault();
    const q = searchEl.value.trim();
    if (!q) return;
    const exact = onAir.find((s) => s.call.toUpperCase() === q.toUpperCase());
    if (exact) return choosePlace(exact);
    if (hitsFor === q && hits[0]) return choosePlace(hits[0]);
    clearTimeout(searchTimer);
    const seq = ++searchSeq;
    setHint("…", "Looking up " + q);
    const places = stationHits(q).concat(await lookupPlaces(q));
    if (seq !== searchSeq || searchEl.value.trim() !== q) return;
    if (places[0]) choosePlace(places[0]);
    else setHint("ERR", `No place called ${q}`);
  });

  if (onAirBtn) {
    onAirBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      searchEl.value = "";
      searchEl.focus();
      showOnAir();
    });
  }

  document.addEventListener("click", (e) => {
    if (!searchEl.contains(e.target) && placesEl && !placesEl.contains(e.target)) hidePlaces();
  });

  setHint("TX", "Click where you are");
  showHourLabel();
  restoreUrl();

  // Used by local checks; not part of the public page API.
  window.__WCR_BAND = {
    setTxLngLat: (lon, lat) => setTx({ lng: lon, lat }),
    setRxLngLat: (lon, lat) => setRx({ lng: lon, lat }),
    state: () => ({
      tx,
      rx,
      hint: hintEl && hintEl.textContent,
      wait: waitEl && !waitEl.hidden,
      card: cardEl && !cardEl.hidden,
      cardText: cardEl && cardEl.innerText,
      terrain: terrain && {
        blocked: terrain.blocked,
        marginal: terrain.marginal,
        byTerrain: terrain.byTerrain,
        n: terrain.samples.length,
        y0: terrain.y0,
        y1: terrain.y1,
      },
    }),
  };
})();
