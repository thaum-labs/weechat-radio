// SPDX-License-Identifier: Apache-2.0
// App view: Live Chat over one Server-Sent Events stream (/api/events),
// mail and stations polled only while their tab is showing.
"use strict";

const POLL_MS = 5000;
const $ = (id) => document.getElementById(id);

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function clock(ts) {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  if (d.toDateString() === new Date().toDateString()) return time;
  return `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
}

function ago(secs) {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m`;
  if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
  return `${Math.floor(secs / 86400)}d`;
}

async function post(path, body) {
  const r = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body || {}),
  });
  if (r.status === 401) location.href = "/login";
  let data = {};
  try { data = await r.json(); } catch (_) { /* empty answer */ }
  if (!r.ok) throw new Error(data.error || `${r.status} ${r.statusText}`);
  return data;
}

async function getJson(path) {
  const r = await fetch(path);
  if (r.status === 401) location.href = "/login";
  if (!r.ok) {
    let why = `${r.status}`;
    try { why = (await r.json()).error || why; } catch (_) { /* not JSON */ }
    throw new Error(why);
  }
  return r.json();
}

// ---------- tabs ----------
let tab = "chat";
const TABS = ["chat", "mail", "stations", "traffic"];
const PANEL = { traffic: "traffic-tab" };
function showTab(name) {
  tab = name;
  for (const t of TABS) {
    $(PANEL[t] || t).hidden = t !== name;
    $(`tab-${t}`).setAttribute("aria-selected", String(t === name));
  }
  try { localStorage.setItem("wcr-tab", name); } catch (_) { /* private mode */ }
  if (name === "mail") loadMail();
  if (name === "stations") loadStations();
  if (name === "traffic") trafficTick();
}
for (const t of TABS) $(`tab-${t}`).onclick = () => showTab(t);

// ---------- chat ----------
const chat = { me: "", up: false, convs: new Map(), order: [], active: "#bulletin", unread: new Map() };

function conv(id) {
  if (!chat.convs.has(id)) chat.convs.set(id, new Map());
  if (!chat.order.includes(id)) chat.order.unshift(id);
  return chat.convs.get(id);
}

function touch(id) {
  chat.order = [id, ...chat.order.filter((c) => c !== id)];
}

function setLink(up) {
  chat.up = up;
  const l = $("link");
  l.className = `light ${up ? "up" : "down"}`;
  l.textContent = up ? "station up" : "station down";
  $("send").disabled = !up;
}

function onEvent(ev) {
  if (ev.type === "hello") {
    chat.me = ev.me || "";
    $("call").textContent = chat.me;
    chat.convs.clear();
    chat.order = [];
    for (const c of ev.convs) {
      const m = conv(c.id);
      for (const l of c.lines) m.set(l.seq, l);
    }
    chat.order = ev.convs.map((c) => c.id);
    conv(chat.active);
    setLink(ev.up);
    drawConvs();
    drawLines(true);
  } else if (ev.type === "link") {
    setLink(ev.up);
    if (ev.me) { chat.me = ev.me; $("call").textContent = ev.me; }
  } else if (ev.type === "reset") {
    conv(ev.conv).clear();
    if (ev.conv === chat.active) drawLines(true);
  } else if (ev.type === "line") {
    const l = ev.line;
    const m = conv(l.conv);
    const fresh = !m.has(l.seq);
    m.set(l.seq, l);
    if (fresh) {
      touch(l.conv);
      if (l.conv !== chat.active || document.hidden) {
        if (!l.mine && !l.sys) chat.unread.set(l.conv, (chat.unread.get(l.conv) || 0) + 1);
      }
      drawConvs();
    }
    if (l.conv === chat.active) drawLine(l);
  } else if (ev.type === "notice") {
    $("chat-note").textContent = ev.text;
  }
}

