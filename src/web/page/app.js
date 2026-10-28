// pocket-ui web companion. Vanilla JS, no libraries; every text from the Pocket goes in with textContent, never as
// HTML. Writes carry the session's CSRF token in X-CSRF-Token.
"use strict";

const S = { csrf: null, via: null, name: "", lib: null, playing: false, statusTimer: null, shot: null };

// --- small helpers -------------------------------------------------------------------------------------------------

function h(tag, attrs, ...kids) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") el.className = v;
    else if (k === "stripe") el.style.setProperty("--stripe", v);
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "text") el.textContent = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const kid of kids.flat(Infinity)) {
    if (kid === null || kid === undefined || kid === false) continue;
    el.append(kid instanceof Node ? kid : document.createTextNode(String(kid)));
  }
  return el;
}

const $ = (id) => document.getElementById(id);

function toast(msg, err) {
  const t = h("div", { class: "toast" + (err ? " err" : ""), role: "status" }, msg);
  document.body.append(t);
  setTimeout(() => t.remove(), err ? 5000 : 2400);
}

function duration(secs) {
  if (!secs) return "–";
  const m = Math.round(secs / 60);
  if (m < 60) return m + " min";
  const hrs = Math.floor(m / 60), rest = m % 60;
  return rest ? `${hrs} h ${rest} min` : `${hrs} h`;
}

function ago(t) {
  if (!t) return "";
  const d = Math.max(0, Date.now() / 1000 - t);
  if (d < 3600) return Math.max(1, Math.round(d / 60)) + " min ago";
  if (d < 86400) return Math.round(d / 3600) + " h ago";
  const days = Math.round(d / 86400);
  return days === 1 ? "yesterday" : days + " days ago";
}

function size(bytes) {
  if (bytes < 1048576) return Math.max(1, Math.round(bytes / 1024)) + " KB";
  if (bytes < 1073741824) return Math.round(bytes / 1048576) + " MB";
  return (bytes / 1073741824).toFixed(1) + " GB";
}

const STATUS = [
  ["not_started", "Not started"], ["playing", "Playing"], ["paused", "Paused"], ["completed", "Completed"], ["dropped", "Dropped"],
];
const statusLabel = (k) => (STATUS.find((s) => s[0] === k) || STATUS[0])[1];
const mark = (k) => (k && k !== "not_started" ? h("span", { class: "mark " + k, title: statusLabel(k) }) : null);

// --- the API -------------------------------------------------------------------------------------------------------

class Playing extends Error {}

async function api(path, opts) {
  const o = Object.assign({ credentials: "same-origin", headers: {} }, opts || {});
  if (o.method === "POST") {
    o.headers["X-CSRF-Token"] = S.csrf || "";
    if (o.json !== undefined) {
      o.headers["Content-Type"] = "application/json";
      o.body = JSON.stringify(o.json);
    }
  }
  const r = await fetch(path, o);
  let body = null;
  const type = r.headers.get("Content-Type") || "";
  if (type.startsWith("application/json")) body = await r.json();
  if (r.status === 503 && body && body.playing) {
    setPlaying(true);
    throw new Playing(body.error);
  }
  if (r.status === 401) {
    S.csrf = null;
    showPair();
    throw new Error(body ? body.error : "Pair this browser first.");
  }
  if (!r.ok) throw new Error((body && body.error) || "Something went wrong (" + r.status + ").");
  return body;
}

const post = (path, json) => api(path, { method: "POST", json });

// --- frame: session, tabs, status ----------------------------------------------------------------------------------

async function start() {
  let s;
  try {
    s = await api("/api/session");
  } catch (e) {
    $("main").replaceChildren(h("p", { class: "alert" }, e.message));
    return;
  }
  $("version").textContent = "pocket-ui " + s.info.version;
  if (!s.signedIn) return showPair(s);
  S.csrf = s.csrf;
  S.via = s.via;
  S.name = s.name;
  $("who").textContent = s.via === "tailscale" ? "Signed in with Tailscale as " + s.name : "Paired: " + s.name;
  if (s.via === "paired") $("who").append(" · ", h("a", { href: "#/forget", class: "quiet" }, "Forget this browser"));
  $("tabs").hidden = false;
  window.addEventListener("hashchange", route);
  document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && pollStatus());
  // The systems' names and colours, for "Now playing" too (a game running answers "playing": then later).
  try { S.lib = await api("/api/library"); } catch (e) { S.lib = null; }
  await pollStatus();
  route();
}

function setPlaying(on) {
  if (S.playing === on) return;
  S.playing = on;
  if (!on) route();
}

async function pollStatus() {
  clearTimeout(S.statusTimer);
  if (document.visibilityState !== "visible" || !S.csrf) return;
  try {
    const st = await api("/api/status");
    renderNow(st.playing, st.gameRunning);
    setPlaying(st.gameRunning);
  } catch (e) {
    /* the next poll tries again */
  }
  // The battery waits while a game runs, like the lists.
  if (!S.playing) try {
    const b = await api("/api/battery");
    if (b.last) $("battery").textContent = (b.last.charging ? "Charging · " : "") + b.last.percent + " %";
  } catch (e) {
    /* no battery, or a game runs */
  }
  S.statusTimer = setTimeout(pollStatus, S.playing ? 10000 : 20000);
}

