/** Replay control for mixed-band chat demo on /docs/mixed-band.html */
(function () {
  const root = document.querySelector("[data-mixed-chat]");
  if (!root) return;

  const lines = Array.from(root.querySelectorAll(".mixed-chat-line"));
  const replayBtn = root.querySelector("[data-mixed-replay]");
  const statusEl = root.querySelector("[data-mixed-status]");
  const STEP_MS = 4200;
  let stepTimer = null;
  let index = -1;

  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  function setStatus(text) {
    if (statusEl) statusEl.textContent = text;
  }

  function paint(i) {
    lines.forEach((line, j) => {
      const visible = j <= i;
      const highlight = j === i;
      line.classList.toggle("is-visible", visible);
      line.classList.toggle("is-highlight", highlight);
      line.classList.toggle("is-dim", visible && !highlight);
    });
    if (i >= 0 && i < lines.length) {
      const who = lines[i].querySelector(".mixed-chat-who");
      setStatus(who ? `Line ${i + 1} of ${lines.length}: ${who.textContent}` : "");
    } else if (i >= lines.length) {
      setStatus("Done — press Replay");
    }
  }

  function clearTimers() {
    if (stepTimer) {
      clearTimeout(stepTimer);
      stepTimer = null;
    }
  }

  function showAllStatic() {
    lines.forEach((line) => {
      line.classList.add("is-visible");
      line.classList.remove("is-highlight", "is-dim");
    });
    setStatus("All lines shown (reduced motion)");
  }

  function run(fromStart) {
    clearTimers();
    if (reducedMotion) {
      showAllStatic();
      return;
    }
    index = fromStart ? -1 : index;
    function next() {
      index += 1;
      if (index >= lines.length) {
        paint(lines.length - 1);
        lines.forEach((line) => line.classList.remove("is-highlight"));
        setStatus("Done — press Replay");
        return;
      }
      paint(index);
      stepTimer = setTimeout(next, STEP_MS);
    }
    lines.forEach((line) => line.classList.remove("is-visible", "is-highlight", "is-dim"));
    setStatus("Playing…");
    next();
  }

  replayBtn?.addEventListener("click", () => run(true));
  run(true);
})();
