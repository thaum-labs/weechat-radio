import { expect, test } from "@playwright/test";

const PAGE = "/docs/mixed-band.html";

test("mixed-band thread plays and lights each line's path", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(PAGE);

  const live = page.locator("[data-mixed-live]");
  await live.scrollIntoViewIfNeeded();
  await expect(page.locator(".mb-line").first()).toHaveClass(/is-highlight/);
  await expect(live.locator('[data-seg="rf-pmr"]')).toHaveClass(/is-live/);
  await expect(live.locator('[data-seg="tls-pmr"]')).toHaveClass(/is-live/);
  await expect(page.getByRole("button", { name: "Pause", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test("clicking a thread line shows its path and pauses", async ({ page }) => {
  await page.goto(PAGE);
  await page.locator("[data-mixed-live]").scrollIntoViewIfNeeded();
  await expect(page.locator(".mb-line").nth(1)).toHaveClass(/is-visible/, { timeout: 10_000 });
  await page.locator(".mb-line__btn").nth(1).click();

  const live = page.locator("[data-mixed-live]");
  await expect(live).toHaveClass(/has-route/);
  await expect(live.locator('[data-seg="tls-sarah"]')).toHaveClass(/is-live/);
  await expect(live.locator('[data-seg="rf-pmr"]')).not.toHaveClass(/is-live/);
  await expect(page.getByRole("button", { name: "Play", exact: true })).toBeVisible();
});

test("reduced motion shows the whole thread without autoplay", async ({ browser }) => {
  const ctx = await browser.newContext({ reducedMotion: "reduce" });
  const page = await ctx.newPage();
  await page.goto(PAGE);
  await expect(page.locator(".mb-line.is-visible")).toHaveCount(5);
  await expect(page.locator(".mb-line.is-highlight")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Play", exact: true })).toBeVisible();

  await page.locator(".mb-line__btn").nth(4).click();
  const live = page.locator("[data-mixed-live]");
  await expect(live).toHaveClass(/is-down/);
  await expect(live.locator('[data-node="g4abc"]')).toHaveClass(/is-live/);
  await ctx.close();
});

test("band diagram fits a phone screen without sideways scrolling", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 375, height: 812 } });
  const page = await ctx.newPage();
  await page.goto(PAGE);
  const wrap = page.locator(".mb-bands-wrap");
  const fits = await wrap.evaluate((el) => el.scrollWidth <= el.clientWidth + 1);
  expect(fits).toBe(true);
  await ctx.close();
});