function drawConvs() {
  let total = 0;
  for (const n of chat.unread.values()) total += n;
  document.title = total ? `(${total}) WeeChat Radio` : "WeeChat Radio";
  const ul = $("convs");
  ul.replaceChildren();
  for (const id of chat.order) {
    const li = el("li", id === chat.active ? "active" : "");
    const b = el("button", "conv-btn", id);
    b.type = "button";
    b.onclick = () => openConv(id);
    const n = chat.unread.get(id) || 0;
    if (n) b.append(el("span", "badge", String(n)));
    li.append(b);
    ul.append(li);
  }
}

// `when` is the station's local "YYYY-MM-DD HH:MM:SS": the time today,
// the date and time on older lines.
function lineWhen(when) {
  if (!when) return "";
  const [day, time] = when.split(" ");
  const t = (time || "").slice(0, 5);
  const d = new Date(`${day}T00:00:00`);
  const today = new Date();
  if (isNaN(d) || d.toDateString() === today.toDateString()) return t;
  return `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${t}`;
}

function lineNode(l) {
  const li = el("li", l.sys ? "sys" : l.mine ? "mine" : "");
  li.dataset.seq = l.seq;
  li.append(el("span", "when", lineWhen(l.when)));
  if (!l.sys) li.append(el("span", "nick", l.nick));
  li.append(el("span", "text", l.text));
  const marks = [];
  // [ok net] / [ok rf]: how the first receipt came back (#226).
  const route = l.via ? (l.via === "inet" ? "net" : l.via) : "";
  if (l.mine && l.ticks) marks.push(route ? l.ticks.replace(/\[(ok|all)\]/, `[$1 ${route}]`) : l.ticks);
  if (l.mine && l.tries > 1) marks.push(`[x${l.tries}]`);
  if (!l.mine && l.via) marks.push(`[${route}]`);
  if (!l.mine && l.snr !== null && l.snr !== undefined) marks.push(`${Math.round(l.snr)} dB`);
  if (marks.length) {
    const m = el("span", `marks${/\[(ok|all)/.test(l.ticks) ? " ok" : /\[no\]/.test(l.ticks) ? " no" : ""}`, marks.join(" "));
    li.append(m);
  }
  return li;
}

function nearBottom() {
  const ol = $("lines");
  return ol.scrollTop + ol.clientHeight >= ol.scrollHeight - 40;
}

function drawLines(jump) {
  const ol = $("lines");
  const lines = [...conv(chat.active).values()].sort((a, b) => a.seq - b.seq);
  ol.replaceChildren(...lines.map(lineNode));
  if (!lines.length) ol.append(el("li", "sys", chat.up ? "No messages yet." : "Waiting for the station…"));
  if (jump) ol.scrollTop = ol.scrollHeight;
}

function drawLine(l) {
  const ol = $("lines");
  const stick = nearBottom();
  const old = ol.querySelector(`li[data-seq="${l.seq}"]`);
  if (old) {
    old.replaceWith(lineNode(l));
    return;
  }
  const empty = ol.querySelector("li:not([data-seq])");
  if (empty) empty.remove();
  ol.append(lineNode(l));
  if (stick || l.mine) ol.scrollTop = ol.scrollHeight;
}

function openConv(id) {
  chat.active = id;
  chat.unread.delete(id);
  $("conv-title").textContent = id;
  document.body.classList.add("in-conv");
  $("chat-note").textContent = "";
  drawConvs();
  drawLines(true);
  try { localStorage.setItem("wcr-conv", id); } catch (_) { /* private mode */ }
}

$("back").onclick = () => document.body.classList.remove("in-conv");

$("open-form").onsubmit = async (e) => {
  e.preventDefault();
  const raw = $("open-to").value.trim();
  if (!raw) return;
  try {
    if (raw.startsWith("#")) {
      const r = await post("/api/chat/join", { channel: raw });
      conv(r.channel);
      touch(r.channel);
      openConv(r.channel);
    } else {
      openDm(raw);
    }
    $("open-to").value = "";
  } catch (err) {
    $("chat-note").textContent = err.message;
  }
};

function openDm(call) {
  const id = call.trim().toUpperCase();
  conv(id);
  touch(id);
  showTab("chat");
  openConv(id);
  $("text").focus();
}

