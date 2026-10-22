/**
 * Live diagrams for docs and guides: step a message through an SVG diagram.
 *
 * Markup (see docs/KIT.md for a full example):
 *   <figure class="diagram live-dgm" data-live-dgm data-setup="cable">
 *     <div class="setup-switch" data-setup-switch hidden>            optional
 *       <button type="button" data-setup-btn="cable" aria-pressed="true">…</button> …
 *     </div>
 *     <svg class="dgm" …>
 *       <g class="seg seg-rf" data-seg="rf"><path class="rf" d="…"/><path class="pkt" d="…"/></g>
 *       <g data-node="radio-a">…</g>                                   lit with .is-on
 *       <text data-show="b-hi">…</text>                                 appears when a step shows it
 *       <g data-setups="noradio" class="is-off">…</g>                   only in some setups
 *     </svg>
 *     <p data-live-status aria-live="polite"></p> <button data-live-replay hidden>Replay</button>
 *     <div class="setup-panel" data-setup-panel="cable">…</div>        optional, one per setup
 *     <script type="application/json" data-live-steps>
 *       { "cable": { "viewBox": "0 0 440 322", "done": "…",
 *                    "steps": [ { "say": "…", "segs": ["a-out", "<rf"], "on": ["radio-a"],
 *                                 "show": ["a-hi"], "ms": 1500 } ] } }
 *     </script>
 *   </figure>
 *
 * Step fields: say (status line), segs (paths to run; "<id" runs backwards), chain (run segs one
 * after another instead of together), on (nodes lit for this step), show (lines that appear and
 * stay), ms (step length, default 1500).
 * Plays once when the figure comes into view, again on Replay or a setup change.
 * Reduced motion and no-script both show the finished state.
 */
(function () {
  const REDUCED = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const STEP_MS = 1500;

  document.querySelectorAll("[data-live-dgm]").forEach(init);

  function init(fig) {
    const svg = fig.querySelector("svg");
    const dataEl = fig.querySelector("script[data-live-steps]");
    if (!svg || !dataEl) return;

    let setups;
    try {
      setups = JSON.parse(dataEl.textContent);
    } catch (err) {
      console.error("live-dgm: bad step data", err);
      return;
    }

    const buttons = Array.from(fig.querySelectorAll("[data-setup-btn]"));
    const panels = Array.from(fig.querySelectorAll("[data-setup-panel]"));
    const switchEl = fig.querySelector("[data-setup-switch]");
    const statusEl = fig.querySelector("[data-live-status]");
    const replayBtn = fig.querySelector("[data-live-replay]");
    let setup = setups[fig.dataset.setup] ? fig.dataset.setup : Object.keys(setups)[0];
    let timers = [];
    let seen = false;

    const all = (sel) => Array.from(svg.querySelectorAll(sel));

    // Packet animation needs each path's length (see .pkt in docs.css).
    all(".pkt").forEach((path) => {
      path.style.setProperty("--len", String(Math.ceil(path.getTotalLength())));
    });

    function steps() {
      return (setups[setup] && setups[setup].steps) || [];
    }

    function setStatus(text) {
      if (statusEl) statusEl.textContent = text || "";
    }

    function clearTimers() {
      timers.forEach(clearTimeout);
      timers = [];
    }

    function clearLive() {
      all(".seg").forEach((seg) => seg.classList.remove("is-live", "is-rev"));
      all(".is-on").forEach((node) => node.classList.remove("is-on"));
    }

    function show(names) {
      (names || []).forEach((name) => {
        all('[data-show~="' + name + '"]').forEach((el) => el.classList.add("is-shown"));
      });
    }

    function runStep(step) {
      clearLive();
      setStatus(step.say);
      show(step.show);
      (step.on || []).forEach((id) => {
        all('[data-node="' + id + '"]').forEach((node) => node.classList.add("is-on"));
      });
      const segs = step.segs || (step.seg ? [step.seg] : []);
      if (!segs.length) return;
      void svg.getBoundingClientRect(); // restart packet animations
      segs.forEach((raw, i) => {
        const rev = raw.charAt(0) === "<";
        const id = rev ? raw.slice(1) : raw;
        all('[data-seg="' + id + '"]').forEach((seg) => {
          seg.style.setProperty("--leg", step.chain ? String(i) : "0");
          seg.classList.toggle("is-rev", rev);
          seg.classList.add("is-live");
        });
      });
    }

    function showDone() {
      clearTimers();
      clearLive();
      all("[data-show]").forEach((el) => el.classList.remove("is-shown"));
      const list = steps();
      list.forEach((step) => show(step.show));
      const last = list[list.length - 1];
      setStatus((setups[setup] && setups[setup].done) || (last && last.say) || "");
    }

    function play() {
      clearTimers();
      clearLive();
      if (REDUCED) {
        showDone();
        return;
      }
      all("[data-show]").forEach((el) => el.classList.remove("is-shown"));
      let at = 0;
      steps().forEach((step) => {
        timers.push(setTimeout(() => runStep(step), at));
        at += step.ms || STEP_MS;
      });
      timers.push(setTimeout(clearLive, at));
    }

    function applySetup(next) {
      setup = next;
      fig.dataset.setup = next;
      const viewBox = setups[next] && setups[next].viewBox;
      if (viewBox) svg.setAttribute("viewBox", viewBox);
      buttons.forEach((btn) => btn.setAttribute("aria-pressed", btn.dataset.setupBtn === next ? "true" : "false"));
      panels.forEach((panel) => panel.classList.toggle("is-active", panel.dataset.setupPanel === next));
      all("[data-setups]").forEach((el) => {
        el.classList.toggle("is-off", !el.dataset.setups.split(/\s+/).includes(next));
      });
    }

    fig.classList.add("js-live");
    if (buttons.length) fig.classList.add("has-switch");
    if (switchEl) switchEl.hidden = false;
    if (replayBtn) replayBtn.hidden = false;
    applySetup(setup);
    showDone();

    buttons.forEach((btn) => btn.addEventListener("click", () => {
      seen = true;
      applySetup(btn.dataset.setupBtn);
      play();
    }));
    replayBtn?.addEventListener("click", () => {
      seen = true;
      play();
    });

    if (REDUCED) return;

    // Play once the diagram is on screen, so readers don't miss the start.
    if ("IntersectionObserver" in window) {
      const io = new IntersectionObserver((entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        io.disconnect();
        if (!seen) {
          seen = true;
          play();
        }
      }, { threshold: 0.35 });
      io.observe(svg);
    } else {
      play();
    }
  }
})();
