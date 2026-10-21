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
      ACTIVE_MS: number;
      HOLD_MS: number;
      MODE_COLOR: Record<string, string>;
      MAIL_COLOR: string;
      LINK_PAINT: Record<string, { color: string; dash: number[] }>;
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
  await page.goto("/map.html");
  const view = await homeCamera(page);
  expectHomeFrame(view);

  await dismissBoot(page);
  await page.locator("#map").screenshot({
    path: testInfo.outputPath("map-home-europe.png"),
  });

  await page.evaluate(() => window.WCR.map.jumpTo({ center: [139.7, 35.7], zoom: 5 }));
  await page.goto("/why.html");
  await page.goto("/map.html");
  const returned = await homeCamera(page);
  expectHomeFrame(returned);
});

test("home map keeps the same places in frame on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/map.html");
  const view = await homeCamera(page);
  expectHomeFrame(view);
  await dismissBoot(page);
  await page.locator("#map").screenshot({
    path: testInfo.outputPath("map-home-europe-phone.png"),
  });
});

test("live map shell loads", async ({ page }) => {
  await page.goto("/map.html");
  await expect(page.locator("#map")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("LIVE MAP")).toBeVisible();
  await expect(page.locator("#hub-panel")).toBeAttached();
});

test("tape speed button cycles 1x, half and quarter speed", async ({ page }) => {
  await page.goto("/map.html");
  await dismissBoot(page);
  const speed = page.locator("#speed");
  await expect(speed).toHaveText("1x");
  await speed.click();
  await expect(speed).toHaveText("½x");
  await speed.click();
  await expect(speed).toHaveText("¼x");
  await speed.click();
  await expect(speed).toHaveText("1x");
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

  await page.goto("/map.html");
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

test("a mobile heard by a tower shows its suffix and the tower it came via", async ({ page }) => {
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
              grid: "IO91WM",
            },
            {
              callsign: "G4ABC/M",
              lat: 51.47,
              lon: -0.3,
              mode: "radio",
              settings: { via: "G4ABC" },
              freq_khz: 144950,
              grid: "IO91VL45",
            },
          ],
        },
      });
      return;
    }
    await route.fulfill({ json: {} });
  });

  await page.goto("/map.html");
  await expect(page.locator(".map-mark-wrap")).toHaveCount(2, { timeout: 15_000 });
  const mobile = page.locator(".map-mark-wrap", { has: page.locator(".kind-tag") });
  await expect(mobile.locator(".kind-tag")).toHaveText("/M");
  await expect(mobile).toHaveAttribute("title", /via G4ABC/);
  await mobile.dispatchEvent("click");
  await expect(page.locator("#card")).toContainText("mobile");
  await expect(page.locator("#card")).toContainText("via");
  await expect(page.locator("#card")).toContainText("IO91VL45");
});