function renderNow(p, running) {
  const box = $("now");
  if (!p) {
    box.hidden = !running;
    box.replaceChildren(running ? h("p", { class: "quiet" }, "A game is running. The lists wait until it ends.") : "");
    return;
  }
  box.hidden = false;
  const shot = h("div", { class: "shot" });
  if (S.shot) shot.append(S.shot);
  const btn = h("button", { onclick: () => takeShot(shot, btn) }, "Screenshot");
  fill(box,
    h("div", { class: "label", stripe: p.stripe },
      h("div", { class: "body" },
        h("div", { class: "sub" }, "Now playing"),
        h("h1", {}, p.name),
        h("div", { class: "sub" }, p.systemName + " · since " + new Date(p.since * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })),
        h("div", { class: "actions" }, btn))),
    shot,
    running ? h("p", { class: "quiet small" }, "While a game runs, the lists and uploads wait until it ends.") : null);
}

async function takeShot(box, btn) {
  btn.disabled = true;
  try {
    const r = await fetch("/api/screenshot?t=" + Date.now(), { credentials: "same-origin" });
    if (!r.ok) throw new Error((await r.json()).error);
    const url = URL.createObjectURL(await r.blob());
    S.shot = h("img", { src: url, alt: "The Pocket's screen" });
    box.replaceChildren(S.shot);
    const age = Number(r.headers.get("X-Shot-Age") || 0);
    if (r.headers.get("X-Screen-Hidden")) box.append(h("p", { class: "quiet small" }, "Hidden: the Pocket shows Settings, which can show codes and passwords. Try again when it is back on Home or in a game."));
    else if (age > 0) box.append(h("p", { class: "quiet small" }, `Taken ${age} s ago. A new one every 10 seconds at most.`));
  } catch (e) {
    toast(e.message, true);
  }
  setTimeout(() => (btn.disabled = false), 3000);
}

function stripeOf(system) {
  const s = S.lib && S.lib.systems.find((x) => x.id === system);
  return s ? s.stripe : null;
}
function systemName(system) {
  const s = S.lib && S.lib.systems.find((x) => x.id === system);
  return s ? s.name : system;
}

function tab(name) {
  for (const a of document.querySelectorAll("#tabs a")) a.classList.toggle("on", a.dataset.tab === name);
}

function fill(el, ...kids) {
  el.replaceChildren(...kids.flat(Infinity).filter((k) => k !== null && k !== undefined && k !== false));
}

function view(...kids) {
  fill($("main"), ...kids);
  window.scrollTo(0, 0);
}

async function route() {
  // A broken escape in the address ("%E0%A4%A") stays as it is instead of stopping the page.
  const decode = (p) => { try { return decodeURIComponent(p); } catch (e) { return p; } };
  const parts = (location.hash || "#/library").slice(2).split("/").map(decode);
  const [where, arg] = parts;
  try {
    if (!S.lib || where === "library") S.lib = await api("/api/library");
    switch (where) {
      case "system": tab("library"); return await listView({ system: arg });
      case "game": tab(""); return await gameView(arg);
      case "search": tab("library"); return await listView({ q: arg });
      case "favorites": tab("shelves"); return await listView({ favorites: 1 });
      case "sticker": tab("shelves"); return await listView({ sticker: arg });
      case "shelf": tab("shelves"); return await listView({ shelf: arg });
      case "shelves": tab("shelves"); return shelvesView();
      case "upload": tab("upload"); return await uploadView();
      case "stats": tab("stats"); if (["week", "month", "year"].includes(arg)) S.period = arg; return await statsView();
      case "forget": return await forget();
      default: tab("library"); return libraryView();
    }
  } catch (e) {
    if (e instanceof Playing) {
      view(h("h2", {}, "A game is running"), h("p", { class: "quiet" }, "The web companion waits until it ends, so the game has the Pocket to itself. This page carries on by itself."));
    } else if (S.csrf) {
      view(h("p", { class: "alert" }, e.message));
    }
  }
}

// --- pairing -------------------------------------------------------------------------------------------------------

function showPair(s) {
  $("tabs").hidden = true;
  const input = h("input", { type: "text", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "7", class: "code-input", "aria-label": "Pairing code" });
  const msg = h("p", { class: "quiet" });
  const go = async (code) => {
    msg.className = "quiet";
    msg.textContent = "Pairing…";
    try {
      const r = await fetch("/api/pair", { method: "POST", credentials: "same-origin", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ code }) });
      const b = await r.json();
      if (!r.ok) throw new Error(b.error);
      history.replaceState(null, "", location.pathname);
      location.reload();
    } catch (e) {
      msg.className = "alert";
      msg.textContent = e.message;
    }
  };
  const form = h("form", { onsubmit: (ev) => { ev.preventDefault(); go(input.value); } },
    h("div", { class: "row" }, input, h("button", { class: "primary", type: "submit" }, "Pair")));
  view(
    h("h2", {}, "Pair this browser"),
    h("p", {}, "On the Pocket, open Settings › Connections › Web companion › Pair a browser, and type the six digits it shows."),
    form, msg,
    h("p", { class: "quiet small" }, "Your own devices on Tailscale open the companion without a code, unless that is switched off on the Pocket."));
  const m = /pair=(\d{6})/.exec(location.hash);
  if (m) { input.value = m[1]; go(m[1]); } else input.focus();
}

async function forget() {
  if (!confirm("Forget this browser? You will need a new code from the Pocket.")) return history.back();
  await post("/api/logout", {});
  location.hash = "";
  location.reload();
}

// --- library -------------------------------------------------------------------------------------------------------

function searchBox(value) {
  const input = h("input", { type: "search", placeholder: "Search all games", value: value || "", "aria-label": "Search", enterkeyhint: "search" });
  let timer;
  input.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => { if (input.value.trim()) location.hash = "#/search/" + encodeURIComponent(input.value.trim()); }, 350);
  });
  return h("form", { class: "search", onsubmit: (e) => { e.preventDefault(); if (input.value.trim()) location.hash = "#/search/" + encodeURIComponent(input.value.trim()); } }, input);
}