async function sendNow() {
  const text = $("text").value.trim();
  if (!text) return;
  $("send").disabled = true;
  try {
    await post("/api/chat/send", { to: chat.active, text });
    $("text").value = "";
    $("chat-note").textContent = "";
  } catch (err) {
    $("chat-note").textContent = err.message;
  } finally {
    $("send").disabled = !chat.up;
  }
}

$("compose").onsubmit = (e) => { e.preventDefault(); sendNow(); };
$("text").addEventListener("keydown", (e) => {
  // Enter sends on a keyboard; Shift+Enter is a new line.
  if (e.key === "Enter" && !e.shiftKey && !e.isComposing) { e.preventDefault(); sendNow(); }
});

let stream = null;
function connect() {
  if (stream) stream.close();
  stream = new EventSource("/api/events");
  stream.onmessage = (m) => {
    try { onEvent(JSON.parse(m.data)); } catch (_) { /* ignore a bad event */ }
  };
  stream.onerror = async () => {
    setLink(false);
    // EventSource retries by itself; a logged-out page goes to the login.
    const r = await fetch("/api/stations").catch(() => null);
    if (r && r.status === 401) location.href = "/login";
  };
}

// ---------- mail ----------
let mailMode = "";
let mailRows = [];
let openMail = null;

function chip(st) {
  const c = el("span", `chip ${st.key || "queued"}`, st.label || "?");
  c.title = st.reason || st.line || "";
  c.setAttribute("aria-label", `${st.mark || ""} ${st.label || ""}`.trim());
  return c;
}

async function loadMail() {
  const folder = $("folder").value;
  $("check-mail").hidden = !(folder === "inbox" && mailMode === "radio-plus");
  let d;
  try {
    d = await getJson(`/api/mail?folder=${folder}`);
  } catch (err) {
    $("mail-list").replaceChildren(el("li", "empty", err.message));
    return;
  }
  mailRows = d.rows || [];
  const ul = $("mail-list");
  ul.replaceChildren();
  for (const w of d.waiting || []) {
    const li = el("li", "waiting");
    const what = el("span", "what");
    what.append(el("span", "subject", w.subject || "(no subject)"), el("span", "who", `${w.from_addr} · waiting at the gateway`));
    const get = el("button", "quiet", "Get");
    get.type = "button";
    get.onclick = async () => {
      get.disabled = true;
      try { await post("/api/mail/get", { ids: [w.id] }); } catch (err) { alert(err.message); }
      loadMail();
    };
    li.append(el("span", "chip waiting", "Waiting"), what, get);
    ul.append(li);
  }
  if (!mailRows.length && !(d.waiting || []).length) {
    ul.append(el("li", "empty", folder === "inbox" ? "No mail." : "Empty."));
  }
  for (const m of mailRows) {
    const li = el("li", m.unread ? "unread" : "");
    const what = el("span", "what");
    const who = folder === "inbox" ? m.from_addr : m.to_addr;
    what.append(el("span", "subject", m.subject || "(no subject)"), el("span", "who", who));
    li.append(el("span", "when", clock(m.ts)), chip(m.status || {}), what);
    li.tabIndex = 0;
    li.onclick = () => showMail(m.id);
    li.onkeydown = (e) => { if (e.key === "Enter") showMail(m.id); };
    ul.append(li);
  }
  if (openMail) {
    const fresh = mailRows.find((r) => r.id === openMail.id);
    if (fresh) drawMail(fresh);
  }
}

const STAGES = { air: "on air", gateway: "at the gateway", sent: "sent (Resend took it)", report: "delivery report", failed: "failed", fetched: "fetched", keyed: "burst keyed" };

function drawMail(m) {
  openMail = m;
  $("mail-view").hidden = false;
  $("mail-form").hidden = true;
  $("mv-subject").textContent = m.subject || "(no subject)";
  $("mv-who").textContent = `${m.from_addr} → ${m.to_addr} · ${clock(m.ts)}`;
  const st = m.status || {};
  const p = $("mv-status");
  p.replaceChildren(chip(st), document.createTextNode(` ${st.word || ""}`));
  if (st.reason) p.append(el("span", "dim small block", st.reason));
  $("mv-hops").replaceChildren(...(m.hops || []).map((h) => el("li", "", `${clock(h.ts)} ${STAGES[h.stage] || h.stage}`)));
  $("mv-body").textContent = m.body || "";
  $("mail-retry").hidden = !st.retry;
  $("mail-reply").hidden = m.folder !== "inbox";
}

