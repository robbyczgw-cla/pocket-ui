// Drives src/web/page/app.js in Node through every view, in a language whose every text is marked («Library»), with
// the Pocket's data in Cyrillic and the Pocket's language set to Russian: what is left in Latin letters once the marked
// texts and the data are taken out is a text of the page that skipped t(), or a date or number not written in the
// Pocket's language. Usage: node tests/web_page_texts.js <app.js> [<another page script>...] (the others load first, as in
// index.html; their texts are marked too, their views are their own tests').
"use strict";
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const SRC = fs.readFileSync(process.argv[2], "utf8");
const OTHERS = process.argv.slice(3).map((f) => fs.readFileSync(f, "utf8"));

// --- just enough of a page ------------------------------------------------------------------------------------------

class Node {}
class Text extends Node {
  constructor(s) { super(); this.data = String(s); }
  get text() { return this.data; }
}
class El extends Node {
  constructor(tag) {
    super();
    this.tag = tag; this.attrs = {}; this.kids = []; this.className = ""; this.hidden = false; this.dataset = {}; this.value = "";
    this.style = { setProperty() {} };
    const el = this;
    this.classList = { toggle(c, on) { const set = new Set(el.className.split(" ").filter(Boolean)); (on === undefined ? !set.has(c) : on) ? set.add(c) : set.delete(c); el.className = [...set].join(" "); }, add(c) { this.toggle(c, true); }, remove(c) { this.toggle(c, false); } };
  }
  setAttribute(k, v) { this.attrs[k] = String(v); if (k === "class") this.className = String(v); }
  getAttribute(k) { return this.attrs[k]; }
  addEventListener() {}
  append(...k) { for (const x of k) this.kids.push(x instanceof Node ? x : new Text(x)); }
  replaceChildren(...k) { this.kids = []; this.append(...k); }
  replaceWith() {}
  remove() { if (this.parent) this.parent.kids = this.parent.kids.filter((k) => k !== this); }
  set textContent(s) { this.kids = [new Text(s)]; }
  get textContent() { return this.text; }
  get text() { return this.kids.map((k) => k.text).join(""); }
  focus() {}
  click() {}
  get clientWidth() { return 700; }
  all() { return [this, ...this.kids.filter((k) => k instanceof El).flatMap((k) => k.all())]; }
  shown() { return this.hidden ? [] : [this, ...this.kids.filter((k) => k instanceof El).flatMap((k) => k.shown())]; }
  querySelectorAll(sel) {
    const last = sel.trim().split(/\s+/).pop();
    const [tag, cls] = last.startsWith(".") ? [null, last.slice(1)] : last.split(".");
    return this.all().filter((e) => e !== this && (!tag || e.tag === tag) && (!cls || e.className.split(" ").includes(cls)));
  }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
}

// What a person reads: the text, and the words of the attributes a browser shows or reads out.
function readable(el) {
  const out = [];
  for (const e of el.shown()) {
    for (const a of ["title", "alt", "placeholder", "aria-label"]) if (e.attrs[a]) out.push(e.attrs[a]);
    for (const k of e.kids) if (k instanceof Text && k.data.trim()) out.push(k.data);
  }
  return out;
}

// --- the Pocket's data: every name, path and line it sends is Cyrillic ----------------------------------------------

