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
  const overrideEl = document.getElementById("band-override");
  const wireEl = document.getElementById("band-hfwire");
  const wireNoteEl = document.getElementById("band-hfwire-note");

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
  // "mobile" is a roof whip on a car: 1.8 m up, 2 dBi (Tobias, #142).
  const MOBILE_M = 1.8;
  const MOBILE_DBI = 2;
  function antennaM(sel) {
    if (!sel) return 10;
    if (sel.value === "mobile") return MOBILE_M;
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
  // A link value: metres, or "mobile".
  function antennaParam(sel) {
    return sel && sel.value === "mobile" ? "mobile" : String(antennaM(sel));
  }
  function setAntenna(sel, m) {
    if (sel && m === "mobile") {
      sel.value = "mobile";
      showAntennaBox(sel);
      return;
    }
    m = Number(m);
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
  // HF antenna at your end; the far end is taken as a simple wire.
  const hfAntEls = [...document.querySelectorAll('input[name="band-hfant"]')];
  function hfAntenna() {
    const hit = hfAntEls.find((el) => el.checked);
    return hit ? hit.value : "dipole";
  }
  // How a wire antenna is strung up. It does not change the prediction; it
  // shows a short note so a new user knows their setup is a good one. Each
  // wire antenna has its own list and drawings: a dipole is fed in the
  // middle, an end-fed at one end through a box (the unun). The first entry
  // is the default and stays out of the link.
  const WIRE_LABELS = { flat: "horizontal", sloper: "sloper", invl: "inverted L", invv: "inverted V", vertical: "vertical" };
  const WIRE_NOTES = {
    flat: "Horizontal: the classic setup. Get it as high and clear as you can; even a few metres up works.",
    sloper: "Sloper: a common, good setup. Keep the far end out of reach, and put a choke on the coax at the feed.",
    invl: "Inverted L: a good fit for a small garden. The upright part helps on long paths; put a choke on the coax at the feed.",
    invv: "Inverted V: a good, easy setup with one support in the middle. Keep the ends out of reach and the angle between the legs wider than 90°.",
    vertical: "Vertical: a good setup when space is tight, and fine for long paths. Lay a few radials or a counterpoise if you can, and put a choke on the coax at the feed.",
  };
  const CENTRE_FED = {
    flat: '<path d="M2 8h20M12 8v13"/><circle cx="12" cy="8" r="1.5"/>',
    invv: '<path d="M3 19 12 6l9 13M12 6v16"/><circle cx="12" cy="6" r="1.5"/>',
    sloper: '<path d="M3 20 21 4M12 12v10"/><circle cx="12" cy="12" r="1.5"/>',
  };
  const UNUN = '<rect x="2" y="17" width="5" height="4"/>';
  const END_FED = {
    flat: '<rect x="2" y="5" width="5" height="4"/><path d="M7 7h15M4.5 9v13"/>',
    sloper: UNUN + '<path d="M7 18 22 4"/>',
    invv: UNUN + '<path d="M7 18 14 5l8 13"/>',
    invl: UNUN + '<path d="M7 19h2V5h13"/>',
    vertical: UNUN + '<path d="M7 19h2V2"/>',
  };
  const WIRE_SHAPES = {
    dipole: CENTRE_FED,
    efhw: END_FED,
    endfed: { flat: END_FED.flat, sloper: END_FED.sloper, invl: END_FED.invl, vertical: END_FED.vertical },
  };
  let wire = "flat";
  function wireShapes() {
    return WIRE_SHAPES[hfAntenna()] || null;
  }
  function isWireAntenna() {
    return !!wireShapes();
  }
  function wireOrientation() {
    const shapes = wireShapes();
    return shapes && shapes[wire] ? wire : "flat";
  }
  // A vertical or a loop still fills the hidden row with the dipole's
  // picker, so the row has its full height from the first load.
  function shownWireShapes() {
    return wireShapes() || WIRE_SHAPES.dipole;
  }
  function wireSvg(key) {
    return `<svg viewBox="0 0 24 24" aria-hidden="true">${shownWireShapes()[key]}</svg>`;
  }
  // Every note sits in the same grid cell, hidden, under the shown one, so
  // the note box is always as tall as the longest note and picking a setup
  // does not move the map on a phone.
  if (wireNoteEl) {
    for (const text of Object.values(WIRE_NOTES)) {
      const sizer = document.createElement("p");
      sizer.className = "band-hfwire-note band-hfwire-sizer";
      sizer.setAttribute("aria-hidden", "true");
      sizer.textContent = text;
      wireNoteEl.parentElement.append(sizer);
    }
  }
  // The picker sits under the selected tile, so it reads as that antenna's
  // setting. The rows keep their space when hidden, so picking a vertical or
  // a loop does not move the map and card up (and back down on dipole).
  function showWireNote() {
    if (!wireEl) return;
    const row = wireEl.parentElement;
    const show = isWireAntenna() ? "" : "hidden";
    row.style.visibility = show;
    if (wireNoteEl) wireNoteEl.style.visibility = show;
    closeWireMenu();
    const o = wireOrientation();
    wireEl.innerHTML = `<b></b>${wireSvg(o)}<span>${WIRE_LABELS[o]}</span><i>▾</i>`;
    wireMenuEl.innerHTML = Object.keys(shownWireShapes())
      .map((k) => `<li role="option" tabindex="-1" data-wire="${k}" aria-selected="${k === o}">${wireSvg(k)}<span>${WIRE_LABELS[k]}</span></li>`)
      .join("");
    if (wireNoteEl) wireNoteEl.textContent = WIRE_NOTES[o];
    placeWirePicker();
  }
  function placeWirePicker() {
    if (!wireEl) return;
    const tile = hfAntEls.find((el) => el.checked);
    const row = wireEl.parentElement;
    if (!tile || row.style.visibility) return;
    // On a phone the tiles wrap; when the selected one is not on the last
    // line the picker can't sit under it, so it starts at the left and names
    // the antenna instead.
    const box = tile.parentElement.getBoundingClientRect();
    const under = box.bottom >= tile.parentElement.parentElement.getBoundingClientRect().bottom - 1;
    row.classList.toggle("is-detached", !under);
    const name = wireEl.querySelector("b");
    if (name) name.textContent = under ? "" : `${tile.parentElement.textContent.trim()}:`;
    const x = under ? box.left - row.getBoundingClientRect().left : 0;
    row.style.paddingLeft = `${Math.max(0, Math.min(x, row.clientWidth - wireEl.offsetWidth))}px`;
  }
  const wireMenuEl = document.getElementById("band-hfwire-menu");
  function openWireMenu() {
    wireMenuEl.hidden = false;
    wireEl.setAttribute("aria-expanded", "true");
    const sel = wireMenuEl.querySelector('[aria-selected="true"]');
    if (sel) sel.focus();
  }
  function closeWireMenu(focus) {
    if (!wireMenuEl || wireMenuEl.hidden) return;
    wireMenuEl.hidden = true;
    wireEl.setAttribute("aria-expanded", "false");
    if (focus) wireEl.focus();
  }
  function pickWire(key) {
    wire = key;
    showWireNote();
    wireEl.focus();
    saveUrl();
  }

  // Bands you can pick by hand instead of the recommendation. Nothing is
  // greyed out for licence rules; `info` is a short note only. Line-of-sight
  // bands are worked out on the page; HF bands come from the path service.
  // `maxW` is the legal power of a licence-free band; the signal budget
  // uses it instead of the station's power.
  const OVERRIDES = {
    "2m": { label: "2m", mhz: 145.5, info: "2m needs an amateur licence." },
    "70cm": { label: "70cm", mhz: 433.5, info: "70cm needs an amateur licence." },
    cb: { label: "CB", freq: "27.185", mhz: 27.185, maxW: 4, info: "CB in the UK: 4 W FM on the 27/81 and CEPT channels. Channel 19 shown." },
    pmr446: { label: "PMR446", freq: "446.00625", mhz: 446.00625, maxW: 0.5, info: "PMR446 in the UK and EU: 0.5 W, built-in antenna, no licence. Channel 1 shown.", mobileNote: "UK PMR446 radios must use their built-in antenna; an external roof antenna isn't licence-free." },
    murs: { label: "MURS", freq: "151.820", mhz: 151.82, maxW: 2, info: "MURS is US only: 2 W, no licence. Channel 1 shown." },
    "80m": { label: "80m", mhz: 3.58, hf: true },
    "60m": { label: "60m", mhz: 5.357, hf: true },
    "40m": { label: "40m", mhz: 7.045, hf: true },
    "30m": { label: "30m", mhz: 10.14, hf: true },
    "20m": { label: "20m", mhz: 14.07, hf: true },
    "17m": { label: "17m", mhz: 18.1, hf: true },
    "15m": { label: "15m", mhz: 21.07, hf: true },
    "12m": { label: "12m", mhz: 24.92, hf: true },
    "10m": { label: "10m", mhz: 28.12, hf: true },
  };
  for (const o of Object.values(OVERRIDES)) if (o.hf) o.info = `${o.label} needs an amateur licence.`;
  const VHF_KEYS = ["2m", "70cm", "cb", "pmr446", "murs"];
  function overrideKey() {
    return overrideEl && OVERRIDES[overrideEl.value] ? overrideEl.value : "";
  }
  function override() {
    return OVERRIDES[overrideKey()] || null;
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
    if (!hintEl) return;
    hintEl.textContent = text;
    hintEl.title = text;
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
  let terrainFailed = false;

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
  // Open-Meteo answers 429 when it is busy. Each call is tried again after
  // 1, 2 and 4 s; if it still fails the lookup throws, so the page says so
  // instead of answering as if there were no hills.
  const ELEV_RETRY_MS = [1000, 2000, 4000];

  function wait(ms, signal) {
    return new Promise((resolve, reject) => {
      const t = setTimeout(resolve, ms);
      signal.addEventListener("abort", () => {
        clearTimeout(t);
        reject(new DOMException("aborted", "AbortError"));
      }, { once: true });
    });
  }

  async function fetchElevationPart(url, signal, onRetry) {
    for (let attempt = 0; ; attempt++) {
      let res = null;
      try {
        res = await fetch(url, { signal });
      } catch (e) {
        if (e.name === "AbortError") throw e;
      }
      if (res && res.ok) return res.json();
      const busy = !res || res.status === 429 || res.status >= 500;
      if (!busy || attempt >= ELEV_RETRY_MS.length) throw new Error("terrain lookup failed");
      onRetry();
      await wait(ELEV_RETRY_MS[attempt], signal);
    }
  }

  async function fetchElevation(a, b, signal, onRetry) {
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
        const body = await fetchElevationPart(url, signal, onRetry);
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

  // Why a path is marginal. A hill pokes above the radio line ("clips"),
  // or stands well above the ground line between the two ends ("passes
  // close to"); otherwise the path is flat and low antennas leave the
  // ground too close to the signal. How much that matters comes from the
  // signal budget below.
  function marginalCause(t) {
    const w = t && t.worst;
    if (!w) return { hill: false, text: "the ground is close to the signal path" };
    const km = w.dKm >= 10 ? Math.round(w.dKm) : w.dKm.toFixed(1);
    const first = t.samples[0].elev || 0;
    const last = t.samples[t.samples.length - 1].elev || 0;
    const groundLine = t.D > 0 ? first + (last - first) * (w.dKm / t.D) : first;
    if (w.clearance < 0) return { hill: true, text: `just clips a hill at ${km} km` };
    if (w.elev - groundLine > 15) return { hill: true, text: `passes close to a hill at ${km} km` };
    return { hill: false, text: "the ground is close to the signal path" };
  }
  function marginalLine(t) {
    const c = marginalCause(t);
    return c.hill
      ? `Marginal: ${c.text}. Raise the antennas or add power for a solid link.`
      : `Marginal: ${c.text}. Raise the antennas for a solid link.`;
  }

  // Rough FM signal budget, so a yellow path or a pair of handhelds says how
  // well it should really work. Path loss is the larger of free space and
  // plane earth (40 log d, what limits low antennas over flat ground), plus
  // knife-edge loss when the ground pokes above the radio line. A handheld whip
  // (2 m up or lower) counts as -5 dBi, a mobile roof whip as 2 dBi, an
  // antenna on a roof or mast as 0 dBi after feeder loss. -116 dBm is a clean FM signal; 10 dB above it
  // counts as "should work well". On a marginal path the signal margin sets
  // the headline: 20 dB or more covers the ground crowding the path.
  const FM_FLOOR_DBM = -116;
  const GOOD_MARGIN_DB = 10;
  const SOLID_MARGIN_DB = 20;
  const PLENTY_NOTE = "Ground is close to the signal path in places; you have plenty of signal to cover it.";
  const POWER_STEPS = [5, 10, 20, 50, 100];

  function antGainDbi(sel) {
    if (sel && sel.value === "mobile") return MOBILE_DBI;
    return antennaM(sel) <= 2 ? -5 : 0;
  }
  function antGainsDbi() {
    return antGainDbi(txAntEl) + antGainDbi(rxAntEl);
  }
  // ITU-R P.526 single knife edge, nu = sqrt(2) x height above the line / F1.
  function knifeEdgeDb(nu) {
    if (!(nu > -0.78)) return 0;
    return 6.9 + 20 * Math.log10(Math.sqrt((nu - 0.1) ** 2 + 1) + nu - 0.1);
  }
  function pathLossDb(t, mhz) {
    const dKm = Math.max(t ? t.D : distKm(tx, rx), 0.01);
    const free = 32.45 + 20 * Math.log10(dKm) + 20 * Math.log10(mhz);
    const w = t && t.worst;
    const edge = w && w.clearance < 0 ? knifeEdgeDb((-Math.SQRT2 * w.clearance) / w.f1) : 0;
    const plane = 40 * Math.log10(dKm * 1000) - 20 * Math.log10(txAntM()) - 20 * Math.log10(rxAntM());
    return Math.max(free, plane) + edge;
  }
  function marginDb(t, watts, mhz) {
    const dbm = 10 * Math.log10(watts * 1000);
    return dbm + antGainsDbi() - pathLossDb(t, mhz) - FM_FLOOR_DBM;
  }
  // The least power (up to `maxW`) that should work well over the terrain
  // `t` at `mhz`, and a plain verdict for the card. A marginal path asks
  // for the solid margin.
  let budget = null;
  function signalBudget(t, mhz, maxW) {
    const target = t && t.marginal ? SOLID_MARGIN_DB : GOOD_MARGIN_DB;
    const steps = POWER_STEPS.filter((w) => w <= maxW);
    const watts = steps.find((w) => marginDb(t, w, mhz) >= target) || maxW;
    const margin = Math.round(marginDb(t, watts, mhz));
    let text;
    if (margin >= GOOD_MARGIN_DB) text = `Should work well: about ${margin} dB to spare at ${watts} W.`;
    else if (margin >= 0) text = `Should work, but may be noisy or drop out as you move: about ${margin} dB to spare at ${watts} W.`;
    else text = `Unlikely at ${watts} W: about ${-margin} dB short. More antenna height helps most.`;
    return { watts, margin, text };
  }

  // Map and profile colours. The card keeps the strict Fresnel rating above;
  // the track only shows yellow where the path is really pinched. Red stays
  // for blocked. Inside the "sure range" from each end (where even
  // plane-earth loss leaves the solid margin, about 1 km for PMR446
  // handhelds) ground below the radio line is blue and ground clipping it
  // is yellow, but with a solid budget only the worst spot is (Tobias, #142). Elsewhere the worst spot or spots are always
  // yellow; with a solid budget nothing else is, otherwise anything under
  // 0.3 x F1 or within 3 dB of the worst obstruction loss is too.
  const PINCH_MARGIN = 0.3;
  const PINCH_DB = 3;
  const WORST_SPOT = 0.05;

  function obstructionDb(margin) {
    return knifeEdgeDb(-Math.SQRT2 * margin);
  }
  function sureRangeKm(watts, mhz) {
    const allowed = 10 * Math.log10(watts * 1000) + antGainsDbi()
      - FM_FLOOR_DBM - SOLID_MARGIN_DB;
    const freeKm = 10 ** ((allowed - 32.45 - 20 * Math.log10(mhz)) / 20);
    const planeKm = 10 ** ((allowed + 20 * Math.log10(txAntM()) + 20 * Math.log10(rxAntM())) / 40) / 1000;
    return Math.min(freeKm, planeKm);
  }
  function markTrack(t, mhz) {
    const o = override();
    const maxW = Math.min(powerW(), (o && o.maxW) || powerW());
    const b = signalBudget(t, mhz, maxW);
    const solid = b.margin >= SOLID_MARGIN_DB;
    const sure = sureRangeKm(b.watts, mhz);
    const worst = t.worst;
    const worstDb = worst ? obstructionDb(worst.margin) : 0;
    const ss = t.samples;
    // A worst spot is a dip in the clearance (a sample no better than its
    // neighbours) about as tight as the tightest one, so a flat path has
    // one spot, not its whole middle.
    const spot = (i) => {
      const m = ss[i].margin;
      const lower = (j) => ss[j] && ss[j].margin != null && ss[j].margin < m;
      return m <= worst.margin + WORST_SPOT && !lower(i - 1) && !lower(i + 1);
    };
    ss.forEach((s, i) => {
      const nearEnd = Math.min(s.dKm, t.D - s.dKm) <= sure;
      const pinch = s.margin < PINCH_MARGIN || obstructionDb(s.margin) >= worstDb - PINCH_DB;
      if (s.state === "clear" || s.state === "blocked") s.show = s.state;
      else if (nearEnd) s.show = s.clearance >= 0 || (solid && !spot(i)) ? "clear" : "marginal";
      else if (spot(i)) s.show = "marginal";
      else s.show = !solid && pinch ? "marginal" : "clear";
    });
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
    // Bow sideways from the chord (to the left going TX to RX, so up for a
    // west-to-east link), never along it: a north-south link bowed screen-up
    // lies on top of its own track. The peak is half the control offset, so
    // 48 px keeps at least a 24 px bow.
    const bulge = Math.min(90, Math.max(48, dist * 0.22));
    const mx = (s1.x + s2.x) / 2 + (dy / dist) * bulge;
    const my = (s1.y + s2.y) / 2 - (dx / dist) * bulge;
    arcCtx.beginPath();
    arcCtx.moveTo(s1.x, s1.y);
    arcCtx.quadraticCurveTo(mx, my, s2.x, s2.y);
    arcCtx.strokeStyle = color;
    arcCtx.setLineDash(dash);
    arcCtx.lineWidth = 2.5;
    arcCtx.lineCap = "round";
    arcCtx.stroke();
  }

  // Blue clear, yellow marginal, red blocked (the `show` colour from
  // markTrack): same colours as the profile.
  const TRACK_COLORS = {
    clear: "rgba(125,155,255,0.9)",
    marginal: "rgba(255,214,10,0.95)",
    blocked: "rgba(255,59,107,0.95)",
  };
  const STATE_RANK = { clear: 0, marginal: 1, blocked: 2 };
  function segmentState(a, b) {
    return STATE_RANK[a.show] >= STATE_RANK[b.show] ? a.show : b.show;
  }

  function paintPath() {
    ensureArcLayer();
    if (!arcCtx || !arcCanvas) return;
    syncArcCanvas();
    const cssW = arcCanvas.clientWidth || 800;
    const cssH = arcCanvas.clientHeight || 600;
    arcCtx.clearRect(0, 0, cssW, cssH);
    if (!tx || !rx) return;
    if (skyOn() && !pathFailed) {
      paintSky();
      return;
    }
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

  // HF sky view (#157). An HF card goes by the ionosphere, not the ground,
  // so the map draws the great circle with its hops and the profile slot
  // shows a side view. Hops, take-off angle, path MUF and the per-hour
  // verdicts come from the path service (VOACAP); anything worked out here
  // from geometry is labelled approx.
  const SKY_COLORS = { open: "#39ff14", marginal: "#ffd60a", closed: "#ff3b6b" };
  const SKY_TAG = { open: "ok", marginal: "warn", closed: "bad" };
  const R_EARTH_KM = 6371;
  const F2_KM = 300; // reflection height for the drawings
  const F2_BAND_KM = [250, 350];
  const NVIS_DEG = 60;
  const SHORT_PATH_KM = 1000;
  // Rough ground-wave reach in km at modest power (approx.).
  const GROUND_WAVE_KM = { "80m": 40, "60m": 35, "40m": 30, "30m": 25, "20m": 20 };
  let shownCard = null;

  function skyOn() {
    return !!(answer && shownCard && shownCard.kind === "hf" && tx && rx);
  }
  function skyBand() {
    if (!skyOn()) return null;
    return (answer.bands || []).find((b) => b.band === shownCard.band) || null;
  }
  function skyState() {
    const b = skyBand();
    return b ? b.state : shownCard && shownCard.ok ? "open" : "closed";
  }
  // Path points along the great circle, longitudes unwrapped so a path over
  // the date line stays one line.
  function greatCircle(n) {
    const pts = [];
    for (let i = 0; i <= n; i++) {
      const p = along(tx, rx, i / n);
      if (pts.length) {
        const prev = pts[pts.length - 1].lon;
        while (p.lon - prev > 180) p.lon -= 360;
        while (p.lon - prev < -180) p.lon += 360;
      }
      pts.push(p);
    }
    return pts;
  }
  // Hops from the engine, else one per 4000 km (approx.).
  function skyHops() {
    const D = distKm(tx, rx);
    const b = skyBand();
    if (b && b.hops) return { n: b.hops, approx: false, D };
    return { n: Math.max(1, Math.ceil(D / 4000)), approx: true, D };
  }
  // One hop of `dKm` on the ground reflected at F2_KM: take-off angle
  // `beta` and the angle `phi` the ray meets the layer at, from vertical.
  function hopAngles(dKm) {
    const theta = dKm / (2 * R_EARTH_KM);
    const k = R_EARTH_KM / (R_EARTH_KM + F2_KM);
    const beta = Math.atan2(Math.cos(theta) - k, Math.sin(theta));
    return { beta, phi: Math.PI / 2 - beta - theta };
  }
  function skyTakeoff() {
    const b = skyBand();
    if (b && b.takeoff_deg != null) return { deg: b.takeoff_deg, approx: false };
    const h = skyHops();
    return { deg: (hopAngles(h.D / h.n).beta * 180) / Math.PI, approx: true };
  }
  // foF2 implied by the engine's path MUF (secant law at its hop length),
  // then the skip distance of `mhz`: 0 when it comes straight back down.
  function foF2Approx() {
    const sky = answer && answer.sky;
    if (!sky || !sky.muf_mhz) return null;
    const D = distKm(tx, rx);
    const n = sky.muf_hops || skyHops().n;
    return sky.muf_mhz * Math.cos(hopAngles(D / n).phi);
  }
  function skipKm(mhz) {
    const fo = foF2Approx();
    if (!fo || mhz <= fo) return 0;
    const phi = Math.acos(fo / mhz);
    const cosBeta = ((R_EARTH_KM + F2_KM) * Math.sin(phi)) / R_EARTH_KM;
    if (cosBeta >= 1) return Infinity;
    const theta = Math.PI / 2 - Math.acos(cosBeta) - phi;
    return 2 * R_EARTH_KM * theta;
  }
  function skyMhz() {
    const b = skyBand();
    return b && b.frequency_mhz ? b.frequency_mhz : shownCard ? Number(shownCard.frequency_mhz) : NaN;
  }
  // Ground wave and skip zone around TX for the shown band, short paths only.
  // Whether there is a skip zone comes from the engine (`skips`); only its
  // size is worked out here (approx.), and it always reaches past RX.
  function skyRings() {
    if (!skyOn()) return null;
    const D = distKm(tx, rx);
    if (D > SHORT_PATH_KM) return null;
    const gw = GROUND_WAVE_KM[shownCard.band] || 15;
    const b = skyBand();
    if (!b || !b.skips) return { gw, skip: 0, beyond: false };
    const skip = skipKm(skyMhz());
    if (skip === Infinity) return { gw, skip: 0, beyond: true };
    return { gw, skip: Math.max(skip, D * 1.1, gw + 1), beyond: false };
  }
  function destination(p, bearingDeg, km) {
    const δ = km / R_EARTH_KM;
    const θ = (bearingDeg * Math.PI) / 180;
    const φ1 = (p.lat * Math.PI) / 180;
    const λ1 = (p.lon * Math.PI) / 180;
    const φ2 = Math.asin(Math.sin(φ1) * Math.cos(δ) + Math.cos(φ1) * Math.sin(δ) * Math.cos(θ));
    const λ2 = λ1 + Math.atan2(Math.sin(θ) * Math.sin(δ) * Math.cos(φ1), Math.cos(δ) - Math.sin(φ1) * Math.sin(φ2));
    return { lat: (φ2 * 180) / Math.PI, lon: (λ2 * 180) / Math.PI };
  }
  function ringPath(center, km) {
    arcCtx.beginPath();
    for (let a = 0; a <= 360; a += 6) {
      const q = destination(center, a, km);
      let lon = q.lon;
      while (lon - center.lon > 180) lon -= 360;
      while (lon - center.lon < -180) lon += 360;
      const s = map.project([lon, q.lat]);
      if (a === 0) arcCtx.moveTo(s.x, s.y);
      else arcCtx.lineTo(s.x, s.y);
    }
    arcCtx.closePath();
  }

  function paintSky() {
    const color = SKY_COLORS[skyState()];
    const rings = skyRings();
    if (rings && rings.skip) {
      ringPath(tx, rings.skip);
      arcCtx.fillStyle = "rgba(255,59,107,0.16)";
      arcCtx.fill();
      ringPath(tx, rings.gw);
      arcCtx.save();
      arcCtx.globalCompositeOperation = "destination-out";
      arcCtx.fill();
      arcCtx.restore();
      ringPath(tx, rings.skip);
      arcCtx.strokeStyle = "rgba(255,59,107,0.8)";
      arcCtx.lineWidth = 1.5;
      arcCtx.setLineDash([]);
      arcCtx.stroke();
    }
    if (rings) {
      ringPath(tx, rings.gw);
      arcCtx.strokeStyle = "rgba(125,155,255,0.9)";
      arcCtx.lineWidth = 1.5;
      arcCtx.setLineDash([5, 4]);
      arcCtx.stroke();
    }
    const pts = greatCircle(96).map((p) => map.project([p.lon, p.lat]));
    arcCtx.beginPath();
    pts.forEach((s, i) => (i ? arcCtx.lineTo(s.x, s.y) : arcCtx.moveTo(s.x, s.y)));
    arcCtx.strokeStyle = color;
    arcCtx.lineWidth = 3;
    arcCtx.lineCap = "round";
    arcCtx.setLineDash([]);
    arcCtx.stroke();
    // Reflection points at the middle of each hop, ground bounces between.
    const hops = skyHops();
    const at = (f) => {
      const p = along(tx, rx, f);
      const ref = along(tx, rx, Math.max(0, f - 0.01));
      while (p.lon - ref.lon > 180) p.lon -= 360;
      while (p.lon - ref.lon < -180) p.lon += 360;
      return map.project([p.lon, p.lat]);
    };
    for (let k = 0; k < hops.n; k++) {
      const s = at((k + 0.5) / hops.n);
      arcCtx.beginPath();
      arcCtx.arc(s.x, s.y, 4.5, 0, 2 * Math.PI);
      arcCtx.fillStyle = color;
      arcCtx.fill();
      arcCtx.strokeStyle = "#07070a";
      arcCtx.lineWidth = 1.5;
      arcCtx.stroke();
    }
    for (let k = 1; k < hops.n; k++) {
      const s = at(k / hops.n);
      arcCtx.beginPath();
      arcCtx.arc(s.x, s.y, 6, 0, 2 * Math.PI);
      arcCtx.strokeStyle = color;
      arcCtx.lineWidth = 1.5;
      arcCtx.stroke();
    }
  }

  function hopText() {
    const h = skyHops();
    const each = Math.round(h.D / h.n / 10) * 10;
    const words = h.n === 1 ? "1 hop" : `${h.n} hops`;
    return `${words} · ~${each} km each${h.approx ? " (approx.)" : ""}`;
  }

  // Map key and hop chip: the terrain key for VHF and UHF, the sky key for HF.
  function showSkyKey() {
    const stage = document.querySelector(".band-map-stage");
    if (stage) stage.classList.toggle("is-sky", skyOn());
    const chip = document.getElementById("band-hop-chip");
    if (chip) chip.textContent = skyOn() ? hopText() : "";
    setGreyline();
  }

  // Side view in the profile slot: curved earth, the F2 layer and the rays,
  // not to scale. A band above the MUF goes through as a dashed red ray.
  function drawSky() {
    profileEl.hidden = false;
    const W = Math.max(240, profileEl.clientWidth || 600);
    const H = 104;
    const narrow = W < 560;
    const pad = 8;
    const ground = H - 18;
    const lift = 14; // how far the earth bows up mid-view
    const layerPx = 52; // F2_BAND_KM top
    const kmPx = layerPx / F2_BAND_KM[1];
    const f = (n) => n.toFixed(1);
    // Earth across the whole width at `u` (0 to 1).
    const ex = (u) => pad + u * (W - 2 * pad);
    const ey = (u, km) => ground - lift * Math.sin(Math.PI * u) - km * kmPx;
    const line = (km, n = 40) => Array.from({ length: n + 1 }, (_, i) => `${f(ex(i / n))},${f(ey(i / n, km))}`).join(" L");
    // The path keeps its take-off angle where it fits: a steep (NVIS) path
    // is drawn narrow in the middle, a long one fills the width.
    const hops = skyHops();
    const to = skyTakeoff();
    const tanB = Math.tan((Math.max(3, Math.min(87, to.deg)) * Math.PI) / 180);
    const spanPx = Math.max(60, Math.min(W - 2 * pad, (2 * hops.n * F2_KM * kmPx) / tanB));
    const u0 = (1 - spanPx / (W - 2 * pad)) / 2;
    const u = (t) => u0 + t * (spanPx / (W - 2 * pad));
    const x = (t) => ex(u(t));
    const y = (t, km) => ey(u(t), km);
    const state = skyState();
    const own = skyBand();
    // The shown band goes through when it is above the MUF; otherwise the
    // lowest band that does is drawn beside its rays.
    const punch = own && own.skips ? own : (answer.bands || []).find((b) => b.skips);
    let ray = "";
    if (!(own && own.skips)) {
      ray = `M${f(x(0))},${f(y(0, 0))}`;
      for (let k = 0; k < hops.n; k++) {
        ray += ` L${f(x((k + 0.5) / hops.n))},${f(y((k + 0.5) / hops.n, F2_KM))} L${f(x((k + 1) / hops.n))},${f(y((k + 1) / hops.n, 0))}`;
      }
    }
    let punchSvg = "";
    if (punch) {
      // Steeper than the hop rays, out through the top.
      const dx = Math.max(10, (x(0.5 / hops.n) - x(0)) * 0.6);
      const x0 = x(0);
      const y0 = y(0, 0);
      const x2 = x0 + (dx * (y0 - 4)) / (y0 - y(0.5 / hops.n, F2_KM));
      punchSvg = `<path class="sky-punch" d="M${f(x0)},${f(y0)} L${f(x2)},4"/>
        <text x="${f(x2 + 4)}" y="12" class="sky-punch-label">${escapeHtml(punch.band)} goes through</text>`;
    }
    const deg = `${Math.round(to.deg)}°${to.approx ? " (approx.)" : ""}`;
    const band = `M${line(F2_BAND_KM[0])} L${line(F2_BAND_KM[1]).split(" L").reverse().join(" L")} Z`;
    profileEl.innerHTML = `<svg class="pf-sky" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Side view of the sky path">
        <path class="sky-f2" d="${band}"/>
        <path class="sky-earth" d="M${line(0)} L${f(ex(1))},${H} L${f(ex(0))},${H} Z"/>
        ${ray ? `<path class="sky-ray" style="stroke:${SKY_COLORS[state]}" d="${ray}"/>` : ""}
        ${punchSvg}
        <text x="${W - pad - 4}" y="${f(ey(1, F2_KM) + 3)}" text-anchor="end">${narrow ? "F2 ≈250–350 km" : "F2 layer ≈ 250–350 km (approx.)"}</text>
        <text x="${W - pad}" y="12" text-anchor="end">${narrow ? "not to scale" : `side view · ${hopText()} · not to scale`}</text>
        <text x="${f(x(0))}" y="${H - 3}" text-anchor="${x(0) < W * 0.25 ? "start" : "end"}">TX · take-off ${deg}</text>
        <text x="${f(x(1))}" y="${H - 3}" text-anchor="${x(1) > W * 0.75 ? "end" : "start"}">RX</text>
        <circle class="sky-end" cx="${f(x(0))}" cy="${f(y(0, 0))}" r="2.5"/>
        <circle class="sky-end" cx="${f(x(1))}" cy="${f(y(1, 0))}" r="2.5"/>
      </svg>`;
  }

  // Next 24 hours of the shown band, from now; the planned hour outlined.
  function skyStrip() {
    const b = skyBand();
    if (!b || !b.hours || b.hours.length !== 24) return "";
    const now = new Date().getUTCHours();
    const cells = Array.from({ length: 24 }, (_, i) => {
      const h = (now + i) % 24;
      const st = b.hours[h];
      const sel = i === hourOffset() ? " is-sel" : "";
      return `<button type="button" class="sky-cell sky-${st}${sel}" data-off="${i}" title="${String(h).padStart(2, "0")}:00 UTC · ${st}" aria-label="${String(h).padStart(2, "0")}:00 UTC ${st}"></button>`;
    }).join("");
    return `<div class="sky-strip" aria-label="${escapeHtml(b.band)} over the next 24 hours">
        <span class="band-label">next 24 h · ${escapeHtml(b.band)}</span>
        <div class="sky-cells">${cells}</div>
      </div>`;
  }

  // The engine's figures for the card: hops, take-off angle, path MUF.
  function skyLines() {
    const to = skyTakeoff();
    const sky = answer.sky || {};
    const parts = [hopText(), `take-off ${Math.round(to.deg)}°${to.approx ? " (approx.)" : ""}`];
    if (sky.muf_mhz) parts.push(`MUF on path ${Number(sky.muf_mhz).toFixed(1)} MHz`);
    let html = `<p class="sky-line">${escapeHtml(parts.join(" · "))}</p>`;
    const own = skyBand();
    const rings = skyRings();
    if (own && own.skips) {
      // The engine says this band skips the path: a skip line, never the NVIS note.
      html += rings && rings.skip
        ? `<p class="sky-skip">${escapeHtml(shownCard.band)} skips: skip zone ≈ ${rings.gw}–${Math.round(rings.skip)} km from TX (approx.), RX is inside it.</p>`
        : `<p class="sky-skip">${escapeHtml(shownCard.band)} skips: it goes through the layer on this path.</p>`;
    } else if (to.deg >= NVIS_DEG) {
      html += `<p class="sky-nvis">NVIS: the signal goes almost straight up and comes back down, so there is no skip zone. A low horizontal antenna suits it.</p>`;
    }
    html += `<p class="band-live" id="band-live" hidden></p>`;
    // Credit and "unavailable" note live in the card, so showing them never
    // moves the card on a phone (#164).
    html += `<p class="band-credit" id="band-credit" hidden></p><p class="band-live-note" id="band-live-note" hidden></p>`;
    return html;
  }

  // Live check: the nearest ionosonde with a recent, confident reading,
  // through the path service (KC2G's stations file has no CORS header).
  // The card never waits for it.
  let live = { key: "", at: 0, body: null, failed: false };
  const LIVE_MS = 5 * 60 * 1000;
  const CREDIT = "MUF(3000) map: prop.kc2g.com (KC2G) · ionosonde data: Lowell GIRO";
  function midpoint() {
    return along(tx, rx, 0.5);
  }
  async function fillLive() {
    if (!skyOn()) return showLiveNotes();
    const mid = midpoint();
    const key = `${mid.lat.toFixed(1)},${mid.lon.toFixed(1)}`;
    if (live.key !== key || Date.now() - live.at > LIVE_MS) {
      live = { key, at: Date.now(), body: null, failed: false, pending: true };
      try {
        const res = await fetch(`${PATH_API}/api/path/ionosonde?lat=${mid.lat.toFixed(3)}&lon=${mid.lon.toFixed(3)}`, {
          signal: AbortSignal.timeout(8000),
        });
        if (!res.ok) throw new Error(String(res.status));
        live.body = await res.json();
      } catch (_) {
        live.failed = true;
      }
      live.pending = false;
      if (live.key !== key) return;
    }
    if (live.pending) return;
    renderLive();
  }
  function renderLive() {
    const el = document.getElementById("band-live");
    if (!el || !skyOn()) return showLiveNotes();
    const s = live.body && live.body.station;
    if (live.failed || !live.body) {
      el.hidden = true;
    } else if (!s) {
      el.hidden = false;
      el.textContent = "live check: no recent ionosonde reading within 3000 km of this path.";
    } else {
      el.hidden = false;
      const age = s.age_min <= 1 ? "just now" : `${s.age_min} min ago`;
      let fit = "";
      const sky = answer.sky || {};
      if (sky.muf_mhz) {
        // The live reading at this path's hop length, by the secant law.
        const D = distKm(tx, rx);
        const n = sky.muf_hops || skyHops().n;
        const liveMuf = s.fof2_mhz / Math.cos(hopAngles(D / n).phi);
        const r = liveMuf / sky.muf_mhz;
        fit = r < 0.85 ? " · lower than predicted" : r > 1.15 ? " · higher than predicted" : " · fits the prediction";
      }
      el.textContent = `live check: ${s.name}, ${s.distance_km} km from the path midpoint, ${age}: foF2 ${s.fof2_mhz.toFixed(1)}, MUF(3000) ${s.mufd_mhz.toFixed(1)} MHz${fit}`;
    }
    showLiveNotes();
  }
  // One credit line whenever live data shows, one note when it failed.
  function showLiveNotes() {
    const credit = document.getElementById("band-credit");
    const note = document.getElementById("band-live-note");
    const sondeShown = skyOn() && live.body && live.body.station && !live.failed;
    const sondeFailed = skyOn() && live.failed;
    if (credit) credit.hidden = !(sondeShown || (mufOn() && mufLayer.ok));
    if (credit) credit.textContent = CREDIT;
    if (note) {
      note.hidden = !(sondeFailed || (mufOn() && mufLayer.failed));
      note.textContent = "Live ionosphere data unavailable right now.";
    }
  }

  // KC2G MUF(3000) contours (CORS open), with `muf=1` in the link.
  const MUF_URL = "https://prop.kc2g.com/renders/current/mufd-normal-now.geojson";
  const MUF_SRC = "kc2g-muf";
  const mufToggle = document.getElementById("map-muf");
  let mufLayer = { data: null, at: 0, ok: false, failed: false, labels: [] };
  function mufOn() {
    return !!(mufToggle && mufToggle.checked);
  }
  async function showMuf() {
    mufLayer.labels.forEach((m) => m.remove());
    mufLayer.labels = [];
    try {
      if (map.getLayer(MUF_SRC)) map.removeLayer(MUF_SRC);
      if (map.getSource(MUF_SRC)) map.removeSource(MUF_SRC);
    } catch (_) {}
    if (!mufOn()) {
      showLiveNotes();
      return;
    }
    if (!mufLayer.data || Date.now() - mufLayer.at > 15 * 60 * 1000) {
      try {
        const res = await fetch(MUF_URL, { signal: AbortSignal.timeout(10000) });
        if (!res.ok) throw new Error(String(res.status));
        mufLayer.data = await res.json();
        mufLayer.at = Date.now();
        mufLayer.failed = false;
      } catch (_) {
        mufLayer.data = null;
        mufLayer.failed = true;
      }
    }
    mufLayer.ok = !!mufLayer.data;
    if (mufLayer.ok && mufOn() && !map.getSource(MUF_SRC)) {
      try {
        map.addSource(MUF_SRC, { type: "geojson", data: mufLayer.data });
        map.addLayer({
          id: MUF_SRC,
          type: "line",
          source: MUF_SRC,
          paint: { "line-color": ["coalesce", ["get", "stroke"], "#ffffff"], "line-width": 1.5, "line-opacity": 0.85 },
        });
      } catch (_) {}
      // One label per contour line, at its middle.
      for (const ft of mufLayer.data.features || []) {
        const g = ft.geometry || {};
        const lines = g.type === "LineString" ? [g.coordinates] : g.type === "MultiLineString" ? g.coordinates : [];
        const level = ft.properties && ft.properties["level-value"];
        for (const coords of lines) {
          if (!coords || coords.length < 2 || level == null) continue;
          const mid = coords[Math.floor(coords.length / 2)];
          const el = document.createElement("span");
          el.className = "band-muf-label";
          el.textContent = `${Math.round(Number(level) * 10) / 10} MHz`;
          el.style.color = (ft.properties && ft.properties.stroke) || "#ffffff";
          mufLayer.labels.push(new maplibregl.Marker({ element: el }).setLngLat(mid).addTo(map));
        }
      }
    }
    showLiveNotes();
  }
  if (mufToggle) {
    mufToggle.addEventListener("change", () => {
      saveUrl();
      showMuf();
    });
  }

  // The greyline (sunrise and sunset line) as a dashed line, from the sun's
  // position at the planned hour. HF view only.
  const GREY_LAYER = "greyline";
  function greylineCoords(date) {
    const jd = julianDay(date);
    const gst = gmstHours(jd);
    const sun = sunEquatorial(jd);
    const out = [];
    for (let lng = -180; lng <= 180; lng += 1) out.push([lng, terminatorLat(lng, sun, gst, 0)]);
    return out;
  }
  function setGreyline() {
    try {
      const data = { type: "Feature", properties: {}, geometry: { type: "LineString", coordinates: greylineCoords(planDate()) } };
      if (!map.getSource(GREY_LAYER)) {
        if (!map.isStyleLoaded()) return;
        map.addSource(GREY_LAYER, { type: "geojson", data });
        map.addLayer({
          id: GREY_LAYER,
          type: "line",
          source: GREY_LAYER,
          paint: { "line-color": "#ffd60a", "line-width": 1.5, "line-opacity": 0.8, "line-dasharray": [3, 3] },
        });
      } else map.getSource(GREY_LAYER).setData(data);
      map.setLayoutProperty(GREY_LAYER, "visibility", skyOn() ? "visible" : "none");
    } catch (_) {}
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
    // The old path's terrain would keep the old RX in the fit (#153).
    terrain = null;
    requestCard();
  }

  // Clear: back to an empty map without a reload. Both pins, the card, a
  // band picked by hand and the time go; the station settings (antenna,
  // power, HF antenna) stay, since they describe your own gear.
  function clearPlan() {
    if (abort) abort.abort();
    abort = null;
    tx = null;
    rx = null;
    if (txMarker) txMarker.remove();
    if (rxMarker) rxMarker.remove();
    txMarker = null;
    rxMarker = null;
    terrain = null;
    answer = null;
    shownCard = null;
    pathFailed = false;
    terrainFailed = false;
    autoFit = false;
    if (overrideEl) overrideEl.value = "";
    if (hourEl) hourEl.value = "0";
    showHourLabel();
    updateDayNight(true);
    if (searchEl) searchEl.value = "";
    const old = cardEl.querySelector(".band-answer");
    if (old) old.remove();
    cardEl.classList.remove("has-answer");
    cardEl.style.minHeight = "";
    waitEl.hidden = true;
    if (emptyEl) emptyEl.hidden = false;
    showSkyKey();
    paintPath();
    drawProfile();
    setLosNote("terrain along the path");
    setHint("TX", "Click where you are");
    saveUrl();
  }

  // Fit the map to the path: both pins, the track and the bowed arc, centred
  // on the midpoint. The padding is worked out per side from the map's real
  // size, so a short map (the result card takes most of the height) still
  // shows the whole path. Until the user pans or zooms, a later size change
  // (the card appearing) fits again.
  const FIT_MARGIN = 16;
  const PIN_HALF_W = 14; // 28 px pin
  const PIN_HALF_H = 28; // 26 px pin plus the TX/RX label under it
  const ARC_PEAK = 45; // strokePinLink bows at most 90 / 2 px off the chord
  let autoFit = false;

  function fitPins(opts) {
    if (!tx || !rx) return;
    const host = map.getContainer();
    const w = host.clientWidth;
    const h = host.clientHeight;
    if (!w || !h) return;
    const b = new maplibregl.LngLatBounds();
    b.extend([tx.lon, tx.lat]);
    b.extend([rx.lon, rx.lat]);
    // The great circle, which bows away from the straight line on a long
    // path (the terrain samples follow it too).
    greatCircle(32).forEach((p) => b.extend([p.lon, p.lat]));
    // The arc bows sideways from the chord, so an east-west path needs room
    // above and below, a north-south one left and right.
    const s1 = map.project([tx.lon, tx.lat]);
    const s2 = map.project([rx.lon, rx.lat]);
    const dist = Math.hypot(s2.x - s1.x, s2.y - s1.y) || 1;
    const bowX = (ARC_PEAK * Math.abs(s2.y - s1.y)) / dist;
    const bowY = (ARC_PEAK * Math.abs(s2.x - s1.x)) / dist;
    const padX = Math.min(w * 0.4, FIT_MARGIN + Math.max(PIN_HALF_W, bowX));
    const padY = Math.min(h * 0.4, FIT_MARGIN + Math.max(PIN_HALF_H, bowY));
    autoFit = true;
    map.fitBounds(b, {
      padding: { top: padY, bottom: padY, left: padX, right: padX },
      maxZoom: 14,
      duration: reduceMotion || (opts && opts.instant) ? 0 : 600,
    });
  }
  map.on("movestart", (e) => {
    if (e.originalEvent) autoFit = false;
  });
  map.on("resize", () => {
    if (autoFit) fitPins({ instant: true });
  });

  async function requestCard(opts) {
    if (!tx || !rx) return;
    const refit = !opts || opts.refit !== false;
    pathFailed = false;
    terrainFailed = false;
    if (abort) abort.abort();
    abort = new AbortController();
    const signal = abort.signal;
    if (emptyEl) emptyEl.hidden = true;
    // Same pins, new band, power or antenna: the old answer stays until the
    // new one replaces it in place, so the card does not jump (#153). A new
    // path starts from the waiting line.
    if (refit || !cardEl.classList.contains("has-answer")) {
      waitEl.hidden = false;
      cardEl.classList.remove("has-answer");
      cardEl.style.minHeight = "";
    }
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
          hf_antenna: hfAntenna(),
          band: override() && override().hf ? overrideKey() : undefined,
        }),
        signal,
      });
      if (!res.ok) throw new Error(await res.text());
      answer = await res.json();
      pathFailed = false;
      paintPath();
      if (emptyEl) emptyEl.hidden = true;
      // No card yet: without the terrain it ignores the hills. loadTerrain
      // shows it once the elevation is in (or straight away when cached).
      if (rxMarker) pulse(rxMarker.getElement(), "rx");
      // An HF answer goes by the sky, so it shows without the terrain
      // (#157); the terrain only fills in the VHF and UHF bands later.
      if (hfOnly()) showAnswer();
      await loadTerrain(signal);
    } catch (e) {
      if (e.name === "AbortError") return;
      pathFailed = true;
      answer = null;
      shownCard = null;
      showSkyKey();
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
    terrainFailed = false;
    if (!elevation || elevation.key !== key) {
      setLosNote("checking terrain…");
      terrain = null;
      drawProfile();
      try {
        const samples = await fetchElevation(tx, rx, signal, showTerrainBusy);
        if (signal.aborted) return;
        elevation = { key, samples };
      } catch (e) {
        if (e.name === "AbortError") return;
        // Not kept, so the next request (or Retry) looks it up again.
        terrainFailed = true;
      }
    }
    judgeTerrain();
  }

  // The frequency the path is judged at: a band picked by hand, else the
  // FM card's frequency, else 2m calling.
  function autoMhz() {
    const fm = answer && (answer.kind === "fm" ? answer : answer.fm);
    const f = fm ? Number(fm.frequency_mhz) : NaN;
    return Number.isFinite(f) && f > 30 ? f : 145;
  }
  function fresnelMhz() {
    const o = override();
    return o ? o.mhz : autoMhz();
  }
  function losAt(mhz) {
    return elevation && elevation.key === pathKey() && elevation.samples
      ? lineOfSight(elevation.samples, txAntM(), rxAntM(), mhz)
      : null;
  }

  function judgeTerrain() {
    waitEl.hidden = true;
    terrain = losAt(fresnelMhz());
    if (terrain) markTrack(terrain, fresnelMhz());
    paintPath();
    drawProfile();
    const o = override();
    const what = o ? o.label : "VHF and UHF";
    if (terrainFailed) setLosNote("terrain lookup failed");
    else if (!terrain) setLosNote("no terrain data for this path");
    else if (terrain.byTerrain) setLosNote(`terrain blocks ${what}`);
    else if (terrain.blocked) setLosNote(`beyond line of sight for ${what}`);
    else if (terrain.marginal) setLosNote(`marginal: ${marginalCause(terrain).text}`);
    else setLosNote(`clear line of sight for ${what}`);
    showAnswer();
  }

  // One line on the direct path for the card, at the band `what`.
  function losLine(t, what) {
    if (t.blocked) return t.byTerrain ? `Terrain blocks ${what}.` : `Beyond line of sight for ${what}.`;
    if (t.marginal) return marginalLine(t);
    return `Clear line of sight for ${what}.`;
  }

  // The server answers on distance alone: FM inside the smooth-earth radio
  // horizon, HF past it. Only a blocked path goes to HF: when the ground or
  // the earth's curve is in the way, switch to the HF card it sent along;
  // when the server said HF but the real terrain leaves a line, switch to
  // the FM card it sent along.
  function recommendedCard(t) {
    let card = { ...answer };
    if (t && t.blocked) {
      const reason = losLine(t, "VHF and UHF");
      if (card.kind === "fm" && card.hf) {
        card = { ...card.hf, why: `${reason} ${card.hf.why || ""}`.trim() };
      }
      card.note = `${reason} ${card.note || ""}`.trim();
      return card;
    }
    if (t && card.kind === "hf" && answer.fm) card = { ...answer.fm };
    if (t && card.kind === "fm") {
      if (t.marginal) card.confidence = "Close path on 2m FM, but the line of sight is marginal.";
      card.note = `${losLine(t, "VHF and UHF")} SSB stays off.`;
    }
    return card;
  }

  // How a line-of-sight band does on this path: blocked by the ground, or
  // its signal budget at its own power (the station's, or the band's legal
  // limit). Null while the terrain is unknown.
  function vhfVerdict(o) {
    const t = losAt(o.mhz);
    if (!t) return null;
    if (t.blocked) return { ok: false, t, budget: null };
    const b = signalBudget(t, o.mhz, Math.min(powerW(), o.maxW || powerW()));
    return { ok: b.margin >= 0, t, budget: b };
  }

  // Bands that work on this path at this hour and power: HF from the
  // service's per-band list, VHF and UHF from the terrain here. The same
  // list whichever band is picked.
  function workingBands() {
    const out = [];
    for (const key of VHF_KEYS) {
      const v = vhfVerdict(OVERRIDES[key]);
      if (v && v.ok) out.push({ key, label: OVERRIDES[key].label, power: v.budget.watts });
    }
    for (const b of answer.bands || []) {
      if (b.ok && OVERRIDES[b.band]) out.push({ key: b.band, label: b.band, power: b.power_w });
    }
    return out;
  }

  // A line-of-sight note for a path that is not blocked: a marginal path
  // with plenty of signal gets the soft note, not the advice to go higher.
  function vhfNote(t, what, margin) {
    if (t.marginal && margin >= SOLID_MARGIN_DB) return PLENTY_NOTE;
    return losLine(t, what);
  }

  // A band picked by hand. HF comes from the path service's band_card,
  // with no terrain line (it goes by the sky); the rest is worked out here
  // from the terrain and the signal budget.
  function overrideCard(o) {
    const what = o.label;
    if (o.hf) {
      if (answer.band_card) return { ...answer.band_card };
      return {
        ok: false,
        kind: "hf",
        frequency_mhz: o.mhz.toFixed(3),
        band: what,
        ssb: true,
        mode: "USB",
        power_w: null,
        preset: "hf-poor",
        qsy: `/radio qsy ${o.mhz.toFixed(3)}`,
        preset_cmd: "/radio preset hf-poor",
        confidence: `${what} doesn't work on this path.`,
        why: `There is no ${what} calling frequency shared by these two countries.`,
        note: "",
        voice_note: null,
        footnote: "",
        distance_km: answer.distance_km,
      };
    }
    let freq = o.freq;
    if (!freq) {
      const fm = answer.kind === "fm" ? answer : answer.fm;
      const us = fm && fm.frequency_mhz === "145.010";
      freq = o.label === "2m" ? (fm ? fm.frequency_mhz : "145.500") : us ? "446.000" : "433.500";
    }
    const v = vhfVerdict(o);
    budget = v && v.budget;
    if (!v) budget = signalBudget(null, o.mhz, Math.min(powerW(), o.maxW || powerW()));
    let confidence = `${what}, picked by hand.`;
    let why = budget ? budget.text : "";
    let note = v && v.budget ? vhfNote(v.t, what, v.budget.margin) : v ? losLine(v.t, what) : "";
    if (v && !v.ok) {
      confidence = `${what} doesn't work on this path.`;
      if (!v.budget) {
        why = `${losLine(v.t, what)} More power will not get past it.`;
        note = "";
      }
    }
    return {
      ok: !v || v.ok,
      kind: "fm",
      frequency_mhz: freq,
      band: o.label,
      ssb: false,
      mode: "FM",
      power_w: budget && (!v || v.ok || v.budget) ? budget.watts : null,
      preset: "vhf-fm",
      qsy: `/radio qsy ${freq}`,
      preset_cmd: "/radio preset vhf-fm",
      confidence,
      why,
      note: `${note} SSB stays off.`.trim(),
      voice_note: null,
      footnote: "",
      distance_km: answer.distance_km,
    };
  }

  // An answer that doesn't depend on the terrain: an HF answer with no
  // FM fallback (the path is past FM range), or an HF band picked by hand.
  function hfOnly() {
    if (!answer) return false;
    const o = override();
    if (o) return !!o.hf;
    return answer.kind === "hf" && !answer.fm;
  }

  function showAnswer() {
    if (!answer) return;
    if (terrainFailed && !hfOnly()) {
      showTerrainFailed();
      return;
    }
    if (terrainFailed) setLosNote("terrain unavailable, VHF/UHF not judged");
    waitEl.hidden = true;
    const o = override();
    budget = null;
    // The recommendation is always judged at its own frequency, so the
    // banner names what the page would pick without the override.
    const rec = recommendedCard(o ? losAt(autoMhz()) : terrain);
    let card = rec;
    if (o) card = overrideCard(o);
    else if (card.kind === "fm" && !(terrain && terrain.blocked)) {
      budget = signalBudget(terrain, fresnelMhz(), powerW());
      card.power_w = budget.watts;
      card.why = `${card.why || ""} ${budget.text}`.trim();
      if (terrain && terrain.marginal && budget.margin >= SOLID_MARGIN_DB) {
        card.confidence = `Should work well on ${card.band} FM.`;
        card.note = `${PLENTY_NOTE} SSB stays off.`;
      }
    }
    // Mobile stays allowed on PMR446 with its roof-whip gain; it only gets a
    // short note (Tobias, #150).
    const mobile = txAntEl.value === "mobile" || rxAntEl.value === "mobile";
    const antNote = o && o.mobileNote && mobile ? o.mobileNote : "";
    shownCard = card;
    renderCard(card, o ? { label: o.label, info: o.info, antNote, rec: rec.band } : null);
    setHint("OK", card.confidence || "Ready");
    showSkyKey();
    paintPath();
    drawProfile();
    fillLive();
  }

  // While a busy elevation service is retried, the answer so far ignores
  // the hills, so the card goes back to waiting until the terrain is in.
  function showTerrainBusy() {
    setLosNote("terrain lookup busy, retrying…");
    if (hfOnly() && cardEl.querySelector(".band-answer")) return;
    cardEl.classList.remove("has-answer");
    const old = cardEl.querySelector(".band-answer");
    if (old) old.remove();
    waitEl.hidden = false;
    setHint("…", "Terrain lookup busy, retrying");
  }

  // Without the terrain the answer could be wrong (an HF card for a path
  // the hills leave open on 2m), so no answer is shown until it loads.
  function showTerrainFailed() {
    shownCard = null;
    showSkyKey();
    cardEl.classList.add("has-answer");
    const old = cardEl.querySelector(".band-answer");
    if (old) old.remove();
    cardEl.insertAdjacentHTML(
      "beforeend",
      `<div class="band-answer"><p class="band-err">Terrain lookup failed: the elevation service is busy, so this path can't be judged yet.</p>
        <button type="button" class="band-retry">Retry</button></div>`
    );
    setHint("ERR", "Terrain lookup failed");
  }

  // Terrain under the path, drawn on the curved earth: the dashed curve is
  // sea level bent by the 4/3-earth bulge, the ground sits on top of it,
  // and the straight line joins the two antenna tops.
  function drawProfile() {
    if (!profileEl) return;
    if (skyOn()) {
      drawSky();
      return;
    }
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

  function copyButton(text) {
    return `<button type="button" class="band-copy" data-copy="${escapeHtml(text)}"><code>${escapeHtml(text)}</code><em>copy</em></button>`;
  }

  // The bands that work here as one switch row; a click picks that band,
  // or goes back to the recommendation when it is the recommended band.
  function worksHtml(c, rec) {
    if (skyOn()) return skyWorksHtml(c, rec);
    if (!terrain && !(answer.bands || []).length) return "";
    const list = workingBands();
    if (!list.length) return `<p class="band-works">No band works on this path at this hour and power.</p>`;
    const chips = list
      .map((b) => {
        const on = b.label === c.band ? "true" : "false";
        const pick = b.label === rec ? "" : b.key;
        const watts = b.power != null ? `<small>${b.power} W</small>` : "";
        return `<button type="button" class="band-works-pick" data-band="${escapeHtml(pick)}" aria-pressed="${on}">${escapeHtml(b.label)}${watts}</button>`;
      })
      .join("");
    return `<div class="band-works" role="group" aria-label="Bands that work on this path at this hour">${chips}</div>`;
  }

  // HF view: every HF band with a dot for the planned hour (closed ones
  // dimmed, "skips" when above the MUF), after the VHF and UHF bands that
  // work.
  function skyWorksHtml(c, rec) {
    const chip = (key, label, state, sub) => {
      const on = label === c.band ? "true" : "false";
      const pick = label === rec ? "" : key;
      const cls = state === "closed" ? " is-closed" : "";
      return `<button type="button" class="band-works-pick${cls}" data-band="${escapeHtml(pick)}" data-state="${state}" aria-pressed="${on}"><span><i class="sky-dot sky-${state}" aria-hidden="true"></i>${escapeHtml(label)}</span><small>${escapeHtml(sub)}</small></button>`;
    };
    const vhf = workingBands()
      .filter((b) => VHF_KEYS.includes(b.key))
      .map((b) => chip(b.key, b.label, "open", b.power != null ? `${b.power} W` : ""));
    const hf = (answer.bands || [])
      .filter((b) => OVERRIDES[b.band])
      .map((b) =>
        chip(b.band, b.band, b.state || (b.ok ? "open" : "closed"), b.ok ? `${b.power_w} W` : b.skips ? "skips" : "closed")
      );
    return `<div class="band-works" role="group" aria-label="HF bands on this path at this hour">${vhf.concat(hf).join("")}</div>`;
  }

  // The band plan reminder gets its own line; the rest of the footnote
  // (what the forecast assumes) stays quiet.
  const BAND_PLAN = "Check your band plan before you transmit.";

  // Colour of the verdict tag: red when the band doesn't work, yellow for a
  // marginal or faint path (for a hand-picked band, also when only the lines
  // under its tag say so), green for clear paths only.
  function verdictState(c, nogo, manual) {
    const t = c.confidence || "";
    if (nogo || /doesn't work|^Nothing|^Not /.test(t)) return "bad";
    const said = manual ? `${t} ${c.why || ""} ${c.note || ""}` : t;
    if (/marginal|faint/i.test(said)) return "warn";
    return "ok";
  }

  // Three zones, left to right: what to tune, will it work, what to type.
  // The band row on top switches bands and shows a hand-picked band.
  function renderCard(c, manual) {
    const rec = manual ? manual.rec : c.band;
    const nogo = manual && c.ok === false;
    const power = c.power_w != null ? `${c.power_w} W` : nogo ? "none" : `over ${powerW()} W`;
    const ssb = c.ssb ? "on" : "off";
    const voice = c.voice_note ? `<p class="band-voice">${escapeHtml(c.voice_note)}</p>` : "";
    // "SSB on, USB for data." only repeats the SSB and mode readouts.
    const noteText = (c.note || "").replace("SSB on, USB for data.", "").trim();
    const note = noteText ? `<p class="band-note">${escapeHtml(noteText)}</p>` : "";
    const foot = (c.footnote || "").replace(BAND_PLAN, "").replace(/\s+/g, " ").trim();
    const plan = (c.footnote || "").includes(BAND_PLAN) ? `<b>${BAND_PLAN}</b>` : "";
    const works = worksHtml(c, rec);
    // The licence or legal note goes with the band shown, picked or not.
    const shown = Object.values(OVERRIDES).find((o) => o.label === c.band);
    const info = manual ? manual.info : shown && shown.info;
    cardEl.classList.add("has-answer");
    const old = cardEl.querySelector(".band-answer");
    if (old) old.remove();
    cardEl.insertAdjacentHTML(
      "beforeend",
      `<div class="band-answer">
        ${works || manual ? `<div class="band-bandbar">
          ${works ? `<span class="band-label">band</span>${works}` : ""}
          ${manual ? `<div class="band-override-banner">
            <span class="band-tag">picked by hand</span>
            <p><b>${escapeHtml(manual.label)}</b> · recommended: ${escapeHtml(manual.rec)}</p>
            <button type="button" class="band-revert">↺ back to ${escapeHtml(manual.rec)}</button>
          </div>` : ""}
        </div>` : ""}
        <div class="band-body">
          <div class="band-freq-block">
            <p class="band-freq">${escapeHtml(c.frequency_mhz)}<span> MHz</span></p>
            <p class="band-band">${escapeHtml(c.band)} · ${escapeHtml(c.mode)}</p>
            <dl class="band-stats">
              <dt>SSB</dt><dd>${ssb}</dd>
              <dt>power</dt><dd>${escapeHtml(power)}</dd>
              <dt>preset</dt><dd>${escapeHtml(c.preset)}</dd>
              <dt>path</dt><dd>${c.distance_km} km</dd>
            </dl>
          </div>
          <div class="band-verdict">
            <p class="band-conf band-conf-${skyOn() ? SKY_TAG[skyState()] : verdictState(c, nogo, manual)}${nogo ? " band-nogo" : ""}"><i class="band-led" aria-hidden="true"></i>${escapeHtml(c.confidence || "")}</p>
            ${c.why ? `<p class="band-why">${escapeHtml(c.why)}</p>` : ""}
            ${skyOn() ? skyLines() : ""}
            ${voice}
            ${note}
            ${skyOn() ? skyStrip() : ""}
          </div>
          <div class="band-do">
            <span class="band-label">set your radio</span>
            ${copyButton(c.qsy)}
            ${copyButton(c.preset_cmd)}
            ${info ? `<p class="band-override-info">⚠ ${escapeHtml(info)}</p>` : ""}
            ${manual && manual.antNote ? `<p class="band-override-info band-ant-note">ⓘ ${escapeHtml(manual.antNote)}</p>` : ""}
          </div>
        </div>
        ${foot || plan ? `<p class="band-foot"><span>${escapeHtml(foot)}</span>${plan}</p>` : ""}
      </div>`
    );
    // While the pins stay put the card only grows, so a shorter band's card
    // does not pull the page up on a phone scrolled to the bottom (#153).
    cardEl.style.minHeight = `${cardEl.offsetHeight}px`;
  }

  cardEl.addEventListener("click", (e) => {
    const cell = e.target.closest(".sky-cell");
    if (!cell || !hourEl) return;
    hourEl.value = cell.getAttribute("data-off");
    hourEl.dispatchEvent(new Event("input"));
  });
  cardEl.addEventListener("click", (e) => {
    if (e.target.closest(".band-retry")) requestCard({ refit: false });
  });
  cardEl.addEventListener("click", (e) => {
    if (!e.target.closest(".band-revert") || !overrideEl) return;
    overrideEl.value = "";
    requestCard({ refit: false });
  });
  cardEl.addEventListener("click", (e) => {
    const btn = e.target.closest(".band-works-pick");
    if (!btn || !overrideEl) return;
    overrideEl.value = btn.getAttribute("data-band") || "";
    requestCard({ refit: false });
  });

  cardEl.addEventListener("click", async (e) => {
    const btn = e.target.closest(".band-copy");
    if (!btn) return;
    const label = btn.querySelector("em");
    const done = await window.WCR.copyText(btn.getAttribute("data-copy") || "");
    label.textContent = done === "copied" ? "copied" : "copy failed";
    btn.classList.toggle("band-copied", done === "copied");
    setTimeout(() => {
      label.textContent = "copy";
      btn.classList.remove("band-copied");
    }, 1200);
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
    setGreyline();
    if (mufOn()) showMuf();
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
  // A new width reflows the card, so its kept height no longer fits. A
  // phone's URL bar only changes the height, which keeps it.
  let cardWidth = window.innerWidth;
  window.addEventListener("resize", () => {
    try {
      map.resize();
    } catch (_) {}
    if (window.innerWidth !== cardWidth) {
      cardWidth = window.innerWidth;
      cardEl.style.minHeight = "";
    }
  });

  // Shareable link: ?tx=lat,lon&rx=lat,lon, plus txant=, rxant= (metres or
  // mobile), hfant= and w= when not the defaults. An old ant= sets both ends. Other query keys (e.g.
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
    set("txant", antennaParam(txAntEl) !== "10" ? antennaParam(txAntEl) : null);
    set("rxant", antennaParam(rxAntEl) !== "10" ? antennaParam(rxAntEl) : null);
    set("hfant", hfAntenna() !== "dipole" ? hfAntenna() : null);
    set("hfwire", isWireAntenna() && wireOrientation() !== "flat" ? wireOrientation() : null);
    set("band", overrideKey() || null);
    set("w", powerW() !== 100 ? String(powerW()) : null);
    set("muf", mufOn() ? "1" : null);
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
    setAntenna(txAntEl, q.get("txant") || both);
    setAntenna(rxAntEl, q.get("rxant") || both);
    pickOption(powerEl, q.get("w"));
    const hf = hfAntEls.find((el) => el.value === q.get("hfant"));
    if (hf) hf.checked = true;
    if (WIRE_NOTES[q.get("hfwire")]) wire = q.get("hfwire");
    showWireNote();
    if (overrideEl && OVERRIDES[q.get("band")]) overrideEl.value = q.get("band");
    if (mufToggle && q.get("muf") === "1") mufToggle.checked = true;
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
      setGreyline();
      cardEl.querySelectorAll(".sky-cell").forEach((el) => el.classList.toggle("is-sel", el.getAttribute("data-off") === hourEl.value));
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
  const clearEl = document.getElementById("band-clear");
  if (clearEl) clearEl.addEventListener("click", clearPlan);
  for (const el of hfAntEls) {
    el.addEventListener("change", () => {
      showWireNote();
      requestCard({ refit: false });
    });
  }
  if (wireEl) {
    wireEl.addEventListener("click", () => (wireMenuEl.hidden ? openWireMenu() : closeWireMenu()));
    wireEl.addEventListener("keydown", (e) => {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        openWireMenu();
      }
    });
    wireMenuEl.addEventListener("click", (e) => {
      const li = e.target.closest("[data-wire]");
      if (li) pickWire(li.getAttribute("data-wire"));
    });
    wireMenuEl.addEventListener("keydown", (e) => {
      const items = [...wireMenuEl.querySelectorAll("[data-wire]")];
      const i = items.indexOf(document.activeElement);
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const next = items[(i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length];
        if (next) next.focus();
      } else if ((e.key === "Enter" || e.key === " ") && items[i]) {
        e.preventDefault();
        pickWire(items[i].getAttribute("data-wire"));
      } else if (e.key === "Escape" || e.key === "Tab") {
        closeWireMenu(e.key === "Escape");
      }
    });
    document.addEventListener("click", (e) => {
      if (!e.target.closest(".band-hfwire")) closeWireMenu();
    });
    window.addEventListener("resize", placeWirePicker);
  }
  if (overrideEl) {
    overrideEl.addEventListener("change", () => {
      saveUrl();
      requestCard({ refit: false });
    });
  }
  window.addEventListener("resize", () => drawProfile());
  // Keep "now" honest as the clock rolls over an hour.
  setInterval(showHourLabel, 60000);

  map.on("click", (e) => {
    if (!tx) setTx(e.lngLat);
    else setRx(e.lngLat);
  });

  // A spinner in the button while the browser looks; repeat clicks are
  // ignored until it answers.
  const LOCATE_ERRORS = { 1: "Location permission denied", 2: "Location not available", 3: "Location timed out" };
  function setLocating(on) {
    locateBtn.disabled = on;
    locateBtn.classList.toggle("is-busy", on);
    if (on) locateBtn.setAttribute("aria-busy", "true");
    else locateBtn.removeAttribute("aria-busy");
  }
  locateBtn.addEventListener("click", () => {
    if (locateBtn.disabled) return;
    if (!navigator.geolocation) {
      setHint("ERR", "Location not available");
      return;
    }
    setLocating(true);
    navigator.geolocation.getCurrentPosition(
      (pos) => {
        setLocating(false);
        const ll = { lng: pos.coords.longitude, lat: pos.coords.latitude };
        map.flyTo({ center: [ll.lng, ll.lat], zoom: 7, duration: reduceMotion ? 0 : 800 });
        setTx(ll);
      },
      (err) => {
        setLocating(false);
        setHint("ERR", LOCATE_ERRORS[err && err.code] || "Could not read location");
      },
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
        // Samples per rating (strict) and per track colour (shown).
        strict: terrain.samples.map((s) => s.state[0]).join(""),
        shown: terrain.samples.map((s) => s.show[0]).join(""),
      },
      budget,
      override: overrideKey(),
      sky: skyOn()
        ? {
            hops: skyHops(),
            takeoff: skyTakeoff(),
            rings: skyRings(),
            state: skyState(),
            greyline: greylineCoords(planDate()),
          }
        : null,
    }),
    zoomTo: (z) => map.jumpTo({ zoom: z }),
    // Where the pins and the arc's peak sit on the map, in px (fit checks).
    fitBox: () => {
      const host = map.getContainer();
      const pins = [tx, rx].filter(Boolean).map((p) => map.project([p.lon, p.lat]));
      let arc = null;
      if (pins.length === 2) {
        const [s1, s2] = pins;
        const dist = Math.hypot(s2.x - s1.x, s2.y - s1.y) || 1;
        const bulge = Math.min(90, Math.max(48, dist * 0.22));
        const cx = (s1.x + s2.x) / 2 + ((s2.y - s1.y) / dist) * bulge;
        const cy = (s1.y + s2.y) / 2 - ((s2.x - s1.x) / dist) * bulge;
        arc = { x: (s1.x + s2.x) / 4 + cx / 2, y: (s1.y + s2.y) / 4 + cy / 2 };
      }
      return { w: host.clientWidth, h: host.clientHeight, pins: pins.map((p) => ({ x: p.x, y: p.y })), arc };
    },
    // How far each pin's centre sits from its point on the map, in px.
    pinOffsets: () =>
      [txMarker, rxMarker].filter(Boolean).map((m) => {
        const r = m.getElement().getBoundingClientRect();
        const box = map.getContainer().getBoundingClientRect();
        const p = map.project(m.getLngLat());
        return Math.hypot(r.left + r.width / 2 - box.left - p.x, r.top + r.height / 2 - box.top - p.y);
      }),
  };
})();
