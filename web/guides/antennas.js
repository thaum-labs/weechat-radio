/* SPDX-License-Identifier: Apache-2.0 */
// Antenna guide pages: animated diagram cards and a calculator.
// <body data-antennas="hf"> or "vhf" picks the set. Lengths are physical
// wire lengths (about 95% of the free-space figure); they are a starting
// point to trim, not a promise.
(function () {
  const set = document.body.dataset.antennas === "vhf" ? "vhf" : "hf";
  const cardsEl = document.getElementById("ant-cards");
  const calcEl = document.getElementById("ant-calc");
  if (!cardsEl || !calcEl) return;

  const HF_BANDS = [
    ["160m", 1.9],
    ["80m", 3.6],
    ["40m", 7.1],
    ["30m", 10.13],
    ["20m", 14.2],
    ["17m", 18.1],
    ["15m", 21.2],
    ["12m", 24.9],
    ["10m", 28.5],
  ];
  const VHF_BANDS = [
    ["6m", 51],
    ["2m", 145],
    ["70cm", 435],
  ];
  const BANDS = set === "hf" ? HF_BANDS : VHF_BANDS;

  const lam = (mhz) => 300 / mhz;
  const near = (a, b, tol) => Math.abs(a - b) / b <= tol;
  const m = (v) => (v >= 10 ? v.toFixed(1) : v.toFixed(2)) + " m";
  const cm = (v) => Math.round(v * 100) + " cm";
  const len = (v) => (v < 1 ? cm(v) : m(v));
  const bandOf = (mhz) => {
    let best = BANDS[0];
    for (const b of BANDS) if (Math.abs(Math.log(b[1] / mhz)) < Math.abs(Math.log(best[1] / mhz))) best = b;
    return best[0];
  };
  const bandFreq = (name) => (BANDS.find((b) => b[0] === name) || BANDS[0])[1];
  const goodList = (rows) => rows.filter((r) => r.v === "good").map((r) => r.band);
  const range = (names) => (names.length > 1 ? `${names[0]}-${names[names.length - 1]}` : names[0] || "");

  // Take-off angle from height in wavelengths, for horizontal HF wires.
  function angleNote(h, mhz) {
    const x = h / lam(mhz);
    if (x < 0.2) return `${x.toFixed(2)} λ up: high angle, best within about 600 km (NVIS)`;
    if (x < 0.45) return `${x.toFixed(2)} λ up: nearby and medium distance`;
    return `${x.toFixed(2)} λ up: low angle, good for DX`;
  }
  // VHF/UHF radio horizon (4/3 earth) from height in metres.
  function horizon(h) {
    const toHandheld = 4.12 * (Math.sqrt(h) + Math.sqrt(1.5));
    const toSame = 8.24 * Math.sqrt(h);
    return `about ${Math.round(toHandheld)} km to a handheld, ${Math.round(toSame)} km to a station as high as you`;
  }

  // Each diagram: viewBox 0 0 220 130, ground at y 118. `el` are the
  // antenna parts (current flows along them), `wave` is where the waves
  // start, `dir` makes the waves one-sided for a beam.
  const GROUND = '<path class="ant-ground" d="M0 118H220"/>';
  const DIAGRAMS = {
    dipole: {
      parts: '<path class="ant-mast" d="M20 118V38M200 118V38M110 44V118"/>',
      el: "M20 40 Q65 45 110 44 Q155 45 200 40",
      wave: [110, 44],
      label: [110, 30, "L"],
    },
    efhw: {
      parts: '<rect class="ant-box" x="14" y="104" width="12" height="10"/><path class="ant-mast" d="M200 118V28"/>',
      el: "M26 106 L200 30",
      wave: [113, 68],
      label: [120, 56, "L"],
    },
    endfed: {
      parts:
        '<rect class="ant-box" x="14" y="104" width="12" height="10"/><path class="ant-mast" d="M120 118V38M200 118V38"/><path class="ant-cp" d="M26 115H96"/>',
      el: "M26 106 L120 40 H200",
      wave: [120, 40],
      label: [150, 30, "L"],
    },
    vertical: {
      parts: '<path class="ant-radial" d="M110 116 L40 126M110 116 L180 126M110 116 L80 128M110 116 L140 128"/>',
      el: "M110 116 V20",
      wave: [110, 68],
      label: [124, 60, "L"],
    },
    loop: {
      parts: '<circle class="ant-mast" cx="110" cy="86" r="7"/><path class="ant-mast" d="M110 93V118"/>',
      el: "M110 24 A30 30 0 1 1 109.9 24",
      wave: [110, 54],
      label: [150, 30, "D"],
    },
    beam: {
      parts: '<path class="ant-mast" d="M60 50H170M110 50V118"/>',
      el: "M62 26V74M100 29V71M146 33V67",
      wave: [146, 50],
      dir: true,
      label: [104, 22, "λ/2"],
    },
    whip: {
      parts:
        '<path class="ant-box" d="M50 112V98L70 88H140L160 98H172V112Z"/><circle class="ant-mast" cx="76" cy="113" r="6"/><circle class="ant-mast" cx="150" cy="113" r="6"/><rect class="ant-box" x="136" y="72" width="8" height="12"/>',
      el: "M140 88 V22",
      wave: [140, 52],
      label: [154, 40, "L"],
    },
    duck: {
      parts: '<rect class="ant-box" x="96" y="70" width="28" height="46" rx="3"/><path class="ant-mast" d="M102 80H118M102 88H118"/>',
      el: "M110 70 V28",
      wave: [110, 44],
      label: [124, 44, "L"],
    },
    groundplane: {
      parts: '<path class="ant-mast" d="M110 76V118"/><path class="ant-radial" d="M110 76 L72 96M110 76 L148 96M110 76 L92 100M110 76 L128 100"/>',
      el: "M110 76 V26",
      wave: [110, 50],
      label: [124, 46, "λ/4"],
    },
    jpole: {
      parts: '<path class="ant-mast" d="M110 100V118"/>',
      el: "M110 100 V14 M98 100 V68 M98 100 H110",
      wave: [110, 50],
      label: [124, 40, "λ/2"],
    },
    collinear: {
      parts: '<path class="ant-mast" d="M110 98V118"/><rect class="ant-box" x="106" y="44" width="8" height="5"/><rect class="ant-box" x="106" y="70" width="8" height="5"/>',
      el: "M110 98 V8",
      wave: [110, 50],
      label: [124, 30, "L"],
    },
    yagi: {
      parts: '<path class="ant-mast" d="M50 50H180M110 50V118"/>',
      el: "M54 30V70M84 31V69M114 33V67M144 34V66M174 35V65",
      wave: [174, 50],
      dir: true,
      label: [88, 24, "λ/2"],
    },
  };

  function diagram(id, labelText) {
    const d = DIAGRAMS[id];
    const waves = [0, 1, 2]
      .map((i) =>
        d.dir
          ? `<path class="ant-wave is-dir" style="--i:${i}" d="M${d.wave[0]} ${d.wave[1] - 40} A40 40 0 0 1 ${d.wave[0]} ${d.wave[1] + 40}"/>`
          : `<circle class="ant-wave" style="--i:${i}" cx="${d.wave[0]}" cy="${d.wave[1]}" r="60"/>`
      )
      .join("");
    const label = d.label
      ? `<text class="ant-label" x="${d.label[0]}" y="${d.label[1]}">${labelText || d.label[2]}</text>`
      : "";
    return `<svg class="ant-svg" viewBox="0 0 220 130" role="img" aria-label="${escapeHtml(TYPES[id].name)} diagram">
      ${waves}${GROUND}${d.parts}
      <path class="ant-el" d="${d.el}"/><path class="ant-current" d="${d.el}"/>${label}</svg>`;
  }

  // Band rows: v is "good", "ok" (works with a tuner, or weaker) or "no".
  const TYPES = {
    dipole: {
      name: "Dipole",
      blurb: "Two equal wires fed in the middle with coax. The reference every other HF antenna is compared with.",
      good: "One band (and the band at three times its frequency), a little gain, quiet on receive.",
      watch: "Needs two supports. Off its band it needs a tuner and ladder line.",
      vars: [
        { key: "L", label: "Total length", min: 3, max: 85, step: 0.1, value: 20.1, unit: "m" },
        { key: "h", label: "Height", min: 2, max: 30, step: 0.5, value: 8, unit: "m" },
      ],
      cut: (f) => ({ L: 142.5 / f }),
      calc({ L, h }) {
        const f0 = 142.5 / L;
        const rows = BANDS.map(([band, f]) => {
          if (near(f, f0, 0.04) || near(f, 3 * f0, 0.04)) return { band, v: "good", note: angleNote(h, f) };
          if (f > f0 * 0.9) return { band, v: "ok", note: "with a tuner and ladder line" };
          return { band, v: "no", note: "too short" };
        });
        const main = bandOf(f0);
        return {
          facts: [
            ["Resonant at", `${f0.toFixed(2)} MHz (${main})`],
            ["Each leg", m(L / 2)],
            ["Feed", "1:1 choke balun, coax to the radio"],
          ],
          rows,
          search: `${main} half wave dipole kit 1:1 balun`,
        };
      },
    },
    efhw: {
      name: "End-fed half-wave",
      blurb: "A half-wave wire fed at one end through a 49:1 transformer. One wire, one support, several bands.",
      good: "Its band and the even harmonics: a 40m EFHW also covers 20m, 15m and 10m without a tuner.",
      watch: "Keep the coax away from the wire; a choke at the radio end stops RF in the shack.",
      vars: [
        { key: "L", label: "Wire length", min: 5, max: 85, step: 0.1, value: 20.1, unit: "m" },
        { key: "h", label: "Height (far end)", min: 2, max: 30, step: 0.5, value: 8, unit: "m" },
      ],
      cut: (f) => ({ L: 142.5 / f }),
      calc({ L, h }) {
        const f0 = 142.5 / L;
        const rows = BANDS.map(([band, f]) => {
          for (let n = 1; n <= 8; n++) if (near(f, n * f0, 0.04)) return { band, v: "good", note: angleNote(h / 2, f) };
          if (f > f0 * 0.95) return { band, v: "ok", note: "with a tuner" };
          return { band, v: "no", note: "too short" };
        });
        return {
          facts: [
            ["Lowest band", `${bandOf(f0)} (${f0.toFixed(2)} MHz)`],
            ["Transformer", "49:1 (64:1 for some designs)"],
            ["Choke", "Common-mode choke on the coax near the radio"],
          ],
          rows,
          search: `EFHW end fed half wave ${range(goodList(rows))} 49:1`,
        };
      },
    },
    endfed: {
      name: "End-fed wire (9:1)",
      blurb: "A random-length wire on a 9:1 unun, matched by a tuner. Easy to put up from a window or a tree.",
      good: "Many bands with a tuner, once the wire is a quarter wave or more on the band.",
      watch: "Needs a tuner and a counterpoise wire. Avoid lengths near a half wave on a band you use.",
      vars: [
        { key: "L", label: "Wire length", min: 4, max: 45, step: 0.1, value: 10, unit: "m" },
        { key: "h", label: "Height (far end)", min: 2, max: 30, step: 0.5, value: 6, unit: "m" },
      ],
      calc({ L, h }) {
        const tried = [8.8, 10.8, 12.5, 17.7, 21.6, 25.6, 32.6, 36.3, 45.1];
        const best = tried.reduce((a, b) => (Math.abs(b - L) < Math.abs(a - L) ? b : a));
        const rows = BANDS.map(([band, f]) => {
          const q = L / (lam(f) * 0.95);
          const half = Math.round(q * 2) / 2;
          if (q >= 0.24 && half > 0 && Math.abs(q - half) < 0.04)
            return { band, v: "ok", note: "near a half wave: the tuner may struggle" };
          if (q >= 0.24) return { band, v: "good", note: angleNote(h / 2, f) };
          if (q >= 0.125) return { band, v: "ok", note: "short for this band: works, but weaker" };
          return { band, v: "no", note: "too short" };
        });
        return {
          facts: [
            ["Nearest tried length", m(best)],
            ["Counterpoise", "A 5-8 m wire on the ground side of the unun (or the coax shield)"],
            ["Tuner", "Yes: at the radio or a remote tuner at the unun"],
          ],
          rows,
          search: `9:1 unun end fed random wire antenna ${Math.round(L)}m`,
        };
      },
    },
    vertical: {
      name: "Vertical",
      blurb: "A quarter-wave upright element over radial wires. Small footprint, low radiation angle.",
      good: "Long distance (DX) on its band and three times its frequency.",
      watch: "Needs radials: 2-4 tuned ones if raised, 16 or more if on the ground. Picks up more local noise.",
      vars: [
        { key: "L", label: "Element length", min: 1.5, max: 25, step: 0.05, value: 5, unit: "m" },
        { key: "r", label: "Radials", min: 2, max: 32, step: 1, value: 4, unit: "" },
      ],
      cut: (f) => ({ L: 71.25 / f }),
      calc({ L, r }) {
        const f0 = 71.25 / L;
        const rows = BANDS.map(([band, f]) => {
          if (near(f, f0, 0.04) || near(f, 3 * f0, 0.04)) return { band, v: "good", note: "low angle, good for DX" };
          if (f > f0) return { band, v: "ok", note: "with a tuner at the base" };
          return { band, v: "no", note: "too short without a loading coil" };
        });
        return {
          facts: [
            ["Resonant at", `${f0.toFixed(2)} MHz (${bandOf(f0)})`],
            ["Radials", `${r} × ${m(L)}: ${r >= 16 ? "fine on the ground" : r >= 2 ? "raise them off the ground" : "add more"}`],
            ["Feed", "Coax at the base, choke balun"],
          ],
          rows,
          search: `${bandOf(f0)} quarter wave vertical antenna radials`,
        };
      },
    },
    loop: {
      name: "Magnetic loop",
      blurb: "A small metal loop tuned by a capacitor. Fits on a balcony and hears well in noisy towns.",
      good: "Quiet receive, small spaces, several bands by retuning.",
      watch: "Very narrow: retune every time you change frequency. Less efficient on the lowest bands.",
      vars: [{ key: "D", label: "Loop diameter", min: 0.5, max: 3, step: 0.05, value: 1, unit: "m" }],
      calc({ D }) {
        const C = Math.PI * D;
        const rows = BANDS.map(([band, f]) => {
          if (f > 75 / C) return { band, v: "no", note: "loop too big for this band" };
          if (f >= 30 / C) return { band, v: "good", note: "efficient" };
          if (f >= 15 / C) return { band, v: "ok", note: "works, lower efficiency" };
          return { band, v: "no", note: "loop too small" };
        });
        return {
          facts: [
            ["Circumference", m(C)],
            ["Range", `${(15 / C).toFixed(1)}-${(75 / C).toFixed(1)} MHz`],
            ["Power", "Check the capacitor's rating; high voltage across it"],
          ],
          rows,
          search: `magnetic loop antenna ${D} m HF ${range(rows.filter((x) => x.v !== "no").map((x) => x.band))}`,
        };
      },
    },
    beam: {
      name: "Beam (Yagi)",
      blurb: "Three or more elements on a boom, turned with a rotator. Pushes power one way.",
      good: "About +6 dB in the direction it points, less noise from behind. Usually 20m and up.",
      watch: "Big, heavy, needs a tower and a rotator.",
      vars: [
        { key: "f", label: "Frequency", min: 14, max: 29.7, step: 0.01, value: 14.2, unit: "MHz" },
        { key: "h", label: "Height", min: 5, max: 30, step: 0.5, value: 12, unit: "m" },
      ],
      cut: (f) => ({ f: Math.max(14, f) }),
      calc({ f, h }) {
        const main = bandOf(f);
        const rows = BANDS.map(([band, bf]) =>
          band === main ? { band, v: "good", note: `+6 dB one way, ${angleNote(h, bf)}` } : { band, v: "no", note: "a different beam" }
        );
        return {
          facts: [
            ["Reflector", m(151.5 / f)],
            ["Driven", m(144.2 / f)],
            ["Director", m(134.1 / f)],
            ["Spacing", `${m(45 / f)} between elements`],
          ],
          rows,
          search: `3 element yagi ${main} beam antenna`,
        };
      },
    },
    whip: {
      name: "Mobile whip",
      blurb: "A short vertical on a car or a backpack, with a loading coil or an auto-tuner at the base.",
      good: "Operating on the move.",
      watch: "Much shorter than a quarter wave on the low bands: expect about 12 dB less than a dipole.",
      vars: [{ key: "L", label: "Whip length", min: 1, max: 3.5, step: 0.05, value: 2.4, unit: "m" }],
      calc({ L }) {
        const f0 = 71.25 / L;
        const rows = BANDS.map(([band, f]) => {
          if (near(f, f0, 0.06)) return { band, v: "good", note: "a full quarter wave" };
          if (f > f0) return { band, v: "ok", note: "with a tuner" };
          if (f >= f0 / 6) return { band, v: "ok", note: "with a loading coil: weak" };
          return { band, v: "no", note: "too short" };
        });
        return {
          facts: [
            ["Quarter wave at", `${f0.toFixed(1)} MHz (${bandOf(f0)})`],
            ["Mount", "A good ground to the car body, short coax"],
          ],
          rows,
          search: `HF mobile whip antenna ${range(rows.filter((x) => x.v !== "no").map((x) => x.band))} screwdriver`,
        };
      },
    },

    duck: {
      name: "Handheld whip",
      blurb: "The antenna on the radio. The stock one is short and coiled inside; a longer whip helps a lot.",
      good: "Carrying the radio. A 40-50 cm whip is a quarter wave on 2m and works on 70cm too.",
      watch: "Your body blocks part of the signal; hold the radio upright, away from your head.",
      vars: [{ key: "L", label: "Whip length", min: 0.05, max: 0.8, step: 0.01, value: 0.15, unit: "m" }],
      calc({ L }) {
        const rows = BANDS.map(([band, f]) => {
          const q = L / (lam(f) * 0.95);
          if ((q >= 0.21 && q <= 0.3) || (q >= 0.6 && q <= 0.8)) return { band, v: "good", note: "full size: works well" };
          if (q >= 0.05) return { band, v: "ok", note: "loaded (coiled): about 3-6 dB weaker" };
          return { band, v: "no", note: "too short for this band" };
        });
        return {
          facts: [
            ["Quarter wave on 2m", cm(71.25 / 145)],
            ["Quarter wave on 70cm", cm(71.25 / 435)],
            ["Connector", "Check your radio: SMA male or SMA female"],
          ],
          rows,
          search: `dual band 2m 70cm handheld whip ${Math.round(L * 100)} cm SMA`,
        };
      },
    },
    groundplane: {
      name: "Ground plane",
      blurb: "A quarter-wave rod with 3-4 sloping radials. The simplest good base antenna.",
      good: "All directions, cheap to build from a coax socket and some wire.",
      watch: "Get it high and clear: height matters more than gain on VHF.",
      vars: [
        { key: "f", label: "Frequency", min: 50, max: 450, step: 0.5, value: 145, unit: "MHz" },
        { key: "h", label: "Height", min: 1, max: 30, step: 0.5, value: 8, unit: "m" },
      ],
      cut: (f) => ({ f }),
      calc({ f, h }) {
        const main = bandOf(f);
        const rows = BANDS.map(([band]) => (band === main ? { band, v: "good", note: horizon(h) } : { band, v: "no", note: "cut for a different band" }));
        return {
          facts: [
            ["Radiator", len(71.25 / f)],
            ["Radials", `4 × ${len(75 / f)}, sloping down 45°`],
            ["Gain", "About 2 dBi"],
          ],
          rows,
          search: `${main} quarter wave ground plane antenna`,
        };
      },
    },
    jpole: {
      name: "J-pole / slim jim",
      blurb: "A half-wave radiator fed by a quarter-wave stub. No radials, easy to build from tube or ladder line.",
      good: "A step up from a ground plane, mounts on a simple pole.",
      watch: "Move the feed tap up or down to tune it; keep the coax at right angles below it.",
      vars: [
        { key: "f", label: "Frequency", min: 50, max: 450, step: 0.5, value: 145, unit: "MHz" },
        { key: "h", label: "Height", min: 1, max: 30, step: 0.5, value: 8, unit: "m" },
      ],
      cut: (f) => ({ f }),
      calc({ f, h }) {
        const main = bandOf(f);
        const rows = BANDS.map(([band]) => (band === main ? { band, v: "good", note: horizon(h) } : { band, v: "no", note: "cut for a different band" }));
        return {
          facts: [
            ["Long element", len(213.75 / f)],
            ["Stub", len(71.25 / f)],
            ["Feed tap", `about ${len(15 / f)} up from the bottom`],
            ["Gain", "About 2-3 dBi"],
          ],
          rows,
          search: `${main} J-pole slim jim antenna`,
        };
      },
    },
    collinear: {
      name: "Collinear",
      blurb: "Stacked sections in a white fibreglass tube. The usual base antenna for 2m and 70cm.",
      good: "More range in all directions; the longer it is, the more it squeezes the signal towards the horizon.",
      watch: "Long ones are heavy on a thin pole. Too much gain can miss stations close by on hills.",
      vars: [
        { key: "L", label: "Length", min: 1, max: 6, step: 0.1, value: 1.7, unit: "m" },
        { key: "h", label: "Height", min: 1, max: 30, step: 0.5, value: 8, unit: "m" },
      ],
      calc({ L, h }) {
        const gain = (f) => 2.15 + 10 * Math.log10(Math.max(1, L / (0.625 * lam(f))));
        const rows = BANDS.map(([band, f]) =>
          band === "6m" ? { band, v: "no", note: "needs its own antenna" } : { band, v: "good", note: `about ${gain(f).toFixed(1)} dBi, ${horizon(h)}` }
        );
        return {
          facts: [
            ["Gain on 2m", `about ${gain(145).toFixed(1)} dBi`],
            ["Gain on 70cm", `about ${gain(435).toFixed(1)} dBi`],
            ["Feed", "Good low-loss coax (the higher the mast, the more it matters)"],
          ],
          rows,
          search: `dual band 2m 70cm collinear base antenna ${L} m`,
        };
      },
    },
    yagi: {
      name: "Yagi",
      blurb: "A beam for VHF/UHF: point it at the other station. Small enough to hand-hold for portable work.",
      good: "Long or blocked paths, satellites, reaching a far repeater or gateway.",
      watch: "Match the other station: vertical for FM, horizontal for SSB.",
      vars: [
        { key: "f", label: "Frequency", min: 50, max: 450, step: 0.5, value: 145, unit: "MHz" },
        { key: "n", label: "Elements", min: 3, max: 11, step: 1, value: 5, unit: "" },
      ],
      cut: (f) => ({ f }),
      calc({ f, n }) {
        const main = bandOf(f);
        const gain = 2.15 + 5.7 + 10 * Math.log10(n / 3);
        const rows = BANDS.map(([band]) => (band === main ? { band, v: "good", note: `about ${gain.toFixed(1)} dBi one way` } : { band, v: "no", note: "a different Yagi" }));
        return {
          facts: [
            ["Reflector", len(148.5 / f)],
            ["Driven", len(141.9 / f)],
            ["Directors", len(132 / f)],
            ["Boom", len(((n - 1) * 0.2 * 300) / f)],
          ],
          rows,
          search: `${n} element yagi ${main} antenna`,
        };
      },
    },
  };

  const ORDER = set === "hf" ? ["dipole", "efhw", "endfed", "vertical", "loop", "beam", "whip"] : ["duck", "groundplane", "jpole", "collinear", "yagi"];

  function escapeHtml(s) {
    return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }

  cardsEl.innerHTML = ORDER.map((id) => {
    const t = TYPES[id];
    return `<li class="ant-card" id="ant-${id}">
      ${diagram(id)}
      <h3>${escapeHtml(t.name)}</h3>
      <p>${escapeHtml(t.blurb)}</p>
      <p><b>Good for:</b> ${escapeHtml(t.good)}</p>
      <p><b>Watch out:</b> ${escapeHtml(t.watch)}</p>
      <button type="button" class="ant-use" data-type="${id}">size it ↓</button>
    </li>`;
  }).join("");

  // Calculator: type tiles, the diagram, sliders, "cut for" band chips, results.
  let type = ORDER[0];
  let values = {};

  calcEl.innerHTML = `
    <div class="ant-types" role="radiogroup" aria-label="Antenna type">
      ${ORDER.map((id) => `<label><input type="radio" name="ant-type" value="${id}"${id === type ? " checked" : ""} /><span>${escapeHtml(TYPES[id].name)}</span></label>`).join("")}
    </div>
    <div class="ant-calc-body">
      <div class="ant-calc-pic" id="ant-calc-pic"></div>
      <div class="ant-calc-ctl">
        <div id="ant-vars"></div>
        <div class="ant-cut" id="ant-cut"></div>
      </div>
    </div>
    <dl class="ant-facts" id="ant-facts"></dl>
    <ul class="ant-bands" id="ant-bands"></ul>
    <div class="ant-search">
      <span>Search for</span>
      <code id="ant-term"></code>
      <button type="button" class="copy-btn" id="ant-copy">copy</button>
      <a id="ant-go" target="_blank" rel="noopener">search the web ↗</a>
    </div>`;

  const varsEl = document.getElementById("ant-vars");
  const cutEl = document.getElementById("ant-cut");

  function fmt(v, unit, step) {
    const d = step < 0.1 ? 2 : step < 1 ? 1 : 0;
    return `${Number(v).toFixed(d)}${unit ? " " + unit : ""}`;
  }

  function buildControls() {
    const t = TYPES[type];
    values = Object.fromEntries(t.vars.map((v) => [v.key, v.value]));
    varsEl.innerHTML = t.vars
      .map(
        (v) => `<label class="ant-var"><span>${escapeHtml(v.label)}</span>
          <input type="range" data-key="${v.key}" min="${v.min}" max="${v.max}" step="${v.step}" value="${v.value}" aria-label="${escapeHtml(v.label)}" />
          <output data-for="${v.key}">${fmt(v.value, v.unit, v.step)}</output></label>`
      )
      .join("");
    cutEl.innerHTML = t.cut
      ? `<span>Cut for</span>${BANDS.map(([b]) => `<button type="button" data-band="${b}">${b}</button>`).join("")}`
      : "";
    update();
  }

  function update() {
    const t = TYPES[type];
    const r = t.calc(values);
    const first = t.vars[0];
    document.getElementById("ant-calc-pic").innerHTML = diagram(type, fmt(values[first.key], first.unit, first.step));
    for (const v of t.vars) {
      const out = varsEl.querySelector(`output[data-for="${v.key}"]`);
      if (out) out.textContent = fmt(values[v.key], v.unit, v.step);
    }
    document.getElementById("ant-facts").innerHTML = r.facts
      .map(([k, v]) => `<div><dt>${escapeHtml(k)}</dt><dd>${escapeHtml(v)}</dd></div>`)
      .join("");
    document.getElementById("ant-bands").innerHTML = r.rows
      .map((row) => `<li class="is-${row.v}"><b>${row.band}</b><span>${row.v === "good" ? "good" : row.v === "ok" ? "works" : "no"}</span><small>${escapeHtml(row.note)}</small></li>`)
      .join("");
    document.getElementById("ant-term").textContent = r.search;
    document.getElementById("ant-go").href = `https://www.google.com/search?q=${encodeURIComponent(r.search)}`;
  }

  calcEl.addEventListener("input", (e) => {
    const key = e.target.dataset && e.target.dataset.key;
    if (key) {
      values[key] = Number(e.target.value);
      update();
    }
  });
  calcEl.addEventListener("change", (e) => {
    if (e.target.name === "ant-type") {
      type = e.target.value;
      buildControls();
    }
  });
  calcEl.addEventListener("click", async (e) => {
    const band = e.target.closest("[data-band]");
    if (band) {
      const t = TYPES[type];
      const next = t.cut(bandFreq(band.dataset.band));
      for (const v of t.vars) {
        if (next[v.key] == null) continue;
        values[v.key] = Math.min(v.max, Math.max(v.min, next[v.key]));
        varsEl.querySelector(`input[data-key="${v.key}"]`).value = values[v.key];
      }
      update();
    }
    if (e.target.id === "ant-copy") {
      const res = window.WCR && WCR.copyText ? await WCR.copyText(document.getElementById("ant-term").textContent) : "fail";
      e.target.textContent = res === "copied" ? "copied" : "copy failed";
      setTimeout(() => (e.target.textContent = "copy"), 1200);
    }
  });
  cardsEl.addEventListener("click", (e) => {
    const btn = e.target.closest(".ant-use");
    if (!btn) return;
    const radio = calcEl.querySelector(`input[name="ant-type"][value="${btn.dataset.type}"]`);
    radio.checked = true;
    type = btn.dataset.type;
    buildControls();
    calcEl.scrollIntoView({ behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
  });

  buildControls();
})();
