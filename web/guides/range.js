/* SPDX-License-Identifier: Apache-2.0 */
/* Range test map: reads /rangetest CSV logs in the browser (nothing is
   uploaded), draws each probe as a dot and shades the area around the
   route by how well probes got through. */
(function () {
  const CARTO_KEY = "cb1_3nxz_1_dba340f0a1c8450af09da179";
  const STYLES = {
    satellite: {
      tiles: ["https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}"],
      attribution: 'Tiles &copy; <a href="https://www.esri.com/">Esri</a> — Esri, Maxar, Earthstar Geographics',
    },
    streets: {
      tiles: [`https://basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}@2x.png?key=${CARTO_KEY}`],
      attribution:
        '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>, &copy; <a href="https://carto.com/attributions">CARTO</a>',
    },
  };
  const STYLE_KEY = "wcr-range-style";

  // Shading: 150 m squares, each the distance-weighted score of probes within 500 m.
  const CELL_M = 150;
  const REACH_M = 500;
  const COLORS = { good: "#3fbf4f", patchy: "#f5a81f", none: "#e03c3c" };

  /** Score of one probe, 0..1. */
  function score(p) {
    if (p.result === "both") {
      const snrs = [p.they, p.we].filter((v) => Number.isFinite(v));
      if (!snrs.length) return 0.85;
      const margin = Math.min(...snrs);
      // -5 dB (near the decode edge) -> 0.7, 10 dB or more -> 1.
      return Math.max(0.7, Math.min(1, 0.7 + ((margin + 5) / 15) * 0.3));
    }
    if (p.result === "heard_them") return 0.4;
    return 0;
  }

  function band(s) {
    if (s >= 0.67) return "good";
    if (s >= 0.34) return "patchy";
    return "none";
  }

  /** Parses one CSV log into probes. Rows without a position are counted, not mapped. */
  function parseCsv(text) {
    const lines = text.split(/\r?\n/).filter((l) => l.trim());
    if (!lines.length) return { probes: [], unplaced: 0, far: "", farGrid: "" };
    const head = lines[0].split(",").map((h) => h.trim());
    const col = (n) => head.indexOf(n);
    const need = ["lat", "lon", "result"];
    if (need.some((n) => col(n) < 0)) throw new Error("not a range test CSV (no lat, lon, result columns)");
    const num = (v) => (v === undefined || v === "" ? NaN : Number(v));
    const probes = [];
    let unplaced = 0;
    let far = "";
    let farGrid = "";
    for (const line of lines.slice(1)) {
      const f = line.split(",");
      const lat = num(f[col("lat")]);
      const lon = num(f[col("lon")]);
      if (col("far_station") >= 0 && f[col("far_station")]) far = f[col("far_station")];
      if (col("far_grid") >= 0 && f[col("far_grid")]) farGrid = f[col("far_grid")];
      if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
        unplaced++;
        continue;
      }
      const time = f[col("time_utc")] || "";
      probes.push({
        lat,
        lon,
        time,
        ts: Date.parse(time) / 1000,
        probe: f[col("probe")] || "",
        result: (f[col("result")] || "none").trim(),
        they: num(f[col("snr_they_heard_db")]),
        we: num(f[col("snr_we_heard_db")]),
      });
    }
    return { probes, unplaced, far, farGrid };
  }

  /** Centre of a Maidenhead square (4, 6 or 8 characters). */
  function gridCentre(g) {
    g = (g || "").trim().toUpperCase();
    if (!/^[A-R]{2}[0-9]{2}([A-X]{2}([0-9]{2})?)?$/.test(g)) return null;
    let lon = (g.charCodeAt(0) - 65) * 20 - 180;
    let lat = (g.charCodeAt(1) - 65) * 10 - 90;
    let w = 2;
    let h = 1;
    lon += Number(g[2]) * 2;
    lat += Number(g[3]);
    if (g.length >= 6) {
      w = 2 / 24;
      h = 1 / 24;
      lon += (g.charCodeAt(4) - 65) * w;
      lat += (g.charCodeAt(5) - 65) * h;
    }
    if (g.length >= 8) {
      w /= 10;
      h /= 10;
      lon += Number(g[6]) * w;
      lat += Number(g[7]) * h;
    }
    return { lat: lat + h / 2, lon: lon + w / 2 };
  }

  function metresBetween(a, b) {
    const R = 6371000;
    const toRad = Math.PI / 180;
    const x = (b.lon - a.lon) * toRad * Math.cos(((a.lat + b.lat) / 2) * toRad);
    const y = (b.lat - a.lat) * toRad;
    return Math.sqrt(x * x + y * y) * R;
  }

  /** Coverage squares as GeoJSON polygons. */
  function coverage(probes) {
    if (!probes.length) return [];
    const lat0 = probes.reduce((s, p) => s + p.lat, 0) / probes.length;
    const mLat = 111320;
    const mLon = 111320 * Math.cos((lat0 * Math.PI) / 180);
    const dLat = CELL_M / mLat;
    const dLon = CELL_M / mLon;
    // Bucket probes by REACH_M so each square only looks at its neighbours.
    const bLat = REACH_M / mLat;
    const bLon = REACH_M / mLon;
    const buckets = new Map();
    const key = (i, j) => i + ":" + j;
    for (const p of probes) {
      const k = key(Math.floor(p.lat / bLat), Math.floor(p.lon / bLon));
      if (!buckets.has(k)) buckets.set(k, []);
      buckets.get(k).push(p);
    }
    // Only squares near a probe: walk each probe's surroundings once.
    const cells = new Set();
    const span = Math.ceil(REACH_M / CELL_M);
    for (const p of probes) {
      const ci = Math.floor(p.lat / dLat);
      const cj = Math.floor(p.lon / dLon);
      for (let di = -span; di <= span; di++) {
        for (let dj = -span; dj <= span; dj++) cells.add(key(ci + di, cj + dj));
      }
    }
    const out = [];
    for (const c of cells) {
      const [i, j] = c.split(":").map(Number);
      const centre = { lat: (i + 0.5) * dLat, lon: (j + 0.5) * dLon };
      const bi = Math.floor(centre.lat / bLat);
      const bj = Math.floor(centre.lon / bLon);
      let wSum = 0;
      let sSum = 0;
      for (let a = -1; a <= 1; a++) {
        for (let b = -1; b <= 1; b++) {
          for (const p of buckets.get(key(bi + a, bj + b)) || []) {
            const d = metresBetween(centre, p);
            if (d > REACH_M) continue;
            const w = 1 / Math.pow(d + 50, 2);
            wSum += w;
            sSum += w * score(p);
          }
        }
      }
      if (!wSum) continue;
      const s = sSum / wSum;
      const lat = i * dLat;
      const lon = j * dLon;
      out.push({
        type: "Feature",
        properties: { band: band(s), score: Math.round(s * 100) / 100 },
        geometry: {
          type: "Polygon",
          coordinates: [[[lon, lat], [lon + dLon, lat], [lon + dLon, lat + dLat], [lon, lat + dLat], [lon, lat]]],
        },
      });
    }
    return out;
  }

  // Coverage area: 10° slices around the far station. In each slice the
  // edge is the furthest both-ways probe with at least 70% of the probes
  // nearer than it also both ways, so one lucky hilltop does not stretch it.
  // A slice with no probes stays empty: no claim where you did not drive.
  const SLICE_DEG = 10;
  const SLICE_GOOD = 0.7;

  function bearingDeg(from, to) {
    const toRad = Math.PI / 180;
    const x = (to.lon - from.lon) * Math.cos(((from.lat + to.lat) / 2) * toRad);
    const y = to.lat - from.lat;
    return (Math.atan2(x, y) / toRad + 360) % 360;
  }

  /** Edge distance in metres for each slice (0 = nothing measured or nothing worked). */
  function sliceReach(probes, home) {
    const slices = Array.from({ length: 360 / SLICE_DEG }, () => []);
    for (const p of probes) {
      const d = metresBetween(home, p);
      slices[Math.floor(bearingDeg(home, p) / SLICE_DEG) % slices.length].push({ d, ok: p.result === "both" });
    }
    return slices.map((list) => {
      list.sort((a, b) => a.d - b.d);
      let good = 0;
      let reach = 0;
      list.forEach((p, i) => {
        if (p.ok) good++;
        if (p.ok && good / (i + 1) >= SLICE_GOOD) reach = p.d;
      });
      return { reach, probes: list.length };
    });
  }

  function offset(home, bearing, m) {
    const toRad = Math.PI / 180;
    return [
      home.lon + (m * Math.sin(bearing * toRad)) / (111320 * Math.cos(home.lat * toRad)),
      home.lat + (m * Math.cos(bearing * toRad)) / 111320,
    ];
  }

  /** The coverage area as one GeoJSON polygon (a rose of slices), its area and the slices measured. */
  function coverageArea(probes, home) {
    if (!home || !probes.length) return { features: [], km2: 0, measured: 0 };
    const slices = sliceReach(probes, home);
    const ring = [];
    let km2 = 0;
    slices.forEach((s, i) => {
      const a0 = i * SLICE_DEG;
      if (!s.reach) {
        ring.push([home.lon, home.lat]);
        return;
      }
      for (let a = a0; a <= a0 + SLICE_DEG; a += 2) ring.push(offset(home, a, s.reach));
      km2 += (Math.PI * Math.pow(s.reach / 1000, 2) * SLICE_DEG) / 360;
    });
    ring.push(ring[0]);
    return {
      features: [{ type: "Feature", properties: {}, geometry: { type: "Polygon", coordinates: [ring] } }],
      km2,
      measured: slices.filter((s) => s.probes).length,
    };
  }

  // Exposed for tests.
  window.wcrRange = { parseCsv, score, band, gridCentre, coverage, coverageArea, sliceReach, metresBetween };

  const mapEl = document.getElementById("rt-map");
  if (!mapEl || typeof maplibregl === "undefined") return;

  function rasterStyle(id) {
    const spec = STYLES[id] || STYLES.satellite;
    return {
      version: 8,
      sources: { base: { type: "raster", tiles: spec.tiles, tileSize: 256, attribution: spec.attribution } },
      layers: [{ id: "base", type: "raster", source: "base" }],
    };
  }

  let styleId = "satellite";
  try {
    if (STYLES[localStorage.getItem(STYLE_KEY)]) styleId = localStorage.getItem(STYLE_KEY);
  } catch (_) {}
  const styleSel = document.getElementById("rt-style");
  styleSel.value = styleId;

  const coarse = matchMedia("(pointer: coarse)").matches;
  const map = new maplibregl.Map({
    container: mapEl,
    style: rasterStyle(styleId),
    center: [-1.5, 53],
    zoom: 5,
    attributionControl: false,
    cooperativeGestures: coarse,
    dragRotate: false,
  });
  window.wcrRange.map = map;
  map.addControl(new maplibregl.AttributionControl({ compact: false }), "bottom-right");
  map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");

  const summaryEl = document.getElementById("rt-summary");
  const areaBox = document.getElementById("rt-show-area");
  const routeBox = document.getElementById("rt-show-route");
  const homeBtn = document.getElementById("rt-move-home");
  const dotsBox = document.getElementById("rt-show-dots");
  let data = { area: [], route: [], dots: [], home: [], car: [] };
  let loaded = { probes: [], unplaced: 0, far: "", bad: [] };
  let home = null;
  let homeMoved = false;
  let placing = false;

  function addLayers() {
    const fc = (features) => ({ type: "FeatureCollection", features });
    map.addSource("rt-area", { type: "geojson", data: fc(data.area) });
    map.addSource("rt-route", { type: "geojson", data: fc(data.route) });
    map.addSource("rt-dots", { type: "geojson", data: fc(data.dots) });
    map.addSource("rt-home", { type: "geojson", data: fc(data.home) });
    map.addSource("rt-car", { type: "geojson", data: fc(data.car) });
    const bandColor = ["match", ["get", "band"], "good", COLORS.good, "patchy", COLORS.patchy, COLORS.none];
    map.addLayer({
      id: "rt-area",
      type: "fill",
      source: "rt-area",
      paint: { "fill-color": COLORS.good, "fill-opacity": 0.22 },
      layout: { visibility: areaBox.checked ? "visible" : "none" },
    });
    map.addLayer({
      id: "rt-area-edge",
      type: "line",
      source: "rt-area",
      paint: { "line-color": COLORS.good, "line-width": 2 },
      layout: { visibility: areaBox.checked ? "visible" : "none" },
    });
    map.addLayer({
      id: "rt-route",
      type: "fill",
      source: "rt-route",
      paint: { "fill-color": bandColor, "fill-opacity": 0.38, "fill-antialias": false },
      layout: { visibility: routeBox.checked ? "visible" : "none" },
    });
    map.addLayer({
      id: "rt-dots",
      type: "circle",
      source: "rt-dots",
      paint: {
        "circle-radius": ["interpolate", ["linear"], ["zoom"], 8, 2.5, 14, 5],
        "circle-color": bandColor,
        "circle-stroke-color": "#07070a",
        "circle-stroke-width": 1,
      },
      layout: { visibility: dotsBox.checked ? "visible" : "none" },
    });
    map.addLayer({
      id: "rt-home",
      type: "circle",
      source: "rt-home",
      paint: { "circle-radius": 7, "circle-color": "#7d9bff", "circle-stroke-color": "#fff", "circle-stroke-width": 2 },
    });
    map.addLayer({
      id: "rt-car",
      type: "circle",
      source: "rt-car",
      paint: { "circle-radius": 9, "circle-color": "#ffffff", "circle-stroke-color": "#ff7a3d", "circle-stroke-width": 4 },
    });
  }
  map.on("load", addLayers);

  styleSel.addEventListener("change", () => {
    styleId = styleSel.value;
    try {
      localStorage.setItem(STYLE_KEY, styleId);
    } catch (_) {}
    map.setStyle(rasterStyle(styleId));
    map.once("styledata", addLayers);
  });
  areaBox.addEventListener("change", () => {
    for (const id of ["rt-area", "rt-area-edge"]) {
      if (map.getLayer(id)) map.setLayoutProperty(id, "visibility", areaBox.checked ? "visible" : "none");
    }
  });
  routeBox.addEventListener("change", () => {
    if (map.getLayer("rt-route")) map.setLayoutProperty("rt-route", "visibility", routeBox.checked ? "visible" : "none");
  });
  homeBtn.addEventListener("click", () => {
    placing = !placing;
    homeBtn.setAttribute("aria-pressed", String(placing));
    homeBtn.textContent = placing ? "click the map where it is" : "move far station";
    map.getCanvas().style.cursor = placing ? "crosshair" : "";
  });
  map.on("click", (e) => {
    if (!placing) return;
    placing = false;
    homeBtn.setAttribute("aria-pressed", "false");
    homeBtn.textContent = "move far station";
    map.getCanvas().style.cursor = "";
    home = { lat: e.lngLat.lat, lon: e.lngLat.lng };
    homeMoved = true;
    draw(false);
  });
  dotsBox.addEventListener("change", () => {
    if (map.getLayer("rt-dots")) map.setLayoutProperty("rt-dots", "visibility", dotsBox.checked ? "visible" : "none");
  });

  map.on("click", "rt-dots", (e) => {
    if (placing) return;
    const p = e.features[0].properties;
    const bits = [`probe ${p.probe}`, p.time, p.words];
    new maplibregl.Popup({ closeButton: false }).setLngLat(e.lngLat).setText(bits.filter(Boolean).join(" · ")).addTo(map);
  });
  map.on("mouseenter", "rt-dots", () => (map.getCanvas().style.cursor = "pointer"));
  map.on("mouseleave", "rt-dots", () => (map.getCanvas().style.cursor = ""));

  function words(p) {
    const w = { both: "both ways", heard_them: "we heard them, no receipt", none: "no contact" }[p.result] || p.result;
    const parts = [w];
    if (Number.isFinite(p.they)) parts.push(`they heard us at ${p.they} dB`);
    if (Number.isFinite(p.we)) parts.push(`we heard them at ${p.we} dB`);
    return parts.join(", ");
  }

  async function load(files) {
    const probes = [];
    let unplaced = 0;
    let far = "";
    let farGrid = "";
    const bad = [];
    for (const f of files) {
      try {
        const r = parseCsv(await f.text());
        probes.push(...r.probes);
        unplaced += r.unplaced;
        far = r.far || far;
        farGrid = r.farGrid || farGrid;
      } catch (e) {
        bad.push(`${f.name}: ${e.message}`);
      }
    }
    // Oldest first, so a replay drives in order; rows without a time keep their place.
    probes.forEach((p, i) => (p.order = Number.isFinite(p.ts) ? p.ts : i));
    probes.sort((a, b) => a.order - b.order);
    stopReplay();
    loaded = { probes, unplaced, far, bad };
    if (!homeMoved) home = gridCentre(farGrid);
    replayBtn.disabled = !probes.length;
    draw(true);
  }

  function draw(fit) {
    const { unplaced, far, bad } = loaded;
    // During a replay only the probes reached so far are drawn.
    const probes = replay ? loaded.probes.slice(0, replay.shown) : loaded.probes;
    const area = coverageArea(probes, home);
    const car = replay && probes.length ? probes[probes.length - 1] : null;
    data = {
      area: area.features,
      route: coverage(probes),
      car: car ? [{ type: "Feature", properties: {}, geometry: { type: "Point", coordinates: [car.lon, car.lat] } }] : [],
      dots: probes.map((p) => ({
        type: "Feature",
        properties: { band: band(score(p)), probe: p.probe, time: p.time, words: words(p) },
        geometry: { type: "Point", coordinates: [p.lon, p.lat] },
      })),
      home: home ? [{ type: "Feature", properties: {}, geometry: { type: "Point", coordinates: [home.lon, home.lat] } }] : [],
    };
    for (const id of ["rt-area", "rt-route", "rt-dots", "rt-home", "rt-car"]) {
      const src = map.getSource(id);
      if (src) src.setData({ type: "FeatureCollection", features: data[id.slice(3)] });
    }
    const both = probes.filter((p) => p.result === "both");
    let text = `${probes.length} probes on the map`;
    if (probes.length) {
      text += `: ${both.length} both ways (${Math.round((both.length / probes.length) * 100)}%), ${
        probes.filter((p) => p.result === "heard_them").length
      } heard them only, ${probes.filter((p) => p.result === "none").length} no contact`;
    }
    if (home && both.length) {
      const km = Math.max(...both.map((p) => metresBetween(home, p))) / 1000;
      text += `. Furthest both ways from ${far || "the far station"}: ${km.toFixed(1)} km`;
      text += `. Coverage area about ${Math.round(area.km2)} km², measured in ${area.measured} of ${360 / SLICE_DEG} directions`;
    } else if (probes.length && !home) {
      text += ". No receipt carried the far station's grid: use move far station to place it and draw the coverage area";
    }
    text += ".";
    if (unplaced) text += ` ${unplaced} probes had no GPS fix and are not shown.`;
    if (bad.length) text += ` Skipped ${bad.join("; ")}.`;
    summaryEl.textContent = text;
    if (replay) return;
    if (fit && probes.length) {
      const b = new maplibregl.LngLatBounds();
      probes.forEach((p) => b.extend([p.lon, p.lat]));
      if (home) b.extend([home.lon, home.lat]);
      map.fitBounds(b, { padding: 40, maxZoom: 15, duration: 0 });
    }
  }

  // Replay: the drive plays back in time order, faster than real time.
  // Gaps over 2 minutes (a stop, or a break between drives) are skipped.
  const replayBtn = document.getElementById("rt-replay");
  const speedSel = document.getElementById("rt-speed");
  const clockEl = document.getElementById("rt-clock");
  const GAP_S = 120;
  let replay = null;

  function clockText(p, n, total) {
    const t = p && p.time ? p.time.replace("T", " ").replace("Z", " UTC") : "";
    return `${t}${t ? " · " : ""}probe ${n} of ${total}`;
  }

  function setReplayButton(playing) {
    replayBtn.textContent = playing ? "❚❚ pause" : replay ? "▶ resume" : "▶ replay drive";
    replayBtn.setAttribute("aria-pressed", String(playing));
  }

  function stopReplay() {
    if (replay && replay.raf) cancelAnimationFrame(replay.raf);
    replay = null;
    if (clockEl) clockEl.textContent = "";
    if (replayBtn) setReplayButton(false);
  }

  function tick(now) {
    const r = replay;
    if (!r) return;
    const list = loaded.probes;
    const dt = r.last ? (now - r.last) / 1000 : 0;
    r.last = now;
    r.clock += dt * Number(speedSel.value || 500);
    // Jump over long gaps so a stop does not freeze the replay.
    const next = list[r.shown];
    if (next && Number.isFinite(next.order) && next.order - r.clock > GAP_S) r.clock = next.order;
    let moved = false;
    while (r.shown < list.length && !(list[r.shown].order > r.clock)) {
      r.shown++;
      moved = true;
    }
    if (moved && now - r.drawn > 120) {
      r.drawn = now;
      draw(false);
      clockEl.textContent = clockText(list[r.shown - 1], r.shown, list.length);
    }
    if (r.shown >= list.length) {
      clockEl.textContent = clockText(list[list.length - 1], list.length, list.length) + " · done";
      replay = null;
      setReplayButton(false);
      draw(false);
      return;
    }
    r.raf = requestAnimationFrame(tick);
  }

  replayBtn.addEventListener("click", () => {
    if (!loaded.probes.length) return;
    if (replay && replay.raf) {
      // Pause.
      cancelAnimationFrame(replay.raf);
      replay.raf = 0;
      setReplayButton(false);
      return;
    }
    if (!replay) {
      const first = loaded.probes[0];
      replay = { shown: 0, clock: (Number.isFinite(first.order) ? first.order : 0) - 1, last: 0, drawn: 0, raf: 0 };
      draw(false);
    }
    replay.last = 0;
    replay.raf = requestAnimationFrame(tick);
    setReplayButton(true);
  });

  window.wcrRange.replaying = () => (replay ? { shown: replay.shown, total: loaded.probes.length } : null);

  const input = document.getElementById("rt-file");
  const drop = document.getElementById("rt-drop");
  input.addEventListener("change", () => input.files.length && load([...input.files]));
  for (const ev of ["dragenter", "dragover"]) {
    drop.addEventListener(ev, (e) => {
      e.preventDefault();
      drop.classList.add("is-over");
    });
  }
  for (const ev of ["dragleave", "drop"]) {
    drop.addEventListener(ev, () => drop.classList.remove("is-over"));
  }
  drop.addEventListener("drop", (e) => {
    e.preventDefault();
    if (e.dataTransfer.files.length) load([...e.dataTransfer.files]);
  });
  // Dropping anywhere on the map works too.
  mapEl.addEventListener("dragover", (e) => e.preventDefault());
  mapEl.addEventListener("drop", (e) => {
    e.preventDefault();
    if (e.dataTransfer.files.length) load([...e.dataTransfer.files]);
  });
})();
