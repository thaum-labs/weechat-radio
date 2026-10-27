import { expect, test, type Page, type Route } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

// The planner at /guides/band.html with the path service, the hub and
// Open-Meteo mocked. Set MAPLIBRE_DIR to an unpacked maplibre-gl package
// to run without unpkg.com.

declare global {
  interface Window {
    __WCR_BAND: {
      setTxLngLat(lon: number, lat: number): void;
      setRxLngLat(lon: number, lat: number): void;
      zoomTo(z: number): void;
      pinOffsets(): number[];
      fitBox(): { w: number; h: number; pins: { x: number; y: number }[]; arc: { x: number; y: number } | null };
      state(): {
        tx: { lat: number; lon: number } | null;
        rx: { lat: number; lon: number } | null;
        cardText: string;
        terrain: {
          blocked: boolean;
          marginal: boolean;
          byTerrain: boolean;
          n: number;
          y0: number;
          y1: number;
          strict: string;
          shown: string;
        } | null;
        sky: {
          hops: { n: number; approx: boolean; D: number };
          takeoff: { deg: number; approx: boolean };
          rings: { gw: number; skip: number; beyond: boolean } | null;
          state: string;
          greyline: [number, number][];
        } | null;
      };
    };
  }
}

type PathBody = {
  tx_lat: number;
  tx_lon: number;
  rx_lat: number;
  rx_lon: number;
  hour_utc: number;
  power_w: number;
  tx_antenna_m: number;
  rx_antenna_m: number;
  hf_antenna: string;
  band?: string;
};

const HF = {
  ok: true,
  kind: "hf",
  frequency_mhz: "10.140",
  frequency_khz: 10140,
  band: "30m",
  ssb: true,
  mode: "USB",
  power_w: 20,
  preset: "hf-poor",
  qsy: "/radio qsy 10.140",
  preset_cmd: "/radio preset hf-poor",
  confidence: "Likely this hour.",
  note: "SSB on, USB for data.",
  why: "30m: open now on this path, 20 W is enough. It closes about 19:00 UTC.",
  voice_note: null,
  footnote: "",
  distance_km: 5570,
  next_utc: null,
};

const FM = {
  ...HF,
  kind: "fm",
  frequency_mhz: "144.950",
  frequency_khz: 144950,
  band: "2m",
  ssb: false,
  mode: "FM",
  power_w: 5,
  preset: "vhf-fm",
  why: "2m FM: 16 km is inside the 26 km radio horizon for 10 m antennas.",
  distance_km: 16,
};

async function mockServices(page: Page, ground: (i: number, n: number) => number) {
  const bodies: PathBody[] = [];
  const dir = process.env.MAPLIBRE_DIR;
  if (dir) {
    await page.route("https://unpkg.com/maplibre-gl@4/**", (route: Route) => {
      const file = path.join(dir, new URL(route.request().url()).pathname.replace(/^\/maplibre-gl@4\//, ""));
      route.fulfill({ body: fs.readFileSync(file), contentType: file.endsWith(".css") ? "text/css" : "text/javascript" });
    });
  }
  await page.route(/arcgisonline|cartocdn/, (route) => route.abort());
  await page.route("http://127.0.0.1:7380/api/path", async (route) => {
    const body = route.request().postDataJSON() as PathBody;
    bodies.push(body);
    const near = Math.abs(body.tx_lat - body.rx_lat) + Math.abs(body.tx_lon - body.rx_lon) < 1;
    // Like the service: two handhelds are past the radio horizon, so HF
    // is the pick with FM sent along for the terrain check.
    const low = body.tx_antenna_m <= 2 && body.rx_antenna_m <= 2;
    const hf80 = { ...HF, frequency_mhz: "3.580", band: "80m" };
    const card = !near ? HF : low ? { ...hf80, fm: FM } : { ...FM, hf: hf80 };
    // Like the service: 60m has no calling spot here, so no band_card.
    const band_card =
      body.band && body.band !== "60m"
        ? { ...HF, band: body.band, frequency_mhz: "7.045", why: `${body.band} by hand.` }
        : undefined;
    // Per band: the verdict at the hour and for every UTC hour, hops and
    // take-off angle from the engine (#157).
    const day = (open: number[], marginal: number[]) =>
      Array.from({ length: 24 }, (_, h) => (open.includes(h) ? "open" : marginal.includes(h) ? "marginal" : "closed"));
    const all = Array.from({ length: 24 }, (_, h) => h);
    const hours = {
      "80m": day(all.filter((h) => h < 7 || h > 17), all.filter((h) => h >= 7 && h <= 17)),
      "40m": day(all, []),
      "20m": day([], []),
    };
    const at = (b: keyof typeof hours) => hours[b][body.hour_utc];
    const bands = [
      { band: "80m", frequency_mhz: 3.58, ok: true, power_w: 20, state: at("80m"), hours: hours["80m"], hops: 1, takeoff_deg: 72, skips: false },
      { band: "40m", frequency_mhz: 7.045, ok: true, power_w: 10, state: at("40m"), hours: hours["40m"], hops: 2, takeoff_deg: 9.5, skips: false },
      { band: "20m", frequency_mhz: 14.07, ok: false, power_w: null, state: at("20m"), hours: hours["20m"], hops: 2, takeoff_deg: 9.5, skips: true },
    ];
    // A short path (under about 300 km) gets a low MUF, so 40m skips it.
    const short = Math.abs(body.tx_lat - body.rx_lat) + Math.abs(body.tx_lon - body.rx_lon) < 3;
    if (short) {
      // Like Frome to Plymouth on the engine: 40m comes back steeply (no
      // skip zone), 30m goes through.
      bands[1] = { ...bands[1], state: "marginal", hops: 1, takeoff_deg: 78, skips: false };
      bands.splice(2, 0, { band: "30m", frequency_mhz: 10.14, ok: false, power_w: null, state: "closed", hours: hours["20m"], hops: 1, takeoff_deg: 80, skips: true });
      // 60m is closed without skipping: a band that won't work (grey chip).
      // The service lists it after 40m; the page sorts chips by frequency (#178).
      bands.splice(2, 0, { band: "60m", frequency_mhz: 5.357, ok: false, power_w: null, state: "closed", hours: hours["20m"], hops: 1, takeoff_deg: 80, skips: false });
    }
    const sky = short ? { muf_mhz: 6.0, muf_hops: 1, solar_live: true } : { muf_mhz: 11.2, muf_hops: 2, solar_live: true };
    await route.fulfill({ json: { ...card, band_card, bands, sky, confidence: `hour ${body.hour_utc}` } });
  });
  await page.route("http://127.0.0.1:7380/api/path/ionosonde?**", (route) =>
    route.fulfill({
      json: {
        station: {
          name: "Dourbes", code: "DB049", lat: 50.1, lon: 4.6, fof2_mhz: 3.4, mufd_mhz: 10.1, cs: 75,
          time: new Date().toISOString(), distance_km: 512, age_min: 6,
        },
        fetched_at: new Date().toISOString(),
        cached_for_s: 900,
      },
    }),
  );
  await page.route("https://prop.kc2g.com/**", (route) =>
    route.fulfill({
      json: {
        type: "FeatureCollection",
        features: [
          { type: "Feature", properties: { "level-value": 14, stroke: "#ff9900" }, geometry: { type: "LineString", coordinates: [[-10, 45], [0, 46], [10, 47]] } },
          { type: "Feature", properties: { "level-value": 10.1, stroke: "#3399ff" }, geometry: { type: "LineString", coordinates: [[-10, 55], [0, 56], [10, 57]] } },
        ],
      },
    }),
  );
  await page.route("http://127.0.0.1:7380/api/path/places?**", async (route) => {
    await new Promise((r) => setTimeout(r, 600));
    await route.fulfill({ json: { places: [{ label: "Edinburgh, GB", lat: 55.95, lon: -3.19, country: "GB" }] } });
  });
  await page.route("http://127.0.0.1:7373/api/v1/nodes?**", (route) =>
    route.fulfill({
      json: {
        nodes: [
          { callsign: "G4QAA", grid: "IO91", band: "40m", lat: 51.5, lon: -0.1, last_seen: Math.floor(Date.now() / 1000) },
          { callsign: "NOPIN", lat: null, lon: null },
        ],
      },
    }),
  );
  await page.route("https://api.open-meteo.com/**", (route) => {
    const n = new URL(route.request().url()).searchParams.get("latitude")!.split(",").length;
    route.fulfill({ json: { elevation: Array.from({ length: n }, (_, i) => ground(i, n)) } });
  });
  return bodies;
}

// `serve` redirects band.html to /guides/band and drops the query, so the
// link test uses the clean path. Caddy serves band.html as it is.
async function open(page: Page, query = "") {
  await page.goto(query ? `/guides/band${query}` : "/guides/band.html");
  await page.waitForFunction(() => !!window.__WCR_BAND);
}

test("Enter right after typing a place sets the other station", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page);
  await page.evaluate(() => window.__WCR_BAND.setTxLngLat(-0.12, 51.5));
  await page.locator("#band-search").pressSequentially("Edinburgh", { delay: 80 });
  await page.locator("#band-search").press("Enter");
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().rx)).toEqual({ lat: 55.95, lon: -3.19 });
  await expect(page.locator("#band-places")).toBeHidden();
});

