/* SPDX-License-Identifier: Apache-2.0 */
(function (global) {
  const host = location.hostname;
  const local = host === "localhost" || host === "127.0.0.1";
  const API = (global.WCR_API || (local
    ? "http://127.0.0.1:7373"
    : `${location.protocol}//hub.${host.replace(/^www\./, "")}`));

  global.WCR = global.WCR || {};
  global.WCR.API = API;
  global.WCR.wrapPres = wrapPres;

  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

  function pad(n) {
    return String(n).padStart(2, "0");
  }

  function clockHtml(d) {
    const colon = '<span class="colon">:</span>';
    return `${pad(d.getHours())}${colon}${pad(d.getMinutes())}${colon}${pad(d.getSeconds())}`;
  }

  function tickClock() {
    const utc = document.getElementById("clock-utc");
    const local = document.getElementById("clock-local");
    if (!utc && !local) return;
    const now = new Date();
    const utcDate = new Date(now.getTime() + now.getTimezoneOffset() * 60000);
    if (utc) utc.innerHTML = `UTC ${clockHtml(utcDate)}`;
    if (local) local.textContent = `LCL ${pad(now.getHours())}:${pad(now.getMinutes())}`;
  }

  function markNav() {
    const path = location.pathname.replace(/\/+$/, "") || "/";
    document.querySelectorAll("nav a").forEach((a) => {
      const href = a.getAttribute("href");
      let on = false;
      if (href === "/") {
        on = path === "/" || path.endsWith("/index.html");
      } else if (href === "/guides.html") {
        on = path === "/guides.html" || path.startsWith("/guides/");
      } else if (href === "/docs.html") {
        on = path === "/docs.html" || path.startsWith("/docs/");
      } else {
        on = path === href || path.endsWith(href);
      }
      a.classList.toggle("on", on);
    });
  }

  function activateModules() {
    const mods = [...document.querySelectorAll(".mod, .mod_column, #main_shell, .page-panel")];
    mods.forEach((el, i) => {
      const apply = () => el.classList.add("activated");
      if (reduceMotion) apply();
      else setTimeout(apply, 80 * i);
    });
  }

  const BOOT_LINES = [
    "weechat-radio:$ boot",
    "> fonts ………… ok",
    "> telemetry …… ok",
    `> hub ${API.replace(/^https?:\/\//, "")} …… ok`,
    "> map …………… ok",
    "ready.",
  ];

  function skipBoot(screen) {
    sessionStorage.setItem("wcr-boot-done", "1");
    screen.hidden = true;
    activateModules();
  }

  function runBoot() {
    const screen = document.getElementById("boot_screen");
    const log = document.getElementById("boot_log");
    if (!screen || !log) {
      activateModules();
      return;
    }
    if (sessionStorage.getItem("wcr-boot-done") || reduceMotion) {
      skipBoot(screen);
      return;
    }
    screen.hidden = false;
    log.textContent = "";
    let i = 0;
    const skip = () => skipBoot(screen);
    screen.addEventListener("click", skip, { once: true });
    document.addEventListener("keydown", skip, { once: true });
    const step = () => {
      if (screen.hidden) return;
      if (i >= BOOT_LINES.length) {
        setTimeout(skip, 280);
        return;
      }
      log.textContent += (i ? "\n" : "") + BOOT_LINES[i];
      i += 1;
      setTimeout(step, 180);
    };
    step();
  }

  function wrapPres() {
    document.querySelectorAll("main pre, .page-panel pre").forEach((pre) => {
      if (pre.parentElement.classList.contains("pre-wrap")) return;
      const wrap = document.createElement("div");
      wrap.className = "pre-wrap" + (pre.classList.contains("plain") ? " plain" : "");
      pre.parentNode.insertBefore(wrap, pre);
      wrap.appendChild(pre);
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "copy-btn";
      btn.textContent = "copy";
      btn.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(pre.textContent);
          btn.textContent = "copied";
          setTimeout(() => { btn.textContent = "copy"; }, 1200);
        } catch (_) {
          btn.textContent = "fail";
        }
      });
      wrap.appendChild(btn);
    });
  }

  function ensureMeta(name, content) {
    if (document.querySelector(`meta[name="${name}"]`)) return;
    const meta = document.createElement("meta");
    meta.name = name;
    meta.content = content;
    document.head.appendChild(meta);
  }

  function setupMobileNav() {
    const header = document.querySelector(".topbar");
    const nav = header && header.querySelector("nav");
    if (!header || !nav) return;

    ensureMeta("theme-color", "#07070a");
    ensureMeta("apple-mobile-web-app-capable", "yes");
    ensureMeta("apple-mobile-web-app-status-bar-style", "black-translucent");
    ensureMeta("mobile-web-app-capable", "yes");
    const viewport = document.querySelector('meta[name="viewport"]');
    if (viewport && !/viewport-fit/.test(viewport.content)) {
      viewport.content = "width=device-width, initial-scale=1, viewport-fit=cover";
    }

    if (!header.querySelector(".nav-toggle")) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "nav-toggle";
      btn.setAttribute("aria-label", "Open menu");
      btn.setAttribute("aria-expanded", "false");
      btn.setAttribute("aria-controls", "site-nav");
      btn.innerHTML = "<span></span><span></span><span></span>";
      nav.id = nav.id || "site-nav";
      header.appendChild(btn);

      let backdrop = document.querySelector(".nav-backdrop");
      if (!backdrop) {
        backdrop = document.createElement("div");
        backdrop.className = "nav-backdrop";
        document.body.appendChild(backdrop);
      }

      const setOpen = (open) => {
        document.body.classList.toggle("nav-open", open);
        btn.setAttribute("aria-expanded", open ? "true" : "false");
        btn.setAttribute("aria-label", open ? "Close menu" : "Open menu");
      };
      btn.addEventListener("click", () => {
        setOpen(!document.body.classList.contains("nav-open"));
      });
      backdrop.addEventListener("click", () => setOpen(false));
      nav.querySelectorAll("a").forEach((a) => {
        a.addEventListener("click", () => setOpen(false));
      });
      document.addEventListener("keydown", (e) => {
        if (e.key === "Escape") setOpen(false);
      });
    }

    const syncTopbar = () => {
      document.documentElement.style.setProperty(
        "--topbar-h",
        `${Math.round(header.getBoundingClientRect().height)}px`
      );
    };
    syncTopbar();
    window.addEventListener("resize", syncTopbar);
    document.body.classList.add("nav-ready");
    requestAnimationFrame(syncTopbar);
  }

  document.addEventListener("DOMContentLoaded", () => {
    markNav();
    tickClock();
    setInterval(tickClock, 1000);
    wrapPres();
    setupMobileNav();
    runBoot();
  });
})(window);
