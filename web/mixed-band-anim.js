/** Play, pause, and replay for the mixed-band thread demo. */
(function () {
  const root = document.querySelector("[data-mixed-chat]");
  if (!root) return;

  const lines = Array.from(root.querySelectorAll(".mixed-chat-line"));
  const playBtn = root.querySelector("[data-mixed-play]");
  const replayBtn = root.querySelector("[data-mixed-replay]");
  const statusEl = root.querySelector("[data-mixed-status]");
  const STEP_MS = 3200;
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  let timer = null;
  let index = -1;
  let playing = false;

  function setStatus(text) {
    if (statusEl) statusEl.textContent = text;
  }

  function paint(i) {
    lines.forEach((line, j) => {
      line.classList.toggle("is-visible", j <= i);
      line.classList.toggle("is-highlight", j === i && i < lines.length);
    });
    if (i >= 0 && i < lines.length) {
      const who = lines[i].querySelector(".mixed-chat-who");
      setStatus(who ? "Line " + (i + 1) + " of " + lines.length + ": " + who.textContent.trim() : "");
    }
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

  function showAll() {
    stopTimer();
    lines.forEach((line) => {
      line.classList.add("is-visible");
      line.classList.remove("is-highlight");
    });
    index = lines.length - 1;
    setPlaying(false);
    setStatus(reducedMotion ? "All lines shown. Reduced motion skips autoplay." : "Done. Replay starts again.");
  }

  function step() {
    index += 1;
    if (index >= lines.length) {
      showAll();
      return;
    }
    paint(index);
    timer = setTimeout(step, STEP_MS);
  }

  function play() {
    if (reducedMotion) {
      showAll();
      return;
    }
    if (index >= lines.length - 1 && lines.every((line) => line.classList.contains("is-visible"))) {
      return;
    }
    stopTimer();
    setPlaying(true);
    step();
  }

  function pause() {
    stopTimer();
    setPlaying(false);
    if (index >= 0 && index < lines.length) {
      const who = lines[index].querySelector(".mixed-chat-who");
      setStatus(who ? "Paused on " + who.textContent.trim() : "Paused");
    }
  }

  function replay() {
    stopTimer();
    index = -1;
    lines.forEach((line) => line.classList.remove("is-visible", "is-highlight"));
    if (reducedMotion) {
      showAll();
      return;
    }
    setPlaying(true);
    setStatus("Playing");
    step();
  }

  playBtn?.addEventListener("click", () => {
    if (playing) pause();
    else play();
  });
  replayBtn?.addEventListener("click", replay);

  if (reducedMotion) showAll();
  else replay();
})();
