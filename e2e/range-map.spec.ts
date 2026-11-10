import { expect, test, type Page, type Route } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

// Range test map (guides/range.html): a /rangetest CSV dropped in the
// browser becomes dots plus a coverage shading. Tiles are blocked; set
// MAPLIBRE_DIR to an unpacked maplibre-gl package to run without unpkg.com.

declare global {
  interface Window {
    wcrRange: {
      parseCsv(text: string): { probes: unknown[]; unplaced: number; far: string; farGrid: string };
      score(p: { result: string; they: number; we: number }): number;
      gridCentre(g: string): { lat: number; lon: number } | null;
      coverage(p: unknown[]): { properties: { band: string } }[];
      replaying(): { shown: number; total: number } | null;
      sliceReach(p: unknown[], home: { lat: number; lon: number }): { reach: number; probes: number }[];
      coverageArea(p: unknown[], home: { lat: number; lon: number }): { features: unknown[]; km2: number; measured: number };
    };
  }
}

const HEADER = "time_utc,lat,lon,probe,result,snr_they_heard_db,snr_we_heard_db,far_station,far_grid";

/** A drive east from home: good, then patchy, then nothing; one row without a fix. */
function drive(): string {
  const rows = [HEADER];
  for (let i = 0; i < 30; i++) {
    const lon = (-0.2 + i * 0.01).toFixed(6);
    const result = i < 12 ? "both" : i < 18 ? "heard_them" : "none";
    const they = result === "both" ? String(10 - i) : "";
    const we = result === "none" ? "" : String(12 - i);
    rows.push(`2026-11-10T19:${String(i).padStart(2, "0")}:00Z,51.500000,${lon},${i + 1},${result},${they},${we},G0HOM,${result === "both" ? "IO91VM" : ""}`);
  }
  rows.push("2026-11-10T19:59:00Z,,,31,none,,,G0HOM,");
  return rows.join("\n") + "\n";
}