function libraryView() {
  const lib = S.lib;
  const list = h("ul", { class: "shelf" },
    lib.systems.map((s) => h("li", {}, h("a", { class: "spine", href: "#/system/" + encodeURIComponent(s.id), stripe: s.stripe },
      h("span", { class: "name" }, s.name), h("span", { class: "side" }, String(s.games))))));
  view(searchBox(), lib.systems.length ? list : h("p", { class: "quiet" }, "No games on the Pocket yet. Add some under Upload."));
}

function gameSpine(g) {
  const art = g.cover ? h("img", { src: "/api/cover?id=" + g.id, alt: "", loading: "lazy" }) : h("span", { class: "tile", stripe: g.stripe }, (g.name || "?").slice(0, 1));
  const side = h("span", { class: "side" }, mark(g.status), g.favorite ? h("span", { class: "star", title: "Favourite" }, "★") : null, g.secs ? duration(g.secs) : "");
  return h("li", {}, h("a", { class: "spine", href: "#/game/" + g.id, stripe: g.stripe }, art, h("span", { class: "name" }, g.name), side));
}

async function listView(q) {
  const params = new URLSearchParams(q).toString();
  const r = await api("/api/games?" + params);
  const head = q.q !== undefined ? searchBox(q.q) : null;
  const shown = r.games.length < r.total ? ` · first ${r.games.length}` : "";
  view(
    head,
    h("a", { class: "back", href: q.system || q.q !== undefined ? "#/library" : "#/shelves" }, q.system || q.q !== undefined ? "Library" : "Shelves"),
    h("h2", {}, r.title, " ", h("span", { class: "count" }, r.total + (r.total === 1 ? " game" : " games") + shown)),
    r.games.length ? h("ul", { class: "shelf" }, r.games.map(gameSpine)) : h("p", { class: "quiet" }, q.q !== undefined ? `No game matches “${q.q}”.` : "Nothing here yet."));
}

