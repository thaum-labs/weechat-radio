import { expect, test } from "@playwright/test";

// Every docs and guides page: loads cleanly, fits a phone, its live diagrams finish
// in reduced motion, and every internal link resolves.
const PAGES = [
  "/docs.html",
  "/guides.html",
  "/docs/api.html",
  "/docs/band-plan.html",
  "/docs/apps.html",
  "/docs/bridging.html",
  "/docs/calling.html",
  "/docs/compare.html",
  "/docs/deploy.html",
  "/docs/email.html",
  "/docs/hops.html",
  "/docs/mixed-band.html",
  "/docs/modes.html",
  "/docs/protocol.html",
  "/docs/scenarios.html",
  "/docs/try.html",
  "/docs/weak.html",
  "/guides/handheld.html",
  "/guides/hf.html",
  "/guides/linux.html",
  "/guides/macos.html",
  "/guides/radio.html",
  "/guides/start.html",
  "/guides/troubleshooting.html",
  "/guides/vox.html",
  "/guides/vr-n76.html",
  "/guides/weechat.html",
  "/guides/windows.html",
];

for (const path of PAGES) {
  test(`${path} loads cleanly and fits a phone`, async ({ browser }) => {
    const ctx = await browser.newContext({ viewport: { width: 375, height: 812 }, reducedMotion: "reduce" });
    const page = await ctx.newPage();
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text());
    });
    await page.goto(path);
    await expect(page.locator("main h1")).toBeVisible();

    // The page scrolls inside .page-wrap; only diagrams and code may scroll sideways, in their own box.
    const overflow = await page.evaluate(() => {
      const wrap = document.querySelector(".page-wrap") ?? document.documentElement;
      return wrap.scrollWidth - wrap.clientWidth;
    });
    expect(overflow, "sideways scroll at phone width").toBeLessThanOrEqual(1);

    // Live diagrams: valid step data, and the finished state has a status line.
    const live = page.locator("[data-live-dgm]");
    for (let i = 0; i < (await live.count()); i++) {
      const fig = live.nth(i);
      await expect(fig.locator("[data-live-status]")).not.toBeEmpty();
      await expect(fig.locator("svg .seg.is-live")).toHaveCount(0);
      const fits = await fig.locator(".dgm-wrap").first().evaluate((el) => el.scrollWidth <= el.clientWidth + 1);
      expect(fits, "live diagram fits a phone").toBe(true);
    }

    expect(errors).toEqual([]);
    await ctx.close();
  });
}

test("internal links on docs and guides pages resolve", async ({ page, request }) => {
  const seen = new Set<string>();
  const broken: string[] = [];
  for (const path of PAGES) {
    await page.goto(path);
    const hrefs = await page.$$eval("main a[href]", (as) => as.map((a) => a.getAttribute("href") ?? ""));
    for (const href of hrefs) {
      if (!href.startsWith("/") || href.startsWith("//")) continue;
      const [url, hash] = href.split("#");
      const key = url || path;
      if (!seen.has(key)) {
        seen.add(key);
        const res = await request.get(key);
        if (!res.ok()) broken.push(`${path} → ${href} (${res.status()})`);
      }
      if (hash) {
        await page.goto(key);
        const found = await page.locator(`[id="${hash}"], a[name="${hash}"]`).count();
        if (!found) broken.push(`${path} → ${href} (no #${hash})`);
        await page.goto(path);
      }
    }
  }
  expect(broken).toEqual([]);
});
