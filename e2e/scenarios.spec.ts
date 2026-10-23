import { expect, test } from "@playwright/test";

const PAGE = "/docs/scenarios.html";

test("desktop: the wide diagram spans the column and the text sits under it", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await ctx.newPage();
  await page.goto(PAGE);

  const wide = page.locator('svg.dgm[data-layout="wide"]');
  await expect(wide).toBeVisible();
  await expect(page.locator('svg.dgm[data-layout="narrow"]')).toBeHidden();

  const pic = await page.locator(".live-dgm__pic").boundingBox();
  const panel = await page.locator('[data-setup-panel="home"]').boundingBox();
  expect(panel!.y).toBeGreaterThan(pic!.y + pic!.height);
  expect(pic!.width).toBeGreaterThan(700);

  await wide.scrollIntoViewIfNeeded();
  await expect(wide.locator(".seg.is-live").first()).toBeAttached({ timeout: 5_000 });

  await page.getByRole("button", { name: "email", exact: true }).click();
  await expect(wide).toHaveAttribute("viewBox", /^0 64 800 /);
  await expect(page.getByRole("heading", { name: "Email over radio" })).toBeVisible();
  await ctx.close();
});

test("phone: the narrow diagram shows instead", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 375, height: 812 } });
  const page = await ctx.newPage();
  await page.goto(PAGE);
  await expect(page.locator('svg.dgm[data-layout="narrow"]')).toBeVisible();
  await expect(page.locator('svg.dgm[data-layout="wide"]')).toBeHidden();
  await ctx.close();
});

test("the app window fills in with the chat as the diagram plays", async ({ page }) => {
  await page.goto(PAGE);
  const win = page.locator('.app-win[data-setups="home"]');
  await expect(win).toBeVisible();
  await expect(page.locator('.app-win[data-setups="hf"]')).toBeHidden();

  await page.locator("[data-live-dgm] .dgm-wrap").scrollIntoViewIfNeeded();
  const mine = win.locator("li", { hasText: "hi #walk" });
  await expect(mine).toBeVisible();
  await expect(mine.locator(".ticks")).toHaveText("[tx]", { useInnerText: true });
  await expect(mine.locator(".ticks")).toHaveText("[gw]", { timeout: 6_000, useInnerText: true });
  await expect(win.locator("li", { hasText: "hello" })).toBeVisible({ timeout: 12_000 });

  await page.getByRole("button", { name: "relay with hops" }).click();
  await expect(page.locator('.app-win[data-setups="hops"]')).toBeVisible();
  await expect(win).toBeHidden();
});

test("reduced motion: the app window shows the finished chat with the last mark", async ({ browser }) => {
  const ctx = await browser.newContext({ reducedMotion: "reduce" });
  const page = await ctx.newPage();
  await page.goto(PAGE);
  const ticks = page.locator('.app-win[data-setups="home"] li', { hasText: "hi #walk" }).locator(".ticks");
  await expect(ticks).toHaveText("[gw]", { useInnerText: true });
  await ctx.close();
});
