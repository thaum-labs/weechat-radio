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
    const card = near ? { ...FM, hf: { ...HF, frequency_mhz: "3.580", band: "80m" } } : HF;
    await route.fulfill({ json: { ...card, confidence: `hour ${body.hour_utc}` } });
  });
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

test("a long sea path is beyond line of sight, explained, and drawn as a profile", async ({ page }) => {
  await mockServices(page, () => 0);
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(-0.12, 51.5);
    window.__WCR_BAND.setRxLngLat(-74, 40.7);
  });
  await expect(page.locator("#los-note")).toHaveText("beyond line of sight for VHF and UHF");
  await expect(page.locator(".band-why")).toContainText("30m: open now");
  await expect(page.locator("#band-card")).not.toContainText("Terrain blocks");
  await expect(page.locator("#band-profile svg .pf-curve")).toBeVisible();
  await expect(page.locator("#band-profile svg .pf-line")).toHaveCount(1);
  await expect(page.locator("#band-profile")).toContainText("4/3 earth");
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
  await expect(page.locator("#band-profile .pf-block")).toHaveCount(1);
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
  await expect(page.locator(".band-note")).toContainText("Marginal for VHF and UHF");
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
