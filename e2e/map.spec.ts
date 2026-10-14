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
      linkIsInet(
        event: { kind?: string; origin?: string; dest?: string; path?: string; mode?: string },
        nodes?: { callsign: string; mode: string }[],
      ): boolean;
      stationLinkCall(dest: string, knownCalls: string[]): string | null;
      linkAlpha(ageMs: number, nowMs: number, reduceMotion?: boolean): number;
      pinAgeOpacity(ageMs: number): number;
      lastHeardLabel(ageMs: number): string;
      rememberLink(
        from: string,
        to: string,
        inet: boolean,
        draw: boolean,
        mail: boolean,
        atMs: number,
      ): void;
      seedLinksFromEvents(events: unknown[]): void;
      linkSnapshot(a: string, b: string): { at: number; inet: boolean; mail: boolean } | null;
      LIVE_MS: number;
      HOLD_MS: number;
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

test("home map runs from the US east coast to 100 km east of Moscow", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  const view = await homeCamera(page);
  expectHomeFrame(view);

  await dismissBoot(page);
  await page.locator("#map").screenshot({
    path: testInfo.outputPath("map-home-europe.png"),
  });

  await page.evaluate(() => window.WCR.map.jumpTo({ center: [139.7, 35.7], zoom: 5 }));
  await page.goto("/why.html");
  await page.goto("/");
  const returned = await homeCamera(page);
  expectHomeFrame(returned);
});

test("home map keeps the same places in frame on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  const view = await homeCamera(page);
  expectHomeFrame(view);
  await dismissBoot(page);
  await page.locator("#map").screenshot({
    path: testInfo.outputPath("map-home-europe-phone.png"),
  });
});

test("live map shell loads", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#map")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("LIVE MAP")).toBeVisible();
  await expect(page.locator("#hub-panel")).toBeAttached();
});

