/* SPDX-License-Identifier: Apache-2.0 */
/* Homepage: install tabs (Linux/macOS or Windows) and the copy button. */
(function () {
  const commands = {
    unix: "curl -fsSL https://weechatradio.com/install.sh | sh",
    win: "irm https://weechatradio.com/install.ps1 | iex",
  };
  const tabs = document.querySelectorAll(".home-install__tab");
  const cmd = document.getElementById("install-cmd");
  const copyBtn = document.getElementById("install-copy");
  const nextSteps = document.querySelectorAll(".home-install__next");
  if (!tabs.length || !cmd || !copyBtn) return;

  let os = /Windows/i.test(navigator.userAgent) ? "win" : "unix";
  let copyTimer;

  function show(next) {
    os = next;
    cmd.textContent = commands[os];
    tabs.forEach((t) => t.setAttribute("aria-pressed", String(t.dataset.os === os)));
    nextSteps.forEach((p) => { p.hidden = p.dataset.os !== os; });
    copyBtn.textContent = "copy";
  }

  tabs.forEach((t) => t.addEventListener("click", () => show(t.dataset.os)));

  copyBtn.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(commands[os]);
      copyBtn.textContent = "copied";
    } catch (e) {
      const range = document.createRange();
      range.selectNodeContents(cmd);
      const sel = getSelection();
      sel.removeAllRanges();
      sel.addRange(range);
      copyBtn.textContent = "selected";
    }
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copyBtn.textContent = "copy"; }, 1600);
  });

  show(os);
})();
