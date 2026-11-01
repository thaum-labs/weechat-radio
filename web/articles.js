/* SPDX-License-Identifier: Apache-2.0 */
// Every guide and doc, plus the Why page, in one list (#171, #176). Each article has one home
// category on one index page. The index pages carry the same cards as
// static HTML (an e2e test keeps them in step); this list drives the
// filter, the "From docs / From guides" matches, the breadcrumb and the
// next / previous links at the foot of each article.
(function (global) {
  const CATEGORIES = [
    { id: "antennas", index: "guides", title: "Antennas" },
    { id: "get-started", index: "guides", title: "Get started" },
    { id: "install", index: "guides", title: "Install on your computer" },
    { id: "connect-a-radio", index: "guides", title: "Connect a radio" },
    { id: "plan-a-contact", index: "guides", title: "Plan a contact" },
    { id: "how-it-works", index: "docs", title: "How it works" },
    { id: "signals-and-modes", index: "docs", title: "Signals and modes" },
    { id: "developers", index: "docs", title: "For developers and hub operators" },
  ];

  const ARTICLES = [
    // guides: Get started
    { href: "/guides/start.html", cat: "get-started", title: "5-minute start", desc: "Install, pick a name, send a test chat. No radio needed.", keywords: "begin first install quick" },
    { href: "/docs/try.html", cat: "get-started", title: "Try it with two computers", desc: "No radio, or two cheap walkie-talkies. Hear a message travel as sound.", keywords: "speaker microphone test demo new" },
    { href: "/guides/troubleshooting.html", cat: "get-started", title: "Troubleshooting", desc: "Administrator PowerShell, a slow install, and a PC clock that is off.", keywords: "help error problem stuck fix clock" },
    // guides: Install on your computer
    { href: "/guides/windows.html", cat: "install", title: "Windows setup", desc: "PowerShell install, then the chat window.", keywords: "install pc powershell" },
    { href: "/guides/macos.html", cat: "install", title: "macOS setup", desc: "One-line installer, then the chat window.", keywords: "install mac apple homebrew" },
    { href: "/guides/linux.html", cat: "install", title: "Linux setup", desc: "Install, then USB radio permission if you need it.", keywords: "install ubuntu debian usb permission" },
    { href: "/guides/pi.html", cat: "install", title: "Raspberry Pi gateway", desc: "An always-on station on a Pi and a Digirig, run from a phone browser.", keywords: "raspberry pi gateway digirig headless server web browser always-on" },
    { href: "/guides/weechat.html", cat: "install", title: "WeeChat", desc: "Use the WeeChat program instead of the built-in window.", keywords: "irc client terminal" },
    // guides: Connect a radio
    { href: "/guides/radio.html", cat: "connect-a-radio", title: "Pick a radio path", desc: "Four setups. Choose the picture that matches your station.", keywords: "radio setup chooser cable" },
    { href: "/guides/radios.html", cat: "connect-a-radio", title: "Supported radios", desc: "Pick your radio by name: X6100, G90, FT-710, QDX and more.", keywords: "xiegu yaesu icom kenwood qrp rig list" },
    { href: "/guides/vr-n76.html", cat: "connect-a-radio", title: "VR-N76 / UV-PRO over Bluetooth", desc: "Radios with a built-in TNC. No cable at all.", keywords: "bluetooth tnc btech vero handheld" },
    { href: "/guides/handheld.html", cat: "connect-a-radio", title: "Handheld + Digirig", desc: "A walkie-talkie and a USB sound cable.", keywords: "digirig usb baofeng ht walkie-talkie" },
    { href: "/guides/vox.html", cat: "connect-a-radio", title: "Any radio with VOX", desc: "A headphone cable. The radio keys when it hears tones.", keywords: "vox cable audio speaker mic" },
    { href: "/guides/hf.html", cat: "connect-a-radio", title: "HF rig with CAT", desc: "A big HF radio the computer can control over USB.", keywords: "cat rig control usb hf transceiver" },
    // guides: Antennas
    { href: "/guides/antennas-hf.html", cat: "antennas", title: "HF antennas", desc: "Dipole, end-fed, vertical, loop or beam: how long, how high, which bands.", keywords: "antenna dipole end-fed efhw vertical loop beam mp1 loft calculator" },
    { href: "/guides/antennas-vhf.html", cat: "antennas", title: "VHF and UHF antennas", desc: "From the handheld whip to a collinear on the roof: sizes and range.", keywords: "antenna whip collinear yagi 2m 70cm calculator" },
    // guides: Plan a contact
    { href: "/guides/band.html", cat: "plan-a-contact", title: "Path planner", desc: "Drop two pins. Get a calling frequency, USB or FM, and a power.", keywords: "plan which frequency band path map propagation" },
    { href: "/docs/calling.html", cat: "plan-a-contact", title: "Calling frequencies", desc: "Suggested VHF and HF spots. Check your band plan first.", keywords: "frequency spot dial meet" },
    { href: "/docs/band-plan.html", cat: "plan-a-contact", title: "Band plan", desc: "Every band from 160m to 23cm, plus CB and PMR446, to scale.", keywords: "bands frequency allocation iaru cb pmr446" },
    // docs: How it works
    { href: "/why.html", cat: "how-it-works", title: "Why WeeChat Radio", desc: "An easier radio app, chat with no radio, and hops between stations.", keywords: "about reasons" },
    { href: "/docs/scenarios.html", cat: "how-it-works", title: "Common setups", desc: "Five everyday setups, animated, from a walkie-talkie to email over radio.", keywords: "scenarios examples gateway animated" },
    { href: "/docs/mixed-band.html", cat: "how-it-works", title: "One group: PMR, CB, 2m, and internet", desc: "How PMR446, CB, 2m and a laptop share one channel through the hub.", keywords: "mixed band pmr446 cb 2m group hub" },
    { href: "/docs/modes.html", cat: "how-it-works", title: "Modes", desc: "What each mode is for: internet, radio, radio-plus and the rest.", keywords: "internet radio radio-plus gateway map" },
    { href: "/docs/bridging.html", cat: "how-it-works", title: "Cross-frequency chat", desc: "Same dial on RF. Different bands via a gateway and the hub.", keywords: "bridging gateway hub band" },
    { href: "/docs/hops.html", cat: "how-it-works", title: "Hops", desc: "One dial, a group that is not all in range: stations relay for each other.", keywords: "relay repeat range mesh" },
    { href: "/docs/email.html", cat: "how-it-works", title: "Email", desc: "Internet mail beside Live Chat. Radio-plus goes through a gateway.", keywords: "mail inbox gateway radio-plus" },
    // docs: Signals and modes
    { href: "/docs/weak.html", cat: "signals-and-modes", title: "Weak signals", desc: "Retries, slower presets, and what TX / RETRY mean on the bar.", keywords: "faint fading retry preset snr" },
    { href: "/docs/compare.html", cat: "signals-and-modes", title: "Compare modes", desc: "WeeChat Radio HF vs JS8, FT8, Olivia, PSK31, APRS. Sortable scores.", keywords: "js8 ft8 olivia psk31 aprs digital modes" },
    { href: "/docs/apps.html", cat: "signals-and-modes", title: "Compare apps", desc: "WeeChat Radio vs Winlink, JS8Call, VarAC, Fldigi, Meshtastic and more.", keywords: "winlink pat js8call varac fldigi freedata meshtastic" },
    // docs: For developers and hub operators
    { href: "/docs/protocol.html", cat: "developers", title: "Wire protocol", desc: "What a message looks like on radio and on the internet.", keywords: "frame format packet bytes" },
    { href: "/docs/api.html", cat: "developers", title: "Public API", desc: "Live map data. No key to read. Writes are signed.", keywords: "api rest json openapi map data" },
    { href: "/docs/deploy.html", cat: "developers", title: "Deploy the hub", desc: "For people running the public server. Most users skip this.", keywords: "server hub self-host docker caddy" },
  ];

  const byCat = (id) => ARTICLES.filter((a) => a.cat === id);
  const category = (id) => CATEGORIES.find((c) => c.id === id);
  const escapeHtml = (s) =>
    String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

  global.WCR = global.WCR || {};
  global.WCR.articles = { CATEGORIES, ARTICLES, byCat, category };

  // Article page: "guides › Antennas › HF antennas" on top and
  // "← Previous" / "Next in Antennas →" at the foot, from the list above.
  function articleNav() {
    const here = location.pathname.replace(/\/$/, "");
    const art = ARTICLES.find((a) => a.href === here || a.href === `${here}.html`);
    const main = document.querySelector("main");
    if (!art || !main) return;
    const cat = category(art.cat);
    const crumb = main.querySelector(".crumb");
    if (crumb) {
      crumb.innerHTML = `<a href="/${cat.index}.html">${cat.index}</a> › <a href="/${cat.index}.html#${cat.id}">${escapeHtml(cat.title)}</a> › <span aria-current="page">${escapeHtml(art.title)}</span>`;
      crumb.setAttribute("aria-label", "Breadcrumb");
    }
    const list = byCat(art.cat);
    const i = list.indexOf(art);
    const prev = list[i - 1];
    const next = list[i + 1];
    if (!prev && !next) return;
    const nav = document.createElement("nav");
    nav.className = "article-next";
    nav.setAttribute("aria-label", `More in ${cat.title}`);
    nav.innerHTML = `${prev ? `<a class="article-prev" href="${prev.href}">← Previous: ${escapeHtml(prev.title)}</a>` : "<span></span>"}${
      next ? `<a class="article-nextlink" href="${next.href}">Next in ${escapeHtml(cat.title)}: ${escapeHtml(next.title)} →</a>` : ""
    }`;
    main.appendChild(nav);
  }

  // Index page: jump bar highlight, and the filter over both indexes.
  function indexPage() {
    const page = document.querySelector("main[data-index]");
    if (!page) return;
    const index = page.getAttribute("data-index");
    const other = index === "guides" ? "docs" : "guides";
    const jump = page.querySelector(".jump-bar");
    const sections = [...page.querySelectorAll("section[data-cat]")];

    // The current category: the last one whose heading is above 40% of the
    // screen, or the last one once the page is scrolled to the bottom (a
    // short last section never reaches the top).
    if (jump) {
      const links = new Map([...jump.querySelectorAll("a")].map((a) => [a.getAttribute("href").slice(1), a]));
      const scroller = document.querySelector(".page-wrap");
      const atBottom = () =>
        [document.scrollingElement, scroller].every((el) => !el || el.scrollTop + el.clientHeight >= el.scrollHeight - 2) &&
        (window.scrollY > 0 || (scroller && scroller.scrollTop > 0));
      let queued = false;
      // A clicked chip stays current until the reader scrolls by hand.
      let picked = null;
      jump.addEventListener("click", (e) => {
        const a = e.target.closest("a");
        if (a) picked = document.getElementById(a.getAttribute("href").slice(1));
      });
      for (const ev of ["wheel", "touchstart", "keydown"]) addEventListener(ev, () => (picked = null), { passive: true });
      const update = () => {
        queued = false;
        const shown = sections.filter((s) => !s.hidden);
        let current = shown.filter((s) => s.getBoundingClientRect().top < innerHeight * 0.4).pop();
        if (atBottom() && shown.length) current = shown[shown.length - 1];
        if (picked && !picked.hidden) current = picked;
        links.forEach((a, id) => {
          if (current && id === current.id) a.setAttribute("aria-current", "true");
          else a.removeAttribute("aria-current");
        });
      };
      jump.addEventListener("click", () => requestAnimationFrame(update));
      const onScroll = () => {
        if (!queued) requestAnimationFrame(update);
        queued = true;
      };
      addEventListener("scroll", onScroll, { passive: true });
      if (scroller) scroller.addEventListener("scroll", onScroll, { passive: true });
      update();
    }

    const box = page.querySelector(".index-filter");
    const input = box && box.querySelector("input");
    if (!input) return;
    box.hidden = false;
    const status = box.querySelector(".index-filter-status");
    const elsewhere = page.querySelector(".index-elsewhere");
    const empty = page.querySelector(".index-empty");
    const cards = [...page.querySelectorAll("a.guide-card")];
    const hide = [page.querySelector(".index-start"), page.querySelector(".index-also")];
    const words = (q) => q.toLowerCase().split(/\s+/).filter(Boolean);
    const matches = (text, q) => words(q).every((w) => text.toLowerCase().includes(w));
    const cardText = (a) =>
      `${a.querySelector("h2").textContent} ${a.querySelector("p").textContent} ${a.getAttribute("data-keywords") || ""}`;

    function run() {
      const q = input.value.trim();
      let shown = 0;
      for (const a of cards) {
        const ok = !q || matches(cardText(a), q);
        a.hidden = !ok;
        if (ok && !a.closest(".index-start")) shown++;
      }
      for (const s of sections) s.hidden = !!q && !s.querySelector("a.guide-card:not([hidden])");
      hide.forEach((el) => el && (el.hidden = !!q));
      const theirs = q
        ? ARTICLES.filter((a) => category(a.cat).index === other && matches(`${a.title} ${a.desc} ${a.keywords}`, q))
        : [];
      elsewhere.hidden = !theirs.length;
      elsewhere.querySelector("ul").innerHTML = theirs
        .map((a) => `<li><a href="${a.href}">${escapeHtml(a.title)}</a> <span>${escapeHtml(a.desc)}</span></li>`)
        .join("");
      empty.hidden = !q || shown + theirs.length > 0;
      status.textContent = q
        ? `${shown} ${shown === 1 ? "match" : "matches"} here, ${theirs.length} in ${other}.`
        : "";
    }
    input.addEventListener("input", run);
    input.addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return;
      input.value = "";
      run();
    });
    empty.querySelector("button").addEventListener("click", () => {
      input.value = "";
      run();
      input.focus();
    });
    run();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => {
      articleNav();
      indexPage();
    });
  } else {
    articleNav();
    indexPage();
  }
})(window);