const NOW = Math.floor(Date.now() / 1000);
const LIB = {
  systems: [{ id: "snes", name: "Приставка", stripe: "#f00", games: 3, extensions: ["sfc"] }, { id: "gba", name: "Карманная", stripe: "#0f0", games: 1, extensions: ["gba"] }],
  shelfPictures: ["2026-11-20 18-00-00"], favorites: 2, noteMax: 80, shelfMax: 24, shelvesReady: false,
  shelves: [{ index: 0, name: "Полка", pinned: true }, { index: 1, name: "Другая", pinned: false }],
};
const GAME = (id, extra) => Object.assign({ id, name: "Игра", stripe: "#f00", cover: false, status: "playing", favorite: true, secs: 7500, rating: 0 }, extra);
const FULL = GAME("g1", {
  system: "snes", systemName: "Приставка", lastPlayed: NOW - 3 * 86400, sessions: 4, file: "игра.ж", note: "", cover: true,
  facts: { year: 1994, developers: ["Студия"], publishers: ["Издатель"], genres: ["Жанр"], series: ["Серия"], players: "1–2" },
  versions: [{ id: "g1", name: "Игра", system: "snes", preferred: true }, { id: "g2", name: "Игра", system: "gba", preferred: false }],
  shots: ["2026-11-20 18-00-00"], pictures: ["snap", "title"],
});
const BARE = GAME("g3", { system: "gba", systemName: "Карманная", status: "not_started", favorite: false, secs: 0, sessions: 0, file: "ж.ж", facts: null });
const PERIOD = (kind) => ({
  // The labels as play_history::Period::label writes them.
  label: { week: "This week", month: "November 2026", year: "2026" }[kind], first: "2026-11-14", last: "2026-11-20", secs: 9000, sessions: 3, days_played: 2, new_games: 1, current_streak: 2,
  longest_streak: { days: 2, from: "2026-11-19", to: "2026-11-20" },
  bars: (kind === "year" ? ["2026-01-05", "2026-02-02", "2026-11-16"] : ["2026-11-14", "2026-11-15", "2026-11-20"]).map((d, i) => ({ first: d, secs: i * 3600 })),
  games: [Object.assign(GAME("g1"), { new: true, system: "snes" })], systems: [{ system: "snes", secs: 9000 }],
  came_back: [Object.assign(GAME("g1"), { gap_days: 40, system: "snes" })], completed: [Object.assign(GAME("g1"), { date: "2026-11-19", system: "snes" })],
});
const HISTORY = {
  counting_since: "2026-03-03", all_time: { secs: 150000, sessions: 120, carried_secs: 7200 }, damaged: true,
  week: PERIOD("week"), month: PERIOD("month"), year: PERIOD("year"),
  recent: [{ id: "g1", name: "Игра", system: "snes", date: "2026-11-20", start: NOW - 3600, secs: 1800, ended: true }, { id: null, name: null, system: "gba", date: null, start: 0, secs: 0, ended: false }],
};
const EMPTY_PERIOD = { label: "Week of 9 November", first: "2026-11-14", last: "2026-11-20", secs: 0, sessions: 0, bars: [], games: [], systems: [], came_back: [], completed: [] };
const DATA = {
  "/api/session": { signedIn: true, via: "paired", name: "Телефон", csrf: "x", info: { version: "0.7.0" } },
  "/api/library": LIB,
  "/api/status": { playing: { name: "Игра", stripe: "#f00", systemName: "Приставка", since: NOW - 600 }, gameRunning: true },
  "/api/battery": { last: { charging: true, percent: 80 }, now: NOW, keep: 48 * 3600, gap: 600, samples: [[NOW - 7200, 90, 0], [NOW - 3600, 85, 1], [NOW, 80, 1]] },
  "/api/games?system=snes": { title: "Приставка", total: 300, games: [GAME("g1"), GAME("g3", { status: "completed", favorite: false, secs: 0 })] },
  "/api/games?favorites=1": { title: "Favourites", total: 1, games: [GAME("g1")] },
  "/api/games?q=": { title: "“ж”", total: 0, games: [] },
  "/api/game?id=g1": FULL,
  "/api/game?id=g3": BARE,
  "/api/uploads": {
    free: 9e9, reserve: 1e8, systems: LIB.systems,
    bios: [{ label: "Приставка", found: "ж.ж", files: ["ж.ж"] }, { label: "Карманная", missing: "Missing for Карманная: папка/ж.ж or ж.ж", files: ["папка/ж.ж", "ж.ж"] }],
    ports: [{ title: "Порт", instructions: "Положи файлы", place: "порты/ж", dir: "ж" }],
  },
  "/api/stats": { history: HISTORY, systems: [{ name: "Приставка", stripe: "#f00", secs: 9000 }], totalSecs: 9000, sessions: 5, played: 4, games: [GAME("g1")] },
};

