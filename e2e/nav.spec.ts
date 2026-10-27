import { expect, test } from "@playwright/test";

// #177: one top bar, in one order, on every page. The markup is copied into
// each page (so it works without JS); this keeps the copies the same.

const NAV = [
  ["why", "/why.html"],
  ["guides", "/guides.html"],
  ["plan", "/guides/band.html"],
  ["map", "/map.html"],
  ["docs", "/docs.html"],
  ["hub", "/status.html"],
  ["changelog", "/changelog.html"],
  ["github", "https://github.com/thaum-labs/weechat-radio"],
  ["download", "/downloads.html"],
];

// Page, and the item marked as the current page (null on home).
const PAGES: [string, string | null][] = [
  ["/", null],
  ["/why.html", "why"],
  ["/map.html", "map"],
  ["/downloads.html", "download"],
  ["/status.html", "hub"],
  ["/changelog.html", "changelog"],
  ["/guides.html", "guides"],
  ["/docs.html", "docs"],
  ["/guides/band.html", "plan"],
  ["/docs/api.html", "docs"],
  ["/docs/apps.html", "docs"],
  ["/docs/bridging.html", "docs"],
  ["/docs/compare.html", "docs"],
  ["/docs/deploy.html", "docs"],
  ["/docs/email.html", "docs"],
  ["/docs/hops.html", "docs"],
  ["/docs/mixed-band.html", "docs"],
  ["/docs/modes.html", "docs"],
  ["/docs/protocol.html", "docs"],
  ["/docs/scenarios.html", "docs"],
  ["/docs/try.html", "guides"],
  ["/docs/weak.html", "docs"],
  // Docs pages that live in Guides › Plan a contact mark guides, like their breadcrumb.
  ["/docs/calling.html", "guides"],
  ["/docs/band-plan.html", "guides"],
  ["/guides/antennas-hf.html", "guides"],
  ["/guides/antennas-vhf.html", "guides"],
  ["/guides/handheld.html", "guides"],
  ["/guides/hf.html", "guides"],
  ["/guides/linux.html", "guides"],
  ["/guides/macos.html", "guides"],
  ["/guides/radio.html", "guides"],
  ["/guides/radios.html", "guides"],
  ["/guides/start.html", "guides"],
  ["/guides/troubleshooting.html", "guides"],
  ["/guides/vox.html", "guides"],
  ["/guides/vr-n76.html", "guides"],
  ["/guides/weechat.html", "guides"],
  ["/guides/windows.html", "guides"],
];

test("every page has the same top bar, with one current item", async ({ page }) => {
  await page.route(/unpkg|arcgisonline|cartocdn|7380|7373|open-meteo|kc2g|github\.com\/repos/, (r) => r.abort());
  for (const [path, current] of PAGES) {
    await page.goto(path);
    const links = page.locator("header.topbar nav a");
    const got = await links.evaluateAll((as) => as.map((a) => [a.textContent!.trim(), a.getAttribute("href")]));
    expect(got, path).toEqual(NAV);
    const marked = await page.locator('header.topbar nav a[aria-current="page"]').allTextContents();
    expect(marked.map((t) => t.trim()), path).toEqual(current ? [current] : []);
    await expect(page.locator("header.topbar nav a.on"), path).toHaveCount(current ? 1 : 0);
  }
});

test("the top bar fits one line at 1280 and download stands out", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  for (const path of ["/", "/map.html", "/guides/band.html", "/guides/antennas-hf.html"]) {
    await page.goto(path);
    const tops = await page.locator("header.topbar nav a").evaluateAll((as) => as.map((a) => Math.round(a.getBoundingClientRect().top + a.getBoundingClientRect().height / 2)));
    expect(Math.max(...tops) - Math.min(...tops), `${path} nav on one line`).toBeLessThanOrEqual(2);
    const scroll = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(scroll, path).toBeLessThanOrEqual(0);
  }
  const cta = page.locator("header.topbar nav a.nav-cta");
  await expect(cta).toHaveCSS("background-color", "rgb(125, 155, 255)");
  await expect(page.getByRole("link", { name: "WeeChat Radio on GitHub" })).toHaveCount(1);
});

for (const path of ["/", "/guides.html"]) {
  test(`the phone menu on ${path} lists the same items, download last`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(path);
    const toggle = page.locator(".nav-toggle");
    await toggle.click();
    const links = page.locator("header.topbar nav a");
    await expect(links.first()).toBeVisible();
    expect(await links.allTextContents().then((t) => t.map((s) => s.trim()))).toEqual(NAV.map(([label]) => label));
    const nav = await page.locator("header.topbar nav").boundingBox();
    const cta = await page.locator("header.topbar nav a.nav-cta").boundingBox();
    expect(cta!.width).toBeGreaterThan(nav!.width - 40);
    await page.keyboard.press("Escape");
    await expect(links.first()).toBeHidden();
  });
}
