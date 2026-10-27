import { expect, test, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";

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
  "/guides/antennas-hf.html",
  "/guides/antennas-vhf.html",
  "/guides/handheld.html",
  "/guides/hf.html",
  "/guides/linux.html",
  "/guides/macos.html",
  "/guides/radio.html",
  "/guides/radios.html",
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

// #171: the two index pages and the shared article list (web/articles.js).
type Article = { href: string; cat: string; title: string; desc: string; keywords: string };
type Category = { id: string; index: string; title: string };
async function articleList(page: Page) {
  return page.evaluate(() => {
    const w = window as unknown as { WCR: { articles: { ARTICLES: Article[]; CATEGORIES: Category[] } } };
    return w.WCR.articles;
  });
}
const WEB = path.join(__dirname, "..", "web");
const FILES = ["guides", "docs"].flatMap((dir) =>
  fs
    .readdirSync(path.join(WEB, dir))
    .filter((f) => f.endsWith(".html"))
    .map((f) => `/${dir}/${f}`)
);
// Site pages that also have a home category (#176).
const HOMED = [...FILES, "/why.html"];

test("every guide and doc has exactly one home category across the two indexes", async ({ page }) => {
  const homes: string[] = [];
  for (const index of ["/guides.html", "/docs.html"]) {
    await page.goto(index);
    const sections = page.locator("section[data-cat]");
    for (let i = 0; i < (await sections.count()); i++) {
      const cards = sections.nth(i).locator("a.guide-card");
      expect(await cards.count(), "no category over 7 cards").toBeLessThanOrEqual(7);
    }
    homes.push(...(await page.$$eval("section[data-cat] a.guide-card", (as) => as.map((a) => a.getAttribute("href") ?? ""))));
    // Only the Start here row repeats, at most 3 cards, all marked featured.
    const start = page.locator(".index-start a.guide-card");
    expect(await start.count()).toBeLessThanOrEqual(3);
    expect(await page.locator(".index-start a.guide-card:not(.featured)").count()).toBe(0);
  }
  expect([...homes].sort()).toEqual([...HOMED].sort());
});

test("the index cards, page titles and h1s match the shared article list", async ({ page }) => {
  await page.goto("/guides.html");
  const { ARTICLES, CATEGORIES } = await articleList(page);
  expect(ARTICLES.map((a) => a.href).sort()).toEqual([...HOMED].sort());
  for (const index of ["guides", "docs"]) {
    await page.goto(`/${index}.html`);
    for (const c of CATEGORIES.filter((c) => c.index === index)) {
      const want = ARTICLES.filter((a) => a.cat === c.id);
      const cards = page.locator(`section[data-cat="${c.id}"] a.guide-card`);
      await expect(cards.locator("h2")).toHaveText(want.map((a) => a.title));
      await expect(cards.locator("p")).toHaveText(want.map((a) => a.desc));
      await expect(page.locator(`.jump-bar a[href="#${c.id}"]`)).toHaveText(c.title);
    }
  }
  for (const a of ARTICLES) {
    expect(a.desc.length, `${a.href} description`).toBeLessThanOrEqual(90);
    const html = fs.readFileSync(path.join(WEB, a.href), "utf8");
    const title = html.match(/<title>(.*?) — WeeChat Radio<\/title>/)?.[1] ?? "";
    expect(title.replace(/&amp;/g, "&"), `${a.href} <title>`).toBe(a.title);
    await page.goto(a.href);
    const h1 = (await page.locator("h1").first().textContent())!.replace("weechat-radio:$", "").trim();
    expect(h1, `${a.href} h1`).toBe(a.title);
  }
});

test("article pages show the breadcrumb and next / previous from the shared list", async ({ page }) => {
  await page.goto("/guides/antennas-hf.html");
  await expect(page.locator(".crumb")).toHaveText("guides › Antennas › HF antennas");
  await expect(page.locator(".crumb a").nth(1)).toHaveAttribute("href", "/guides.html#antennas");
  await expect(page.locator(".article-next")).toHaveText(/Next in Antennas: VHF and UHF antennas →/);
  await page.locator(".article-next a").click();
  await expect(page).toHaveURL(/antennas-vhf(\.html)?$/);
  await expect(page.locator(".article-next")).toHaveText(/← Previous: HF antennas/);
  await page.goto("/why.html");
  await expect(page.locator(".crumb")).toHaveText("docs › How it works › Why WeeChat Radio");
  await expect(page.locator(".article-next")).toHaveText(/Next in How it works: Common setups →/);
});

test("the filter finds both antenna guides, shows docs matches, and Escape clears it", async ({ page }) => {
  await page.goto("/guides.html");
  const input = page.getByLabel("Filter guides and docs");
  await input.fill("antenna");
  const shown = page.locator("section[data-cat] a.guide-card:not([hidden])");
  await expect(shown.locator("h2")).toHaveText(["HF antennas", "VHF and UHF antennas"]);
  await expect(page.locator(".index-filter-status")).toHaveText("2 matches here, 0 in docs.");
  await input.fill("winlink");
  await expect(page.locator(".index-elsewhere")).toBeVisible();
  await expect(page.locator(".index-elsewhere h2")).toHaveText("From docs");
  await expect(page.locator(".index-elsewhere a")).toHaveText(["Compare apps"]);
  await input.fill("why");
  await expect(page.locator(".index-elsewhere a")).toHaveText(["Why WeeChat Radio"]);
  await input.fill("zzzz");
  await expect(page.locator(".index-empty")).toBeVisible();
  await input.press("Escape");
  await expect(input).toHaveValue("");
  await expect(page.locator(".index-empty")).toBeHidden();
  await expect(page.locator(".index-start")).toBeVisible();
  expect(await shown.count()).toBe(await page.locator("section[data-cat] a.guide-card").count());
});

test("a jump chip marks its own category, even a short one near the bottom", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/guides.html");
  const chip = page.locator('.jump-bar a[href="#plan-a-contact"]');
  await chip.click();
  await expect(chip).toHaveAttribute("aria-current", "true");
  await expect(page.locator(".jump-bar a[aria-current]")).toHaveCount(1);
});

