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

// --- the Pocket's language (src/web/texts.rs) ----------------------------------------------------------------------
// t("English") is the text in the language set on the Pocket; {n}, {s} and the like are filled from vars. Write each
// text as a literal: the Pocket finds them in this file to translate them. What its tables lack stays English.

const T = {};

function t(en, vars) {
  let s = Object.prototype.hasOwnProperty.call(T, en) ? T[en] : en;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (Object.prototype.hasOwnProperty.call(vars, k) ? String(vars[k]) : m));
  return s;
}

async function loadTexts() {
  try {
    const r = await fetch("/api/texts", { credentials: "same-origin" });
    if (!r.ok) return;
    const b = await r.json();
    Object.assign(T, b.texts || {});
    if (b.lang) document.documentElement.lang = b.lang;
  } catch (e) {
    // English, then.
  }
}

function toast(msg, err) {
  const t = h("div", { class: "toast" + (err ? " err" : ""), role: "status" }, msg);
  document.body.append(t);
  setTimeout(() => t.remove(), err ? 5000 : 2400);
}

// Dates, numbers and units in the Pocket's language too (set by loadTexts), not the browser's.
const locale = () => document.documentElement.lang || undefined;

function duration(secs) {
  if (!secs) return "–";
  const m = Math.round(secs / 60);
  if (m < 60) return t("{n} min", { n: m });
  const hrs = Math.floor(m / 60), rest = m % 60;
  return rest ? t("{n} h {m} min", { n: hrs, m: rest }) : t("{n} h", { n: hrs });
}

function ago(at) {
  if (!at) return "";
  const d = Math.max(0, Date.now() / 1000 - at);
  if (d < 3600) return t("{n} min ago", { n: Math.max(1, Math.round(d / 60)) });
  if (d < 86400) return t("{n} h ago", { n: Math.round(d / 3600) });
  const days = Math.round(d / 86400);
  return days === 1 ? t("yesterday") : t("{n} days ago", { n: days });
}

// "1.5 GB", "1,5 Go": the unit and the decimal comma as the language writes them.
function size(bytes) {
  const unit = (n, u, digits) => new Intl.NumberFormat(locale(), { style: "unit", unit: u, maximumFractionDigits: digits }).format(n);
  if (bytes < 1048576) return unit(Math.max(1, Math.round(bytes / 1024)), "kilobyte", 0);
  if (bytes < 1073741824) return unit(Math.round(bytes / 1048576), "megabyte", 0);
  return unit(Math.round(bytes / 107374182.4) / 10, "gigabyte", 1);
}

const percent = (n) => new Intl.NumberFormat(locale(), { style: "percent" }).format(n / 100);

// A function: the texts are only there once loadTexts has run.
const STATUS = () => [
  ["not_started", t("Not started")], ["playing", t("Playing")], ["paused", t("Paused")], ["completed", t("Completed")], ["dropped", t("Dropped")],
];
const statusLabel = (k) => (STATUS().find((s) => s[0] === k) || STATUS()[0])[1];
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
  let r;
  try {
    r = await fetch(path, o);
  } catch (e) {
    // No answer at all: the browser's own words ("Failed to fetch") would be English and say nothing.
    throw new Error(t("The Pocket did not answer. Try again in a moment."));
  }
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
    throw new Error(body ? body.error : t("Pair this browser first."));
  }
  if (!r.ok) throw new Error((body && body.error) || t("Something went wrong ({n}).", { n: r.status }));
  return body;
}

const post = (path, json) => api(path, { method: "POST", json });

// --- frame: session, tabs, status ----------------------------------------------------------------------------------

async function start() {
  await loadTexts();
  frameTexts();
  if (window.PocketMediaUpdate) window.PocketMediaUpdate.translateNavTabs();
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
  $("who").textContent = s.via === "tailscale" ? t("Signed in with Tailscale as {s}", { s: s.name }) : t("Paired: {s}", { s: s.name });
  if (s.via === "paired") $("who").append(" · ", h("a", { href: "#/forget", class: "quiet" }, t("Forget this browser")));
  $("tabs").hidden = false;
  const savesTab = $("tab-saves");
  if (savesTab) savesTab.textContent = t("Saves");
  $("diagnostics").textContent = t("Diagnosis");
  $("diagnostics").hidden = false;
  window.addEventListener("hashchange", route);
  document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && pollStatus());
  // The systems' names and colours, for "Now playing" too (a game running answers "playing": then later).
  try { S.lib = await api("/api/library"); } catch (e) { S.lib = null; }
  await pollStatus();
  if (window.PocketMediaUpdate) window.PocketMediaUpdate.refreshBadge();
  route();
}

// The page's frame in index.html: the tabs and the first line, in the Pocket's language.
function frameTexts() {
  const TABS = { library: t("Library"), shelves: t("Shelves"), upload: t("Upload"), stats: t("Stats") };
  for (const a of document.querySelectorAll("#tabs a")) if (TABS[a.dataset.tab]) a.textContent = TABS[a.dataset.tab];
  $("tabs").setAttribute("aria-label", t("Pages"));
  const loading = document.querySelector("#main .quiet");
  if (loading) loading.textContent = t("Loading…");
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
    if (b.last) $("battery").textContent = b.last.charging ? t("Charging · {s}", { s: percent(b.last.percent) }) : percent(b.last.percent);
  } catch (e) {
    /* no battery, or a game runs */
  }
  S.statusTimer = setTimeout(pollStatus, S.playing ? 10000 : 20000);
}