test("internet-radio tower pin shows the dial frequency", async ({ page }, testInfo) => {
  await page.route("**/api/v1/**", async (route) => {
    const url = route.request().url();
    if (url.includes("/api/v1/nodes") && /[?&]since=/.test(url)) {
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
    path: testInfo.outputPath("map-tower-pin.png"),
  });
});

test("radio and radio-plus contacts draw the RF line", async ({ page }) => {
  await page.goto("/");
  const style = await page.evaluate(() => {
    const nodes = [
      { callsign: "RFSIMA", mode: "radio" },
      { callsign: "RFSIMB", mode: "internet" },
      { callsign: "RFSIMC", mode: "radio-plus" },
      { callsign: "RFSIMD", mode: "internet-radio" },
    ];
    const inet = window.WCR.linkIsInet;
    return {
      radioTx: inet({ kind: "tx", origin: "RFSIMA", dest: "RFSIMB" }, nodes),
      inetTx: inet({ kind: "tx", origin: "RFSIMB", dest: "RFSIMA" }, nodes),
      plusRx: inet({ kind: "rx", origin: "RFSIMC", dest: "RFSIMA" }, nodes),
      relay: inet({ kind: "relay", origin: "RFSIMB", dest: "RFSIMA" }, nodes),
      pathRf: inet({ kind: "tx", origin: "RFSIMB", dest: "RFSIMA", path: "rf" }, nodes),
      pathInet: inet({ kind: "tx", origin: "RFSIMA", dest: "RFSIMB", path: "inet" }, nodes),
      gateway: inet({ kind: "tx", origin: "RFSIMD", dest: "RFSIMA" }, nodes),
    };
  });
  expect(style.radioTx).toBe(false);
  expect(style.inetTx).toBe(true);
  expect(style.plusRx).toBe(false);
  expect(style.relay).toBe(false);
  expect(style.pathRf).toBe(false);
  expect(style.pathInet).toBe(true);
  expect(style.gateway).toBe(true);
});

test("bulletin and group traffic do not draw arcs to every station", async ({ page }) => {
  await page.goto("/");
  const links = await page.evaluate(() => {
    const pins = ["~RFSIMA", "M7TJF", "TF101", "~QATST10", "~NHTEST"];
    const link = window.WCR.stationLinkCall;
    return {
      bulletin: link("BULLETIN", pins),
      hashBulletin: link("#bulletin", pins),
      beacon: link("BEACON", pins),
      group: link("QARP", pins),
      hashGroup: link("#qarp", pins),
      direct: link("M7TJF", pins),
      absent: link("G0ZZZ", pins),
      self: link("~RFSIMA", pins),
    };
  });
  expect(links.bulletin).toBeNull();
  expect(links.hashBulletin).toBeNull();
  expect(links.beacon).toBeNull();
  expect(links.group).toBeNull();
  expect(links.hashGroup).toBeNull();
  expect(links.direct).toBe("M7TJF");
  expect(links.absent).toBeNull();
  expect(links.self).toBe("~RFSIMA");
});

test("lines pulse while live, stay faint through 48h, then go", async ({ page }) => {
  await page.goto("/");
  const fade = await page.evaluate(() => {
    const live = window.WCR.LIVE_MS;
    const hold = window.WCR.HOLD_MS;
    const alpha = window.WCR.linkAlpha;
    const hour = 60 * 60 * 1000;
    const early = alpha(1_000, 0);
    const lateLive = alpha(live - 1, 600);
    const settled = alpha(live + 1, 0);
    const at47 = alpha(47 * hour, 0);
    const at48 = alpha(hold, 0);
    const after = alpha(hold + 1, 0);
    return { early, lateLive, settled, at47, at48, after, live, hold };
  });
  expect(fade.early).toBeGreaterThanOrEqual(0.55);
  expect(fade.early).toBeLessThanOrEqual(0.95);
  expect(fade.lateLive).toBeGreaterThanOrEqual(0.55);
  expect(fade.lateLive).toBeLessThanOrEqual(0.95);
  expect(fade.settled).toBeLessThan(0.55);
  expect(fade.settled).toBeGreaterThan(0.5);
  expect(fade.at47).toBeGreaterThan(0.12);
  expect(fade.at47).toBeLessThan(0.25);
  expect(fade.at48).toBeGreaterThan(0.11);
  expect(fade.at48).toBeLessThanOrEqual(0.13);
  expect(fade.after).toBe(0);
});

test("station pins dim with age and the sidebar lists who is reporting first", async ({ page }) => {
  const now = Math.floor(Date.now() / 1000);
  await page.route("**/api/v1/**", async (route) => {
    const url = route.request().url();
    if (url.includes("/api/v1/nodes") && /[?&]since=/.test(url)) {
      await route.fulfill({ json: { since: 1, trail: [] } });
      return;
    }
    if (url.includes("/api/v1/nodes")) {
      await route.fulfill({
        json: {
          nodes: [
            {
              callsign: "G4OLD",
              lat: 51.5,
              lon: -0.12,
              mode: "radio",
              band: "40m",
              last_seen: now - 20 * 3600,
            },
            {
              callsign: "G4NEW",
              lat: 52.2,
              lon: 0.12,
              mode: "radio-plus",
              band: "2m",
              last_seen: now - 60,
            },
            {
              callsign: "G4GONE",
              lat: 53.0,
              lon: -1.5,
              mode: "internet",
              band: "inet",
              last_seen: now - 49 * 3600,
            },
          ],
        },
      });
      return;
    }
    if (url.includes("/links")) {
      await route.fulfill({ json: { links: [], since: now - 48 * 3600 } });
      return;
    }
    if (url.includes("/events")) {
      await route.fulfill({ json: { events: [] } });
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
    await route.fulfill({ json: {} });
  });

  await page.goto("/");
  const rows = page.locator("#station-list .station");
  await expect(rows).toHaveCount(2, { timeout: 15_000 });
  await expect(rows.nth(0)).toContainText("G4NEW");
  await expect(rows.nth(0).locator(".heard")).toHaveText(/last heard (just now|1m ago)/);
  await expect(rows.nth(1)).toContainText("G4OLD");
  await expect(rows.nth(1).locator(".heard")).toHaveText("last heard 20h ago");
  await expect(page.locator(".station", { hasText: "G4GONE" })).toHaveCount(0);
  await expect(page.locator(".map-mark-wrap")).toHaveCount(2);

  const pins = await page.evaluate(() => {
    const wrap = (call: string) => {
      const el = [...document.querySelectorAll(".map-mark-wrap")].find((node) =>
        (node.getAttribute("aria-label") || "").startsWith(call),
      ) as HTMLElement | undefined;
      const age = el?.querySelector(".map-mark-age") as HTMLElement | undefined;
      return el && age
        ? { opacity: Number(age.style.opacity), title: el.title }
        : null;
    };
    return { fresh: wrap("G4NEW"), stale: wrap("G4OLD") };
  });
  expect(pins.fresh).not.toBeNull();
  expect(pins.stale).not.toBeNull();
  expect(pins.fresh!.opacity).toBe(1);
  expect(pins.fresh!.title).toMatch(/last heard (just now|1m ago)/);
  expect(pins.stale!.opacity).toBeLessThan(0.85);
  expect(pins.stale!.opacity).toBeGreaterThan(0.4);
  expect(pins.stale!.title).toBe("last heard 20h ago");

  await dismissBoot(page);
  await page.evaluate(() => {
    window.WCR.map.jumpTo({ center: [-0.02, 51.85], zoom: 7 });
  });
  await page.locator("#station-list").screenshot({
    path: "/opt/cursor/artifacts/map-stations-48h.png",
  });
  await page.locator("#map").screenshot({
    path: "/opt/cursor/artifacts/map-pins-48h.png",
  });
});

test("a live line is kept when the events cache has not caught up", async ({ page }) => {
  await page.route("**/api/v1/**", async (route) => {
    const url = route.request().url();
    if (url.includes("/api/v1/links")) {
      await route.fulfill({ json: { links: [], since: 1 } });
      return;
    }
    if (url.includes("/api/v1/nodes") && /[?&]since=/.test(url)) {
      await route.fulfill({ json: { since: 1, trail: [] } });
      return;
    }
    if (url.includes("/api/v1/nodes")) {
      await route.fulfill({
        json: {
          nodes: [
            { callsign: "G4AAA", lat: 51.5, lon: -0.12, mode: "radio", band: "40m", last_seen: Math.floor(Date.now() / 1000) },
            { callsign: "G4BBB", lat: 52.2, lon: 0.12, mode: "radio-plus", band: "2m", last_seen: Math.floor(Date.now() / 1000) },
          ],
        },
      });
      return;
    }
    if (url.includes("/events")) {
      await route.fulfill({ json: { events: [] } });
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
    await route.fulfill({ json: {} });
  });

  await page.goto("/");
  await expect(page.locator(".map-mark-wrap")).toHaveCount(2, { timeout: 15_000 });
  const kept = await page.evaluate(() => {
    const at = Date.now() - 60_000;
    window.WCR.rememberLink("G4AAA", "G4BBB", false, false, false, at);
    const before = window.WCR.linkSnapshot("G4AAA", "G4BBB");
    window.WCR.seedLinksFromEvents([]);
    const afterEmpty = window.WCR.linkSnapshot("G4AAA", "G4BBB");
    const hour = 60 * 60 * 1000;
    const calm = window.WCR.linkAlpha(1_000, 0, true);
    const faint = window.WCR.linkAlpha(47 * hour, 0, true);
    const gone = window.WCR.linkAlpha(window.WCR.HOLD_MS + 1, 0, true);
    return { before, afterEmpty, calm, faint, gone };
  });
  expect(kept.before).not.toBeNull();
  expect(kept.afterEmpty).toEqual(kept.before);
  expect(kept.calm).toBeCloseTo(0.55, 2);
  expect(kept.faint).toBeGreaterThan(0.12);
  expect(kept.faint).toBeLessThan(0.25);
  expect(kept.gone).toBe(0);
});