async function open(page: Page) {
  const dir = process.env.MAPLIBRE_DIR;
  if (dir) {
    await page.route("https://unpkg.com/maplibre-gl@4/**", (route: Route) => {
      const file = path.join(dir, new URL(route.request().url()).pathname.replace(/^\/maplibre-gl@4\//, ""));
      route.fulfill({ body: fs.readFileSync(file), contentType: file.endsWith(".css") ? "text/css" : "text/javascript" });
    });
  }
  await page.route(/arcgisonline|cartocdn/, (route) => route.abort());
  await page.goto("/guides/range.html");
}

test("scores, grid centres and coverage squares", async ({ page }) => {
  await open(page);
  const r = await page.evaluate((csv) => {
    const w = window.wcrRange;
    const parsed = w.parseCsv(csv);
    const bands = w.coverage(parsed.probes).map((f) => f.properties.band);
    return {
      n: parsed.probes.length,
      unplaced: parsed.unplaced,
      farGrid: parsed.farGrid,
      strong: w.score({ result: "both", they: 15, we: 20 }),
      edge: w.score({ result: "both", they: -5, we: 3 }),
      oneWay: w.score({ result: "heard_them", they: NaN, we: 2 }),
      none: w.score({ result: "none", they: NaN, we: NaN }),
      home: w.gridCentre("IO91VM"),
      bands: [...new Set(bands)].sort(),
      cells: bands.length,
    };
  }, drive());
  expect(r.n).toBe(30);
  expect(r.unplaced).toBe(1);
  expect(r.farGrid).toBe("IO91VM");
  expect(r.strong).toBe(1);
  expect(r.edge).toBeCloseTo(0.7, 5);
  expect(r.oneWay).toBe(0.4);
  expect(r.none).toBe(0);
  expect(r.home!.lat).toBeCloseTo(51.52, 1);
  expect(r.home!.lon).toBeCloseTo(-0.21, 1);
  expect(r.bands).toEqual(["good", "none", "patchy"]);
  expect(r.cells).toBeGreaterThan(100);
});

test("a dropped log shades the map and says what it found, at phone width", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page);
  await page.setInputFiles("#rt-file", { name: "range-test-G0HOM.csv", mimeType: "text/csv", buffer: Buffer.from(drive()) });
  const summary = page.locator("#rt-summary");
  await expect(summary).toContainText("30 probes on the map: 12 both ways (40%), 6 heard them only, 12 no contact");
  await expect(summary).toContainText("Furthest both ways from G0HOM:");
  await expect(summary).toContainText(/Coverage area about \d+ km², measured in \d+ of 36 directions/);
  await expect(summary).toContainText("1 probes had no GPS fix");
  const overflow = await page.evaluate(() => {
    const wrap = document.querySelector(".page-wrap") ?? document.documentElement;
    return wrap.scrollWidth - wrap.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(1);
});

test("the coverage edge in a slice ignores one lucky far probe", async ({ page }) => {
  await open(page);
  const r = await page.evaluate(() => {
    const w = window.wcrRange;
    const home = { lat: 51.5, lon: 0 };
    // Due north: both ways out to 5 km, then nothing, then one lucky probe at 12 km.
    const probes = [];
    for (let km = 1; km <= 10; km++) {
      probes.push({ lat: 51.5 + km / 111.32, lon: 0, result: km <= 5 ? "both" : "none" });
    }
    probes.push({ lat: 51.5 + 12 / 111.32, lon: 0, result: "both" });
    const slices = w.sliceReach(probes, home);
    const area = w.coverageArea(probes, home);
    return { north: slices[0], east: slices[9], km2: area.km2, measured: area.measured, n: area.features.length };
  });
  expect(r.north.probes).toBe(11);
  expect(r.north.reach / 1000).toBeCloseTo(5, 1);
  expect(r.east).toEqual({ reach: 0, probes: 0 });
  expect(r.measured).toBe(1);
  expect(r.km2).toBeCloseTo((Math.PI * 25) / 36, 1);
  expect(r.n).toBe(1);
});

test("replay plays the drive back in order, pauses, and ends with everything drawn", async ({ page }) => {
  await open(page);
  const btn = page.locator("#rt-replay");
  await expect(btn).toBeDisabled();
  await page.setInputFiles("#rt-file", { name: "range-test-G0HOM.csv", mimeType: "text/csv", buffer: Buffer.from(drive()) });
  await expect(btn).toBeEnabled();
  // 120x: 30 probes 60 s apart take about 15 s, so a pause lands mid-drive.
  await page.selectOption("#rt-speed", "120");
  await btn.click();
  await expect(btn).toHaveText("❚❚ pause");
  await expect.poll(async () => (await page.evaluate(() => window.wcrRange.replaying()))?.shown ?? 0).toBeGreaterThan(1);
  await btn.click();
  await expect(btn).toHaveText("▶ resume");
  const paused = (await page.evaluate(() => window.wcrRange.replaying()))!;
  expect(paused.shown).toBeLessThan(paused.total);
  await page.waitForTimeout(600);
  expect((await page.evaluate(() => window.wcrRange.replaying()))!.shown).toBe(paused.shown);
  await expect(page.locator("#rt-clock")).toContainText(`probe ${paused.shown} of 30`);
  await page.selectOption("#rt-speed", "2000");
  await btn.click();
  await expect(page.locator("#rt-clock")).toContainText("probe 30 of 30 · done", { timeout: 10_000 });
  await expect(btn).toHaveText("▶ replay drive");
  expect(await page.evaluate(() => window.wcrRange.replaying())).toBeNull();
});

test("a file that is not a range test log is named and skipped", async ({ page }) => {
  await open(page);
  await page.setInputFiles("#rt-file", { name: "notes.csv", mimeType: "text/csv", buffer: Buffer.from("a,b\n1,2\n") });
  await expect(page.locator("#rt-summary")).toContainText("Skipped notes.csv: not a range test CSV");
});