function renderNow(p, running) {
  const box = $("now");
  if (!p) {
    box.hidden = !running;
    box.replaceChildren(running ? h("p", { class: "quiet" }, t("A game is running. The lists wait until it ends.")) : "");
    return;
  }
  box.hidden = false;
  const shot = h("div", { class: "shot" });
  if (S.shot) shot.append(S.shot);
  const btn = h("button", { onclick: () => takeShot(shot, btn) }, t("Screenshot"));
  fill(box,
    h("div", { class: "label", stripe: p.stripe },
      h("div", { class: "body" },
        h("div", { class: "sub" }, t("Now playing")),
        h("h1", {}, p.name),
        h("div", { class: "sub" }, t("{s} · since {s2}", { s: p.systemName, s2: new Date(p.since * 1000).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" }) })),
        h("div", { class: "actions" }, btn))),
    shot,
    running ? h("p", { class: "quiet small" }, t("While a game runs, the lists and uploads wait until it ends.")) : null);
}

async function takeShot(box, btn) {
  btn.disabled = true;
  try {
    const r = await fetch("/api/screenshot?t=" + Date.now(), { credentials: "same-origin" });
    if (!r.ok) throw new Error((await r.json()).error);
    const url = URL.createObjectURL(await r.blob());
    S.shot = h("img", { src: url, alt: t("The Pocket's screen") });
    box.replaceChildren(S.shot);
    const age = Number(r.headers.get("X-Shot-Age") || 0);
    if (r.headers.get("X-Screen-Hidden")) box.append(h("p", { class: "quiet small" }, t("Hidden: the Pocket shows Settings, which can show codes and passwords. Try again when it is back on Home or in a game.")));
    else if (age > 0) box.append(h("p", { class: "quiet small" }, t("Taken {n} s ago. A new one every 10 seconds at most.", { n: age })));
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
  if (window.PocketMediaUpdate) {
    window.PocketMediaUpdate.stopPolls();
    window.PocketMediaUpdate.syncBadge();
  }
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
      case "saves": tab("saves"); return await savesView();
      case "stats": tab("stats"); if (["week", "month", "year"].includes(arg)) S.period = arg; return await statsView();
      case "diagnostics": tab(""); return await diagnosticsView();
      case "more": tab("more"); return window.PocketMediaUpdate ? await window.PocketMediaUpdate.moreView() : libraryView();
      case "photos": tab("more"); return window.PocketMediaUpdate ? await window.PocketMediaUpdate.photosView() : libraryView();
      case "updates": tab("more"); return window.PocketMediaUpdate ? await window.PocketMediaUpdate.updatesView() : libraryView();
      case "forget": return await forget();
      default: tab("library"); return libraryView();
    }
  } catch (e) {
    if (e instanceof Playing) {
      view(h("h2", {}, t("A game is running")), h("p", { class: "quiet" }, t("The web companion waits until it ends, so the game has the Pocket to itself. This page carries on by itself.")));
    } else if (S.csrf) {
      view(h("p", { class: "alert" }, e.message));
    }
  }
}

// --- pairing -------------------------------------------------------------------------------------------------------

function showPair(s) {
  $("tabs").hidden = true;
  $("diagnostics").hidden = true;
  const input = h("input", { type: "text", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "7", class: "code-input", "aria-label": t("Pairing code") });
  const msg = h("p", { class: "quiet" });
  const go = async (code) => {
    msg.className = "quiet";
    msg.textContent = t("Pairing…");
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
    h("div", { class: "row" }, input, h("button", { class: "primary", type: "submit" }, t("Pair"))));
  view(
    h("h2", {}, t("Pair this browser")),
    h("p", {}, t("On the Pocket, open Settings › Connections › Web companion › Pair a browser, and type the six digits it shows.")),
    form, msg,
    h("p", { class: "quiet small" }, t("Your own devices on Tailscale open the companion without a code, unless that is switched off on the Pocket.")));
  const m = /pair=(\d{6})/.exec(location.hash);
  if (m) { input.value = m[1]; go(m[1]); } else input.focus();
}

async function forget() {
  if (!confirm(t("Forget this browser? You will need a new code from the Pocket."))) return history.back();
  await post("/api/logout", {});
  location.hash = "";
  location.reload();
}

// --- library -------------------------------------------------------------------------------------------------------

function searchBox(value) {
  const input = h("input", { type: "search", placeholder: t("Search all games"), value: value || "", "aria-label": t("Search"), enterkeyhint: "search" });
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
  // The pictures made of your shelf (Settings › Games › Screenshots on the Pocket): view, download, or delete.
  const pics = window.PocketMediaUpdate
    ? window.PocketMediaUpdate.shelfPictures(lib.shelfPictures, async () => { S.lib = await api("/api/library"); libraryView(); })
    : (lib.shelfPictures && lib.shelfPictures.length ? [h("h3", {}, t("Your shelf as a picture")),
        h("div", { class: "shots" }, lib.shelfPictures.map((at) => {
          const src = "/api/shot?shelf=1&at=" + encodeURIComponent(at);
          return h("a", { href: src, target: "_blank", rel: "noopener", download: "pocket-ui shelf " + at + ".png", title: at.replace(/^(\d+-\d+-\d+ \d+)-(\d+)-(\d+)/, "$1:$2:$3") }, h("img", { src, alt: t("Shelf picture") + " " + at, loading: "lazy" }));
        }))] : null);
  view(searchBox(), lib.systems.length ? list : h("p", { class: "quiet" }, t("No games on the Pocket yet. Add some under Upload.")), pics);
}

function gameSpine(g) {
  const art = g.cover ? h("img", { src: boxArtUrl(g), alt: "", loading: "lazy" }) : h("span", { class: "tile", stripe: g.stripe }, (g.name || "?").slice(0, 1));
  const side = h("span", { class: "side" }, mark(g.status), g.favorite ? h("span", { class: "star", title: t("Favourite") }, "★") : null, g.secs ? duration(g.secs) : "");
  return h("li", {}, h("a", { class: "spine", href: "#/game/" + g.id, stripe: g.stripe }, art, h("span", { class: "name" }, g.name), side));
}

async function listView(q) {
  const params = new URLSearchParams(q).toString();
  const r = await api("/api/games?" + params);
  const head = q.q !== undefined ? searchBox(q.q) : null;
  const shown = r.games.length < r.total ? " · " + t("first {n}", { n: r.games.length }) : "";
  view(
    head,
    h("a", { class: "back", href: q.system || q.q !== undefined ? "#/library" : "#/shelves" }, q.system || q.q !== undefined ? t("Library") : t("Shelves")),
    // The Pocket names the favourites "Favourites" in English; a system's, a shelf's and a sticker's names are theirs.
    h("h2", {}, q.favorites ? t("Favourites") : r.title, " ", h("span", { class: "count" }, (r.total === 1 ? t("1 game") : t("{n} games", { n: r.total })) + shown)),
    r.games.length ? h("ul", { class: "shelf" }, r.games.map(gameSpine)) : h("p", { class: "quiet" }, q.q !== undefined ? t("No game matches “{s}”.", { s: q.q }) : t("Nothing here yet.")));
}

async function gameView(id) {
  const g = await api("/api/game?id=" + encodeURIComponent(id));
  const lib = S.lib;
  const favText = () => (g.favorite ? "★ " : "☆ ") + t("Favourite");
  const fav = h("button", { class: g.favorite ? "on" : "" }, favText());
  fav.addEventListener("click", async () => {
    try { await post("/api/game/favorite", { id, on: !g.favorite }); g.favorite = !g.favorite; fav.className = g.favorite ? "on" : ""; fav.textContent = favText(); }
    catch (e) { toast(e.message, true); }
  });
  const sub = [g.systemName, g.secs ? t("{s} played", { s: duration(g.secs) }) : t("Not played yet"), g.lastPlayed ? t("last played {t}", { t: ago(g.lastPlayed) }) : null].filter(Boolean).join(" · ");
  const art = h("div", { class: "art" });
  const label = h("div", { class: "label", stripe: g.stripe },
    h("div", { class: "body" }, h("h1", {}, g.name), h("div", { class: "sub" }, sub), h("div", { class: "actions" }, fav)),
    art);

  // Status
  const seg = h("div", { class: "seg", role: "group", "aria-label": t("Play status") });
  const paintSeg = () => seg.replaceChildren(...STATUS().map(([k, l]) => h("button", { class: g.status === k ? "on" : "", onclick: () => setStatus(k) }, mark(k), l)));
  const setStatus = async (k) => {
    try { await post("/api/game/status", { id, status: k }); g.status = k; paintSeg(); toast(t("Status: {t}", { t: statusLabel(k) })); }
    catch (e) { toast(e.message, true); }
  };
  paintSeg();

  // My rating: five stars; the one given again takes it away.
  const stars = h("div", { class: "stars", role: "group", "aria-label": t("My rating") });
  const paintStars = () => stars.replaceChildren(
    ...[1, 2, 3, 4, 5].map((n) => h("button", { class: (g.rating || 0) >= n ? "on" : "", title: n === 1 ? t("1 star") : t("{n} stars", { n }), "aria-pressed": g.rating === n ? "true" : "false", onclick: () => setRating(g.rating === n ? 0 : n) }, (g.rating || 0) >= n ? "★" : "☆")),
    h("span", { class: "quiet small" }, g.rating ? "" : t("Not rated yet")));
  const setRating = async (n) => {
    try { await post("/api/game/rating", { id, rating: n }); g.rating = n; paintStars(); toast(n ? (n === 1 ? t("Rated 1 star") : t("Rated {n} stars", { n })) : t("Rating removed")); }
    catch (e) { toast(e.message, true); }
  };
  paintStars();

  // Note
  const note = h("input", { type: "text", value: g.note || "", maxlength: String(lib.noteMax), placeholder: t("A short note"), "aria-label": t("Note") });
  const saveNote = h("button", {}, t("Save"));
  saveNote.addEventListener("click", async () => {
    try { await post("/api/game/note", { id, note: note.value }); toast(note.value.trim() ? t("Note saved") : t("Note removed")); }
    catch (e) { toast(e.message, true); }
  });

  // Facts
  const f = g.facts;
  const facts = f ? h("dl", { class: "facts" },
    f.year ? [h("dt", {}, t("Released")), h("dd", {}, String(f.year))] : null,
    f.developers && f.developers.length ? [h("dt", {}, t("Developer")), h("dd", {}, f.developers.join(", "))] : null,
    f.publishers && f.publishers.length ? [h("dt", {}, t("Publisher")), h("dd", {}, f.publishers.join(", "))] : null,
    f.genres && f.genres.length ? [h("dt", {}, t("Genre")), h("dd", {}, f.genres.slice(0, 3).join(", "))] : null,
    f.series && f.series.length ? [h("dt", {}, t("Series")), h("dd", {}, f.series.join(", "))] : null,
    f.players ? [h("dt", {}, t("Players")), h("dd", {}, f.players)] : null) : h("p", { class: "quiet" }, t("No facts about this game in the offline list."));

  const yours = h("dl", { class: "facts" },
    h("dt", {}, t("Status")), h("dd", {}, statusLabel(g.status)),
    h("dt", {}, t("Rating")), h("dd", {}, g.rating ? "★".repeat(g.rating) + "☆".repeat(5 - g.rating) : t("Not rated")),
    h("dt", {}, t("Time played")), h("dd", {}, g.secs ? duration(g.secs) : "–"),
    h("dt", {}, t("Sessions")), h("dd", {}, String(g.sessions || 0)),
    h("dt", {}, t("File")), h("dd", {}, g.file));

  // Versions
  const versions = g.versions ? [h("h3", {}, t("Versions")), h("p", { class: "quiet small" }, t("The preferred one is what mixed lists show and start.")),
    h("ul", { class: "shelf" }, g.versions.map((v) => h("li", {}, h("div", { class: "spine", stripe: stripeOf(v.system) },
      h("span", { class: "name" }, v.name), h("span", { class: "side" }, systemName(v.system),
        v.preferred ? h("span", {}, t("Preferred")) : h("button", { onclick: async () => {
          try { await post("/api/game/preferred", { id: v.id }); toast(t("Preferred: {s}", { s: systemName(v.system) })); route(); } catch (e) { toast(e.message, true); }
        } }, t("Prefer")))))))] : null;

  // Photo album and screenshots: view, download individually, delete individually, or download as ZIP.
  const shots = window.PocketMediaUpdate
    ? window.PocketMediaUpdate.gameMedia(g, () => gameView(id))
    : (g.shots && g.shots.length ? [h("h3", {}, t("Screenshots")),
        h("div", { class: "shots" }, g.shots.map((at) => {
          const src = "/api/shot?id=" + encodeURIComponent(g.id) + "&at=" + encodeURIComponent(at);
          return h("a", { href: src, target: "_blank", rel: "noopener", title: at.replace(/^(\d+-\d+-\d+ \d+)-(\d+)-(\d+)/, "$1:$2:$3") }, h("img", { src, alt: t("Screenshot") + " " + at, loading: "lazy" }));
        }))] : null);

  // Saves and states (saves.js): filled in when they are read.
  const saves = h("section", { class: "game-saves" });

  // The game running and its title screen from the libretro collection, when the Pocket has them (read only).
  const PICTURE_NAMES = { snap: t("In the game"), title: t("Title screen") };
  const pictures = g.pictures && g.pictures.length ? [h("h3", {}, t("Pictures")),
    h("div", { class: "shots" }, g.pictures.map((kind) => {
      const src = "/api/cover?id=" + encodeURIComponent(g.id) + "&kind=" + kind;
      return h("a", { href: src, target: "_blank", rel: "noopener", title: PICTURE_NAMES[kind] }, h("img", { src, alt: PICTURE_NAMES[kind], loading: "lazy" }));
    }))] : null;

  // Delete from the card: only where the Pocket can (not on dArkOS's read-only games, not on Android). Asks with the
  // name and the size; saves and states stay unless ticked.
  let remove = null;
  if (g.delete) {
    const d = g.delete;
    const withSaves = h("input", { type: "checkbox" });
    const button = h("button", { class: "danger" }, t("Delete…"));
    button.addEventListener("click", async () => {
      const saves = withSaves.checked;
      const bytes = d.bytes + (saves ? d.saves : 0);
      const ask = saves ? t("Delete “{s}” and its saves and states from the card? This frees {s2}.", { s: g.name, s2: size(bytes) })
        : t("Delete “{s}” from the card? This frees {s2}. Its saves and states stay.", { s: g.name, s2: size(bytes) });
      if (!confirm(ask)) return;
      button.disabled = true;
      try {
        await post("/api/game/delete", { id, saves });
        toast(t("Deleted {s}", { s: g.name }));
        setTimeout(() => { location.hash = "#/system/" + encodeURIComponent(g.system); }, 800);
      } catch (e) { button.disabled = false; toast(e.message, true); }
    });
    remove = [h("h3", {}, t("Delete from the card")),
      h("p", { class: "quiet small" }, d.files === 1 ? t("1 file, {s}.", { s: size(d.bytes) }) : t("{n} files, {s}.", { n: d.files, s: size(d.bytes) })),
      d.saveFiles ? h("label", { class: "check" }, withSaves, " ", t("Delete its saves and states too ({s})", { s: size(d.saves) })) : null,
      h("div", { class: "actions" }, button)];
  }

  view(
    h("a", { class: "back", href: "#/system/" + encodeURIComponent(g.system) }, g.systemName),
    label,
    coverControls(g, art),
    h("div", { class: "cols" }, h("div", {}, h("h3", {}, t("About the game")), facts), h("div", {}, h("h3", {}, t("Your game")), yours)),
    h("h3", {}, t("Status")), seg,
    h("h3", {}, t("My rating")), stars,
    h("h3", {}, t("Note")), h("div", { class: "note" }, note, saveNote),
    pictures,
    shots,
    saves,
    versions,
    remove);
  savesBlock(g.id, saves);
}

// --- shelves -------------------------------------------------------------------------------------------------------

function shelvesView() {
  const lib = S.lib;
  const rename = async (what, cur, max, send) => {
    const name = prompt(t("New name"), cur);
    if (name === null || !name.trim()) return;
    try { await send(name.trim().slice(0, max)); S.lib = await api("/api/library"); toast(t("Renamed")); shelvesView(); } catch (e) { toast(e.message, true); }
  };
  const del = async (what, send) => {
    if (!confirm(t("Delete “{s}”? Your games stay; only the shelf goes.", { s: what }))) return;
    try { await send(); S.lib = await api("/api/library"); toast(t("Deleted")); shelvesView(); } catch (e) { toast(e.message, true); }
  };
  const manage = (href, name, side, onRename, onDelete) => h("li", {}, h("div", { class: "row" },
    h("a", { class: "spine", href }, h("span", { class: "name" }, name), h("span", { class: "side" }, side)),
    onRename ? h("button", { onclick: onRename }, t("Rename")) : null, onDelete ? h("button", { onclick: onDelete }, t("Delete")) : null));
  view(
    h("h2", {}, t("Favourites")),
    h("ul", { class: "shelf" }, h("li", {}, h("a", { class: "spine", href: "#/favorites", stripe: "var(--amber)" }, h("span", { class: "name" }, t("Favourites")), h("span", { class: "side" }, String(lib.favorites))))),
    h("h2", {}, t("Smart shelves")),
    lib.shelves.length ? h("ul", { class: "shelf" }, lib.shelves.map((s) => manage("#/shelf/" + s.index, s.name, s.pinned ? t("On Home") : "",
      () => rename(s.name, s.name, lib.shelfMax, (name) => post("/api/shelves/rename", { index: s.index, was: s.name, name })),
      () => del(s.name, () => post("/api/shelves/delete", { index: s.index, was: s.name }))))) : h("p", { class: "quiet" }, t("Make smart shelves on the Pocket with Curator's Lens.")),
    lib.shelves.length && !lib.shelvesReady ? h("p", { class: "quiet small" }, t("Curator is still reading the library; the shelves' games show in a moment.")) : null);
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
    xhr.onerror = () => resolve({ ok: false, status: 0, body: { error: t("The connection dropped. Nothing was kept.") } });
    xhr.send(file);
  });
}