test("a long sea path is beyond line of sight, explained, and drawn as a sky view", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(-0.12, 51.5);
    window.__WCR_BAND.setRxLngLat(-74, 40.7);
  });
  await expect(page.locator("#los-note")).toHaveText("beyond line of sight for VHF and UHF");
  await expect(page.locator(".band-why")).toContainText("30m: open now");
  await expect(page.locator("#band-card")).not.toContainText("Terrain blocks");
  // HF goes by the sky: the profile slot holds the side view (#157).
  await expect(page.locator("#band-profile svg.pf-sky")).toBeVisible();
  await expect(page.locator("#band-profile")).toContainText("not to scale");
});

test("a long path with land on it is still beyond line of sight, not terrain", async ({ page }) => {
  // London to New York: sea with a 424 m stretch of land near each end.
  await mockServices(page, (i, n) => (i === Math.round(n / 6) || i === Math.round((5 * n) / 6) ? 424 : 0));
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(-0.12, 51.5);
    window.__WCR_BAND.setRxLngLat(-74, 40.7);
  });
  await expect(page.locator("#los-note")).toHaveText("beyond line of sight for VHF and UHF");
  await expect(page.locator("#band-card")).not.toContainText("Terrain blocks");
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().terrain)).toMatchObject({ blocked: true, byTerrain: false });
});

test("a hill switches FM to HF; antenna height and power go to the service", async ({ page }) => {
  // A 900 m ridge in the middle of a 16 km path.
  const bodies = await mockServices(page, (i, n) => (Math.abs(i - n / 2) < 2 ? 900 : 100));
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  await expect(page.locator("#los-note")).toHaveText("terrain blocks VHF and UHF");
  await expect(page.locator(".band-freq")).toContainText("3.580");
  await expect(page.locator(".band-why")).toContainText("Terrain blocks VHF and UHF.");
  // A blocked path switching to HF shows the sky view (#157).
  await expect(page.locator(".band-map-stage")).toHaveClass(/is-sky/);
  await expect(page.locator("#band-profile svg.pf-sky")).toBeVisible();
  await page.locator("#band-tx-ant").selectOption("20");
  await page.locator("#band-power").selectOption("5");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ tx_antenna_m: 20, rx_antenna_m: 10, power_w: 5 });
  await expect(page).toHaveURL(/txant=20/);
  await expect(page).not.toHaveURL(/rxant=/);
  await expect(page).toHaveURL(/w=5/);
});

test("TX and RX heights are separate, typed heights work, and links carry both", async ({ page }) => {
  const bodies = await mockServices(page, () => 100);
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25&txant=20");
  await expect(page.locator("#band-tx-ant")).toHaveValue("20");
  await expect(page.locator("#band-rx-ant")).toHaveValue("10");
  await page.locator("#band-rx-ant").selectOption("other");
  const box = page.locator("#band-rx-ant-m");
  await expect(box).toBeVisible();
  await box.fill("2.5");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ tx_antenna_m: 20, rx_antenna_m: 2.5 });
  await expect(page).toHaveURL(/txant=20/);
  await expect(page).toHaveURL(/rxant=2\.5/);
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().terrain)).toMatchObject({ y0: 120, y1: 102.5 });
});

test("an old ant= link sets both ends", async ({ page }) => {
  const bodies = await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74&ant=6");
  await expect(page.locator("#band-tx-ant")).toHaveValue("6");
  await expect(page.locator("#band-rx-ant")).toHaveValue("6");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ tx_antenna_m: 6, rx_antenna_m: 6 });
});

test("the mobile preset is a 1.8 m roof whip with more gain than a handheld, per end and in links", async ({ page }) => {
  const bodies = await mockServices(page, () => 100);
  const spare = async () => {
    const t = await page.locator(".band-why").innerText();
    const m = t.match(/about (\d+) dB (to spare|short)/);
    return m ? (m[2] === "short" ? -Number(m[1]) : Number(m[1])) : NaN;
  };
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25&txant=1.5&rxant=1.5&w=20");
  await expect(page.locator(".band-why")).toContainText("dB");
  const handheld = await spare();
  await page.locator("#band-tx-ant").selectOption("mobile");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ tx_antenna_m: 1.8, rx_antenna_m: 1.5 });
  await expect(page).toHaveURL(/txant=mobile/);
  await expect(page.locator("#band-tx-ant-m")).toBeHidden();
  await page.locator("#band-rx-ant").selectOption("mobile");
  await expect(page).toHaveURL(/rxant=mobile/);
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().terrain)).toMatchObject({ y0: 101.8, y1: 101.8 });
  // 2 dBi each end instead of -5 dBi, plus a little height: about 17 dB more.
  await expect.poll(spare).toBeGreaterThanOrEqual(handheld + 15);
  // The link opens with both ends on mobile.
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25&txant=mobile&rxant=mobile");
  await expect(page.locator("#band-tx-ant")).toHaveValue("mobile");
  await expect(page.locator("#band-rx-ant")).toHaveValue("mobile");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ tx_antenna_m: 1.8, rx_antenna_m: 1.8 });
});

