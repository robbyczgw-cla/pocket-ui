// Drives src/web/page/media-update.js in Node with just enough of a page: More hub, Photo album & screenshots (empty
// and populated), game media section, every software update state, and localized 24h date formatting.
// Usage: node tests/web_media_update_page.js <media-update.js>
"use strict";
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

class El {
  constructor(tag, attrs, kids) {
    this.tag = tag;
    this.attrs = attrs || {};
    this.kids = [];
    this.textContent = "";
    this.className = (attrs && attrs.class) || "";
    this.classes = new Set(this.className ? this.className.split(/\s+/) : []);
    this.classList = {
      toggle: (name, on) => { if (on) this.classes.add(name); else this.classes.delete(name); },
      contains: (name) => this.classes.has(name),
    };
    this.add(kids);
  }
  add(kids) {
    for (const k of [kids].flat(Infinity)) {
      if (k !== null && k !== undefined && k !== false) this.kids.push(k instanceof El ? k : String(k));
    }
  }
  append(...k) { this.add(k); }
  replaceChildren(...k) { this.kids = []; this.textContent = ""; this.add(k); }
  get text() { return (this.attrs.text || "") + this.textContent + this.kids.map((k) => (k instanceof El ? k.text : k)).join(""); }
}

function run(data, lang = "en") {
  const shown = { main: null };
  const versionSlot = new El("span", { id: "version" });
  const moreTab = new El("a", { "data-tab": "more" }, ["More"]);
  const timers = [];
  const t = (en, vars) => (vars ? en.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : en);
  const h = (tag, attrs, ...kids) => new El(tag, attrs, kids);
  const api = async (path) => {
    const key = Object.keys(data).find((k) => path.startsWith(k));
    if (!key) throw new Error("no stand-in for " + path);
    return data[key];
  };
  const view = (...kids) => { shown.main = new El("main", {}, kids); };
  const S = { csrf: "tok", playing: false };
  const ctx = {
    console, URLSearchParams, Date, Math, Promise, Object, String, Number, Array, JSON, Intl, Boolean,
    setTimeout: (fn, ms) => { const id = timers.length + 1; timers.push({ id, fn, ms }); return id; },
    clearTimeout: () => {},
    location: { hash: "#/more", reload: () => {} },
    document: {
      documentElement: { lang },
      getElementById: (id) => (id === "version" ? versionSlot : null),
      querySelector: (sel) => (sel.includes('data-tab="more"') ? moreTab : null),
    },
    confirm: () => true,
    t,
  };
  ctx.window = ctx;
  ctx.PocketWeb = {
    h,
    $: (id) => (id === "version" ? versionSlot : null),
    api,
    post: async () => ({ ok: true }),
    toast: () => {},
    view,
    size: (b) => b + " B",
    S,
  };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(process.argv[2], "utf8"), ctx);
  return { mod: ctx.PocketMediaUpdate, shown, versionSlot, moreTab, timers, location: ctx.location };
}

const EMPTY_ALBUM = { album: [], shots: [], shelf: [], albumTotal: 0, shotsTotal: 0, shelfTotal: 0, albumOn: false };
const UP_TO_DATE = { current: "0.6.0", available: null, state: "up_to_date", checked: true, releaseBuild: true, busy: false, playing: false, message: "Everything is up to date." };