function fileRow(name, sizeText) {
  const state = h("span", { class: "quiet small" }, t("Waiting"));
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
    check = { ok: false, error: t("The Pocket did not answer. Nothing was sent.") };
  }
  if (!check.ok && !check.error) check.error = t("The Pocket refused this file.");
  if (!check.ok) { row.state.className = "alert small"; row.state.textContent = check.error; row.bar.remove(); return false; }
  let overwrite = 0;
  if (check.exists) {
    if (!confirm(t("{s} is there already. Replace it?", { s: check.path }))) { row.state.textContent = t("Kept the one on the Pocket"); row.bar.remove(); return false; }
    overwrite = 1;
  }
  row.state.textContent = t("Uploading to {s}", { s: check.path });
  const r = await uploadXhr("/api/upload?" + new URLSearchParams(Object.assign({ overwrite }, q)), file, row);
  if (r.ok) {
    row.state.className = "ok small";
    row.state.textContent = (r.body.replaced ? t("Replaced {s}", { s: r.body.path }) : t("Saved as {s}", { s: r.body.path })) + (r.body.bios ? " · " + t("BIOS for {s} is ready", { s: r.body.bios }) : "");
    row.bar.value = 1;
    return true;
  }
  row.state.className = "alert small";
  row.state.textContent = r.body.error || t("Not uploaded ({n}).", { n: r.status });
  row.bar.remove();
  return false;
}