function run(data, opts) {
  opts = opts || {};
  const els = {};
  for (const id of ["battery", "now", "main", "who", "version", "tabs", "diagnostics"]) els[id] = new El(id === "tabs" ? "nav" : id === "diagnostics" ? "a" : "div");
  els.diagnostics.hidden = true;
  for (const tab of ["library", "shelves", "upload", "saves", "stats"]) {
    const a = new El("a");
    a.dataset.tab = tab;
    a.textContent = tab === "saves" ? "Saves" : tab[0].toUpperCase() + tab.slice(1);
    if (tab === "saves") els["tab-saves"] = a;
    els.tabs.append(a);
  }
  els.tabs.hidden = true; // as in index.html, until signed in
  els.now.hidden = true;
  const loading = new El("p");
  loading.className = "quiet";
  loading.textContent = "Loading…";
  els.main.append(loading);
  const body = new El("body");
  const asked = [];
  // The marked language: every text of the page's t("…") calls as «text», placeholders kept.
  const texts = {};
  for (const m of [SRC, ...OTHERS].join("\n").matchAll(/(?<![\w$.])t\("((?:[^"\\\n]|\\.)*)"/g)) texts[JSON.parse('"' + m[1] + '"')] = "«" + JSON.parse('"' + m[1] + '"') + "»";
  const reply = (status, json) => ({ ok: status === 200, status, headers: { get: (k) => (k === "Content-Type" ? "application/json" : null) }, json: async () => json, blob: async () => ({}) });
  const ctx = {
    console, URLSearchParams, Date, Math, Promise, Object, String, Number, Array, JSON, Intl, Error, Set, encodeURIComponent, decodeURIComponent,
    Node,
    document: {
      documentElement: { lang: "en" }, visibilityState: "visible", body,
      getElementById: (id) => els[id] || null,
      createElement: (tag) => new El(tag), createElementNS: (ns, tag) => new El(tag), createTextNode: (s) => new Text(s),
      querySelectorAll: (sel) => (sel.startsWith("#tabs") ? els.tabs.querySelectorAll("a") : sel.startsWith("#main") ? els.main.querySelectorAll(sel.split(" ").pop()) : []),
      querySelector: (sel) => ctx.document.querySelectorAll(sel)[0] || null,
      addEventListener() {},
    },
    location: { hash: "", pathname: "/", reload() {} },
    history: { replaceState() {}, back() {} },
    setTimeout: () => 0, clearTimeout() {},
    confirm: (q) => { asked.push(q); return false; },
    prompt: (q) => { asked.push(q); return null; },
    URL: { createObjectURL: () => "blob:" },
    savesView: async () => {}, savesBlock() {},
    fetch: async (path) => {
      if (path === "/api/texts") return reply(200, { lang: "ru", texts });
      if (opts.down) throw new Error("down");
      if (opts.playing && path !== "/api/session" && path !== "/api/status") return reply(503, { playing: true, error: "Идёт игра" });
      const key = Object.keys(data).sort((a, b) => b.length - a.length).find((k) => path.startsWith(k));
      if (!key) return reply(404, { error: "Нет такого" });
      return reply(200, data[key]);
    },
  };
  ctx.window = { addEventListener() {}, scrollTo() {}, PocketMediaUpdate: undefined };
  ctx.globalThis = ctx;
  vm.createContext(ctx);
  for (const other of OTHERS) vm.runInContext(other, ctx);
  vm.runInContext(SRC, ctx);
  // toast() puts its line into the body.
  return { ctx, els, body, asked };
}

// What is left in Latin letters once the marked texts and the few names that stay English are out.
const KEPT = [/pocket-ui/g, /\bPocket\b/g, /roms\/bios/g, /PortMaster/g, /Tailscale/g, /BIOS/g, /ZIP/g];
function english(lines) {
  const left = [];
  for (const line of lines) {
    let s = line;
    for (let before = ""; before !== s; ) { before = s; s = s.replace(/«[^«»]*»/g, " "); }
    for (const k of KEPT) s = s.replace(k, " ");
    if (/[A-Za-z]{2,}/.test(s)) left.push(line);
  }
  return left;
}

async function look(name, p, ...more) {
  const lines = [...readable(p.els.main), ...readable(p.els.now), ...readable(p.els.tabs), ...readable(p.els.diagnostics), ...readable(p.els.who), ...readable(p.els.battery), ...readable(p.body), ...p.asked, ...more];
  assert.ok(lines.length, name + ": the view shows something");
  const left = english(lines);
  assert.deepStrictEqual(left, [], name + ": English the page did not hand to t(), or a date or number not in the Pocket's language");
  return lines;
}

async function view(hash, data, opts) {
  const p = run(data || DATA, opts);
  await p.ctx.start();
  if (hash !== undefined) { p.ctx.location.hash = hash; await p.ctx.route(); }
  return p;
}

