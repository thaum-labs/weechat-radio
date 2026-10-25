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
  const locateBtn = document.getElementById("band-locate");

  let tx = null;
  let rx = null;
  let txMarker = null;
  let rxMarker = null;
  let abort = null;
  let crawlT = 0;
  let dashTimer = null;
  let pathReady = false;
  let pathFailed = false;
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

  function greatCircle(a, b, n) {
    const toRad = (d) => (d * Math.PI) / 180;
    const toDeg = (r) => (r * 180) / Math.PI;
    const lat1 = toRad(a.lat);
    const lon1 = toRad(a.lon);
    const lat2 = toRad(b.lat);
    const lon2 = toRad(b.lon);
    const d =
      2 *
      Math.asin(
        Math.sqrt(
          Math.sin((lat2 - lat1) / 2) ** 2 +
            Math.cos(lat1) * Math.cos(lat2) * Math.sin((lon2 - lon1) / 2) ** 2
        )
      );
    if (!(d > 1e-9)) return [[a.lon, a.lat], [b.lon, b.lat]];
    const out = [];
    for (let i = 0; i < n; i++) {
      const f = i / (n - 1);
      const A = Math.sin((1 - f) * d) / Math.sin(d);
      const B = Math.sin(f * d) / Math.sin(d);
      const x = A * Math.cos(lat1) * Math.cos(lon1) + B * Math.cos(lat2) * Math.cos(lon2);
      const y = A * Math.cos(lat1) * Math.sin(lon1) + B * Math.cos(lat2) * Math.sin(lon2);
      const z = A * Math.sin(lat1) + B * Math.sin(lat2);
      out.push([toDeg(Math.atan2(y, x)), toDeg(Math.atan2(z, Math.sqrt(x * x + y * y)))]);
    }
    return out;
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
      map.addSource(NIGHT_SRC, { type: "geojson", data: nightGeoJSON(new Date()) });
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
      if (src) src.setData(nightGeoJSON(new Date()));
    } catch (_) {}
  }

  // Radio horizon for a 10 m antenna at each end. VHF uses 4/3-earth
  // refraction; UHF stays closer to the optical horizon.
  const VHF_KM = 4.12 * 2 * Math.sqrt(10);
  const UHF_KM = 3.57 * 2 * Math.sqrt(10);

  function destination(lat, lon, km, bearing) {
    const R = 6371;
    const δ = km / R;
    const φ1 = (lat * Math.PI) / 180;
    const λ1 = (lon * Math.PI) / 180;
    const φ2 = Math.asin(
      Math.sin(φ1) * Math.cos(δ) + Math.cos(φ1) * Math.sin(δ) * Math.cos(bearing)
    );
    const λ2 =
      λ1 +
      Math.atan2(
        Math.sin(bearing) * Math.sin(δ) * Math.cos(φ1),
        Math.cos(δ) - Math.sin(φ1) * Math.sin(φ2)
      );
    return [(λ2 * 180) / Math.PI, (φ2 * 180) / Math.PI];
  }

  function circleRing(center, km) {
    const pts = [];
    for (let i = 0; i <= 72; i++) pts.push(destination(center.lat, center.lon, km, (i / 72) * 2 * Math.PI));
    return pts;
  }

  function ensureLosLayers() {
    if (map.getSource("los")) return;
    map.addSource("los", {
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
    });
    const before = map.getLayer("path-line") ? "path-line" : undefined;
    const add = (layer) => map.addLayer(layer, before);
    add({
      id: "los-vhf",
      type: "fill",
      source: "los",
      filter: ["==", ["get", "band"], "vhf"],
      paint: { "fill-color": "#ffbf00", "fill-opacity": 0.14 },
    });
    add({
      id: "los-vhf-line",
      type: "line",
      source: "los",
      filter: ["==", ["get", "band"], "vhf"],
      paint: { "line-color": "#ffbf00", "line-width": 1.5, "line-dasharray": [2, 1] },
    });
    add({
      id: "los-uhf",
      type: "fill",
      source: "los",
      filter: ["==", ["get", "band"], "uhf"],
      paint: { "fill-color": "#7d9bff", "fill-opacity": 0.2 },
    });
    add({
      id: "los-uhf-line",
      type: "line",
      source: "los",
      filter: ["==", ["get", "band"], "uhf"],
      paint: { "line-color": "#7d9bff", "line-width": 1.5 },
    });
  }

  function paintLos() {
    if (!map.isStyleLoaded()) return;
    ensureLosLayers();
    const features = [];
    if (tx) {
      features.push({
        type: "Feature",
        properties: { band: "vhf" },
        geometry: { type: "Polygon", coordinates: [circleRing(tx, VHF_KM)] },
      });
      features.push({
        type: "Feature",
        properties: { band: "uhf" },
        geometry: { type: "Polygon", coordinates: [circleRing(tx, UHF_KM)] },
      });
    }
    map.getSource("los").setData({ type: "FeatureCollection", features });
  }

  function frameLos() {
    if (!tx) return;
    const b = new maplibregl.LngLatBounds();
    circleRing(tx, VHF_KM).forEach((c) => b.extend(c));
    map.fitBounds(b, { padding: 36, duration: reduceMotion ? 0 : 500 });
  }

  function ensurePathLayer() {
    if (map.getSource("path")) return;
    map.addSource("path", {
      type: "geojson",
      data: { type: "FeatureCollection", features: [] },
    });
    map.addLayer({
      id: "path-line",
      type: "line",
      source: "path",
      paint: {
        "line-color": "#39ff14",
        "line-width": 2.5,
        "line-dasharray": [2, 1.33],
        "line-opacity": 0.95,
      },
      layout: {
        "line-cap": "round",
        "line-join": "round",
      },
    });
  }

  function paintPath() {
    if (!tx || !rx || !map.isStyleLoaded()) return;
    ensurePathLayer();
    const all = greatCircle(tx, rx, 128);
    let coords = all;
    if (!pathReady && !pathFailed && !reduceMotion) {
      const n = Math.max(2, Math.floor(all.length * Math.min(1, crawlT)));
      coords = all.slice(0, n);
    }
    const features =
      coords.length >= 2
        ? [{ type: "Feature", geometry: { type: "LineString", coordinates: coords } }]
        : [];
    map.getSource("path").setData({ type: "FeatureCollection", features });
    const color = pathFailed ? "#7a7388" : "#39ff14";
    map.setPaintProperty("path-line", "line-color", color);
    // RF dash [6,4] ≈ 1.5 : 1; solid when ready; dim dashes on failure.
    if (pathFailed) map.setPaintProperty("path-line", "line-dasharray", [1.5, 1.5]);
    else if (pathReady) map.setPaintProperty("path-line", "line-dasharray", [1, 0]);
    else map.setPaintProperty("path-line", "line-dasharray", [1.5, 1]);
  }

  function startCrawl() {
    stopCrawl();
    crawlT = reduceMotion ? 1 : 0;
    paintPath();
    if (reduceMotion) return;
    dashTimer = setInterval(() => {
      // Grow to full, then keep a soft pulse by looping growth slightly past 1.
      crawlT += 0.035;
      if (crawlT > 1.15) crawlT = 0.55;
      paintPath();
    }, 50);
  }

  function stopCrawl() {
    if (dashTimer) {
      clearInterval(dashTimer);
      dashTimer = null;
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
      paintLos();
      if (tx && rx) requestCard();
      else paintPath();
    });
    return marker;
  }

  function setTx(ll) {
    tx = { lat: ll.lat, lon: ll.lng };
    if (txMarker) txMarker.remove();
    txMarker = placeMarker("tx", ll);
    paintLos();
    if (!rx) {
      setHint("RX", "Click the other station");
      frameLos();
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

  async function requestCard() {
    if (!tx || !rx) return;
    pathReady = false;
    pathFailed = false;
    if (abort) abort.abort();
    abort = new AbortController();
    if (emptyEl) emptyEl.hidden = true;
    waitEl.hidden = false;
    cardEl.classList.remove("has-answer");
    setHint("…", "Working out the path");
    if (txMarker) pulse(txMarker.getElement(), "tx");
    paintPath();
    startCrawl();
    fitPins();
    try {
      const res = await fetch(`${PATH_API}/api/path`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          tx_lat: tx.lat,
          tx_lon: tx.lon,
          rx_lat: rx.lat,
          rx_lon: rx.lon,
        }),
        signal: abort.signal,
      });
      if (!res.ok) throw new Error(await res.text());
      const card = await res.json();
      pathReady = true;
      pathFailed = false;
      stopCrawl();
      paintPath();
      waitEl.hidden = true;
      if (emptyEl) emptyEl.hidden = true;
      renderCard(card);
      if (rxMarker) pulse(rxMarker.getElement(), "rx");
      setHint("OK", card.confidence || "Ready");
    } catch (e) {
      if (e.name === "AbortError") return;
      pathFailed = true;
      stopCrawl();
      paintPath();
      waitEl.hidden = true;
      if (emptyEl) emptyEl.hidden = true;
      cardEl.classList.add("has-answer");
      const answer = cardEl.querySelector(".band-answer");
      if (answer) answer.remove();
      cardEl.insertAdjacentHTML(
        "beforeend",
        `<div class="band-answer"><p class="band-err">${escapeHtml(e.message || "request failed")}</p></div>`
      );
      setHint("ERR", "Try again");
    }
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
    paintLos();
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

  let searchTimer = null;
  searchEl.addEventListener("input", () => {
    clearTimeout(searchTimer);
    const q = searchEl.value.trim();
    if (q.length < 2) return;
    searchTimer = setTimeout(async () => {
      try {
        const res = await fetch(
          `${PATH_API}/api/path/places?q=${encodeURIComponent(q)}`
        );
        if (!res.ok) return;
        const data = await res.json();
        const p = (data.places || [])[0];
        if (!p) return;
        map.flyTo({
          center: [p.lon, p.lat],
          zoom: 6,
          duration: reduceMotion ? 0 : 700,
        });
      } catch (_) {}
    }, 350);
  });

  searchEl.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      searchEl.dispatchEvent(new Event("input"));
    }
  });

  setHint("TX", "Click where you are");

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
    }),
  };
})();