test("antennas are the first category on guides, with featured cards (#177)", async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 780 });
  await page.goto("/guides.html");
  await expect(page.locator("section[data-cat]").first()).toHaveAttribute("id", "antennas");
  const chip = page.locator(".jump-bar a").first();
  await expect(chip).toHaveText("Antennas");
  // On screen at 360 without swiping the bar.
  const box = await chip.boundingBox();
  expect(box!.x + box!.width).toBeLessThanOrEqual(360);
  await expect(page.locator("#antennas a.guide-card.featured")).toHaveCount(2);
  await chip.click();
  await expect(page.locator('#antennas a[href="/guides/antennas-hf.html"]')).toBeInViewport();
  await expect(page.locator('#antennas a[href="/guides/antennas-vhf.html"]')).toBeInViewport();
});

test("the radio articles point on to the antenna guides (#177)", async ({ page }) => {
  for (const [slug, first] of [
    ["radio", "hf"], ["hf", "hf"], ["radios", "hf"], ["handheld", "vhf"], ["vox", "vhf"], ["vr-n76", "vhf"],
  ]) {
    await page.goto(`/guides/${slug}.html`);
    const links = page.locator(".next-antenna a");
    await expect(links, slug).toHaveCount(2);
    await expect(links.first(), slug).toHaveAttribute("href", `/guides/antennas-${first}.html`);
  }
});

test("the skip link is the first Tab stop on both indexes", async ({ page }) => {
  for (const [index, name] of [["/guides.html", "guides"], ["/docs.html", "docs"]]) {
    await page.goto(index);
    await page.keyboard.press("Tab");
    await expect(page.locator(":focus")).toHaveText(`Skip to ${name}`);
  }
});

for (const width of [390, 360]) {
  test(`the indexes fit a ${width} px phone: no sideways scroll, a sticky jump bar, under 2400 px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 844 });
    await page.emulateMedia({ reducedMotion: "reduce" });
    for (const index of ["/guides.html", "/docs.html"]) {
      await page.goto(index);
      await expect(page.locator(".index-filter")).toBeVisible();
      const size = await page.evaluate(() => ({
        sideways: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        height: document.documentElement.scrollHeight,
      }));
      expect(size.sideways, `${index} sideways scroll`).toBeLessThanOrEqual(0);
      if (width === 390) expect(size.height, `${index} height`).toBeLessThanOrEqual(2400);
      const cut = await page.$$eval(".section-lead", (ps) => ps.filter((p) => p.scrollWidth > p.clientWidth).map((p) => p.textContent));
      expect(cut, `${index} section intro lines wrap, not cut`).toEqual([]);
      const rows = await page.$$eval("section[data-cat] a.guide-card", (as) => as.map((a) => a.getBoundingClientRect().height));
      expect(Math.max(...rows), `${index} row height`).toBeLessThanOrEqual(72);
      // A jump chip goes to its section and the bar stays under the top bar.
      const last = page.locator(".jump-bar a").last();
      await last.click();
      await expect(last).toHaveAttribute("aria-current", "true");
      const bar = await page.locator(".jump-bar").boundingBox();
      const top = await page.locator(".topbar").boundingBox();
      expect(Math.abs(bar!.y - (top!.y + top!.height))).toBeLessThanOrEqual(2);
    }
  });
}