(async () => {
  // 1. More hub with nothing and no update.
  let p = run({ "/api/album": EMPTY_ALBUM, "/api/update": UP_TO_DATE });
  await p.mod.moreView();
  let text = p.shown.main.text;
  for (const want of ["More", "Photo album & screenshots", "No photos yet", "Software update", "pocket-ui 0.6.0"]) {
    assert.ok(text.includes(want), want + " in " + text);
  }
  assert.strictEqual(p.moreTab.classList.contains("has-update"), false);

  // 2. More hub with photos + screenshots + available update badge.
  p = run({
    "/api/album": { album: [], shots: [], shelf: [], albumTotal: 2, shotsTotal: 1, shelfTotal: 0, albumOn: true },
    "/api/update": { current: "0.6.0", available: "0.6.1", state: "available", checked: true, releaseBuild: true, busy: false, playing: false },
  });
  await p.mod.moreView();
  text = p.shown.main.text;
  for (const want of ["2 album photos · 1 screenshot", "new: 0.6.1"]) {
    assert.ok(text.includes(want), want + " in " + text);
  }
  assert.strictEqual(p.moreTab.classList.contains("has-update"), true);
  // On #/more, the update pill lives in the Software update card and is not repeated in the footer.
  assert.ok(!p.versionSlot.text.includes("new: 0.6.1"), "no duplicate footer pill on #/more: " + p.versionSlot.text);
  // On another tab, the footer shows the update pill.
  p.location.hash = "#/library";
  p.mod.syncBadge();
  assert.ok(p.versionSlot.text.includes("new: 0.6.1"), p.versionSlot.text);

  // 3. Photos view: empty with album off.
  p = run({ "/api/album": EMPTY_ALBUM });
  await p.mod.photosView();
  text = p.shown.main.text;
  for (const want of ["Photo album & screenshots", "Settings › Screen & sound › Photos of big moments", "No screenshots yet. Press HOME + A during a game to take one."]) {
    assert.ok(text.includes(want), want + " in " + text);
  }

  // 4. Photos view: empty with album on.
  p = run({ "/api/album": Object.assign({}, EMPTY_ALBUM, { albumOn: true }) });
  await p.mod.photosView();
  assert.ok(p.shown.main.text.includes("Big moments from your saves land here automatically."), p.shown.main.text);

  // 5. Photos view: populated + gameMedia empty and populated.
  p = run({
    "/api/album": {
      album: [{ id: "g1", game: "Golden Sun", stripe: "#4a7", file: "01.jpg", note: "Beat Saturos", at: 1795119600, bytes: 12000 }],
      shots: [{ id: "g1", game: "Golden Sun", stripe: "#4a7", at: "2026-11-18 21-14-05", bytes: 8000 }],
      shelf: [{ shelf: true, at: "2026-11-19 10-00-00", bytes: 9000 }],
      albumTotal: 1, shotsTotal: 1, shelfTotal: 1, albumOn: true,
    },
  }, "de");
  await p.mod.photosView();
  text = p.shown.main.text;
  for (const want of ["1 album photo", "2 screenshots", "Beat Saturos", "Golden Sun", "Your shelf as a picture", "Download all as ZIP"]) {
    assert.ok(text.includes(want), want + " in " + text);
  }
  // Unified localized 24h German date format for both album unix timestamp and screenshot stamp.
  const shotDate = p.mod.formatStamp("2026-11-18 21-14-05");
  assert.ok(/18\.11\.2026.*21:14/.test(shotDate), "German 24h stamp: " + shotDate);
  assert.ok(!/AM|PM/.test(shotDate), "no 12h AM/PM: " + shotDate);
  const albumDate = p.mod.formatUnix(1795119600);
  assert.ok(/2026/.test(albumDate) && !/AM|PM/.test(albumDate), "German 24h album unix: " + albumDate);

  const emptyGame = new El("div", {}, p.mod.gameMedia({ id: "g1", name: "Golden Sun", album: [], shots: [], albumOn: true }, () => {}));
  for (const want of ["No album photos for this game yet.", "No screenshots for this game yet."]) {
    assert.ok(emptyGame.text.includes(want), want + " in " + emptyGame.text);
  }
  const fullGame = new El("div", {}, p.mod.gameMedia({ id: "g1", name: "Golden Sun", album: [{ file: "01.jpg", note: "Boss", at: 1795119600, bytes: 1000 }], shots: ["2026-11-18 21-14-05"], albumOn: true }, () => {}));
  for (const want of ["Download game ZIP", "Boss", "Download", "Delete"]) {
    assert.ok(fullGame.text.includes(want), want + " in " + fullGame.text);
  }

  // 6. Updates view: every state says something clear.
  const states = [
    [{ current: "0.6.0-dev", available: null, state: "idle", checked: false, releaseBuild: false, busy: false, playing: false }, "Development build: install a release package to use app updates."],
    [{ current: "0.6.0", available: null, state: "idle", checked: false, releaseBuild: true, busy: false, playing: false }, "Not checked yet. Check for updates to see if a new version is ready."],
    [UP_TO_DATE, "Everything is up to date."],
    [{ current: "0.6.0", available: "0.6.1", state: "available", checked: true, releaseBuild: true, busy: false, playing: false, size: 12345, notes: "Faster boot" }, "0.6.0, new: 0.6.1"],
    [{ current: "0.6.0", available: "0.6.1", state: "available", checked: true, releaseBuild: true, busy: false, playing: false, size: 12345 }, "The update comes straight from pocket-ui. Your games and settings stay."],
    [{ current: "0.6.0", available: "0.6.1", state: "downloading", checked: true, releaseBuild: true, busy: true, playing: false, stepLine: "Step 1 of 4 · Downloading (42%)", percent: 42 }, "Step 1 of 4 · Downloading (42%)"],
    [{ current: "0.6.0", available: "0.6.1", state: "restarting", checked: true, releaseBuild: true, busy: true, playing: false }, "Installed. pocket-ui is restarting; this page will reconnect automatically."],
    [{ current: "0.6.0", available: "0.6.1", state: "failed", checked: true, releaseBuild: true, busy: false, playing: false, message: "Download failed." }, "Download failed."],
    [{ current: "0.6.0", available: "0.6.1", state: "available", checked: true, releaseBuild: true, busy: false, playing: true }, "A game is running. Quit the game on the Pocket before installing an update."],
  ];
  for (const [upd, want] of states) {
    p = run({ "/api/update": upd });
    await p.mod.updatesView();
    assert.ok(p.shown.main.text.includes(want), want + " in " + p.shown.main.text);
  }

  console.log("web media-update page: ok");
})().catch((e) => { console.error(e); process.exit(1); });