async function showMail(id) {
  try {
    drawMail(await getJson(`/api/mail/open?id=${encodeURIComponent(id)}`));
    $("mail-view").scrollIntoView({ block: "start" });
  } catch (err) {
    alert(err.message);
  }
}

$("mail-close").onclick = () => { openMail = null; $("mail-view").hidden = true; };
$("mail-retry").onclick = async () => {
  if (!openMail) return;
  try { await post("/api/mail/retry", { id: openMail.id }); } catch (err) { alert(err.message); }
  loadMail();
};
$("mail-reply").onclick = () => {
  if (!openMail) return;
  const subj = openMail.subject || "";
  newMail(openMail.from_addr, /^re:/i.test(subj) ? subj : `Re: ${subj}`);
};
$("folder").onchange = () => { openMail = null; $("mail-view").hidden = true; loadMail(); };
$("check-mail").onclick = async () => {
  $("check-mail").disabled = true;
  try { await post("/api/mail/check"); } catch (err) { alert(err.message); }
  $("check-mail").disabled = false;
  loadMail();
};

function newMail(to, subject) {
  openMail = null;
  $("mail-view").hidden = true;
  $("mail-form").hidden = false;
  $("m-to").value = to || "";
  $("m-subject").value = subject || "";
  $("m-body").value = "";
  $("m-note").textContent = "";
  (to ? $("m-body") : $("m-to")).focus();
}
$("new-mail").onclick = () => newMail("", "");
$("m-cancel").onclick = () => { $("mail-form").hidden = true; };
$("mail-form").onsubmit = async (e) => {
  e.preventDefault();
  $("m-note").textContent = "Sending…";
  try {
    await post("/api/mail/send", { to: $("m-to").value.trim(), subject: $("m-subject").value, body: $("m-body").value });
    $("mail-form").hidden = true;
    $("folder").value = "outbox";
    loadMail();
  } catch (err) {
    $("m-note").textContent = err.message;
  }
};

// ---------- stations ----------
async function loadStations() {
  let d;
  try {
    d = await getJson("/api/stations");
  } catch (err) {
    $("heard").replaceChildren(el("li", "empty", err.message));
    return;
  }
  mailMode = d.mode || "";
  const ul = $("heard");
  ul.replaceChildren();
  if (!d.heard.length) ul.append(el("li", "empty", d.me ? "Nobody heard yet." : "Station stopped."));
  for (const h of d.heard) {
    const li = el("li");
    const snr = h.snr === null || h.snr === undefined ? "" : ` ${Math.round(h.snr)} dB`;
    const age = h.last_heard ? ago(Math.max(0, d.now - h.last_heard)) : "";
    const what = el("span", "what");
    what.append(
      el("span", "subject", `${h.callsign}${h.gateway ? " (gateway)" : ""}`),
      el("span", "who", `${h.band || h.medium || ""}${snr}${h.grid ? ` · ${h.grid}` : ""}`),
    );
    const msg = el("button", "quiet", "Message");
    msg.type = "button";
    msg.onclick = () => openDm(h.callsign);
    li.append(el("span", "when", age), what, msg);
    ul.append(li);
  }
}

// ---------- on the air (#243) ----------
// Every frame heard or sent, read only. Reply opens a separate DM with
// that station; nothing is ever posted into the monitor or anyone else's chat.
const KIND_WORD = { dm: "DM", bulletin: "bulletin", group: "group", receipt: "receipt", beacon: "beacon", mail: "mail", status: "status", form: "form", checkin: "check-in", ping: "ping" };
const FILTER = {
  all: () => true,
  chat: (r) => ["dm", "bulletin", "group", "form", "checkin", "status"].includes(r.kind),
  receipt: (r) => r.kind === "receipt",
  beacon: (r) => r.kind === "beacon",
  mail: (r) => r.kind === "mail",
};
let traffic = [], trafficLast = 0, trafficFilter = "all", trafficBusy = false;

