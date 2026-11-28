// Drives src/web/page/saves.js in Node with just enough of a page: what each view shows with no saves, no Pocket Sync,
// and with some. Usage: node tests/web_saves_page.js <saves.js>
"use strict";
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

class El {
  constructor(tag, attrs, kids) { this.tag = tag; this.attrs = attrs || {}; this.kids = []; this.textContent = ""; this.className = (attrs && attrs.class) || ""; this.add(kids); }
  add(kids) { for (const k of [kids].flat(Infinity)) if (k !== null && k !== undefined && k !== false) this.kids.push(k instanceof El ? k : String(k)); }
  append(...k) { this.add(k); }
  remove() {}
  get text() { return (this.attrs.text || "") + this.textContent + this.kids.map((k) => (k instanceof El ? k.text : k)).join(""); }
}

function run(data) {
  const shown = { main: null };
  const ctx = {
    console, URLSearchParams, Date, Math, Promise, Object, String, Number, Array, JSON,
    document: { documentElement: { lang: "en" } },
    S: { csrf: "x" },
    h: (tag, attrs, ...kids) => new El(tag, attrs, kids),
    t: (en, vars) => (vars ? en.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : en),
    api: async (path) => {
      const key = Object.keys(data).find((k) => path.startsWith(k));
      if (!key) throw new Error("no stand-in for " + path);
      return data[key];
    },
    view: (...kids) => { shown.main = new El("main", {}, kids); },
    fill: (el, ...kids) => { el.kids = []; el.add(kids); },
    size: (b) => b + " B",
    toast: () => {},
    dropZone: (text) => new El("div", { class: "drop" }, [text]),
    fileRow: () => ({ el: new El("div"), state: new El("span"), bar: new El("progress") }),
    uploadPieces: async () => ({ ok: true, body: {} }),
    confirm: () => true,
  };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(process.argv[2], "utf8"), ctx);
  return { ctx, shown };
}

const NONE = { "/api/saves/all": { saves: { count: 0, bytes: 0, fits: true }, states: { count: 0, bytes: 0, fits: true }, backups: 0 }, "/api/pockets": { enabled: false, now: 10000, pockets: [], recent: [], questions: [] } };

(async () => {
  // The Saves tab on a Pocket with nothing yet.
  let p = run(NONE);
  await p.ctx.savesView();
  let text = p.shown.main.text;
  assert.ok(text.includes("No saves on the Pocket yet."), text);
  assert.ok(text.includes("Pocket Sync is off."), text);
  assert.ok(!text.includes("Download all saves"), "no download of nothing");

  // Pocket Sync on, nothing paired.
  p = run(Object.assign({}, NONE, { "/api/pockets": { enabled: true, now: 10000, pockets: [], recent: [], questions: [] } }));
  await p.ctx.savesView();
  assert.ok(p.shown.main.text.includes("No other Pocket is paired yet."), p.shown.main.text);

  // Paired, but nothing went yet; and Pocket Sync switched off since.
  p = run(Object.assign({}, NONE, { "/api/pockets": { enabled: false, now: 10000, pockets: [{ name: "TrimUI Brick", last: "never", failed: 0, okAt: null, downSince: null }], recent: [], questions: [] } }));
  await p.ctx.savesView();
  text = p.shown.main.text;
  for (const want of ["TrimUI Brick", "Not synced yet", "No saves have gone between your Pockets yet.", "Pocket Sync is off on this Pocket."]) assert.ok(text.includes(want), want + " in " + text);

  // With saves, Pockets and a question.
  p = run({
    "/api/saves/all": { saves: { count: 3, bytes: 300, fits: true }, states: { count: 2, bytes: 200, fits: true }, backups: 1 },
    "/api/pockets": { enabled: true, now: 10000, pockets: [{ name: "TrimUI Brick", last: "synced", failed: 0, okAt: 9820, downSince: null }, { name: "RG35XX Plus", last: "unreachable", failed: 0, okAt: 100, downSince: 10000 - 3 * 86400 }],
      recent: [{ at: 9820, dir: "from", pocket: "TrimUI Brick", game: "Golden Sun", id: "abc", file: "Golden Sun.srm" }, { at: 100, dir: "to", pocket: null, game: null, id: null, file: "Gone.srm" }],
      questions: [{ game: "Golden Sun", id: "abc", file: "Golden Sun.srm" }] },
  });
  await p.ctx.savesView();
  text = p.shown.main.text;
  for (const want of ["3 saves · 300 B", "2 save states · 200 B", "Download all saves (ZIP)", "Synced · 3 min ago", "Not reachable for 3 days", "Came from TrimUI Brick", "Went to a Pocket no longer paired", "Gone.srm", "Two versions", "Backup & reset"]) assert.ok(text.includes(want), want + " in " + text);

  // A game's page: no saves; a game that keeps its own; some.
  const box = new El("section");
  p = run({ "/api/saves?": { saves: [], states: [], folder: false, bytes: 0, max: {} } });
  await p.ctx.savesBlock("abc", box);
  assert.ok(box.text.includes("No saves of this game on the Pocket yet."), box.text);
  assert.ok(box.text.includes("Put a save back"), "putting one back is offered with none there");
  p = run({ "/api/saves?": { saves: [], states: [], folder: true, bytes: 0, max: {} } });
  await p.ctx.savesBlock("abc", box);
  assert.ok(box.text.includes("keeps its saves in its own folder"), box.text);
  p = run({ "/api/saves?": { saves: [{ name: "G.srm", bytes: 8192, modified: 1000, kind: "save" }], states: [{ name: "G.pstate.auto", bytes: 9, modified: 1, slot: "continue", n: 0 }, { name: "G.pstate2", bytes: 9, modified: 1, slot: "slot", n: 3 }], folder: false, bytes: 8210, max: {} } });
  await p.ctx.savesBlock("abc", box);
  for (const want of ["Game save", "G.srm", "Continue", "Slot 3", "All of this game as a ZIP"]) assert.ok(box.text.includes(want), want + " in " + box.text);

  // A game page whose saves can't be read says why instead of leaving a gap.
  p = run({});
  await p.ctx.savesBlock("abc", box);
  assert.ok(box.text.includes("no stand-in"), box.text);
  console.log("web saves page: ok");
})().catch((e) => { console.error(e); process.exit(1); });