test("a near miss is marginal, stays on FM, and is drawn yellow", async ({ page }) => {
  // A ridge 15 m above the radio line in the middle of a 16 km path: well
  // inside 0.6 of the 2m Fresnel zone (about 90 m there), so not blocked.
  await mockServices(page, (i, n) => (Math.abs(i - n / 2) < 2 ? 130 : 100));
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  await expect(page.locator("#los-note")).toContainText("marginal: just clips a hill at");
  await expect(page.locator(".band-freq")).toContainText("144.950");
  // 10 m masts leave over 20 dB to spare, so the margin sets the headline.
  await expect(page.locator(".band-conf")).toHaveText("Should work well on 2m FM.");
  await expect(page.locator(".band-note")).toContainText("you have plenty of signal to cover it");
  // Lower masts leave less than 20 dB: the marginal headline and advice.
  await page.locator("#band-tx-ant").selectOption("6");
  await page.locator("#band-rx-ant").selectOption("6");
  await page.locator("#band-power").selectOption("5");
  await expect(page.locator(".band-note")).toContainText("Marginal: just clips a hill at");
  await expect(page.locator(".band-note")).toContainText("Raise the antennas or add power for a solid link.");
  await expect(page.locator(".band-conf")).toContainText("line of sight is marginal");
  await expect(page.locator("#band-profile .pf-marginal")).toHaveCount(1);
  await expect(page.locator("#band-profile .pf-block")).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().terrain)).toMatchObject({ blocked: false, marginal: true });
});

test("the signal budget says how well FM should work and picks the power", async ({ page }) => {
  // Flat 14 km path: fine between 10 m masts, too far for two handhelds.
  await mockServices(page, () => 100);
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  await expect(page.locator(".band-why")).toContainText("Should work well");
  await expect(page.locator(".band-stats")).toContainText("5 W");
  await page.locator("#band-tx-ant").selectOption("1.5");
  await page.locator("#band-rx-ant").selectOption("1.5");
  await expect(page.locator(".band-why")).toContainText("Unlikely at 100 W");
  await expect(page.locator(".band-why")).toContainText("More antenna height helps most");
});

test("the HF antenna picker goes to the service and the link", async ({ page }) => {
  const bodies = await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ hf_antenna: "dipole" });
  await page.locator(".band-hfant label", { hasText: "end-fed wire" }).click();
  await expect.poll(() => bodies.at(-1)).toMatchObject({ hf_antenna: "endfed" });
  await expect(page).toHaveURL(/hfant=endfed/);
  await page.reload();
  await expect(page.locator('input[name="band-hfant"][value="endfed"]')).toBeChecked();
});

test("the time slider asks for a later hour and moves the night shading", async ({ page }) => {
  const bodies = await mockServices(page, () => 0);
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(-0.12, 51.5);
    window.__WCR_BAND.setRxLngLat(-74, 40.7);
  });
  await expect(page.locator(".band-conf")).toHaveText(`hour ${new Date().getUTCHours()}`);
  await page.locator("#band-hour").fill("5");
  const later = (new Date().getUTCHours() + 5) % 24;
  await expect(page.locator("#band-hour-label")).toHaveText(`${String(later).padStart(2, "0")}:00 UTC · +5 h`);
  await expect(page.locator(".band-conf")).toHaveText(`hour ${later}`);
  expect(bodies.at(-1)!.hour_utc).toBe(later);
});

test("a shared link restores both stations", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74&w=50");
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().rx)).toEqual({ lat: 40.7, lon: -74 });
  expect(await page.evaluate(() => window.__WCR_BAND.state().tx)).toEqual({ lat: 51.5, lon: -0.12 });
  await expect(page.locator("#band-power")).toHaveValue("50");
  await expect(page.locator(".band-freq")).toContainText("10.140");
  await page.evaluate(() => window.__WCR_BAND.setRxLngLat(2.35, 48.85));
  await expect(page).toHaveURL(/rx=48\.8500,2\.3500/);
});

test("on air lists stations from the live map and picks one as the other station", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page);
  await page.evaluate(() => window.__WCR_BAND.setTxLngLat(-3.19, 55.95));
  await page.locator("#band-onair").click();
  const hit = page.locator(".band-onair-hit");
  await expect(hit).toHaveCount(1);
  await expect(hit).toContainText("G4QAA · IO91 · 40m");
  await hit.click();
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().rx)).toEqual({ lat: 51.5, lon: -0.1 });
  await expect(page.locator("#band-search")).toHaveValue("G4QAA");
});

test("on a phone the page has no sideways scroll", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74");
  await expect(page.locator(".band-freq")).toBeVisible();
  const widths = await page.evaluate(() => ({
    page: document.documentElement.scrollWidth,
    tiles: Math.max(...[...document.querySelectorAll(".band-hfant label")].map((el) => el.getBoundingClientRect().right)),
    style: document.getElementById("map-style")!.getBoundingClientRect().right,
  }));
  expect(widths.page).toBeLessThanOrEqual(390);
  expect(widths.tiles).toBeLessThanOrEqual(390);
  expect(widths.style).toBeLessThanOrEqual(390);
});

test("a flat marginal path says the ground is close, and stays on FM with handhelds", async ({ page }) => {
  // Flat 16 km path: at 10 m the ground crowds the Fresnel zone.
  await mockServices(page, () => 100);
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  await expect(page.locator("#los-note")).toContainText("marginal: the ground is close to the signal path");
  // The ground never reaches the line and the whole path is in the sure
  // range at 10 m, so the profile shows no yellow (#142).
  await expect(page.locator("#band-profile .pf-marginal")).toHaveCount(0);
  // Two handhelds: the service says HF (past the radio horizon), but the
  // terrain is only marginal, so the card keeps 2m FM and says it is unlikely.
  await page.locator("#band-tx-ant").selectOption("1.5");
  await page.locator("#band-rx-ant").selectOption("1.5");
  await expect.poll(() => page.evaluate(() => window.__WCR_BAND.state().terrain)).toMatchObject({ blocked: false });
  await expect(page.locator(".band-freq")).toContainText("144.950");
  await expect(page.locator(".band-why")).toContainText("Unlikely at 100 W");
  await expect(page.locator(".band-note")).toContainText("Marginal:");
  await expect(page.locator(".band-note")).toContainText("Raise the antennas");
});

test("a wire antenna shows how it can be strung, and the link keeps it", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74");
  const pick = page.locator("#band-hfwire");
  const menu = page.locator("#band-hfwire-menu");
  await expect(pick).toBeVisible();
  await expect(pick).toContainText("horizontal");
  await expect(page.locator("#band-hfwire-note")).toContainText("Horizontal:");
  // A dipole has its own list: no inverted L or vertical.
  await pick.click();
  await expect(menu.locator("li")).toHaveText(["horizontal", "inverted V", "sloper"]);
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await page.locator(".band-hfant label", { hasText: "end-fed ½λ" }).click();
  await pick.click();
  await expect(menu.locator("li")).toHaveText(["horizontal", "sloper", "inverted V", "inverted L", "vertical"]);
  await menu.locator("li", { hasText: "sloper" }).click();
  await expect(menu).toBeHidden();
  await expect(pick).toContainText("sloper");
  await expect(page.locator("#band-hfwire-note")).toContainText("Sloper: a common, good setup.");
  await expect(page).toHaveURL(/hfwire=sloper/);
  // The picker sits under the selected tile.
  const tileX = await page.locator(".band-hfant label", { hasText: "end-fed ½λ" }).evaluate((e) => e.getBoundingClientRect().left);
  const pickX = await pick.evaluate((e) => e.getBoundingClientRect().left);
  expect(Math.abs(pickX - tileX)).toBeLessThan(2);
  await page.reload();
  await expect(pick).toContainText("sloper");
  await expect(page.locator("#band-hfwire-note")).toContainText("Sloper:");
  // The row keeps its space when hidden, so the map below does not move.
  const mapTop = () => page.locator("#map").evaluate((e) => e.getBoundingClientRect().top);
  const top = await mapTop();
  await page.locator(".band-hfant label", { hasText: "vertical" }).first().click();
  await expect(pick).toBeHidden();
  await expect(page).not.toHaveURL(/hfwire=/);
  expect(await mapTop()).toBe(top);
});