test("radio and radio-plus contacts draw the RF line", async ({ page }) => {
  await page.goto("/map.html");
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
  await page.goto("/map.html");
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

test("lines glow for a few seconds, settle, stay faint through 48h, then go", async ({ page }) => {
  await page.goto("/map.html");
  const fade = await page.evaluate(() => {
    const active = window.WCR.ACTIVE_MS;
    const hold = window.WCR.HOLD_MS;
    const alpha = window.WCR.linkAlpha;
    const hour = 60 * 60 * 1000;
    const glowLow = alpha(1_000, 900);
    const glowHigh = alpha(1_000, 300);
    const settledA = alpha(active + 1, 0);
    const settledB = alpha(active + 1, 900);
    const at47 = alpha(47 * hour, 0);
    const at48 = alpha(hold, 0);
    const after = alpha(hold + 1, 0);
    const calmA = alpha(1_000, 0, true);
    const calmB = alpha(1_000, 300, true);
    return { glowLow, glowHigh, settledA, settledB, at47, at48, after, calmA, calmB, active, hold };
  });
  expect(fade.active).toBe(8_000);
  expect(fade.glowLow).toBeGreaterThanOrEqual(0.55);
  expect(fade.glowLow).toBeLessThanOrEqual(0.6);
  expect(fade.glowHigh).toBeCloseTo(0.95, 5);
  expect(fade.glowHigh).toBeGreaterThan(fade.glowLow);
  expect(fade.settledA).toBeCloseTo(fade.settledB, 5);
  expect(fade.settledA).toBeLessThan(0.55);
  expect(fade.settledA).toBeGreaterThan(0.5);
  expect(fade.at47).toBeGreaterThan(0.12);
  expect(fade.at47).toBeLessThan(0.25);
  expect(fade.at48).toBeGreaterThan(0.11);
  expect(fade.at48).toBeLessThanOrEqual(0.13);
  expect(fade.after).toBe(0);
  expect(fade.calmA).toBeCloseTo(0.55, 2);
  expect(fade.calmB).toBeCloseTo(fade.calmA, 5);
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

  await page.goto("/map.html");
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

test("map key explains line styles, pin colours, and the 48h fade", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/map.html");
  await dismissBoot(page);
  const key = page.locator("#map-key");
  await expect(key).toBeVisible();
  await expect(key).toHaveAttribute("open", "");
  await expect(key.locator(".map-key-note")).toHaveText(
    "Lines glow while live, then fade faintly over 48h.",
  );

  const painted = await page.evaluate(() => {
    const line = (kind: string) => {
      const el = document.querySelector(`#map-key [data-link="${kind}"] line`);
      return {
        stroke: el?.getAttribute("stroke") || "",
        width: el?.getAttribute("stroke-width") || "",
        dash: el?.getAttribute("stroke-dasharray") || "",
      };
    };
    const pin = (mode: string) => {
      const rect = document.querySelector(`#map-key [data-mode="${mode}"] rect`);
      return rect?.getAttribute("fill") || "";
    };
    const map = document.getElementById("map")!.getBoundingClientRect();
    const box = document.getElementById("map-key")!.getBoundingClientRect();
    const keyBody = document.querySelector(".map-key-body") as HTMLElement | null;
    const attrib = document.querySelector(".maplibregl-ctrl-attrib")?.getBoundingClientRect();
    const controls = [...document.querySelectorAll(".maplibregl-ctrl")].map((el) => ({
      cls: el.className,
      box: el.getBoundingClientRect(),
    }));
    const hits = (a: DOMRect, b: DOMRect) =>
      a.width > 0 &&
      b.width > 0 &&
      a.left < b.right &&
      a.right > b.left &&
      a.top < b.bottom &&
      a.bottom > b.top;
    return {
      lines: { rf: line("rf"), inet: line("inet"), mail: line("mail") },
      pins: {
        internet: pin("internet"),
        "internet-radio": pin("internet-radio"),
        radio: pin("radio"),
        "radio-plus": pin("radio-plus"),
      },
      paint: window.WCR.LINK_PAINT,
      modes: window.WCR.MODE_COLOR,
      mail: window.WCR.MAIL_COLOR,
      keyBodyFits:
        keyBody != null &&
        keyBody.scrollWidth <= keyBody.clientWidth + 1 &&
        keyBody.scrollHeight <= keyBody.clientHeight + 1,
      inside:
        box.left >= map.left &&
        box.right <= map.right &&
        box.top >= map.top &&
        box.bottom <= map.bottom,
      leftOfCenter: box.left < map.left + map.width / 2,
      nearBottom: box.bottom > map.top + map.height * 0.55,
      hitsAttrib: attrib ? hits(box, attrib) : false,
      controlHits: controls
        .filter((c) => !String(c.cls).includes("maplibregl-ctrl-attrib") && hits(box, c.box))
        .map((c) => c.cls),
    };
  });

  expect(painted.lines.rf).toEqual({
    stroke: painted.paint.rf.color,
    width: "2.5",
    dash: painted.paint.rf.dash.join(" "),
  });
  expect(painted.lines.inet).toEqual({
    stroke: painted.paint.inet.color,
    width: "2.5",
    dash: "",
  });
  expect(painted.lines.mail).toEqual({
    stroke: painted.paint.mail.color,
    width: "2.5",
    dash: painted.paint.mail.dash.join(" "),
  });
  expect(painted.paint.rf).toEqual({ color: "#39ff14", dash: [6, 4] });
  expect(painted.paint.inet).toEqual({ color: "#7d9bff", dash: [] });
  expect(painted.paint.mail).toEqual({ color: "#ff7a3d", dash: [3, 3] });
  expect(painted.mail).toBe("#ff7a3d");
  expect(painted.pins.internet).toBe(painted.modes.internet);
  expect(painted.pins["internet-radio"]).toBe(painted.modes["internet-radio"]);
  expect(painted.pins.radio).toBe(painted.modes.radio);
  expect(painted.pins["radio-plus"]).toBe(painted.modes["radio-plus"]);
  expect(painted.modes).toEqual({
    internet: "#00e5ff",
    "internet-radio": "#39ff14",
    radio: "#ffbf00",
    "radio-plus": "#ff4dff",
  });
  expect(painted.keyBodyFits).toBe(true);
  expect(painted.inside).toBe(true);
  expect(painted.leftOfCenter).toBe(true);
  expect(painted.nearBottom).toBe(true);
  expect(painted.hitsAttrib).toBe(false);
  expect(painted.controlHits).toEqual([]);

  await page.locator(".map-panel").screenshot({
    path: testInfo.outputPath("map-key-desktop.png"),
  });
  await page.locator(".map-panel").screenshot({
    path: "/opt/cursor/artifacts/map-key-desktop.png",
  });

  await key.locator("summary").focus();
  await expect(key.locator("summary")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(key).not.toHaveAttribute("open", "");
  await page.keyboard.press("Enter");
  await expect(key).toHaveAttribute("open", "");

  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  await dismissBoot(page);
  const phone = page.locator("#map-key");
  await expect(phone).toBeVisible();
  await expect(phone).not.toHaveAttribute("open", "");
  const phoneFit = await page.evaluate(() => {
    const map = document.getElementById("map")!.getBoundingClientRect();
    const box = document.getElementById("map-key")!.getBoundingClientRect();
    const attrib = document.querySelector(".maplibregl-ctrl-attrib")?.getBoundingClientRect();
    const hits = (a: DOMRect, b: DOMRect) =>
      a.width > 0 &&
      b.width > 0 &&
      a.left < b.right &&
      a.right > b.left &&
      a.top < b.bottom &&
      a.bottom > b.top;
    const summary = document.querySelector("#map-key summary")!.getBoundingClientRect();
    return {
      inside: box.left >= map.left - 1 && box.right <= map.right + 1 && box.bottom <= map.bottom + 1,
      hitsAttrib: attrib ? hits(box, attrib) : false,
      summaryTall: summary.height >= 44,
      left: box.left - map.left,
    };
  });
  expect(phoneFit.inside).toBe(true);
  expect(phoneFit.hitsAttrib).toBe(false);
  expect(phoneFit.summaryTall).toBe(true);
  expect(phoneFit.left).toBeLessThan(80);
  await phone.locator("summary").click();
  await expect(phone).toHaveAttribute("open", "");
  await expect(phone.locator(".map-key-note")).toBeVisible();
  await page.locator(".map-panel").screenshot({
    path: "/opt/cursor/artifacts/map-key-phone.png",
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

  await page.goto("/map.html");
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
