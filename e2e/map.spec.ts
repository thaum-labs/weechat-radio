import { test, expect } from "@playwright/test";

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