(async () => {
  const seen = {};
  const all = async (name, p, ...more) => { seen[name] = await look(name, p, ...more); };

  // Before pairing; and the Pocket not answering at all.
  await all("pair", await view(undefined, Object.assign({}, DATA, { "/api/session": { signedIn: false, info: { version: "0.7.0" } } })));
  await all("down", await view(undefined, DATA, { down: true }));

  // Every tab and page, with things in it.
  await all("library", await view("#/library"));
  await all("system", await view("#/system/snes"));
  await all("favourites", await view("#/favorites"));
  await all("search, nothing found", await view("#/search/ж"));
  await all("game", await view("#/game/g1"));
  await all("game, nothing known", await view("#/game/g3"));
  await all("shelves", await view("#/shelves"));
  await all("upload", await view("#/upload"));
  for (const period of ["week", "month", "year"]) await all("stats " + period, await view("#/stats/" + period));
  await all("a game running", await view("#/library", DATA, { playing: true }));

  // The same with nothing on the Pocket.
  const nothing = Object.assign({}, DATA, {
    "/api/library": Object.assign({}, LIB, { systems: [], shelves: [], shelfPictures: [], favorites: 0 }),
    "/api/status": { playing: null, gameRunning: false },
    "/api/battery": { last: { charging: false, percent: 5 }, now: NOW, keep: 48 * 3600, gap: 600, samples: [] },
    "/api/uploads": { free: null, reserve: 0, systems: [], bios: [], ports: [] },
    "/api/stats": { history: null, systems: [], totalSecs: 0, sessions: 0, played: 0, games: [] },
  });
  await all("library, empty", await view("#/library", nothing));
  await all("shelves, empty", await view("#/shelves", nothing));
  await all("upload, empty", await view("#/upload", nothing));
  await all("stats, no history", await view("#/stats", nothing));
  await all("stats, empty period", await view("#/stats/week", Object.assign({}, DATA, { "/api/stats": Object.assign({}, DATA["/api/stats"], { history: Object.assign({}, HISTORY, { damaged: false, recent: [], week: EMPTY_PERIOD, all_time: { secs: 60, sessions: 1, carried_secs: 0 } }) }) })));
  await all("stats, not recorded", await view("#/stats/week", Object.assign({}, DATA, { "/api/stats": Object.assign({}, DATA["/api/stats"], { history: Object.assign({}, HISTORY, { week: Object.assign({}, EMPTY_PERIOD, { not_recorded: true }) }) }) })));

  // What the buttons say back, and what they ask.
  const shelves = await view("#/shelves");
  await shelves.ctx.forget();
  await all("forget asks", shelves);
  const up = await view("#/upload");
  const row = up.ctx.fileRow("ж.sfc", "1 KB");
  const exists = Object.assign({}, DATA, { "/api/upload/check": { ok: true, exists: true, path: "ж/ж.sfc" } });
  const asks = run(exists);
  await asks.ctx.start();
  await asks.ctx.sendOne({ size: 10 }, { kind: "rom" }, row);
  await all("upload asks before replacing", asks, row.state.text);
  const gone = run(DATA, { down: true });
  await gone.ctx.start();
  const row2 = gone.ctx.fileRow("ж.sfc", "1 KB");
  await gone.ctx.sendOne({ size: 10 }, { kind: "rom" }, row2);
  await all("upload, the Pocket gone", gone, row2.state.text);
  const sizes = run(DATA);
  await sizes.ctx.start();
  await all("sizes and times", sizes, sizes.ctx.size(1500), sizes.ctx.size(5e6), sizes.ctx.size(3e9), sizes.ctx.duration(60), sizes.ctx.duration(3600), sizes.ctx.duration(3900), sizes.ctx.ago(NOW - 60), sizes.ctx.ago(NOW - 7200), sizes.ctx.ago(NOW - 86400), sizes.ctx.ago(NOW - 5 * 86400));

  // The test sees what it should: the marks are there, and an English text left in the page would be caught.
  assert.ok(seen.library.some((l) => l.includes("«Your shelf as a picture»")), seen.library.join(" | "));
  assert.ok(seen["stats week"].some((l) => /ноя/.test(l)), "dates in the Pocket's language: " + seen["stats week"].join(" | "));
  assert.deepStrictEqual(english(["«Library»", "Игра · «{s} played»", "2 h ago"]), ["2 h ago"]);
  console.log("web page texts: ok, " + Object.keys(seen).length + " views");
})().catch((e) => { console.error(e); process.exit(1); });