async function gameView(id) {
  const g = await api("/api/game?id=" + encodeURIComponent(id));
  const lib = S.lib;
  const fav = h("button", { class: g.favorite ? "on" : "" }, g.favorite ? "★ Favourite" : "☆ Favourite");
  fav.addEventListener("click", async () => {
    try { await post("/api/game/favorite", { id, on: !g.favorite }); g.favorite = !g.favorite; fav.className = g.favorite ? "on" : ""; fav.textContent = g.favorite ? "★ Favourite" : "☆ Favourite"; }
    catch (e) { toast(e.message, true); }
  });
  const sub = [g.systemName, g.secs ? duration(g.secs) + " played" : "Not played yet", g.lastPlayed ? "last " + ago(g.lastPlayed) : null].filter(Boolean).join(" · ");
  const label = h("div", { class: "label", stripe: g.stripe },
    h("div", { class: "body" }, h("h1", {}, g.name), h("div", { class: "sub" }, sub), h("div", { class: "actions" }, fav)),
    g.cover ? h("div", { class: "art" }, h("img", { src: "/api/cover?id=" + g.id, alt: "Box art" })) : null);

  // Status
  const seg = h("div", { class: "seg", role: "group", "aria-label": "Play status" });
  const paintSeg = () => seg.replaceChildren(...STATUS.map(([k, l]) => h("button", { class: g.status === k ? "on" : "", onclick: () => setStatus(k) }, mark(k), l)));
  const setStatus = async (k) => {
    try { await post("/api/game/status", { id, status: k }); g.status = k; paintSeg(); toast("Status: " + statusLabel(k)); }
    catch (e) { toast(e.message, true); }
  };
  paintSeg();

  // My rating: five stars; the one given again takes it away.
  const stars = h("div", { class: "stars", role: "group", "aria-label": "My rating" });
  const paintStars = () => stars.replaceChildren(
    ...[1, 2, 3, 4, 5].map((n) => h("button", { class: (g.rating || 0) >= n ? "on" : "", title: n === 1 ? "1 star" : n + " stars", "aria-pressed": g.rating === n ? "true" : "false", onclick: () => setRating(g.rating === n ? 0 : n) }, (g.rating || 0) >= n ? "★" : "☆")),
    h("span", { class: "quiet small" }, g.rating ? "" : "Not rated yet"));
  const setRating = async (n) => {
    try { await post("/api/game/rating", { id, rating: n }); g.rating = n; paintStars(); toast(n ? (n === 1 ? "Rated 1 star" : `Rated ${n} stars`) : "Rating removed"); }
    catch (e) { toast(e.message, true); }
  };
  paintStars();

  // Note
  const note = h("input", { type: "text", value: g.note || "", maxlength: String(lib.noteMax), placeholder: "A short note", "aria-label": "Note" });
  const saveNote = h("button", {}, "Save");
  saveNote.addEventListener("click", async () => {
    try { await post("/api/game/note", { id, note: note.value }); toast(note.value.trim() ? "Note saved" : "Note removed"); }
    catch (e) { toast(e.message, true); }
  });

  // Stickers
  const chips = h("div", { class: "chips" });
  const paintChips = () => chips.replaceChildren(
    ...lib.stickers.map((s) => {
      const on = g.stickers.includes(s.id);
      return h("button", { class: "chip" + (on ? " on" : ""), "aria-pressed": on ? "true" : "false", onclick: async () => {
        try { await post("/api/game/sticker", { id, sticker: s.id, on: !on }); g.stickers = on ? g.stickers.filter((x) => x !== s.id) : g.stickers.concat(s.id); paintChips(); }
        catch (e) { toast(e.message, true); }
      } }, s.name);
    }));
  paintChips();
  const newSticker = h("input", { type: "text", maxlength: String(lib.stickerMax), placeholder: "New sticker", "aria-label": "New sticker" });
  const addSticker = h("button", {}, "Add");
  addSticker.addEventListener("click", async () => {
    const name = newSticker.value.trim();
    if (!name) return;
    try { await post("/api/stickers/new", { name, id }); S.lib = await api("/api/library"); route(); }
    catch (e) { toast(e.message, true); }
  });

  // Facts
  const f = g.facts;
  const facts = f ? h("dl", { class: "facts" },
    f.year ? [h("dt", {}, "Released"), h("dd", {}, String(f.year))] : null,
    f.developers && f.developers.length ? [h("dt", {}, "Developer"), h("dd", {}, f.developers.join(", "))] : null,
    f.publishers && f.publishers.length ? [h("dt", {}, "Publisher"), h("dd", {}, f.publishers.join(", "))] : null,
    f.genres && f.genres.length ? [h("dt", {}, "Genre"), h("dd", {}, f.genres.slice(0, 3).join(", "))] : null,
    f.series && f.series.length ? [h("dt", {}, "Series"), h("dd", {}, f.series.join(", "))] : null,
    f.players ? [h("dt", {}, "Players"), h("dd", {}, f.players)] : null) : h("p", { class: "quiet" }, "No facts about this game in the offline list.");

  const yours = h("dl", { class: "facts" },
    h("dt", {}, "Status"), h("dd", {}, statusLabel(g.status)),
    h("dt", {}, "Rating"), h("dd", {}, g.rating ? "★".repeat(g.rating) + "☆".repeat(5 - g.rating) : "Not rated"),
    h("dt", {}, "Time"), h("dd", {}, g.secs ? duration(g.secs) : "–"),
    h("dt", {}, "Sessions"), h("dd", {}, String(g.sessions || 0)),
    h("dt", {}, "File"), h("dd", {}, g.file));

  // Versions
  const versions = g.versions ? [h("h3", {}, "Versions"), h("p", { class: "quiet small" }, "The preferred one is what mixed lists show and start."),
    h("ul", { class: "shelf" }, g.versions.map((v) => h("li", {}, h("div", { class: "spine", stripe: stripeOf(v.system) },
      h("span", { class: "name" }, v.name), h("span", { class: "side" }, systemName(v.system),
        v.preferred ? h("span", {}, "Preferred") : h("button", { onclick: async () => {
          try { await post("/api/game/preferred", { id: v.id }); toast("Preferred: " + systemName(v.system)); route(); } catch (e) { toast(e.message, true); }
        } }, "Prefer"))))))] : null;

  // Screenshots (HOME + A in a game): the newest first, each opens full size. Only the time travels, never a path.
  const shots = g.shots && g.shots.length ? [h("h3", {}, "Screenshots"),
    h("div", { class: "shots" }, g.shots.map((at) => {
      const src = "/api/shot?id=" + encodeURIComponent(g.id) + "&at=" + encodeURIComponent(at);
      return h("a", { href: src, target: "_blank", rel: "noopener", title: at.replace(/^(\d+-\d+-\d+ \d+)-(\d+)-(\d+)/, "$1:$2:$3") }, h("img", { src, alt: "Screenshot " + at, loading: "lazy" }));
    }))] : null;

  // The game running and its title screen from the libretro collection, when the Pocket has them (read only).
  const PICTURE_NAMES = { snap: "In the game", title: "Title screen" };
  const pictures = g.pictures && g.pictures.length ? [h("h3", {}, "Pictures"),
    h("div", { class: "shots" }, g.pictures.map((kind) => {
      const src = "/api/cover?id=" + encodeURIComponent(g.id) + "&kind=" + kind;
      return h("a", { href: src, target: "_blank", rel: "noopener", title: PICTURE_NAMES[kind] }, h("img", { src, alt: PICTURE_NAMES[kind], loading: "lazy" }));
    }))] : null;

  view(
    h("a", { class: "back", href: "#/system/" + encodeURIComponent(g.system) }, g.systemName),
    label,
    h("div", { class: "cols" }, h("div", {}, h("h3", {}, "About the game"), facts), h("div", {}, h("h3", {}, "Your game"), yours)),
    h("h3", {}, "Status"), seg,
    h("h3", {}, "My rating"), stars,
    h("h3", {}, "Note"), h("div", { class: "note" }, note, saveNote),
    h("h3", {}, "Stickers"), lib.stickers.length ? chips : h("p", { class: "quiet small" }, "No stickers yet."), h("div", { class: "note" }, newSticker, addSticker),
    pictures,
    shots,
    versions);
}

// --- shelves -------------------------------------------------------------------------------------------------------