test("on a phone, the wire setup row and note never move the map", async ({ page }) => {
  // A link with a non-wire antenna used to load an empty row, so the first
  // wire tap pushed the map down; longer notes did the same on a phone.
  await mockServices(page, () => 0);
  for (const width of [390, 360]) {
    await page.setViewportSize({ width, height: 844 });
    await open(page, "?tx=51.5,-0.12&rx=40.7,-74&hfant=vertical");
    const mapTop = () => page.locator("#map").evaluate((e) => e.getBoundingClientRect().top);
    const top = await mapTop();
    await page.locator(".band-hfant label", { hasText: "dipole" }).click();
    await expect(page.locator("#band-hfwire")).toBeVisible();
    expect(await mapTop()).toBe(top);
    await page.locator(".band-hfant label", { hasText: "end-fed ½λ" }).click();
    for (const setup of ["inverted V", "inverted L", "vertical", "sloper"]) {
      await page.locator("#band-hfwire").click();
      await page.locator("#band-hfwire-menu li", { hasText: setup }).click();
      await expect(page.locator("#band-hfwire")).toContainText(setup);
      expect(await mapTop()).toBe(top);
    }
  }
});

test("an old link with a setup the antenna does not have falls back to horizontal", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74&hfwire=vertical");
  await expect(page.locator("#band-hfwire")).toContainText("horizontal");
  await page.locator(".band-hfant label", { hasText: "end-fed wire" }).click();
  await expect(page.locator("#band-hfwire")).toContainText("vertical");
  await expect(page).toHaveURL(/hfant=endfed&hfwire=vertical|hfwire=vertical.*hfant=endfed/);
});

test("clear removes both pins and the card without a reload", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74&w=50");
  await expect(page.locator(".band-freq")).toBeVisible();
  await page.locator("#band-clear").click();
  await expect(page.locator(".band-answer")).toHaveCount(0);
  await expect(page.locator("#band-empty")).toBeVisible();
  await expect(page.locator(".map-mark-wrap")).toHaveCount(0);
  await expect(page.locator("#band-hint")).toHaveText("Click where you are");
  await expect(page).not.toHaveURL(/tx=|rx=/);
  // Your own power stays.
  await expect(page).toHaveURL(/w=50/);
  expect(await page.evaluate(() => window.__WCR_BAND.state().terrain)).toBeNull();
});

test("a band picked by hand rates the path at that band, shows a banner, and reverts", async ({ page }) => {
  // A ridge 15 m above the radio line mid-path: marginal at 2m, so the
  // recommendation is 2m FM.
  await mockServices(page, (i, n) => (Math.abs(i - n / 2) < 2 ? 130 : 100));
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  await expect(page.locator(".band-freq")).toContainText("144.950");
  // The recommended band carries its licence note too.
  await expect(page.locator(".band-override-info")).toHaveText("⚠ 2m needs an amateur licence.");
  await page.locator("#band-override").selectOption("pmr446");
  await expect(page.locator(".band-override-banner")).toContainText("PMR446 · recommended: 2m");
  await expect(page.locator(".band-override-info")).toContainText("0.5 W");
  await expect(page.locator(".band-freq")).toContainText("446.00625");
  await expect(page).toHaveURL(/band=pmr446/);
  // A picked band on a marginal path gets a yellow tag, not green.
  await expect(page.locator(".band-note")).toContainText("Marginal:");
  await expect(page.locator(".band-conf")).toHaveClass(/band-conf-warn/);
  // At 446 MHz the Fresnel zone is narrower, so the same ridge reads worse.
  await expect(page.locator("#los-note")).not.toContainText("for VHF and UHF");
  await page.locator("#band-override").selectOption("cb");
  await expect(page.locator(".band-freq")).toContainText("27.185");
  await expect(page.locator(".band-override-info")).toContainText("CB in the UK");
  await page.locator("#band-override").selectOption("murs");
  await expect(page.locator(".band-freq")).toContainText("151.820");
  await page.reload();
  await expect(page.locator("#band-override")).toHaveValue("murs");
  await expect(page.locator(".band-override-banner")).toContainText("MURS · recommended");
  await page.locator(".band-revert").click();
  await expect(page.locator(".band-override-banner")).toHaveCount(0);
  await expect(page.locator(".band-freq")).toContainText("144.950");
  await expect(page).not.toHaveURL(/band=/);
});

// #153: changing the band, power or antenna on the same pins swaps the
// answer in place. No waiting line above the card, and the card never
// shrinks, so the page does not jump.
test("a band change updates the card in place without a jump", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockServices(page, (i, n) => (Math.abs(i - n / 2) < 2 ? 130 : 100));
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25");
  await expect(page.locator(".band-freq")).toContainText("144.950");
  await page.evaluate(() => {
    const card = document.getElementById("band-card")!;
    const wait = document.getElementById("band-wait")!;
    const w = window as unknown as { __jank: string[] };
    w.__jank = [];
    let h = card.offsetHeight;
    new MutationObserver(() => {
      if (!wait.hidden) w.__jank.push("wait shown");
      if (!card.classList.contains("has-answer")) w.__jank.push("answer dropped");
      if (card.offsetHeight < h) w.__jank.push(`shrank ${h}->${card.offsetHeight}`);
      h = Math.max(h, card.offsetHeight);
    }).observe(card, { attributes: true, childList: true, subtree: true });
  });
  const top = await page.locator(".band-answer").evaluate((e) => e.getBoundingClientRect().top);
  for (const band of ["pmr446", "cb", "murs", ""]) {
    await page.locator("#band-override").selectOption(band);
    await expect(page.locator(".band-freq")).toContainText(
      { pmr446: "446.00625", cb: "27.185", murs: "151.820", "": "144.950" }[band]!
    );
  }
  await page.locator("#band-power").selectOption("20");
  await expect(page.locator(".band-freq")).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { __jank: string[] }).__jank)).toEqual([]);
  expect(await page.locator(".band-answer").evaluate((e) => e.getBoundingClientRect().top)).toBe(top);
});

