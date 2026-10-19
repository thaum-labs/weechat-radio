import { expect, test } from "@playwright/test";

test("homepage links to the map, downloads and docs", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("h1")).toBeVisible();
  const nav = page.getByRole("navigation", { name: "Main" });
  await expect(nav.getByRole("link", { name: "map", exact: true })).toHaveAttribute("href", "/map.html");
  await expect(nav.getByRole("link", { name: "downloads", exact: true })).toHaveAttribute("href", "/downloads.html");
  await expect(nav.getByRole("link", { name: "docs", exact: true })).toHaveAttribute("href", "/docs.html");

  await nav.getByRole("link", { name: "map", exact: true }).click();
  await expect(page.locator("#map")).toBeVisible({ timeout: 30_000 });
});

test("old map links with ?api= open the map", async ({ page }) => {
  await page.goto("/?api=http://127.0.0.1:7373");
  await expect(page).toHaveURL(/\/map(\.html)?(\?|$)/);
  await expect(page.locator("#map")).toBeVisible({ timeout: 30_000 });
});

test("install box switches between Linux/macOS and Windows", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "windows (powershell)" }).click();
  await expect(page.locator("#install-cmd")).toHaveText("irm https://weechatradio.com/install.ps1 | iex");
  await page.getByRole("button", { name: "linux / macos" }).click();
  await expect(page.locator("#install-cmd")).toHaveText("curl -fsSL https://weechatradio.com/install.sh | sh");
});
