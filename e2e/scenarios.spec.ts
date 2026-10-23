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