function dropZone(text, multiple, onFiles, dir) {
  const input = h("input", { type: "file", multiple: multiple ? true : null, hidden: true });
  if (dir) { input.setAttribute("webkitdirectory", ""); input.setAttribute("directory", ""); }
  input.addEventListener("change", () => { onFiles([...input.files]); input.value = ""; });
  const zone = h("div", { class: "drop" }, h("p", {}, text), h("button", { type: "button", onclick: () => input.click() }, dir ? t("Choose a folder") : t("Choose files")), input);
  zone.addEventListener("dragover", (e) => { e.preventDefault(); zone.classList.add("over"); });
  zone.addEventListener("dragleave", () => zone.classList.remove("over"));
  zone.addEventListener("drop", (e) => { e.preventDefault(); zone.classList.remove("over"); onFiles([...e.dataTransfer.files]); });
  return zone;
}

/// "Missing for Famicom Disk System: disksys.rom", "… a, b or c": made here from the need's files, in the Pocket's language
/// (the Pocket's own line, `missing`, is English).
function missingBios(b) {
  if (!b.files || !b.files.length) return b.missing;
  return t("Missing for {s}: {s2}", { s: b.label, s2: new Intl.ListFormat(locale(), { type: "disjunction" }).format(b.files) });
}

async function uploadView() {
  const u = await api("/api/uploads");
  const free = u.free !== null && u.free !== undefined ? t("{s} free on the SD card", { s: size(Math.max(0, u.free - u.reserve)) }) : "";

  // Games
  const gameRows = h("div", {});
  const pick = h("select", { "aria-label": t("System") }, h("option", { value: "" }, t("By file type")), u.systems.map((s) => h("option", { value: s.id }, s.name)));
  const noSystems = u.systems.length ? null : h("p", { class: "alert small" }, t("No system folder on the Pocket takes uploads. Check that the SD card with the games is in."));
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
          const n = Number(prompt(t("{s} fits several systems. Which one?", { s: file.name }) + "\n" + names, "1"));
          system = fits[n - 1] ? fits[n - 1].id : "";
        }
      }
      if (!system) { row.state.className = "alert small"; row.state.textContent = t("No system takes this file type. Choose one above."); row.bar.remove(); continue; }
      await sendOne(file, { kind: "rom", system, name: file.name }, row);
    }
  };
  const games = h("div", { class: "panel" },
    h("h3", {}, t("Games")),
    h("p", { class: "quiet small" }, t("Each file goes into its system's folder, found by its file type. A file with the same name is only replaced if you say so.")),
    noSystems,
    h("div", { class: "row" }, h("span", { class: "quiet" }, t("System")), pick),
    dropZone(t("Drop game files here"), true, sendGames), gameRows);

  // BIOS
  const missing = u.bios.filter((b) => b.missing);
  const biosRows = h("div", {});
  const bios = h("div", { class: "panel" },
    h("h3", {}, t("BIOS files")),
    h("p", { class: "quiet small" }, t("They go into roms/bios. Below: what the Pocket's own emulator needs for the systems that can't start without one.")),
    h("ul", { class: "checks" }, u.bios.map((b) => b.found ? h("li", { class: "ok" }, `${b.label}: ${b.found}`) : h("li", { class: "missing" }, missingBios(b)))),
    missing.length === 0 ? h("p", { class: "ok small" }, t("Nothing missing.")) : null,
    dropZone(t("Drop BIOS files here"), true, async (filesIn) => {
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
    h("h3", {}, t("PortMaster: your own game files")),
    u.ports.length === 0 ? h("p", { class: "quiet" }, t("No installed port is waiting for files.")) : null,
    u.ports.map((p) => {
      const rows = h("div", {});
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
        h("p", { class: "quiet small" }, t("They go into {s}.", { s: p.place })),
        h("div", { class: "stack" }, dropZone(t("Drop files here"), true, (f) => send(f, false)), dropZone(t("Or a whole folder: its contents keep their folders"), false, (f) => send(f, true), true)),
        rows);
    }));

  view(h("h2", {}, t("Upload"), " ", h("span", { class: "count" }, free)), games, bios, ports);
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
  const out = svg("svg", { class: "chart", viewBox: `0 0 ${W} ${H}`, width: W, height: H, role: "img", "aria-label": t("Battery over the last 48 hours") });
  for (const p of [0, 50, 100]) out.append(svg("line", { class: "grid", x1: L, x2: W - R, y1: y(p), y2: y(p) }), svg("text", { x: 0, y: y(p) + 4 }, percent(p)));
  for (let hrs = 48; hrs >= 0; hrs -= 12) {
    const t = t1 - hrs * 3600;
    out.append(svg("text", { x: x(t) - (hrs === 0 ? 22 : 12), y: H - 6 }, hrs === 0 ? t("now") : "−" + t("{n} h", { n: hrs })));
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
const dayText = (d, opts) => (d ? day(d).toLocaleDateString(locale(), opts || { day: "numeric", month: "short" }) : "");
const span = (a, b) => (a === b ? dayText(a) : dayText(a) + " – " + dayText(b));

/// One bar per day (a week, a month) or per week (a year), the period's time as their height.
function periodChart(p, kind, W) {
  const H = Math.round(Math.min(220, W * 0.4)), L = 8, R = 8, T = 18, B = 24;
  const n = Math.max(1, p.bars.length), max = Math.max(60, ...p.bars.map((b) => b.secs));
  const slot = (W - L - R) / n, bw = Math.max(2, slot * 0.7);
  const out = svg("svg", { class: "chart", viewBox: `0 0 ${W} ${H}`, width: W, height: H, role: "img", "aria-label": kind === "year" ? t("Time played per week") : t("Time played per day") });
  out.append(svg("line", { class: "grid", x1: L, x2: W - R, y1: H - B, y2: H - B }));
  let lastMonth = -1;
  p.bars.forEach((b, i) => {
    const x = L + i * slot + (slot - bw) / 2, hgt = b.secs ? Math.max(2, (b.secs / max) * (H - T - B)) : 0;
    const when = kind === "year" ? t("Week of {s}", { s: dayText(b.first) }) : dayText(b.first, { weekday: "short", day: "numeric", month: "short" });
    const bar = svg("rect", { class: "bar", x: x.toFixed(1), y: (H - B - hgt).toFixed(1), width: bw.toFixed(1), height: hgt.toFixed(1), rx: Math.min(3, bw / 3) },
      svg("title", {}, when + ": " + (b.secs ? duration(b.secs) : t("nothing played"))));
    out.append(bar);
    const d = day(b.first);
    let label = null;
    if (kind === "week") label = d.toLocaleDateString(locale(), { weekday: "short" }).replace(/\.$/, "");
    else if (kind === "month" && (i === 0 || (d.getDate() - 1) % 7 === 0)) label = String(d.getDate());
    else if (kind === "year" && d.getMonth() !== lastMonth) { label = d.toLocaleDateString(locale(), { month: "narrow" }); lastMonth = d.getMonth(); }
    if (label) out.append(svg("text", { x: (L + i * slot + slot / 2).toFixed(1), y: H - 6, "text-anchor": "middle" }, label));
  });
  const top = p.bars.reduce((a, b) => (b.secs > a.secs ? b : a), p.bars[0] || { secs: 0 });
  if (top.secs) out.append(svg("text", { x: W - R, y: 12, "text-anchor": "end" }, t("Most: {s}", { s: duration(top.secs) })));
  return out;
}

function historyGame(g, side) {
  const name = h("span", { class: "name" }, g.name || t("A game without a name"));
  const stripe = stripeOf(g.system);
  const body = [h("span", { class: "tile", stripe }, (g.name || "?").slice(0, 1)), name, h("span", { class: "side" }, side)];
  return h("li", {}, g.id ? h("a", { class: "spine", href: "#/game/" + g.id, stripe }, body) : h("div", { class: "spine", stripe }, body));
}

/// The period's heading in the Pocket's language. The Pocket names it in English ("This week", "November 2026", "2026",
/// play_history::Period::label); the page writes it from the period's first day instead.
function periodTitle(p, kind) {
  const d = day(p.first);
  if (kind === "week") return p.label === "This week" ? t("This week") : t("Week of {s}", { s: dayText(p.first, { day: "numeric", month: "long" }) });
  if (kind === "month") return d.toLocaleDateString(locale(), { month: "long", year: "numeric" });
  return String(d.getFullYear());
}

function historyBlock(hist) {
  const box = h("div", { class: "history" });
  const draw = () => {
    const kind = S.period || "week", p = hist[kind];
    const seg = h("div", { class: "seg", role: "group", "aria-label": t("Period") },
      [["week", t("Week")], ["month", t("Month")], ["year", t("Year")]].map(([k, l]) => h("button", { class: k === kind ? "on" : "", "aria-pressed": k === kind ? "true" : "false", onclick: () => { S.period = k; history.replaceState(null, "", "#/stats/" + k); draw(); } }, l)));
    const head = h("h3", {}, periodTitle(p, kind), " ", h("span", { class: "count" }, span(p.first, p.last)));
    let body;
    if (p.not_recorded) {
      body = h("p", { class: "quiet" }, t("Not recorded: the history counts from {s}.", { s: dayText(hist.counting_since, { day: "numeric", month: "long", year: "numeric" }) }));
    } else if (!p.secs && !p.sessions) {
      body = h("p", { class: "quiet" }, t("Nothing played in this period."));
    } else {
      const sysMax = Math.max(1, ...p.systems.map((x) => x.secs));
      const sysBars = h("div", { class: "bars" }, p.systems.map((x) => h("div", { class: "bar-row" },
        h("span", { class: "n" }, systemName(x.system)), h("span", { class: "track" }, h("span", { class: "fill", stripe: stripeOf(x.system) })), h("span", { class: "v" }, duration(x.secs)))));
      [...sysBars.querySelectorAll(".fill")].forEach((el, i) => (el.style.width = Math.max(2, (p.systems[i].secs / sysMax) * 100) + "%"));
      const streak = p.longest_streak && p.longest_streak.days > 1 ? t("Longest run in this period: {n} days in a row ({s}).", { n: p.longest_streak.days, s: span(p.longest_streak.from, p.longest_streak.to) }) : null;
      const now = p.current_streak > 1 ? t("Played {n} days in a row up to now.", { n: p.current_streak }) : "";
      body = [
        h("div", { class: "tiles" },
          h("div", { class: "tile-stat" }, h("div", { class: "big" }, duration(p.secs)), h("div", { class: "quiet" }, t("played"))),
          h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(p.sessions)), h("div", { class: "quiet" }, p.sessions === 1 ? t("session") : t("sessions"))),
          h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(p.days_played)), h("div", { class: "quiet" }, p.days_played === 1 ? t("day played") : t("days played"))),
          p.new_games ? h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(p.new_games)), h("div", { class: "quiet" }, p.new_games === 1 ? t("game new to you") : t("games new to you"))) : null),
        p.bars.length ? chart((W) => periodChart(p, kind, W)) : null,
        streak || now ? h("p", { class: "quiet small" }, [streak, now].filter(Boolean).join(" ")) : null,
        p.games.length ? [h("h3", {}, t("Most played")), h("ul", { class: "shelf" }, p.games.map((g) => historyGame(g, [g.new ? h("span", { class: "quiet" }, t("new") + " · ") : null, duration(g.secs)])))] : null,
        p.systems.length ? [h("h3", {}, t("By system")), sysBars] : null,
        p.came_back.length ? [h("h3", {}, t("Back after a long break")), h("ul", { class: "shelf" }, p.came_back.map((g) => historyGame(g, t("after {n} days", { n: g.gap_days }))))] : null,
        p.completed.length ? [h("h3", {}, t("Completed")), h("ul", { class: "shelf" }, p.completed.map((g) => historyGame(g, dayText(g.date))))] : null,
      ];
    }
    fill(box, seg, head, body);
  };
  draw();
  return box;
}