test("an HF band picked by hand comes from the service", async ({ page }) => {
  const bodies = await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74");
  await page.locator("#band-override").selectOption("40m");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ band: "40m" });
  await expect(page.locator(".band-freq")).toContainText("7.045");
  await expect(page.locator(".band-override-banner")).toContainText("40m · recommended: 30m");
  // HF goes by the sky: no VHF line-of-sight line on its card.
  await expect(page.locator(".band-verdict")).not.toContainText("line of sight");
  await expect(page.locator(".band-verdict")).not.toContainText("Marginal");
  // A band with no calling spot here still gets the banner and revert.
  await page.locator("#band-override").selectOption("60m");
  await expect(page.locator(".band-conf")).toHaveText("60m doesn't work on this path.");
  await expect(page.locator(".band-override-banner")).toContainText("60m · recommended");
  await page.locator(".band-revert").click();
  await expect(page.locator(".band-freq")).toContainText("10.140");
});

test("a blocked band picked by hand says it doesn't work and lists bands that do", async ({ page }) => {
  // A 900 m ridge in the middle of a 16 km path blocks VHF and UHF.
  await mockServices(page, (i, n) => (Math.abs(i - n / 2) < 2 ? 900 : 100));
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  await expect(page.locator(".band-freq")).toContainText("3.580");
  await page.locator("#band-override").selectOption("pmr446");
  await expect(page.locator(".band-conf")).toHaveText("PMR446 doesn't work on this path.");
  await expect(page.locator(".band-conf")).toHaveClass(/band-nogo/);
  await expect(page.locator(".band-why")).toContainText("Terrain blocks PMR446.");
  await expect(page.locator(".band-why")).not.toContainText("Should work");
  await expect(page.locator(".band-stats")).toContainText("none");
  // The HF bands with their colour: green works, yellow skips, grey won't
  // work; a click picks one, the recommended one reverts. Low to high
  // frequency, although the service sends 60m after 40m.
  const chips = page.locator(".band-works-pick");
  await expect(chips).toHaveText([/^80m\s*20 W$/, /^60m\s*100 W$/, /^40m\s*10 W$/, /^30m\s*100 W$/, /^20m\s*100 W$/]);
  expect(await chips.evaluateAll((els) => els.map((e) => e.getAttribute("data-state")))).toEqual([
    "works",
    "none",
    "works",
    "skips",
    "skips",
  ]);
  await expect(page.locator(".band-legend")).toHaveText(/works.*skips, may or may not work.*won't work/);
  await chips.filter({ hasText: "40m" }).click();
  await expect(page.locator("#band-override")).toHaveValue("40m");
  await expect(page.locator(".band-freq")).toContainText("7.045");
  await chips.filter({ hasText: "80m" }).click();
  await expect(page.locator(".band-override-banner")).toHaveCount(0);
  await expect(page).not.toHaveURL(/band=/);
});

// #168/#170: the band chips depend only on the path and setup. Picking a
// band (HF, VHF or UHF) never adds, removes or recolours a chip.
async function expectSameChipsForEveryPick(page: Page, picks: { band: string; freq: string }[]) {
  const chips = page.locator(".band-works-pick");
  await expect(chips.first()).toBeVisible();
  const states = () => chips.evaluateAll((els) => els.map((e) => `${e.getAttribute("data-state")} ${e.className}`));
  const base = await chips.allTextContents();
  const baseStates = await states();
  for (const { band, freq } of picks) {
    await page.locator("#band-override").selectOption(band);
    // Wait for the new card, not just the banner: the old chips are still
    // on screen the moment the select changes.
    await expect(page.locator(".band-freq")).toContainText(freq);
    await expect(chips).toHaveText(base);
    expect(await states()).toEqual(baseStates);
  }
  return base;
}

test("the band chips are the same whichever band is picked", async ({ page }) => {
  await mockServices(page, () => 100);
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  await expect(page.locator(".band-works")).toContainText("2m");
  const base = await expectSameChipsForEveryPick(page, [
    { band: "pmr446", freq: "446.0" },
    { band: "40m", freq: "7.045" },
    { band: "cb", freq: "27.185" },
    { band: "2m", freq: "144.950" },
    { band: "20m", freq: "7.045" },
    { band: "60m", freq: "5.357" },
  ]);
  expect(base.some((t) => /^PMR446\s*0.5 W$/.test(t))).toBe(true);
  // No chip says "skips" or "closed": the colour and the legend say it.
  expect(base.some((t) => /skips|closed/.test(t))).toBe(false);
  // A licence-free band is budgeted at its own legal power.
  await page.locator("#band-override").selectOption("pmr446");
  await expect(page.locator(".band-stats")).toContainText("0.5 W");
  await expect(page.locator(".band-why")).toContainText("at 0.5 W");
});

test("picking an HF band on a VHF path keeps the same chips", async ({ page }) => {
  await mockServices(page, () => 100);
  await open(page, "?tx=51.2247,-2.3124&rx=51.3734,-2.3257");
  await expect(page.locator(".band-freq")).toContainText("144.950");
  await expectSameChipsForEveryPick(page, [
    { band: "40m", freq: "7.045" },
    { band: "2m", freq: "144.950" },
    { band: "20m", freq: "7.045" },
  ]);
});

for (const width of [390, 360]) {
  test(`the band legend wraps inside the card at ${width} px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 844 });
    await mockServices(page, () => 100);
    await open(page, "?tx=51.2247,-2.3124&rx=51.3734,-2.3257");
    const legend = page.locator(".band-legend");
    await expect(legend).toBeVisible();
    const box = await legend.boundingBox();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
  });
}

// #146/#150: after RX is set (or a link opens) the map shows both pins and
// the whole arc with a margin, centred on the middle of the path, even when
// the result card leaves the map short. The map never drops under 240 px.
for (const size of [
  { width: 1280, height: 800 },
  { width: 390, height: 844 },
  { width: 1920, height: 1080 },
  { width: 1024, height: 600 },
]) {
  test(`the map fits the whole path at ${size.width}×${size.height}`, async ({ page }) => {
    await page.setViewportSize(size);
    await mockServices(page, () => 100);
    const paths = {
      "Frome to Bath": "?tx=51.2247,-2.3124&rx=51.3734,-2.3257",
      "London to Manchester": "?tx=51.5074,-0.1278&rx=53.4808,-2.2426",
      "London to New York": "?tx=51.5074,-0.1278&rx=40.7128,-74.0060",
      "1 km PMR": "?tx=51.2247,-2.3124&rx=51.2337,-2.3124",
      "east to west": "?tx=51.30,-2.10&rx=51.30,-2.60",
    };
    const runs: [string, string, [number, number] | null][] = Object.entries(paths).map(([n, q]) => [n, q, null]);
    // Same paths with RX set the way a click sets it, after TX only.
    runs.push(["Frome to Bath by click", "?tx=51.2247,-2.3124", [-2.3257, 51.3734]]);
    runs.push(["London to Manchester by click", "?tx=51.5074,-0.1278", [-2.2426, 53.4808]]);
    // #153: a new RX on a path that is already drawn fits the new path,
    // not the old one's bounds.
    runs.push(["new RX on Frome to Bath", "?tx=51.2247,-2.3124&rx=51.3734,-2.3257", [-2.2981, 51.299]]);
    for (const [name, query, click] of runs) {
      await open(page, query);
      if (click) await page.evaluate(([lon, lat]) => window.__WCR_BAND.setRxLngLat(lon, lat), click);
      await expect(page.locator(".band-freq")).toBeVisible();
      await expect
        .poll(async () => {
          const f = await page.evaluate(() => window.__WCR_BAND.fitBox());
          if (f.h < 238) return `map only ${f.h} px tall`;
          const m = 16;
          const inside = (p: { x: number; y: number }, dx: number, dy: number) =>
            p.x - dx >= m - 1 && p.x + dx <= f.w - m + 1 && p.y - dy >= m - 1 && p.y + dy <= f.h - m + 1;
          const [a, b] = f.pins;
          const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
          const centred = Math.abs(mid.x - f.w / 2) <= f.w * 0.1 && Math.abs(mid.y - f.h / 2) <= f.h * 0.1;
          // Zoomed in to the path, not a whole-region view. London to New
          // York also has to fit its great-circle track, which bows north.
          const filled =
            name.startsWith("London to New York") ||
            Math.max(Math.abs(a.x - b.x) / f.w, Math.abs(a.y - b.y) / f.h) >= 0.4;
          return `${f.pins.every((p) => inside(p, 14, 13)) && inside(f.arc!, 0, 0) && centred && filled}`;
        }, { message: name })
        .toBe("true");
    }
  });
}

// #150: Mobile stays allowed on PMR446 with its gain, plus a short note.
test("PMR446 with a mobile antenna keeps the gain and shows a short note", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockServices(page, () => 0);
  await open(page, "?tx=46.2,6.14&rx=46.2,6.192&band=pmr446");
  await expect(page.locator(".band-freq")).toBeVisible();
  await expect(page.locator(".band-ant-note")).toHaveCount(0);
  await page.locator("#band-tx-ant").selectOption("mobile");
  const note = page.locator(".band-ant-note");
  await expect(note).toContainText("built-in antenna; an external roof antenna isn't licence-free");
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  const box = await note.boundingBox();
  expect(box!.x + box!.width).toBeLessThanOrEqual(390);
  await page.locator("#band-override").selectOption("cb");
  await expect(page.locator(".band-ant-note")).toHaveCount(0);
});

test("both pins stay on their points when zooming", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page, "?tx=51.2247,-2.3124&rx=51.2846,-2.1986");
  await expect(page.locator(".band-freq")).toBeVisible();
  for (const z of [7, 9, 11]) {
    await page.evaluate((zoom) => window.__WCR_BAND.zoomTo(zoom), z);
    const off = await page.evaluate(() => window.__WCR_BAND.pinOffsets());
    expect(off).toHaveLength(2);
    for (const d of off) expect(d).toBeLessThanOrEqual(2);
  }
});

test("the track is yellow only where the path is pinched", async ({ page }) => {
  const ridge = (i: number, n: number) => (Math.abs(i - n / 2) < 2 ? 130 : 100);
  const shown = () => page.evaluate(() => window.__WCR_BAND.state().terrain?.shown ?? "");
  // Flat 14 km between 10 m masts: marginal all along by the strict rating,
  // but the ground stays under the line and the sure range at 10 m covers
  // the whole path, so it is all blue (#142).
  await mockServices(page, () => 100);
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25&txant=10&rxant=10");
  await expect.poll(shown).toMatch(/^c+$/);
  expect((await page.evaluate(() => window.__WCR_BAND.state().terrain))?.strict).toMatch(/^cm+c$/);
  // Two handhelds: short of signal, so the whole pinch is yellow, but the
  // first and last 2 km or so (the sure range) stay blue.
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25&txant=1.5&rxant=1.5&w=20");
  await expect.poll(shown).toMatch(/^c{8,}m+c{8,}$/);
  // A ridge on an 8 dB path: yellow at the ridge only, not the flat ends.
  await page.unrouteAll();
  await mockServices(page, ridge);
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25&txant=6&rxant=6&w=5");
  await expect.poll(shown).toMatch(/^c{20,}m{1,6}c{20,}$/);
  await expect(page.locator("#band-profile .pf-marginal")).toHaveCount(1);
});

test("inside the sure range the track is blue under the line and yellow where it clips", async ({ page }) => {
  const shown = () => page.evaluate(() => window.__WCR_BAND.state().terrain?.shown ?? "");
  // 4 km of flat ground, PMR446 handhelds, a bump 0.2 km from TX. Below
  // the radio line it is the tightest spot on the path, but well inside the
  // sure range, so it stays blue (#142).
  await mockServices(page, (i) => (i === 1 ? 101.1 : 100));
  await open(page, "?tx=46.2,6.14&rx=46.2,6.192&txant=1.5&rxant=1.5&band=pmr446");
  await expect.poll(shown).toMatch(/^c{6}m+c{6}$/);
  // Raised to clip the line: yellow, sure range or not (Tobias, #142).
  await page.unrouteAll();
  await mockServices(page, (i) => (i === 1 ? 101.6 : 100));
  await open(page, "?tx=46.2,6.14&rx=46.2,6.192&txant=1.5&rxant=1.5&band=pmr446");
  await expect.poll(shown).toMatch(/^cmc{4}m+c{6}$/);
});

test("with a solid budget only the worst spot is yellow, even inside the sure range", async ({ page }) => {
  const shown = () => page.evaluate(() => window.__WCR_BAND.state().terrain?.shown ?? "");
  // 10 m masts with two bumps clipping the line, one near each end, both
  // inside the sure range. 23 dB to spare, so only the worse one is
  // yellow (Tobias, #142 / #144).
  await mockServices(page, (i, n) => (i === 2 ? 113 : i === n - 3 ? 111 : 100));
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25&txant=10&rxant=10");
  await expect.poll(shown).toMatch(/^ccmc+$/);
  await expect(page.locator(".band-why")).toContainText("Should work well");
});

test("no card shows before the first terrain reply", async ({ page }) => {
  await mockServices(page, () => 100);
  let release: () => void = () => {};
  const held = new Promise<void>((r) => (release = r));
  await page.route("https://api.open-meteo.com/**", async (route) => {
    await held;
    return route.fallback();
  });
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25");
  await expect(page.locator("#los-note")).toHaveText("checking terrain…");
  await expect(page.locator("#band-wait")).toBeVisible();
  await expect(page.locator(".band-freq")).toHaveCount(0);
  release();
  await expect(page.locator(".band-freq")).toContainText("144.950");
  await expect(page.locator("#band-wait")).toBeHidden();
});

test("a busy elevation service is retried, then says so with a Retry button", async ({ page }) => {
  await mockServices(page, () => 100);
  let busy = true;
  let calls = 0;
  await page.route("https://api.open-meteo.com/**", (route) => {
    calls += 1;
    if (busy) return route.fulfill({ status: 429, body: "rate limited" });
    return route.fallback();
  });
  await open(page, "?tx=46.2,6.14&rx=46.1,6.25");
  await expect(page.locator("#los-note")).toHaveText("terrain lookup busy, retrying…");
  // While retrying, the card waits rather than showing an answer that
  // ignores the hills.
  await expect(page.locator("#band-wait")).toBeVisible();
  await expect(page.locator(".band-freq")).toHaveCount(0);
  await expect(page.locator(".band-err")).toContainText("Terrain lookup failed", { timeout: 15_000 });
  await expect(page.locator("#los-note")).toHaveText("terrain lookup failed");
  // No answer that ignores the hills, but the HF chips and legend stay:
  // they go by the sky (#185).
  await expect(page.locator(".band-freq")).toHaveCount(0);
  await expect(page.locator(".band-works-pick").first()).toBeVisible();
  await expect(page.locator(".band-legend")).toBeVisible();
  expect(calls).toBeGreaterThanOrEqual(4);
  busy = false;
  await page.locator(".band-retry").click();
  await expect(page.locator(".band-freq")).toContainText("144.950");
  await expect(page.locator("#los-note")).toContainText("marginal");
});

test("use my location shows a spinner, ignores repeat clicks and names the error", async ({ page }) => {
  await page.addInitScript(() => {
    (window as any).__geoCalls = 0;
    Object.defineProperty(navigator, "geolocation", {
      value: {
        getCurrentPosition(_ok: unknown, fail: (e: { code: number }) => void) {
          (window as any).__geoCalls += 1;
          setTimeout(() => fail({ code: 1 }), 800);
        },
      },
    });
  });
  await mockServices(page, () => 0);
  await open(page);
  const btn = page.locator("#band-locate");
  const width = (await btn.boundingBox())!.width;
  await btn.click();
  await expect(btn).toHaveAttribute("aria-busy", "true");
  await expect(btn).toBeDisabled();
  await expect(btn).toHaveClass(/is-busy/);
  expect(Math.abs((await btn.boundingBox())!.width - width)).toBeLessThan(1);
  await btn.click({ force: true });
  await expect(btn).not.toHaveAttribute("aria-busy", "true");
  await expect(btn).toBeEnabled();
  await expect(page.locator("#band-hint")).toHaveText("Location permission denied");
  expect(await page.evaluate(() => (window as any).__geoCalls)).toBe(1);
});

// #157: HF sky view.
const BATH_MADRID = "?tx=51.38,-2.36&rx=40.42,-3.70";

test("an HF answer shows even when the elevation service fails", async ({ page }) => {
  await mockServices(page, () => 100);
  await page.route("https://api.open-meteo.com/**", (route) => route.abort());
  await open(page, `${BATH_MADRID}&band=40m`);
  await expect(page.locator(".band-freq")).toContainText("7.045");
  await expect(page.locator("#band-card")).not.toContainText("Terrain lookup failed");
  await expect(page.locator("#los-note")).toHaveText("terrain unavailable, VHF/UHF not judged", { timeout: 15_000 });
  await expect(page.locator(".band-freq")).toContainText("7.045");
  // Recommended HF too, with no band picked.
  await open(page, BATH_MADRID);
  await expect(page.locator(".band-freq")).toContainText("10.140");
  await expect(page.locator("#los-note")).toHaveText("terrain unavailable, VHF/UHF not judged", { timeout: 15_000 });
  await expect(page.locator(".band-err")).toHaveCount(0);
});

test("the sky view draws hops, a side view and a 24 hour strip, and goes for VHF", async ({ page }) => {
  await mockServices(page, () => 100);
  await open(page, `${BATH_MADRID}&band=40m`);
  await expect(page.locator(".band-map-stage")).toHaveClass(/is-sky/);
  // 2 hops from the engine: no "approx." on the hop count.
  await expect(page.locator("#band-hop-chip")).toHaveText("2 hops · ~610 km each");
  await expect(page.locator(".sky-line")).toContainText("2 hops · ~610 km each · take-off 10° · MUF on path 11.2 MHz");
  await expect(page.locator("#band-profile svg.pf-sky")).toContainText("take-off 10°");
  await expect(page.locator("#band-profile svg.pf-sky")).toContainText("not to scale");
  // 20m is above the MUF: a yellow "skips" chip, and drawn going through.
  const c20 = page.locator('.band-works-pick[data-band="20m"]');
  await expect(c20).toHaveClass(/is-skips/);
  await expect(c20).toHaveAttribute("data-state", "skips");
  await expect(c20).toHaveText(/^20m\s*100 W$/);
  await expect(page.locator("#band-profile .sky-punch")).toHaveCount(1);
  // Strip: 24 cells from now, the planned hour outlined, same verdict as the chip.
  const cells = page.locator(".sky-cell");
  await expect(cells).toHaveCount(24);
  await expect(page.locator(".sky-cell.is-sel")).toHaveAttribute("data-off", "0");
  await page.locator("#band-hour").fill("5");
  await expect(page.locator(".sky-cell.is-sel")).toHaveAttribute("data-off", "5");
  // 80m works at every hour (open or marginal), so its chip stays green.
  await expect(page.locator('.band-works-pick[data-band="80m"]')).toHaveAttribute("data-state", "works");
  await expect(cells.nth(5)).toHaveClass(/sky-open/);
  // A click on a cell sets the slider.
  await cells.nth(9).click();
  await expect(page.locator("#band-hour")).toHaveValue("9");
  await expect(page.locator("#band-hour-label")).toContainText("+9 h");
  // The tag colour comes from the band's verdict.
  await expect(page.locator(".band-conf")).toHaveClass(/band-conf-ok/);
  // 2m: back to the terrain key and profile, no sky pieces.
  await page.locator("#band-override").selectOption("2m");
  await expect(page.locator(".band-map-stage")).not.toHaveClass(/is-sky/);
  await expect(page.locator("#band-profile svg.pf-sky")).toHaveCount(0);
  await expect(page.locator(".sky-strip")).toHaveCount(0);
  await expect(page.locator(".sky-dot")).toHaveCount(0);
});

test("a short HF path shows NVIS, the ground wave and the skip zone", async ({ page }) => {
  await mockServices(page, () => 100);
  // Frome to Plymouth, 80m: 72° take-off from the engine.
  await open(page, "?tx=51.2286,-2.3206&rx=50.3755,-4.1427&band=80m");
  await expect(page.locator(".sky-nvis")).toContainText("no skip zone");
  await expect(page.locator(".sky-skip")).toHaveCount(0);
  // 40m: the engine says it does not skip (78° take-off), so NVIS and no
  // skip ring, even though the 6 MHz MUF alone would draw one (#164).
  await page.locator("#band-override").selectOption("40m");
  await expect(page.locator(".band-freq")).toContainText("7.045");
  await expect(page.locator(".sky-nvis")).toContainText("no skip zone");
  await expect(page.locator(".sky-skip")).toHaveCount(0);
  expect((await page.evaluate(() => window.__WCR_BAND.state().sky!.rings))!.skip).toBe(0);
  // 30m: the engine says it skips, so a skip line with RX inside the ring,
  // a yellow skipping chip, and no NVIS note.
  await page.locator("#band-override").selectOption("30m");
  await expect(page.locator(".sky-skip")).toContainText("30m skips: skip zone ≈ 25–");
  await expect(page.locator(".sky-nvis")).toHaveCount(0);
  await expect(page.locator('.band-works-pick[aria-pressed="true"]')).toHaveText("30m100 W");
  await expect(page.locator('.band-works-pick[aria-pressed="true"]')).toHaveAttribute("data-state", "skips");
  const rings = await page.evaluate(() => window.__WCR_BAND.state().sky!.rings);
  expect(rings!.gw).toBe(25);
  expect(rings!.skip).toBeGreaterThan(159);
});

test("on a phone, HF and VHF band changes and antenna taps never move the map or the card", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockServices(page, () => 100);
  await open(page, `${BATH_MADRID}&band=40m`);
  await expect(page.locator(".band-map-stage")).toHaveClass(/is-sky/);
  await expect(page.locator("#band-credit")).toBeVisible();
  const tops = () =>
    page.evaluate(() =>
      ["#map", ".band-answer"].map((s) => Math.round(document.querySelector(s)!.getBoundingClientRect().top + scrollY)),
    );
  const want = await tops();
  for (const band of ["2m", "80m", "pmr446", "20m", ""]) {
    await page.locator("#band-override").selectOption(band);
    await expect(page.locator("#band-wait")).toBeHidden();
    await page.waitForTimeout(300);
    expect(await tops()).toEqual(want);
  }
  for (const ant of ["vertical", "endfed", "dipole"]) {
    await page.locator(`.band-hfant-tiles input[value="${ant}"]`).check({ force: true });
    // Straight after the tap, while the path is worked out, and once done.
    expect((await tops())[0]).toBe(want[0]);
    await page.waitForTimeout(800);
    expect(await tops()).toEqual(want);
  }
});

test("the greyline is the sunrise and sunset line at the planned hour", async ({ page }) => {
  await mockServices(page, () => 100);
  await open(page, `${BATH_MADRID}&band=40m`);
  await expect(page.locator(".band-map-stage")).toHaveClass(/is-sky/);
  for (const off of ["0", "6", "13"]) {
    await page.locator("#band-hour").fill(off);
    const line = await page.evaluate(() => window.__WCR_BAND.state().sky!.greyline);
    // Independent check: every point is 90° from the subsolar point.
    const t = new Date(Date.now() + Number(off) * 3600000);
    const n = t.getTime() / 86400000 + 2440587.5 - 2451545.0;
    const rad = Math.PI / 180;
    const g = (357.529 + 0.98560028 * n) * rad;
    const q = 280.459 + 0.98564736 * n;
    const L = (q + 1.915 * Math.sin(g) + 0.02 * Math.sin(2 * g)) * rad;
    const e = (23.439 - 0.00000036 * n) * rad;
    const decl = Math.asin(Math.sin(e) * Math.sin(L));
    const ra = Math.atan2(Math.cos(e) * Math.sin(L), Math.cos(L)) / rad;
    const gmst = (18.697374558 + 24.06570982441908 * n) * 15;
    const subLon = ((((ra - gmst) % 360) + 540) % 360) - 180;
    for (const [lon, lat] of line.filter((_, i) => i % 30 === 0)) {
      const d = Math.acos(
        Math.sin(lat * rad) * Math.sin(decl) + Math.cos(lat * rad) * Math.cos(decl) * Math.cos((lon - subLon) * rad)
      ) / rad;
      expect(Math.abs(d - 90)).toBeLessThan(0.5);
    }
  }
});

test("live data: KC2G contours with muf=1 and the nearest ionosonde, with credits", async ({ page }) => {
  await mockServices(page, () => 100);
  await open(page, `${BATH_MADRID}&band=40m&muf=1`);
  await expect(page.locator("#map-muf")).toBeChecked();
  await expect(page.locator(".band-muf-label").first()).toHaveText("14 MHz");
  await expect(page.locator("#band-live")).toContainText(
    "live check: Dourbes, 512 km from the path midpoint, 6 min ago: foF2 3.4, MUF(3000) 10.1 MHz"
  );
  await expect(page.locator("#band-credit")).toHaveText("MUF(3000) map: prop.kc2g.com (KC2G) · ionosonde data: Lowell GIRO");
  await expect(page.locator("#band-live-note")).toBeHidden();
  await page.locator("#map-muf").uncheck();
  await expect(page).not.toHaveURL(/muf=/);
  await expect(page.locator(".band-muf-label")).toHaveCount(0);
});

test("when live data fails it hides with one note, and the card still shows", async ({ page }) => {
  await mockServices(page, () => 100);
  await page.route("https://prop.kc2g.com/**", (route) => route.abort());
  await page.route("http://127.0.0.1:7380/api/path/ionosonde?**", (route) => route.fulfill({ status: 502, body: "down" }));
  await open(page, `${BATH_MADRID}&band=40m&muf=1`);
  await expect(page.locator(".band-freq")).toContainText("7.045");
  await expect(page.locator("#band-live-note")).toHaveText("Live ionosphere data unavailable right now.");
  await expect(page.locator("#band-live")).toBeHidden();
  await expect(page.locator("#band-credit")).toBeHidden();
  await expect(page.locator(".band-muf-label")).toHaveCount(0);
});

test("on a phone the sky view fits with no sideways scroll", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await mockServices(page, () => 100);
  await open(page, `${BATH_MADRID}&band=40m&muf=1`);
  await expect(page.locator(".sky-cell")).toHaveCount(24);
  await expect(page.locator("#band-profile svg.pf-sky")).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(390);
  const svgW = await page.locator("#band-profile svg.pf-sky").evaluate((e) => e.getBoundingClientRect().width);
  expect(svgW).toBeLessThanOrEqual(390);
});

// #177: one tap from the antenna pickers to the antenna guides, on a line
// that is already there, so nothing below moves.
for (const width of [1280, 390, 360]) {
  test(`the antenna guide links sit on existing lines at ${width} px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 844 });
    await mockServices(page, () => 100);
    await open(page);
    const hf = page.locator('.band-hfant a.band-ant-guide[href="/guides/antennas-hf.html"]');
    const vhf = page.locator('.band-opts a.band-ant-guide[href="/guides/antennas-vhf.html"]');
    for (const link of [hf, vhf]) {
      await expect(link).toBeVisible();
      const box = (await link.boundingBox())!;
      expect(box.height).toBeGreaterThanOrEqual(24);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
    }
    const mid = (b: { y: number; height: number }) => b.y + b.height / 2;
    const near = (a: number, b: number) => expect(Math.abs(a - b)).toBeLessThanOrEqual(4);
    // VHF/UHF link: on the TX antenna line. HF link: beside the tiles on a
    // desktop, beside the "HF antenna" label on a phone.
    near(mid((await vhf.boundingBox())!), mid((await page.locator("label.band-ant").first().boundingBox())!));
    const beside = width > 600 ? ".band-hfant-tiles" : ".band-hfant-label";
    near(mid((await hf.boundingBox())!), mid((await page.locator(beside).boundingBox())!));
  });
}

test("a direct 2m link keeps the HF chips when the elevation service fails (#185)", async ({ page }) => {
  await mockServices(page, () => 100);
  await page.route("https://api.open-meteo.com/**", (route) => route.fulfill({ status: 503, body: "busy" }));
  await open(page, `${BATH_MADRID}&band=2m`);
  await expect(page.locator(".band-err")).toContainText("VHF and UHF can't be judged yet", { timeout: 15_000 });
  await expect(page.locator(".band-retry")).toBeVisible();
  await expect(page.locator('.band-works-pick[data-band="40m"]')).toBeVisible();
  await expect(page.locator(".band-legend")).toBeVisible();
  // A chip still picks its HF band, which needs no terrain.
  await page.locator('.band-works-pick[data-band="40m"]').click();
  await expect(page.locator(".band-freq")).toContainText("7.045");
});
