import { test, expect, type Page } from "@playwright/test";

declare global {
  interface Window {
    WCR: {
      map: {
        isStyleLoaded(): boolean;
        areTilesLoaded(): boolean;
        getBounds(): {
          contains(lngLat: [number, number]): boolean;
          getEast(): number;
          getWest(): number;
        };
        getCenter(): { lng: number; lat: number };
        getZoom(): number;
        jumpTo(camera: { center: [number, number]; zoom: number }): void;
      };
    };
  }
}

async function dismissBoot(page: Page) {
  const boot = page.locator("#boot_screen");
  if (await boot.isVisible()) await boot.click();
  await expect(boot).toBeHidden();
  await page
    .waitForFunction(() => window.WCR.map && window.WCR.map.areTilesLoaded(), undefined, {
      timeout: 10_000,
    })
    .catch(() => {});
}

async function homeCamera(page: Page) {
  await page.waitForFunction(() => {
    const map = window.WCR && window.WCR.map;
    if (!map || !map.isStyleLoaded()) return false;
    const bounds = map.getBounds();
    return bounds.contains([-74.0, 40.7]) && bounds.contains([37.6, 55.8]);
  });
  return page.evaluate(() => {
    const map = window.WCR.map;
    const center = map.getCenter();
    const bounds = map.getBounds();
    const inside = (lng: number, lat: number) => bounds.contains([lng, lat]);
    return {
      lng: center.lng,
      lat: center.lat,
      zoom: map.getZoom(),
      nyc: inside(-74.0, 40.7),
      miami: inside(-80.2, 25.8),
      boston: inside(-71.1, 42.4),
      moscow: inside(37.6, 55.8),
      petersburg: inside(30.3, 59.9),
      eastOfMoscow: inside(39.0, 55.75),
      pastMoscow: inside(43.0, 55.75),
      california: inside(-118.2, 34.1),
      tokyo: inside(139.7, 35.7),
      east: bounds.getEast(),
      west: bounds.getWest(),
    };
  });
}

function expectHomeFrame(view: {
  lng: number;
  lat: number;
  nyc: boolean;
  miami: boolean;
  boston: boolean;
  moscow: boolean;
  petersburg: boolean;
  eastOfMoscow: boolean;
  pastMoscow: boolean;
  california: boolean;
  tokyo: boolean;
  east: number;
  west: number;
}) {
  expect(view.lng).toBeGreaterThan(-30);
  expect(view.lng).toBeLessThan(-12);
  expect(view.lat).toBeGreaterThan(45);
  expect(view.lat).toBeLessThan(55);
  expect(view.nyc).toBe(true);
  expect(view.miami).toBe(true);
  expect(view.boston).toBe(true);
  expect(view.moscow).toBe(true);
  expect(view.petersburg).toBe(true);
  expect(view.eastOfMoscow).toBe(true);
  expect(view.pastMoscow).toBe(false);
  expect(view.california).toBe(false);
  expect(view.tokyo).toBe(false);
  expect(view.west).toBeLessThan(-80);
  expect(view.east).toBeGreaterThan(38.6);
  expect(view.east).toBeLessThan(40.2);
}

test("home map runs from the US east coast to 100 km east of Moscow", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  const view = await homeCamera(page);
  expectHomeFrame(view);

  await dismissBoot(page);
  await page.locator("#map").screenshot({
    path: "/opt/cursor/artifacts/map-home-europe.png",
  });

  await page.evaluate(() => window.WCR.map.jumpTo({ center: [139.7, 35.7], zoom: 5 }));
  await page.goto("/why.html");
  await page.goto("/");
  const returned = await homeCamera(page);
  expectHomeFrame(returned);
});

test("home map keeps the same places in frame on a phone", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  const view = await homeCamera(page);
  expectHomeFrame(view);
  await dismissBoot(page);
  await page.locator("#map").screenshot({
    path: "/opt/cursor/artifacts/map-home-europe-phone.png",
  });
});

test("live map shell loads", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#map")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("LIVE MAP")).toBeVisible();
  await expect(page.locator("#hub-panel")).toBeAttached();
});

test("internet-radio tower pin shows the dial frequency", async ({ page }) => {
  await page.route("**/api/v1/**", async (route) => {
    const url = route.request().url();
    if (url.includes("/api/v1/nodes") && url.includes("since=")) {
      await route.fulfill({ json: { since: 1, trail: [] } });
      return;
    }
    if (url.includes("/api/v1/nodes")) {
      await route.fulfill({
        json: {
          nodes: [
            {
              callsign: "G4ABC",
              lat: 51.5,
              lon: -0.12,
              mode: "internet-radio",
              tower: true,
              settings: { tower: true },
              freq_khz: 144950,
              frequency: "144.950",
              band: "2m",
              grid: "IO91",
              ptt: "idle",
              preset: "vhf-fm",
              snr: 12,
            },
            {
              callsign: "M0XYZ",
              lat: 53.4,
              lon: -2.2,
              mode: "internet-radio",
              tower: false,
              freq_khz: 7045,
              frequency: "7.045",
              band: "40m",
              grid: "IO83",
              ptt: "idle",
              preset: "hf-poor",
              snr: 4,
            },
          ],
        },
      });
      return;
    }
    if (url.includes("/bands")) {
      await route.fulfill({ json: { bands: [] } });
      return;
    }
    if (url.includes("/hubs")) {
      await route.fulfill({ json: { hubs: [] } });
      return;
    }
    if (url.includes("/stats")) {
      await route.fulfill({ json: { forwarded: 0 } });
      return;
    }
    if (url.includes("/events")) {
      await route.fulfill({ json: { events: [] } });
      return;
    }
    await route.fulfill({ json: {} });
  });

  await page.goto("/");
  const tower = page.locator(".map-mark-wrap.tower");
  await expect(tower).toHaveCount(1, { timeout: 15_000 });
  await expect(tower.locator(".tower-freq")).toHaveText("144.950");
  await expect(page.locator(".map-mark-wrap")).toHaveCount(2);
  await expect(page.locator(".station", { hasText: "G4ABC" })).toBeVisible();
  await page.locator(".station", { hasText: "G4ABC" }).click();
  await page.waitForTimeout(600);
  await page.locator("#map").screenshot({
    path: "/opt/cursor/artifacts/map-tower-pin.png",
  });
});
