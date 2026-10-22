/**
 * Try-it demo: switch between setups and watch a "hi" cross as sound and the
 * "[ok]" come back. Each step lights one path segment ([data-seg]) and says
 * what is happening in the status line.
 */
(function () {
  const fig = document.querySelector("[data-try-setup]");
  if (!fig) return;

  const svg = fig.querySelector("svg.try-dgm");
  const buttons = Array.from(fig.querySelectorAll("[data-setup-btn]"));
  const panels = Array.from(fig.querySelectorAll("[data-setup-panel]"));
  const switchEl = fig.querySelector("[data-setup-switch]");
  const statusEl = fig.querySelector("[data-try-status]");
  const replayBtn = fig.querySelector("[data-try-replay]");
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const STEP_MS = 1500;
  const VIEWBOX = { radio: "0 0 440 322", noradio: "0 0 440 146" };
  const SCREENS = ["show-a-hi", "show-b-hi", "show-a-ok"];

  // seg: path to light · rev: run it backwards · tx: radio whose light comes on · screen: line to show
  const RADIO_STEPS = [
    { say: "Computer A plays “hi” as sound into radio A’s microphone.", seg: "a-out", screen: "show-a-hi" },
    { say: "Radio A hears the sound and transmits. Its light comes on.", seg: "rf", tx: "radio-a" },
    { say: "Radio B plays it from its speaker into computer B’s microphone.", seg: "b-in" },
    { say: "Computer B shows the line and plays [ok] back into radio B.", seg: "b-out", screen: "show-b-hi" },
    { say: "Radio B transmits the [ok]. Its light comes on.", seg: "rf", rev: true, tx: "radio-b" },
    { say: "Radio A plays it into computer A’s microphone.", seg: "a-in" },
    { say: "Computer A shows [ok]. It worked both ways.", screen: "show-a-ok" },
  ];
  const AIR_STEPS = [
    { say: "Computer A plays “hi” as sound. Computer B’s microphone hears it.", seg: "air-ab", screen: "show-a-hi" },
    { say: "Computer B shows the line and plays [ok] back.", seg: "air-ba", screen: "show-b-hi" },
    { say: "Computer A shows [ok]. It worked both ways.", screen: "show-a-ok" },
  ];
  const DONE = "Computer A shows [ok]. It worked both ways.";

  let setup = fig.dataset.setup || "cable";
  let timers = [];
  let seen = false;

  // Packet animation needs each path's length (see svg.dgm .pkt in styles.css).
  svg.querySelectorAll(".pkt").forEach((path) => {
    path.style.setProperty("--len", String(Math.ceil(path.getTotalLength())));
  });

  function setStatus(text) {
    if (statusEl) statusEl.textContent = text;
  }

  function clearTimers() {
    timers.forEach(clearTimeout);
    timers = [];
  }

  function clearPaths() {
    svg.querySelectorAll(".seg").forEach((seg) => seg.classList.remove("is-live", "is-rev"));
    svg.querySelectorAll(".radio").forEach((radio) => radio.classList.remove("is-tx"));
  }

  function lightStep(step) {
    clearPaths();
    if (step.screen) svg.classList.add(step.screen);
    setStatus(step.say);
    if (step.tx) svg.querySelector('[data-node="' + step.tx + '"]')?.classList.add("is-tx");
    if (!step.seg) return;
    const seg = svg.querySelector('[data-seg="' + step.seg + '"]');
    if (!seg) return;
    void svg.getBoundingClientRect(); // restart the packet animation
    seg.classList.toggle("is-rev", Boolean(step.rev));
    seg.classList.add("is-live");
  }

  function showDone() {
    clearTimers();
    clearPaths();
    SCREENS.forEach((name) => svg.classList.add(name));
    setStatus(DONE);
  }

  function play() {
    clearTimers();
    clearPaths();
    if (reducedMotion) {
      showDone();
      return;
    }
    SCREENS.forEach((name) => svg.classList.remove(name));
    const steps = setup === "noradio" ? AIR_STEPS : RADIO_STEPS;
    steps.forEach((step, i) => {
      timers.push(setTimeout(() => lightStep(step), i * STEP_MS));
    });
    timers.push(setTimeout(clearPaths, steps.length * STEP_MS));
  }

  function choose(next) {
    setup = next;
    fig.dataset.setup = next;
    svg.setAttribute("viewBox", next === "noradio" ? VIEWBOX.noradio : VIEWBOX.radio);
    buttons.forEach((btn) => btn.setAttribute("aria-pressed", btn.dataset.setupBtn === next ? "true" : "false"));
    panels.forEach((panel) => panel.classList.toggle("is-active", panel.dataset.setupPanel === next));
    play();
  }

  // Turn the stacked text into a switch now that the script is running.
  fig.classList.add("has-switch");
  if (switchEl) switchEl.hidden = false;
  if (replayBtn) replayBtn.hidden = false;
  panels.forEach((panel) => panel.classList.toggle("is-active", panel.dataset.setupPanel === setup));
  buttons.forEach((btn) => btn.addEventListener("click", () => {
    seen = true;
    choose(btn.dataset.setupBtn);
  }));
  replayBtn?.addEventListener("click", () => {
    seen = true;
    play();
  });

  if (reducedMotion) {
    showDone();
    return;
  }

  // Play once the diagram is on screen, so readers don't miss the start.
  setStatus("");
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
})();
