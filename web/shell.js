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
      const on = href === "/"
        ? path === "/" || path.endsWith("/index.html")
        : path.endsWith(href);
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
      wrap.className = "pre-wrap";
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

  document.addEventListener("DOMContentLoaded", () => {
    markNav();
    tickClock();
    setInterval(tickClock, 1000);
    wrapPres();
    runBoot();
  });
})(window);
