/* SPDX-License-Identifier: Apache-2.0 */
(function () {
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const host = location.hostname;
  const local = host === "localhost" || host === "127.0.0.1";
  const PATH_API = local ? "http://127.0.0.1:7380" : "";

  const CARTO_KEY = "cb1_3nxz_1_dba340f0a1c8450af09da179";
  const voyager = {
    version: 8,
    sources: {
      osm: {
        type: "raster",
        tiles: [
          `https://basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}@2x.png?key=${CARTO_KEY}`,
        ],
        tileSize: 256,
        attribution:
          '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>, &copy; <a href="https://carto.com/attributions">CARTO</a>',
      },
    },
    layers: [{ id: "osm", type: "raster", source: "osm" }],
  };

  const coarse = matchMedia("(pointer: coarse)").matches;
  const map = new maplibregl.Map({
    container: "map",
    style: voyager,
    center: [0, 20],
    zoom: 1.6,
    attributionControl: false,
    cooperativeGestures: coarse,
  });
  map.addControl(new maplibregl.AttributionControl({ compact: false }), "bottom-right");
  map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");

  const hintEl = document.getElementById("band-hint");
  const stepEl = document.getElementById("band-step");
  const waitEl = document.getElementById("band-wait");
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

  function setHint(step, text) {
    if (stepEl) stepEl.textContent = step;
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
      if (tx && rx) requestCard();
      else paintPath();
    });
    return marker;
  }

  function setTx(ll) {
    tx = { lat: ll.lat, lon: ll.lng };
    if (txMarker) txMarker.remove();
    txMarker = placeMarker("tx", ll);
    if (!rx) setHint("RX", "Click the other station");
    else requestCard();
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
    cardEl.hidden = true;
    waitEl.hidden = false;
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
      renderCard(card);
      if (rxMarker) pulse(rxMarker.getElement(), "rx");
      setHint("OK", card.confidence || "Ready");
    } catch (e) {
      if (e.name === "AbortError") return;
      pathFailed = true;
      stopCrawl();
      paintPath();
      waitEl.hidden = true;
      cardEl.hidden = false;
      cardEl.innerHTML = `<h3 class="title"><p>PATH</p><p>ERR</p></h3><p class="band-err">${escapeHtml(
        e.message || "request failed"
      )}</p>`;
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

  function renderCard(c) {
    const power =
      c.power_w != null ? `About ${c.power_w} W` : "Over 100 W — wait for a better hour";
    const ssb = c.ssb ? "SSB on" : "SSB off";
    const voice = c.voice_note
      ? `<p class="band-voice">${escapeHtml(c.voice_note)}</p>`
      : "";
    cardEl.hidden = false;
    cardEl.classList.remove("activated");
    void cardEl.offsetWidth;
    cardEl.classList.add("activated");
    cardEl.innerHTML = `
      <h3 class="title"><p>${escapeHtml(c.frequency_mhz)} MHz</p><p>${escapeHtml(c.band)}</p></h3>
      <div class="kv">
        <span>ssb</span><b>${ssb}</b>
        <span>mode</span><b>${escapeHtml(c.mode)}</b>
        <span>power</span><b>${escapeHtml(power)}</b>
        <span>preset</span><b>${escapeHtml(c.preset)}</b>
        <span>path</span><b>${c.distance_km} km</b>
      </div>
      <p class="band-conf">${escapeHtml(c.confidence || "")}</p>
      ${voice}
      <p class="band-note">${escapeHtml(c.note || "")}</p>
      <pre class="plain">${escapeHtml(c.qsy)}\n${escapeHtml(c.preset_cmd)}</pre>
      <p class="band-foot">${escapeHtml(c.footnote || "")}</p>
    `;
    if (window.WCR && WCR.wrapPres) WCR.wrapPres();
  }

  map.on("load", () => {
    ensurePathLayer();
    try {
      map.resize();
    } catch (_) {}
  });
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