function recentSessions(recent) {
  if (!recent || !recent.length) return null;
  return [h("h3", {}, t("Recent sessions")), h("ul", { class: "shelf" }, recent.map((r) => {
    const when = r.date ? new Date(r.start * 1000).toLocaleString(locale(), { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) : t("before the clock was set");
    const time = r.ended ? duration(r.secs) : t("no time: it did not end normally");
    return historyGame(r, when + " · " + time);
  }))];
}

/// "Counted since 3 March 2026. In all: 41 h over 120 sessions, 2 h of it from before the history began."
function allTime(hist) {
  const since = t("Counted since {s}.", { s: dayText(hist.counting_since, { day: "numeric", month: "long", year: "numeric" }) });
  const a = hist.all_time, secs = duration(a.secs);
  let all;
  if (a.carried_secs) all = a.sessions === 1 ? t("In all: {s} over 1 session, {s2} of it from before the history began.", { s: secs, s2: duration(a.carried_secs) }) : t("In all: {s} over {n} sessions, {s2} of it from before the history began.", { s: secs, n: a.sessions, s2: duration(a.carried_secs) });
  else all = a.sessions === 1 ? t("In all: {s} over 1 session.", { s: secs }) : t("In all: {s} over {n} sessions.", { s: secs, n: a.sessions });
  return since + " " + all;
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
  let battery = h("p", { class: "quiet" }, t("No battery history yet."));
  try {
    const b = await api("/api/battery");
    if (b.samples.length > 1) battery = chart((W) => batteryChart(b, W));
  } catch (e) { /* keep the line */ }

  const historyPart = hist
    ? [
        h("p", { class: "quiet small" }, allTime(hist)),
        hist.damaged ? h("p", { class: "alert small" }, t("Part of the history file could not be read; those sessions are not counted.")) : null,
        historyBlock(hist),
        recentSessions(hist.recent),
      ]
    : h("p", { class: "quiet" }, t("Play history is off, or nothing has been recorded yet (Settings › Games › Curator › Play history on the Pocket). The totals below are the ones the library keeps for every game."));

  view(
    h("h2", {}, t("Play history")),
    historyPart,
    h("h2", {}, hist ? t("All time, by game") : t("Play time")),
    h("div", { class: "tiles" },
      h("div", { class: "tile-stat" }, h("div", { class: "big" }, duration(s.totalSecs)), h("div", { class: "quiet" }, t("played in all"))),
      h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(s.sessions)), h("div", { class: "quiet" }, t("sessions of a minute or more"))),
      h("div", { class: "tile-stat" }, h("div", { class: "big" }, String(s.played)), h("div", { class: "quiet" }, s.played === 1 ? t("game played") : t("games played")))),
    h("p", { class: "quiet small" }, t("Kept with each game in the library: time in the game, and sessions of a minute or more.")),
    s.systems.length ? [h("h3", {}, t("By system")), bars] : h("p", { class: "quiet" }, t("Nothing played yet. Play time shows here after the first game.")),
    s.games.length ? [h("h3", {}, s.played > s.games.length ? t("Most played (top {n})", { n: s.games.length }) : t("Most played")), h("ul", { class: "shelf" }, s.games.map(gameSpine))] : null,
    h("h2", {}, t("Battery")), h("p", { class: "quiet small" }, t("The last 48 hours. Green: charging. Gaps: off or asleep.")), battery);
}

window.PocketWeb = { h, $, api, post, toast, fill, view, size, route, S, t };

document.addEventListener("DOMContentLoaded", start);
