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
    const bands = [
      { band: "80m", ok: true, power_w: 20 },
      { band: "40m", ok: true, power_w: 10 },
      { band: "20m", ok: false, power_w: null },
    ];
    await route.fulfill({ json: { ...card, band_card, bands, confidence: `hour ${body.hour_utc}` } });
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
  await expect(page.locator("#band-hfwire")).toBeVisible();
  await expect(page.locator("#band-hfwire-note")).toContainText("Horizontal:");
  await page.locator(".band-hfant label", { hasText: "end-fed ½λ" }).click();
  await page.locator("#band-hfwire").selectOption("sloper");
  await expect(page.locator("#band-hfwire-note")).toContainText("Sloper: a common, good setup.");
  await expect(page).toHaveURL(/hfwire=sloper/);
  await page.reload();
  await expect(page.locator("#band-hfwire")).toHaveValue("sloper");
  await expect(page.locator("#band-hfwire-note")).toContainText("Sloper:");
  await page.locator(".band-hfant label", { hasText: "vertical" }).first().click();
  await expect(page.locator("#band-hfwire")).toBeHidden();
  await expect(page).not.toHaveURL(/hfwire=/);
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
  await page.locator("#band-override").selectOption("pmr446");
  await expect(page.locator(".band-override-banner")).toContainText("Manual override: PMR446 (recommended: 2m)");
  await expect(page.locator(".band-override-banner")).toContainText("0.5 W");
  await expect(page.locator(".band-freq")).toContainText("446.00625");
  await expect(page).toHaveURL(/band=pmr446/);
  // At 446 MHz the Fresnel zone is narrower, so the same ridge reads worse.
  await expect(page.locator("#los-note")).not.toContainText("for VHF and UHF");
  await page.locator("#band-override").selectOption("cb");
  await expect(page.locator(".band-freq")).toContainText("27.185");
  await expect(page.locator(".band-override-banner")).toContainText("CB in the UK");
  await page.locator("#band-override").selectOption("murs");
  await expect(page.locator(".band-freq")).toContainText("151.820");
  await page.reload();
  await expect(page.locator("#band-override")).toHaveValue("murs");
  await expect(page.locator(".band-override-banner")).toContainText("Manual override: MURS");
  await page.locator(".band-revert").click();
  await expect(page.locator(".band-override-banner")).toHaveCount(0);
  await expect(page.locator(".band-freq")).toContainText("144.950");
  await expect(page).not.toHaveURL(/band=/);
});

test("an HF band picked by hand comes from the service", async ({ page }) => {
  const bodies = await mockServices(page, () => 0);
  await open(page, "?tx=51.5,-0.12&rx=40.7,-74");
  await page.locator("#band-override").selectOption("40m");
  await expect.poll(() => bodies.at(-1)).toMatchObject({ band: "40m" });
  await expect(page.locator(".band-freq")).toContainText("7.045");
  await expect(page.locator(".band-override-banner")).toContainText("Manual override: 40m (recommended: 30m)");
  // HF goes by the sky: no VHF line-of-sight line on its card.
  await expect(page.locator(".band-note")).not.toContainText("line of sight");
  await expect(page.locator(".band-note")).not.toContainText("Marginal");
  // A band with no calling spot here still gets the banner and revert.
  await page.locator("#band-override").selectOption("60m");
  await expect(page.locator(".band-conf")).toHaveText("60m doesn't work on this path.");
  await expect(page.locator(".band-override-banner")).toContainText("Manual override: 60m");
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
  // Only the HF bands that work; a click picks one, the recommended one reverts.
  const chips = page.locator(".band-works-pick");
  await expect(chips).toHaveText(["80m · 20 W", "40m · 10 W"]);
  await chips.filter({ hasText: "40m" }).click();
  await expect(page.locator("#band-override")).toHaveValue("40m");
  await expect(page.locator(".band-freq")).toContainText("7.045");
  await chips.filter({ hasText: "80m" }).click();
  await expect(page.locator(".band-override-banner")).toHaveCount(0);
  await expect(page).not.toHaveURL(/band=/);
});

test("the list of bands that work is the same whichever band is picked", async ({ page }) => {
  await mockServices(page, () => 100);
  await open(page);
  await page.evaluate(() => {
    window.__WCR_BAND.setTxLngLat(6.14, 46.2);
    window.__WCR_BAND.setRxLngLat(6.25, 46.1);
  });
  const chips = page.locator(".band-works-pick");
  await expect(chips.first()).toBeVisible();
  await expect(page.locator(".band-works")).toContainText("2m");
  const base = await chips.allTextContents();
  expect(base.some((t) => t.startsWith("PMR446 · 0.5 W"))).toBe(true);
  for (const band of ["pmr446", "40m", "cb"]) {
    await page.locator("#band-override").selectOption(band);
    await expect(page.locator(".band-override-banner")).toBeVisible();
    await expect(chips).toHaveText(base);
  }
  // A licence-free band is budgeted at its own legal power.
  await page.locator("#band-override").selectOption("pmr446");
  await expect(page.locator(".band-stats")).toContainText("0.5 W");
  await expect(page.locator(".band-why")).toContainText("at 0.5 W");
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
  // No answer that ignores the hills.
  await expect(page.locator(".band-freq")).toHaveCount(0);
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
