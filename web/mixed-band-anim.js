/**
 * Mixed-band demo: steps through the #field thread and lights each line's
 * path on the band diagram. Click a line to show its path on demand.
 *
 * Each thread line carries:
 *   data-route  segment ids in travel order, e.g. "rf-pmr tls-pmr"
 *   data-nodes  boxes to light, e.g. "pmr1 m0pmr hub"
 *   data-dir    "down" when the line travels hub → radio
 */
(function () {
  const root = document.querySelector("[data-mixed-live]");
  if (!root) return;

  const lines = Array.from(root.querySelectorAll(".mb-line[data-route]"));
  const segs = Array.from(root.querySelectorAll("[data-seg]"));
  const nodes = Array.from(root.querySelectorAll("[data-node]"));
  const playBtn = root.querySelector("[data-mixed-play]");
  const replayBtn = root.querySelector("[data-mixed-replay]");
  const statusEl = root.querySelector("[data-mixed-status]");
  const STEP_MS = 3200;
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  let timer = null;
  let index = -1;
  let playing = false;
  let started = false;

  // Packet animation needs each path's length (see .pkt in mixed-band.css).
  root.querySelectorAll(".pkt").forEach((path) => {
    path.style.setProperty("--len", String(Math.ceil(path.getTotalLength())));
  });

  function setStatus(text) {
    if (statusEl) statusEl.textContent = text;
  }

  function whoOf(line) {
    const who = line.querySelector(".mb-line__who");
    return who ? who.textContent.trim() : "";
  }

  function showRoute(line) {
    segs.forEach((seg) => seg.classList.remove("is-live"));
    nodes.forEach((node) => node.classList.remove("is-live"));
    root.classList.toggle("has-route", Boolean(line));
    root.classList.toggle("is-down", Boolean(line) && line.dataset.dir === "down");
    if (!line) return;

    void root.getBoundingClientRect(); // restart packet animations
    line.dataset.route.split(" ").forEach((id, leg) => {
      const seg = root.querySelector('[data-seg="' + id + '"]');
      if (!seg) return;
      seg.style.setProperty("--leg", String(leg));
      seg.classList.add("is-live");
    });
    (line.dataset.nodes || "").split(" ").forEach((id) => {
      const node = root.querySelector('[data-node="' + id + '"]');
      if (node) node.classList.add("is-live");
    });
  }

  function paint(i) {
    lines.forEach((line, j) => {
      line.classList.toggle("is-visible", j <= i);
      line.classList.toggle("is-highlight", j === i);
    });
    showRoute(lines[i] || null);
  }

  function stopTimer() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function setPlaying(on) {
    playing = on;
    if (!playBtn) return;
    playBtn.textContent = on ? "Pause" : "Play";
    playBtn.setAttribute("aria-pressed", on ? "true" : "false");
  }

  function showAll(text) {
    stopTimer();
    lines.forEach((line) => {
      line.classList.add("is-visible");
      line.classList.remove("is-highlight");
    });
    showRoute(null);
    index = lines.length - 1;
    setPlaying(false);
    setStatus(text);
  }

  function step() {
    index += 1;
    if (index >= lines.length) {
      showAll("Done. Replay starts again, or tap a line to see its path.");
      return;
    }
    paint(index);
    setStatus("Line " + (index + 1) + " of " + lines.length + ": " + whoOf(lines[index]));
    timer = setTimeout(step, STEP_MS);
  }

  function play() {
    if (reducedMotion) {
      showAll("All lines shown. Reduced motion skips autoplay.");
      return;
    }
    if (index >= lines.length - 1) index = -1; // at the end: start over
    stopTimer();
    setPlaying(true);
    step();
  }

  function pause() {
    stopTimer();
    setPlaying(false);
    if (index >= 0 && index < lines.length) setStatus("Paused on " + whoOf(lines[index]));
  }

  function replay() {
    stopTimer();
    index = -1;
    lines.forEach((line) => line.classList.remove("is-visible", "is-highlight"));
    showRoute(null);
    play();
  }

  function showLine(i) {
    started = true;
    stopTimer();
    setPlaying(false);
    index = i;
    lines.forEach((line, j) => {
      line.classList.toggle("is-visible", j <= i || line.classList.contains("is-visible"));
      line.classList.toggle("is-highlight", j === i);
    });
    showRoute(lines[i]);
    setStatus("Showing line " + (i + 1) + ": " + whoOf(lines[i]) + (reducedMotion ? "." : ". Play to continue."));
  }

  playBtn?.addEventListener("click", () => {
    started = true;
    if (playing) pause();
    else play();
  });
  replayBtn?.addEventListener("click", () => {
    started = true;
    replay();
  });
  lines.forEach((line, i) => {
    line.querySelector(".mb-line__btn")?.addEventListener("click", () => showLine(i));
  });

  if (reducedMotion) {
    showAll("All lines shown. Reduced motion skips autoplay. Tap a line to see its path.");
    return;
  }

  // Autoplay once the figure is on screen, so readers don't miss the start.
  if ("IntersectionObserver" in window) {
    const io = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      io.disconnect();
      if (!started) {
        started = true;
        replay();
      }
    }, { threshold: 0.35 });
    io.observe(root);
  } else {
    started = true;
    replay();
  }
})();
