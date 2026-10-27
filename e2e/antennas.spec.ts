import { expect, test, type Page } from "@playwright/test";

// The antenna calculator must keep one height while a slider moves (#173).
// It is the last block on the page, so any change in its height makes it jump
// on screen: band notes and the search term change length with the values.

const PAGES = ["/guides/antennas-hf.html", "/guides/antennas-vhf.html"];
const SIZES = [
  { width: 1280, height: 800 },
  { width: 390, height: 844 },
  { width: 360, height: 740 },
];

// Step every slider of every type through its whole range, both ways, and
// return the heights seen (one entry per type and slider that changed height).
async function sweepHeights(page: Page) {
  return page.evaluate(() => {
    const calc = document.getElementById("ant-calc")!;
    const bad: string[] = [];
    for (const radio of Array.from(calc.querySelectorAll<HTMLInputElement>('input[name="ant-type"]'))) {
      radio.checked = true;
      radio.dispatchEvent(new Event("change", { bubbles: true }));
      const height = calc.offsetHeight;
      for (const slider of Array.from(calc.querySelectorAll<HTMLInputElement>('input[type="range"]'))) {
        const start = slider.value;
        const min = Number(slider.min);
        const max = Number(slider.max);
        const step = Number(slider.step);
        const values: number[] = [];
        for (let v = min; v <= max + step / 2; v += step) values.push(v);
        const seen = new Set<number>();
        for (const v of [...values, ...values.reverse()]) {
          slider.value = String(v);
          slider.dispatchEvent(new Event("input", { bubbles: true }));
          seen.add(calc.offsetHeight);
        }
        slider.value = start;
        slider.dispatchEvent(new Event("input", { bubbles: true }));
        if (seen.size > 1 || !seen.has(height)) bad.push(`${radio.value} ${slider.dataset.key}: ${[...seen].join(", ")} (start ${height})`);
      }
    }
    return bad;
  });
}

for (const url of PAGES) {
  for (const size of SIZES) {
    test(`${url} calculator keeps one height while sliders move at ${size.width}px`, async ({ page }) => {
      await page.setViewportSize(size);
      await page.goto(url);
      await expect(page.locator("#ant-bands li").first()).toBeVisible();
      expect(await sweepHeights(page)).toEqual([]);
    });
  }
}

// The reported case: scrolled to the bottom, EFHW wire 19.6 → 19.1 m by keyboard.
for (const size of [SIZES[0], SIZES[1]]) {
  test(`EFHW calculator does not jump at the bottom of the page at ${size.width}px`, async ({ page }) => {
    await page.setViewportSize(size);
    await page.goto("/guides/antennas-hf.html");
    await page.locator('#ant-calc label:has(input[value="efhw"])').click();
    const slider = page.locator('#ant-calc input[data-key="L"]');
    await slider.evaluate((el: HTMLInputElement) => {
      el.value = "19.6";
      el.dispatchEvent(new Event("input", { bubbles: true }));
    });
    // Scroll whichever element scrolls the page (.page-wrap on desktop, the document on a phone).
    await page.evaluate(() => {
      const wrap = document.querySelector(".page-wrap");
      if (wrap) wrap.scrollTop = wrap.scrollHeight;
      window.scrollTo(0, document.documentElement.scrollHeight);
    });
    await slider.focus();
    const top = () => page.locator("#ant-calc").evaluate((el) => Math.round(el.getBoundingClientRect().top));
    const before = await top();
    for (let i = 0; i < 5; i++) {
      await page.keyboard.press("ArrowLeft");
      expect(await top()).toBe(before);
    }
    await page.keyboard.press("ArrowRight");
    expect(await top()).toBe(before);
  });
}

// Every band note fits its fixed 3 lines at every slider value (#181).
// The full text is also in the tooltip, but nothing should need it.
for (const url of PAGES) {
  for (const width of [1440, 1280, 900, 390, 360]) {
    test(`${url} band notes are never cut off at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 800 });
      await page.goto(url);
      await expect(page.locator("#ant-bands li").first()).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      const cut = await page.evaluate(() => {
        const calc = document.getElementById("ant-calc")!;
        const found = new Set<string>();
        const check = () => {
          // Measure two characters narrower than the chip: fonts render a little
          // wider on some machines, and QA saw cuts that fit exactly here.
          for (const s of Array.from(document.querySelectorAll<HTMLElement>("#ant-bands small"))) {
            s.style.width = `${s.clientWidth - 14}px`;
            if (s.scrollHeight > s.clientHeight) found.add(s.textContent!.replace(/\d+\.\d+/g, "#"));
            s.style.width = "";
          }
        };
        for (const radio of Array.from(calc.querySelectorAll<HTMLInputElement>('input[name="ant-type"]'))) {
          radio.checked = true;
          radio.dispatchEvent(new Event("change", { bubbles: true }));
          for (const slider of Array.from(calc.querySelectorAll<HTMLInputElement>('input[type="range"]'))) {
            const start = slider.value;
            const step = Number(slider.step);
            for (let v = Number(slider.min); v <= Number(slider.max) + step / 2; v += step) {
              slider.value = String(v);
              slider.dispatchEvent(new Event("input", { bubbles: true }));
              check();
            }
            slider.value = start;
            slider.dispatchEvent(new Event("input", { bubbles: true }));
          }
        }
        return [...found];
      });
      expect(cut).toEqual([]);
    });
  }
}
