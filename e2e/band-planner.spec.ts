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
        terrain: { blocked: boolean; byTerrain: boolean; n: number } | null;
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
  antenna_m: number;
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
  await page.locator("#band-antenna").selectOption("20");
  await page.locator("#band-power").selectOption("5");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ antenna_m: 20, power_w: 5 });
  await expect(page).toHaveURL(/ant=20/);
  await expect(page).toHaveURL(/w=5/);
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