function shelvesView() {
  const lib = S.lib;
  const rename = async (what, cur, max, send) => {
    const name = prompt("New name", cur);
    if (name === null || !name.trim()) return;
    try { await send(name.trim().slice(0, max)); S.lib = await api("/api/library"); toast("Renamed"); shelvesView(); } catch (e) { toast(e.message, true); }
  };
  const del = async (what, send) => {
    if (!confirm(`Delete “${what}”? Your games stay; only the ${"shelf"} goes.`)) return;
    try { await send(); S.lib = await api("/api/library"); toast("Deleted"); shelvesView(); } catch (e) { toast(e.message, true); }
  };
  const manage = (href, name, side, onRename, onDelete) => h("li", {}, h("div", { class: "row" },
    h("a", { class: "spine", href }, h("span", { class: "name" }, name), h("span", { class: "side" }, side)),
    onRename ? h("button", { onclick: onRename }, "Rename") : null, onDelete ? h("button", { onclick: onDelete }, "Delete") : null));
  view(
    h("h2", {}, "Favourites"),
    h("ul", { class: "shelf" }, h("li", {}, h("a", { class: "spine", href: "#/favorites", stripe: "var(--amber)" }, h("span", { class: "name" }, "Favourites"), h("span", { class: "side" }, String(lib.favorites))))),
    h("h2", {}, "Stickers"),
    lib.stickers.length ? h("ul", { class: "shelf" }, lib.stickers.map((s) => manage("#/sticker/" + s.id, s.name, s.games + (s.pinned ? " · on Home" : ""),
      () => rename(s.name, s.name, lib.stickerMax, (name) => post("/api/stickers/rename", { sticker: s.id, name })),
      () => del(s.name, () => post("/api/stickers/delete", { sticker: s.id }))))) : h("p", { class: "quiet" }, "Put stickers on games from a game's page, or on the Pocket."),
    h("h2", {}, "Smart shelves"),
    lib.shelves.length ? h("ul", { class: "shelf" }, lib.shelves.map((s) => manage("#/shelf/" + s.index, s.name, s.pinned ? "on Home" : "",
      () => rename(s.name, s.name, lib.shelfMax, (name) => post("/api/shelves/rename", { index: s.index, was: s.name, name })),
      () => del(s.name, () => post("/api/shelves/delete", { index: s.index, was: s.name }))))) : h("p", { class: "quiet" }, "Make smart shelves on the Pocket with Curator's Lens."),
    lib.shelves.length && !lib.shelvesReady ? h("p", { class: "quiet small" }, "Curator is still reading the library; the shelves' games show in a moment.") : null);
}

// --- upload --------------------------------------------------------------------------------------------------------

function extOf(name) {
  return name.toLowerCase();
}

function systemsFor(name, systems) {
  const lower = extOf(name);
  return systems.filter((s) => s.extensions.some((e) => lower.endsWith("." + e) && lower.length > e.length + 1));
}

function uploadXhr(url, file, row) {
  return new Promise((resolve) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", url);
    xhr.setRequestHeader("X-CSRF-Token", S.csrf || "");
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    xhr.upload.onprogress = (e) => { if (e.lengthComputable) row.bar.value = e.loaded / e.total; };
    xhr.onload = () => {
      let b = {};
      try { b = JSON.parse(xhr.responseText); } catch (e) { /* not JSON */ }
      resolve({ ok: xhr.status === 200, status: xhr.status, body: b });
    };
    xhr.onerror = () => resolve({ ok: false, status: 0, body: { error: "The connection dropped. Nothing was kept." } });
    xhr.send(file);
  });
}

function fileRow(name, sizeText) {
  const state = h("span", { class: "quiet small" }, "Waiting");
  const bar = h("progress", { max: "1", value: "0" });
  const el = h("div", { class: "file-row" }, h("span", { class: "fname" }, name, " ", h("span", { class: "quiet small" }, sizeText)), state, bar);
  return { el, bar, state };
}

/// Checks, asks before replacing, then uploads one file. `q` is the query without overwrite.
async function sendOne(file, q, row) {
  let check;
  try {
    const r = await fetch("/api/upload/check?" + new URLSearchParams(Object.assign({ size: file.size }, q)), { method: "POST", credentials: "same-origin", headers: { "X-CSRF-Token": S.csrf || "" } });
    check = await r.json();
  } catch (e) {
    check = { ok: false, error: "The Pocket did not answer. Nothing was sent." };
  }
  if (!check.ok && !check.error) check.error = "The Pocket refused this file.";
  if (!check.ok) { row.state.className = "alert small"; row.state.textContent = check.error; row.bar.remove(); return false; }
  let overwrite = 0;
  if (check.exists) {
    if (!confirm(`${check.path} is there already. Replace it?`)) { row.state.textContent = "Kept the one on the Pocket"; row.bar.remove(); return false; }
    overwrite = 1;
  }
  row.state.textContent = "Uploading to " + check.path;
  const r = await uploadXhr("/api/upload?" + new URLSearchParams(Object.assign({ overwrite }, q)), file, row);
  if (r.ok) {
    row.state.className = "ok small";
    row.state.textContent = (r.body.replaced ? "Replaced " : "Saved as ") + r.body.path + (r.body.bios ? " · " + r.body.bios + " is ready" : "");
    row.bar.value = 1;
    return true;
  }
  row.state.className = "alert small";
  row.state.textContent = r.body.error || "Not uploaded (" + r.status + ").";
  row.bar.remove();
  return false;
}

function dropZone(text, multiple, onFiles, dir) {
  const input = h("input", { type: "file", multiple: multiple ? true : null, hidden: true });
  if (dir) { input.setAttribute("webkitdirectory", ""); input.setAttribute("directory", ""); }
  input.addEventListener("change", () => { onFiles([...input.files]); input.value = ""; });
  const zone = h("div", { class: "drop" }, h("p", {}, text), h("button", { type: "button", onclick: () => input.click() }, dir ? "Choose a folder" : "Choose files"), input);
  zone.addEventListener("dragover", (e) => { e.preventDefault(); zone.classList.add("over"); });
  zone.addEventListener("dragleave", () => zone.classList.remove("over"));
  zone.addEventListener("drop", (e) => { e.preventDefault(); zone.classList.remove("over"); onFiles([...e.dataTransfer.files]); });
  return zone;
}