function trafficRow(r) {
  const li = el("li", r.dir);
  const t = new Date(r.ts * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false });
  li.append(el("span", "t-time", t), el("span", `chip k-${r.kind}`, KIND_WORD[r.kind] || r.kind));
  const who = el("span", "t-who");
  who.append(document.createTextNode(r.from || "?"));
  if (r.to) who.append(el("span", "dir", " → "), document.createTextNode(r.to));
  li.append(who);
  const side = el("span", "t-side");
  if (typeof r.snr === "number") side.append(el("span", r.snr >= 10 ? "up" : r.snr >= 6 ? "amber" : "down", `${r.snr.toFixed(0)} dB`));
  side.append(document.createTextNode(r.via === "rf" ? (r.dir === "sent" ? "sent rf" : "rf") : r.via === "inet" ? "internet" : r.via));
  const chatty = ["dm", "bulletin", "group", "checkin", "status", "form"].includes(r.kind);
  const station = r.dir === "sent" ? (r.kind === "dm" ? r.to : "") : r.from;
  if (chatty && station && /^[A-Z0-9]{3,}(\/[A-Z0-9]+)?$/i.test(station) && station.toUpperCase() !== (chat.me || "").toUpperCase()) {
    const b = el("button", "link", "Reply");
    b.type = "button";
    b.title = `Open a direct chat with ${station}`;
    b.onclick = () => openDm(station);
    side.append(b);
  }
  li.append(side);
  if (r.text) li.append(el("span", ["dm", "bulletin", "group"].includes(r.kind) ? "t-text" : "t-text dim", r.text));
  return li;
}

function drawTraffic() {
  const rows = traffic.filter(FILTER[trafficFilter]).slice(-80).reverse();
  const list = $("traffic");
  list.replaceChildren();
  if (!rows.length) list.append(el("li", "empty", traffic.length ? "Nothing of this kind yet." : "Nothing heard or sent since the station started."));
  for (const r of rows) list.append(trafficRow(r));
  $("traffic-count").textContent = traffic.length ? `${traffic.length} frames` : "";
}

async function trafficTick() {
  if (trafficBusy) return;
  trafficBusy = true;
  try {
    const d = await getJson(`/api/traffic?since=${trafficLast}`);
    if (d.last < trafficLast) { traffic = []; trafficLast = 0; return; }
    traffic = traffic.concat(d.rows).slice(-400);
    trafficLast = d.last;
    drawTraffic();
  } catch (err) {
    $("traffic").replaceChildren(el("li", "empty", err.message));
  } finally {
    trafficBusy = false;
  }
}

for (const b of document.querySelectorAll(".filters button")) {
  b.addEventListener("click", () => {
    trafficFilter = b.dataset.f;
    for (const o of document.querySelectorAll(".filters button")) o.setAttribute("aria-pressed", String(o === b));
    drawTraffic();
  });
}

// ---------- start ----------
setInterval(() => {
  if (document.hidden) return;
  if (tab === "mail") loadMail();
  if (tab === "stations") loadStations();
  if (tab === "traffic") trafficTick();
}, POLL_MS);

try {
  chat.active = localStorage.getItem("wcr-conv") || chat.active;
  $("conv-title").textContent = chat.active;
} catch (_) { /* private mode */ }
connect();
loadStations();
let first = "chat";
try { first = localStorage.getItem("wcr-tab") || "chat"; } catch (_) { /* private mode */ }
if (!TABS.includes(first)) first = "chat";
showTab(first);
// The station page's Reply opens /app#dm=CALL: a separate direct chat.
let dmHash = "";
try { dmHash = decodeURIComponent(location.hash); } catch (_) { /* bad escape */ }
const dm = /^#dm=([A-Za-z0-9/]+)$/.exec(dmHash);
if (dm) {
  history.replaceState(null, "", "/app");
  openDm(dm[1]);
}
