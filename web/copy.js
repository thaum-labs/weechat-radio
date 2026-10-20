/* SPDX-License-Identifier: Apache-2.0 */
(function (global) {
  function legacyCopy(text) {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.cssText =
      "position:fixed;top:0;left:0;width:2em;height:2em;padding:0;border:none;outline:none;box-shadow:none;background:transparent";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    ta.setSelectionRange(0, text.length);
    let ok = false;
    try {
      ok = document.execCommand("copy");
    } catch (_) {}
    document.body.removeChild(ta);
    return ok;
  }

  /**
   * Copy text from a user click handler. Resolves to "copied" or "fail".
   */
  function copyText(text) {
    const value = String(text).trim();
    if (!value) return Promise.resolve("fail");
    if (navigator.clipboard && global.isSecureContext) {
      return navigator.clipboard.writeText(value).then(
        () => "copied",
        () => (legacyCopy(value) ? "copied" : "fail")
      );
    }
    return Promise.resolve(legacyCopy(value) ? "copied" : "fail");
  }

  global.WCR = global.WCR || {};
  global.WCR.copyText = copyText;
})(window);
