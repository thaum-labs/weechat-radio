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
      linkAppearance(
        ageMs: number,
        nowMs: number,
        reduceMotion?: boolean,
      ): { alpha: number; pulse: boolean; expired: boolean };
      linkPaint(link: { mail?: boolean; inet?: boolean }): { rgb: string; dash: number[] };
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
      LINK_MS: number;
      ACTIVE_MS: number;
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

test("a contact pulses, then fades across 10 minutes, and a new one restarts it", async ({ page }) => {
  await page.goto("/");
  const fade = await page.evaluate(() => {
    const look = window.WCR.linkAppearance;
    const active = window.WCR.ACTIVE_MS;
    const life = window.WCR.LINK_MS;
    const paint = window.WCR.linkPaint;
    return {
      active,
      life,
      fresh: look(0, 0, false),
      stillLive: look(active - 1, 0, false),
      settled: look(active, 0, false),
      mid: look(life / 2, 0, false),
      late: look(life - 30_000, 0, false),
      done: look(life + 1, 0, false),
      calm: look(0, 0, true),
      calmMid: look(life / 2, 0, true),
      rf: paint({}),
      inet: paint({ inet: true }),
      mail: paint({ mail: true }),
    };
  });
  expect(fade.life).toBe(10 * 60 * 1000);
  expect(fade.fresh.pulse).toBe(true);
  expect(fade.fresh.alpha).toBeGreaterThanOrEqual(0.55);
  expect(fade.fresh.alpha).toBeLessThanOrEqual(0.95);
  expect(fade.stillLive.pulse).toBe(true);
  expect(fade.settled.pulse).toBe(false);
  expect(fade.settled.alpha).toBeCloseTo(0.55, 2);
  expect(fade.mid.pulse).toBe(false);
  expect(fade.mid.alpha).toBeLessThan(fade.settled.alpha);
  expect(fade.mid.alpha).toBeGreaterThan(0.15);
  expect(fade.late.alpha).toBeLessThan(fade.mid.alpha);
  expect(fade.late.alpha).toBeGreaterThan(0);
  expect(fade.done.expired).toBe(true);
  expect(fade.done.alpha).toBe(0);
  expect(fade.calm.pulse).toBe(false);
  expect(fade.calm.alpha).toBeCloseTo(0.55, 2);
  expect(fade.calmMid.alpha).toBeLessThan(fade.calm.alpha);
  expect(fade.rf).toEqual({ rgb: "57,255,20", dash: [6, 4] });
  expect(fade.inet).toEqual({ rgb: "125,155,255", dash: [] });
  expect(fade.mail).toEqual({ rgb: "255,122,61", dash: [3, 3] });

  await page.emulateMedia({ reducedMotion: "reduce" });
  const fromPage = await page.evaluate(() => window.WCR.linkAppearance(0, 0));
  expect(fromPage.pulse).toBe(false);
  expect(fromPage.alpha).toBeCloseTo(0.55, 2);
});

test("a live line survives a refresh that has not caught the event yet", async ({ page }, testInfo) => {
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
            { callsign: "G4AAA", lat: 51.5, lon: -0.12, mode: "radio", band: "40m" },
            { callsign: "G4BBB", lat: 52.2, lon: 0.12, mode: "radio-plus", band: "2m" },
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
    window.WCR.seedLinksFromEvents([
      {
        ts: Math.floor((at - 30_000) / 1000),
        kind: "tx",
        origin: "G4AAA",
        dest: "G4BBB",
        path: "rf",
      },
    ]);
    const afterOlder = window.WCR.linkSnapshot("G4AAA", "G4BBB");
    const newer = at + 20_000;
    window.WCR.seedLinksFromEvents([
      {
        ts: Math.floor(newer / 1000),
        kind: "rx",
        origin: "G4BBB",
        dest: "G4AAA",
        path: "inet",
      },
    ]);
    const afterNewer = window.WCR.linkSnapshot("G4AAA", "G4BBB");
    return {
      before,
      afterEmpty,
      afterOlder,
      afterNewer,
      newerAt: Math.floor(newer / 1000) * 1000,
    };
  });
  expect(kept.before).not.toBeNull();
  expect(kept.afterEmpty).toEqual(kept.before);
  expect(kept.afterOlder).toEqual(kept.before);
  expect(kept.afterNewer!.at).toBe(kept.newerAt);
  expect(kept.afterNewer!.inet).toBe(true);
  expect(kept.afterNewer!.at).toBeGreaterThan(kept.before!.at);

  await page.evaluate(() => {
    const at = Date.now() - 60_000;
    window.WCR.rememberLink("G4AAA", "G4BBB", false, true, false, at);
    window.WCR.map.jumpTo({ center: [-0.02, 51.85], zoom: 7 });
  });
  const boot = page.locator("#boot_screen");
  if (await boot.isVisible()) await boot.click();
  await page.locator("#map").screenshot({
    path: testInfo.outputPath("map-link-fade.png"),
  });
  await page.locator("#map").screenshot({
    path: "/opt/cursor/artifacts/map-link-fade.png",
  });
});
