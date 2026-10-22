import { expect, test } from "@playwright/test";

const PAGE = "/docs/try.html";

test("try page plays the sound path once the diagram is on screen", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(PAGE);

  const svg = page.locator("[data-live-dgm] svg");
  await svg.scrollIntoViewIfNeeded();
  await expect(svg.locator('[data-seg="a-out"]').first()).toHaveClass(/is-live/);
  await expect(page.locator("[data-live-status]")).toContainText("Computer A plays");
  await expect(svg.locator('[data-node="radio-a"]')).toHaveClass(/is-on/, { timeout: 5_000 });
  expect(errors).toEqual([]);
});

test("setup switch shows the chosen setup's picture and text", async ({ page }) => {
  await page.goto(PAGE);
  const fig = page.locator("[data-live-dgm]");

  await page.getByRole("button", { name: "no radio", exact: true }).click();
  await expect(fig).toHaveAttribute("data-setup", "noradio");
  await expect(page.getByRole("button", { name: "no radio", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("heading", { name: "With no radio at all" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "With two walkie-talkies" })).toBeHidden();
  await expect(fig.locator('[data-seg="air-ab"]')).toBeVisible();

  await page.getByRole("button", { name: "radios, no cable" }).click();
  await expect(page.getByRole("heading", { name: "No cable" })).toBeVisible();
  await expect(fig.locator('[data-node="radio-a"]')).toBeVisible();
});

test("reduced motion shows the finished exchange without animating", async ({ browser }) => {
  const ctx = await browser.newContext({ reducedMotion: "reduce" });
  const page = await ctx.newPage();
  await page.goto(PAGE);
  await expect(page.locator('[data-show="a-ok"]')).toHaveClass(/is-shown/);
  await expect(page.locator("[data-live-status]")).toContainText("It worked both ways");
  await expect(page.locator("[data-live-dgm] svg .seg.is-live")).toHaveCount(0);
  await ctx.close();
});

test("try diagram fits a phone screen without sideways scrolling", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 375, height: 812 } });
  const page = await ctx.newPage();
  await page.goto(PAGE);
  const fits = await page.locator("[data-live-dgm] .dgm-wrap").evaluate((el) => el.scrollWidth <= el.clientWidth + 1);
  expect(fits).toBe(true);
  await ctx.close();
});