async function uploadView() {
  const u = await api("/api/uploads");
  const free = u.free !== null && u.free !== undefined ? size(Math.max(0, u.free - u.reserve)) + " free on the SD card" : "";

  // Games
  const gameRows = h("div", {});
  const pick = h("select", { "aria-label": "System" }, h("option", { value: "" }, "By file type"), u.systems.map((s) => h("option", { value: s.id }, s.name)));
  const noSystems = u.systems.length ? null : h("p", { class: "alert small" }, "No system folder on the Pocket takes uploads. Check that the SD card with the games is in.");
  const sendGames = async (filesIn) => {
    for (const file of filesIn) {
      const row = fileRow(file.name, size(file.size));
      gameRows.append(row.el);
      let system = pick.value;
      if (!system) {
        const fits = systemsFor(file.name, u.systems);
        if (fits.length === 1) system = fits[0].id;
        else if (fits.length > 1) {
          const names = fits.map((s, i) => `${i + 1}. ${s.name}`).join("\n");
          const n = Number(prompt(`${file.name} fits several systems. Which one?\n${names}`, "1"));
          system = fits[n - 1] ? fits[n - 1].id : "";
        }
      }
      if (!system) { row.state.className = "alert small"; row.state.textContent = "No system takes this file type. Choose one above."; row.bar.remove(); continue; }
      await sendOne(file, { kind: "rom", system, name: file.name }, row);
    }
  };
  const games = h("div", { class: "panel" },
    h("h3", {}, "Games"),
    h("p", { class: "quiet small" }, "Each file goes into its system's folder, found by its file type. A file with the same name is only replaced if you say so."),
    noSystems,
    h("div", { class: "row" }, h("span", { class: "quiet" }, "System"), pick),
    dropZone("Drop game files here", true, sendGames), gameRows);

  // BIOS
  const missing = u.bios.filter((b) => b.missing);
  const biosRows = h("div", {});
  const bios = h("div", { class: "panel" },
    h("h3", {}, "BIOS files"),
    h("p", { class: "quiet small" }, "They go into roms/bios. Below: what the Pocket's own emulator needs for the systems that can't start without one."),
    h("ul", { class: "checks" }, u.bios.map((b) => b.found ? h("li", { class: "ok" }, `${b.label}: ${b.found}`) : h("li", { class: "missing" }, b.missing))),
    missing.length === 0 ? h("p", { class: "ok small" }, "Nothing missing.") : null,
    dropZone("Drop BIOS files here", true, async (filesIn) => {
      for (const file of filesIn) {
        const row = fileRow(file.name, size(file.size));
        biosRows.append(row.el);
        const need = u.bios.find((b) => b.files.some((f) => f.includes("/") && f.split("/").pop().toLowerCase() === file.name.toLowerCase()));
        const sub = need ? need.files.find((f) => f.toLowerCase().endsWith("/" + file.name.toLowerCase())).split("/")[0] : "";
        await sendOne(file, { kind: "bios", name: file.name, sub }, row);
      }
      uploadView();
    }), biosRows);

  // PortMaster
  const ports = h("div", { class: "panel" },
    h("h3", {}, "PortMaster: your own game files"),
    u.ports.length === 0 ? h("p", { class: "quiet" }, "No installed port is waiting for files.") : null,
    u.ports.map((p) => {
      const rows = h("div", {});
      const where = p.folders.length ? "Into " + p.folders.join(", ") : p.files.length ? "The files " + p.files.join(", ") : "";
      const send = async (filesIn, fromFolder) => {
        for (const file of filesIn) {
          // A chosen folder's own name is left out: its contents go where the port wants them.
          let rel = fromFolder && file.webkitRelativePath ? file.webkitRelativePath.split("/").slice(1).join("/") : file.name;
          const row = fileRow(rel, size(file.size));
          rows.append(row.el);
          await sendOne(file, { kind: "port", dir: p.dir, rel, name: file.name }, row);
        }
      };
      return h("div", {},
        h("h3", {}, p.title),
        h("p", { class: "small" }, p.instructions),
        where ? h("p", { class: "quiet small" }, where + ". They wait in roms/port-files/" + p.dir + " and move into the port when you start it.") : null,
        p.waiting ? h("p", { class: "quiet small" }, p.waiting + " waiting for the next start.") : null,
        p.drop ? h("div", { class: "stack" }, dropZone("Drop files here", true, (f) => send(f, false)), dropZone("Or a whole folder: its contents keep their folders", false, (f) => send(f, true), true)) : h("p", { class: "quiet small" }, "Its instructions name no folder: copy these files with the SD card or over SSH."),
        rows);
    }));

  view(h("h2", {}, "Upload", " ", h("span", { class: "count" }, free)), games, bios, ports);
}

// --- stats ---------------------------------------------------------------------------------------------------------

function svg(tag, attrs, ...kids) {
  const el = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [k, v] of Object.entries(attrs || {})) el.setAttribute(k, v);
  for (const k of kids.flat()) if (k) el.append(k instanceof Node ? k : document.createTextNode(String(k)));
  return el;
}

// Charts are drawn for the width they get; a chart made by `chart(draw)` is drawn again when the window changes size
// (a phone turned, a window made wider), so its type never ends up scaled up or down.
function chartWidth() {
  return Math.round(Math.max(300, Math.min(760, $("main").clientWidth || 760)));
}

function chart(draw) {
  const el = draw(chartWidth());
  el._draw = draw;
  return el;
}

let resizeTimer;
window.addEventListener("resize", () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    for (const el of document.querySelectorAll("svg.chart")) if (el._draw) el.replaceWith(chart(el._draw));
  }, 150);
});

