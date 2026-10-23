import { expect, test } from "@playwright/test";

const PAGE = "/docs/band-plan.html";

test("band plan lists every band with its spots", async ({ page }) => {
  await page.goto(PAGE);
  await expect(page.locator(".bp-band")).toHaveCount(20);
  const twoM = page.locator("#band-2m");
  await expect(twoM.locator(".bp-spots li", { hasText: "144.950" })).toBeVisible();
  await expect(twoM.locator(".bp-spots li.bp-avoid")).toHaveCount(2);
});

test("region filter hides spots and bands with nothing for that region", async ({ page }) => {
  await page.goto(PAGE);
  await page.getByRole("button", { name: "UK & Europe" }).click();
  await expect(page.locator("#band-murs")).toBeHidden();
  await expect(page.locator("#band-pmr446")).toBeVisible();
  await expect(page.locator("#band-40m .bp-spots li", { hasText: "7.090" })).toBeHidden();
  await expect(page.locator("#band-20m .bp-spots li", { hasText: "14.070" })).toBeVisible();

  await page.getByRole("button", { name: "US & Canada" }).click();
  await expect(page.locator("#band-murs")).toBeVisible();
  await expect(page.locator("#band-pmr446")).toBeHidden();
  await expect(page.getByRole("button", { name: "US & Canada" })).toHaveAttribute("aria-pressed", "true");
});

test("every band says why its spots sit where they do, and CB has a reason per country", async ({ page }) => {
  await page.goto(PAGE);
  await expect(page.getByRole("heading", { name: "How the spots were picked" })).toBeVisible();
  const bands = page.locator(".bp-band");
  const n = await bands.count();
  for (let i = 0; i < n; i++) await expect(bands.nth(i).locator(".bp-why").last()).not.toBeEmpty();
  await expect(page.locator("#band-cb .bp-why--region")).toHaveCount(6);
  await expect(page.locator("#band-23cm .bp-why")).toContainText("beacons only");
});