function batteryChart(b, W) {
  const H = Math.round(W * 0.36) + 30, L = 34, R = 8, T = 10, B = 26;
  const t1 = b.now, t0 = t1 - b.keep;
  const x = (t) => L + ((t - t0) / (t1 - t0)) * (W - L - R);
  const y = (p) => T + (1 - p / 100) * (H - T - B);
  const out = svg("svg", { class: "chart", viewBox: `0 0 ${W} ${H}`, width: W, height: H, role: "img", "aria-label": "Battery over the last 48 hours" });
  for (const p of [0, 50, 100]) out.append(svg("line", { class: "grid", x1: L, x2: W - R, y1: y(p), y2: y(p) }), svg("text", { x: 0, y: y(p) + 4 }, p + " %"));
  for (let hrs = 48; hrs >= 0; hrs -= 12) {
    const t = t1 - hrs * 3600;
    out.append(svg("text", { x: x(t) - (hrs === 0 ? 22 : 12), y: H - 6 }, hrs === 0 ? "now" : "−" + hrs + " h"));
  }
  // Charging as a quiet band, the level as one line broken where the Pocket was off or asleep.
  let seg = [], path = "";
  const flush = () => { if (seg.length > 1) path += "M" + seg.map((s) => x(s[0]).toFixed(1) + "," + y(s[1]).toFixed(1)).join("L"); seg = []; };
  let prev = null;
  for (const s of b.samples) {
    if (prev && s[0] - prev[0] > b.gap) flush();
    if (s[2]) out.append(svg("rect", { class: "charge", x: x(s[0]), y: T, width: Math.max(1, (60 / (t1 - t0)) * (W - L - R)), height: H - T - B }));
    seg.push(s);
    prev = s;
  }
  flush();
  out.append(svg("path", { class: "line", d: path }));
  return out;
}

// --- play history (play_history::stats_json, docs/HISTORY.md "Read API"; no paths, games have ids) ---------------

const day = (d) => (d ? new Date(d + "T12:00:00") : null);
const dayText = (d, opts) => (d ? day(d).toLocaleDateString([], opts || { day: "numeric", month: "short" }) : "");
const span = (a, b) => (a === b ? dayText(a) : dayText(a) + " – " + dayText(b));

/// One bar per day (a week, a month) or per week (a year), the period's time as their height.
function periodChart(p, kind, W) {
  const H = Math.round(Math.min(220, W * 0.4)), L = 8, R = 8, T = 18, B = 24;
  const n = Math.max(1, p.bars.length), max = Math.max(60, ...p.bars.map((b) => b.secs));
  const slot = (W - L - R) / n, bw = Math.max(2, slot * 0.7);
  const out = svg("svg", { class: "chart", viewBox: `0 0 ${W} ${H}`, width: W, height: H, role: "img", "aria-label": "Time played per " + (kind === "year" ? "week" : "day") });
  out.append(svg("line", { class: "grid", x1: L, x2: W - R, y1: H - B, y2: H - B }));
  let lastMonth = -1;
  p.bars.forEach((b, i) => {
    const x = L + i * slot + (slot - bw) / 2, hgt = b.secs ? Math.max(2, (b.secs / max) * (H - T - B)) : 0;
    const when = kind === "year" ? "Week of " + dayText(b.first) : dayText(b.first, { weekday: "short", day: "numeric", month: "short" });
    const bar = svg("rect", { class: "bar", x: x.toFixed(1), y: (H - B - hgt).toFixed(1), width: bw.toFixed(1), height: hgt.toFixed(1), rx: Math.min(3, bw / 3) },
      svg("title", {}, when + ": " + (b.secs ? duration(b.secs) : "nothing")));
    out.append(bar);
    const d = day(b.first);
    let label = null;
    if (kind === "week") label = d.toLocaleDateString([], { weekday: "narrow" });
    else if (kind === "month" && (i === 0 || (d.getDate() - 1) % 7 === 0)) label = String(d.getDate());
    else if (kind === "year" && d.getMonth() !== lastMonth) { label = d.toLocaleDateString([], { month: "narrow" }); lastMonth = d.getMonth(); }
    if (label) out.append(svg("text", { x: (L + i * slot + slot / 2).toFixed(1), y: H - 6, "text-anchor": "middle" }, label));
  });
  const top = p.bars.reduce((a, b) => (b.secs > a.secs ? b : a), p.bars[0] || { secs: 0 });
  if (top.secs) out.append(svg("text", { x: W - R, y: 12, "text-anchor": "end" }, "Most: " + duration(top.secs)));
  return out;
}

function historyGame(g, side) {
  const name = h("span", { class: "name" }, g.name || "A game without a name");
  const stripe = stripeOf(g.system);
  const body = [h("span", { class: "tile", stripe }, (g.name || "?").slice(0, 1)), name, h("span", { class: "side" }, side)];
  return h("li", {}, g.id ? h("a", { class: "spine", href: "#/game/" + g.id, stripe }, body) : h("div", { class: "spine", stripe }, body));
}

function historyBlock(hist) {
  const box = h("div", { class: "history" });
  const draw = () => {
    const kind = S.period || "week", p = hist[kind];
    const seg = h("div", { class: "seg", role: "group", "aria-label": "Period" },
      [["week", "Week"], ["month", "Month"], ["year", "Year"]].map(([k, l]) => h("button", { class: k === kind ? "on" : "", "aria-pressed": k === kind ? "true" : "false", onclick: () => { S.period = k; history.replaceState(null, "", "#/stats/" + k); draw(); } }, l)));
    const head = h("h3", {}, p.label, " ", h("span", { class: "count" }, span(p.first, p.last)));
    let body;
    if (p.not_recorded) {
      body = h("p", { class: "quiet" }, `Not recorded: the history counts from ${dayText(hist.counting_since, { day: "numeric", month: "long", year: "numeric" })}.`);
    } else if (!p.secs && !p.sessions) {
      body = h("p", { class: "quiet" }, "Nothing played in this period.");
    } else {
      const sysMax = Math.max(1, ...p.systems.map((x) => x.secs));
      const sysBars = h("div", { class: "bars" }, p.systems.map((x) => h("div", { class: "bar-row" },
        h("span", { class: "n" }, systemName(x.system)), h("span", { class: "track" }, h("span", { class: "fill", stripe: stripeOf(x.system) })), h("span", { class: "v" }, duration(x.secs)))));
      [...sysBars.querySelectorAll(".fill")].forEach((el, i) => (el.style.width = Math.max(2, (p.systems[i].secs / sysMax) * 100) + "%"));
      const streak = p.longest_streak && p.longest_streak.days > 1 ? `Longest run in this period: ${p.longest_streak.days} days in a row (${span(p.longest_streak.from, p.longest_streak.to)}).` : null;
      const now = p.current_streak > 1 ? ` Played ${p.current_streak} days in a row up to now.` : "";
      body = [
        h("div", { class: "tiles" },
          h("div", { class: "tile-stat" }, h("div", { class: "big" }, duration(p.secs)), h("div", { class: "quiet" }, "played")),
          h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(p.sessions)), h("div", { class: "quiet" }, p.sessions === 1 ? "session" : "sessions")),
          h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(p.days_played)), h("div", { class: "quiet" }, p.days_played === 1 ? "day played" : "days played")),
          p.new_games ? h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(p.new_games)), h("div", { class: "quiet" }, p.new_games === 1 ? "game new to you" : "games new to you")) : null),
        p.bars.length ? chart((W) => periodChart(p, kind, W)) : null,
        streak || now ? h("p", { class: "quiet small" }, (streak || "") + now) : null,
        p.games.length ? [h("h3", {}, "Most played"), h("ul", { class: "shelf" }, p.games.map((g) => historyGame(g, [g.new ? h("span", { class: "quiet" }, "new · ") : null, duration(g.secs)])))] : null,
        p.systems.length ? [h("h3", {}, "By system"), sysBars] : null,
        p.came_back.length ? [h("h3", {}, "Back after a long break"), h("ul", { class: "shelf" }, p.came_back.map((g) => historyGame(g, `after ${g.gap_days} days`)))] : null,
        p.completed.length ? [h("h3", {}, "Completed"), h("ul", { class: "shelf" }, p.completed.map((g) => historyGame(g, dayText(g.date))))] : null,
      ];
    }
    fill(box, seg, head, body);
  };
  draw();
  return box;
}

function recentSessions(recent) {
  if (!recent || !recent.length) return null;
  return [h("h3", {}, "Recent sessions"), h("ul", { class: "shelf" }, recent.map((r) => {
    const when = r.date ? new Date(r.start * 1000).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) : "before the clock was set";
    const time = r.ended ? duration(r.secs) : "no time: it did not end normally";
    return historyGame(r, when + " · " + time);
  }))];
}

async function statsView() {
  const s = await api("/api/stats");
  const hist = s.history;
  const max = Math.max(1, ...s.systems.map((x) => x.secs));
  const bars = h("div", { class: "bars" }, s.systems.map((x) => h("div", { class: "bar-row" },
    h("span", { class: "n" }, x.name),
    h("span", { class: "track" }, h("span", { class: "fill", stripe: x.stripe })),
    h("span", { class: "v" }, duration(x.secs)))));
  // Bar widths through the CSSOM (no style attributes under the page's CSP).
  [...bars.querySelectorAll(".fill")].forEach((el, i) => (el.style.width = Math.max(2, (s.systems[i].secs / max) * 100) + "%"));
  let battery = h("p", { class: "quiet" }, "No battery history yet.");
  try {
    const b = await api("/api/battery");
    if (b.samples.length > 1) battery = chart((W) => batteryChart(b, W));
  } catch (e) { /* keep the line */ }

  const historyPart = hist
    ? [
        h("p", { class: "quiet small" }, `Counted since ${dayText(hist.counting_since, { day: "numeric", month: "long", year: "numeric" })}. In all: ${duration(hist.all_time.secs)} over ${hist.all_time.sessions} ${hist.all_time.sessions === 1 ? "session" : "sessions"}` +
          (hist.all_time.carried_secs ? `, ${duration(hist.all_time.carried_secs)} of it from before the history began.` : ".")),
        hist.damaged ? h("p", { class: "alert small" }, "Part of the history file could not be read; those sessions are not counted.") : null,
        historyBlock(hist),
        recentSessions(hist.recent),
      ]
    : h("p", { class: "quiet" }, "Play history is off, or nothing has been recorded yet (Settings › Playing › Curator › Play history on the Pocket). The totals below are the ones the library keeps for every game.");

  view(
    h("h2", {}, "Play history"),
    historyPart,
    h("h2", {}, hist ? "All time, by game" : "Play time"),
    h("div", { class: "tiles" },
      h("div", { class: "tile-stat" }, h("div", { class: "big" }, duration(s.totalSecs)), h("div", { class: "quiet" }, "played in all")),
      h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(s.sessions)), h("div", { class: "quiet" }, "sessions of a minute or more")),
      h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(s.played)), h("div", { class: "quiet" }, s.played === 1 ? "game played" : "games played"))),
    h("p", { class: "quiet small" }, "Kept with each game in the library: time in the game, and sessions of a minute or more."),
    s.systems.length ? [h("h3", {}, "By system"), bars] : h("p", { class: "quiet" }, "Nothing played yet. Play time shows here after the first game."),
    s.games.length ? [h("h3", {}, s.played > s.games.length ? `Most played (top ${s.games.length})` : "Most played"), h("ul", { class: "shelf" }, s.games.map(gameSpine))] : null,
    h("h2", {}, "Battery"), h("p", { class: "quiet small" }, "The last 48 hours. Green: charging. Gaps: off or asleep."), battery);
}

document.addEventListener("DOMContentLoaded", start);
