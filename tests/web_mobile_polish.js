// Verifies the web companion's mobile polish at 390 px across all six languages (en, de, es, fr, it, pt):
//   1. No tab cut off at 390 px (tabs wrap cleanly, all 6 tabs including Stats and More stay visible).
//   2. Available-update badge ("new: 0.6.1") is not repeated in the footer or active More tab on #/more and #/updates.
//   3. Download buttons are unambiguous in every language (never German "Laden" / "laden").
//   4. Stats week chart shows 7 distinct, unambiguous weekday labels (never "S S M D M D F").
//   5. Stats tiles keep "4 h 55 min" on one line and system bars do not clip long console names at 390 px.
// Usage: node tests/web_mobile_polish.js <repo-root>
"use strict";
const assert = require("assert");
const fs = require("fs");
const path = require("path");
const vm = require("vm");

const ROOT = process.argv[2] || process.cwd();
const PAGE = path.join(ROOT, "src", "web", "page");
const I18N = path.join(ROOT, "assets", "i18n");
const LANGS = ["en", "de", "es", "fr", "it", "pt"];

const INDEX_HTML = fs.readFileSync(path.join(PAGE, "index.html"), "utf8");
const APP_CSS = fs.readFileSync(path.join(PAGE, "app.css"), "utf8");
const MEDIA_CSS = fs.readFileSync(path.join(PAGE, "media-update.css"), "utf8");
const APP_JS = fs.readFileSync(path.join(PAGE, "app.js"), "utf8");
const SAVES_JS = fs.readFileSync(path.join(PAGE, "saves.js"), "utf8");
const COVER_JS = fs.readFileSync(path.join(PAGE, "cover-diagnostics.js"), "utf8");
const MEDIA_JS = fs.readFileSync(path.join(PAGE, "media-update.js"), "utf8");

function loadTsv(lang) {
  if (lang === "en") return {};
  const map = {};
  const raw = fs.readFileSync(path.join(I18N, lang + ".tsv"), "utf8");
  for (const line of raw.split(/\r?\n/)) {
    if (!line || line.startsWith("#")) continue;
    const tab = line.indexOf("\t");
    if (tab > 0) map[line.slice(0, tab)] = line.slice(tab + 1);
  }
  return map;
}

const TABLES = Object.fromEntries(LANGS.map((l) => [l, loadTsv(l)]));

function tr(lang, en, vars) {
  const table = TABLES[lang] || {};
  let s = Object.prototype.hasOwnProperty.call(table, en) ? table[en] : en;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  return s;
}

// --- tiny CSS rule collector for a given viewport width --------------------------------------------------------------

function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

function collectCssRules(css, viewportWidth) {
  const src = stripComments(css);
  const rules = [];
  let i = 0;
  function parseBlock(endAt) {
    while (i < endAt) {
      while (i < endAt && /\s/.test(src[i])) i++;
      if (i >= endAt) break;
      const brace = src.indexOf("{", i);
      if (brace < 0 || brace >= endAt) break;
      const header = src.slice(i, brace).trim();
      let depth = 1, j = brace + 1;
      while (j < endAt && depth > 0) {
        if (src[j] === "{") depth++;
        else if (src[j] === "}") depth--;
        j++;
      }
      const bodyStart = brace + 1, bodyEnd = j - 1;
      if (header.startsWith("@media")) {
        const maxM = /max-width:\s*(\d+)px/.exec(header);
        const minM = /min-width:\s*(\d+)px/.exec(header);
        const schemeM = /prefers-color-scheme/.test(header);
        const matches = (!maxM || viewportWidth <= Number(maxM[1]))
          && (!minM || viewportWidth >= Number(minM[1]))
          && !schemeM;
        if (matches) {
          i = bodyStart;
          parseBlock(bodyEnd);
        }
      } else if (!header.startsWith("@")) {
        const decls = {};
        for (const part of src.slice(bodyStart, bodyEnd).split(";")) {
          const colon = part.indexOf(":");
          if (colon > 0) decls[part.slice(0, colon).trim()] = part.slice(colon + 1).trim();
        }
        for (const sel of header.split(",")) {
          rules.push({ selector: sel.trim(), decls });
        }
      }
      i = j;
    }
  }
  parseBlock(src.length);
  return rules;
}

function computedFor(rules, selector) {
  const out = {};
  for (const r of rules) {
    if (r.selector === selector) Object.assign(out, r.decls);
  }
  return out;
}

function parsePaddingX(padStr) {
  if (!padStr) return 0;
  const nums = padStr.split(/\s+/).map((v) => parseFloat(v) || 0);
  if (nums.length === 1) return nums[0] * 2;
  if (nums.length === 2 || nums.length === 3) return nums[1] * 2;
  return nums[1] + nums[3];
}

function parseGapX(gapStr) {
  if (!gapStr) return 0;
  const nums = gapStr.split(/\s+/).map((v) => parseFloat(v) || 0);
  return nums.length >= 2 ? nums[1] : nums[0];
}

function parseFontPx(val, fallback) {
  if (!val) return fallback;
  const m = /(\d+(?:\.\d+)?)px/.exec(val);
  return m ? Number(m[1]) : fallback;
}

// Conservative Barlow proportional text width estimate (matches real headless Chromium within ~5%).
function estimateTextWidth(text, fontPx) {
  let units = 0;
  for (const ch of text) {
    if ("ilI1.,:;!'| ".includes(ch)) units += 0.28;
    else if ("fjrt".includes(ch)) units += 0.36;
    else if ("mwMW".includes(ch)) units += 0.80;
    else if (ch >= "A" && ch <= "Z") units += 0.62;
    else units += 0.50;
  }
  return units * fontPx;
}

// --- minimal DOM harness (for app.js + media-update.js + saves.js) ---------------------------------------------------

class Node {}
class Text extends Node {
  constructor(s) { super(); this.data = String(s); }
  get text() { return this.data; }
}
class El extends Node {
  constructor(tag, clientWidth = 358) {
    super();
    this.tag = tag;
    this.attrs = {};
    this.kids = [];
    this.className = "";
    this.hidden = false;
    this.dataset = {};
    this.value = "";
    this._clientWidth = clientWidth;
    this.style = { setProperty() {} };
    const el = this;
    this.classList = {
      toggle(c, on) {
        const set = new Set(el.className.split(/\s+/).filter(Boolean));
        const next = on === undefined ? !set.has(c) : Boolean(on);
        if (next) set.add(c); else set.delete(c);
        el.className = [...set].join(" ");
      },
      add(c) { this.toggle(c, true); },
      remove(c) { this.toggle(c, false); },
      contains(c) { return el.className.split(/\s+/).includes(c); },
    };
  }
  setAttribute(k, v) { this.attrs[k] = String(v); if (k === "class") this.className = String(v); }
  getAttribute(k) { return this.attrs[k]; }
  addEventListener() {}
  append(...k) { for (const x of k) this.kids.push(x instanceof Node ? x : new Text(x)); }
  replaceChildren(...k) { this.kids = []; this.append(...k); }
  replaceWith() {}
  remove() {}
  set textContent(s) { this.kids = [new Text(s)]; }
  get textContent() { return this.text; }
  get text() { return this.kids.map((k) => k.text).join(""); }
  focus() {}
  click() {}
  get clientWidth() { return this._clientWidth; }
  all() { return [this, ...this.kids.filter((k) => k instanceof El).flatMap((k) => k.all())]; }
  querySelectorAll(sel) {
    const s = sel.trim();
    const attrM = /\[data-tab="([^"]+)"\]/.exec(s);
    if (attrM) return this.all().filter((e) => e !== this && e.dataset.tab === attrM[1]);
    const last = s.split(/\s+/).pop();
    const [tag, cls] = last.startsWith(".") ? [null, last.slice(1)] : last.split(".");
    return this.all().filter((e) => e !== this && (!tag || e.tag === tag) && (!cls || e.className.split(/\s+/).includes(cls)));
  }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
}

function makeApp(lang, extraData) {
  const els = {};
  for (const id of ["battery", "now", "main", "who", "version", "tabs", "diagnostics"]) {
    els[id] = new El(id === "tabs" ? "nav" : id === "diagnostics" ? "a" : "div", 358);
  }
  const tabDefs = [
    ["library", "Library"], ["shelves", "Shelves"], ["upload", "Upload"],
    ["saves", "Saves"], ["stats", "Stats"], ["more", "More"],
  ];
  for (const [id, label] of tabDefs) {
    const a = new El("a");
    a.dataset.tab = id;
    a.textContent = label;
    if (id === "saves") els["tab-saves"] = a;
    if (id === "more") els["tab-more"] = a;
    els.tabs.append(a);
  }
  const NOW = Math.floor(Date.now() / 1000);
  const weekBars = ["2026-11-14", "2026-11-15", "2026-11-16", "2026-11-17", "2026-11-18", "2026-11-19", "2026-11-20"]
    .map((first, i) => ({ first, secs: (i % 3) * 1800 + 600 }));
  const data = Object.assign({
    "/api/session": { signedIn: true, via: "tailscale", name: "player", csrf: "tok", info: { version: "0.6.0" } },
    "/api/library": {
      systems: [
        { id: "gba", name: "Game Boy Advance", stripe: "#4a7", games: 6, extensions: ["gba"] },
        { id: "snes", name: "Super Nintendo", stripe: "#c44", games: 5, extensions: ["sfc"] },
      ],
      shelfPictures: ["2026-11-19 19-42-11"], favorites: 2, noteMax: 80, shelfMax: 24, shelvesReady: true, shelves: [],
    },
    "/api/status": { playing: null, gameRunning: false },
    "/api/battery": { last: { charging: false, percent: 84 }, now: NOW, keep: 48 * 3600, gap: 600, samples: [] },
    "/api/album": {
      album: [{ id: "g1", game: "Golden Sun", stripe: "#4a7", file: "01.jpg", note: "Badge 4", at: NOW - 1800, bytes: 12000 }],
      shots: [{ id: "g1", game: "Golden Sun", stripe: "#4a7", at: "2026-11-18 21-14-05", bytes: 47000 }],
      shelf: [{ shelf: true, at: "2026-11-19 19-42-11", bytes: 180000 }],
      albumTotal: 1, shotsTotal: 1, shelfTotal: 1, albumOn: true,
    },
    "/api/update": {
      current: "0.6.0", available: "0.6.1", state: "available", checked: true, releaseBuild: true, busy: false, playing: false,
      size: 18 * 1024 * 1024, notes: "Polish fixes.",
    },
    "/api/saves/all": {
      saves: { count: 2, bytes: 65536, fits: true }, states: { count: 3, bytes: 928768, fits: true }, backups: 1,
    },
    "/api/saves?": {
      saves: [{ name: "Golden Sun.srm", bytes: 65536, modified: NOW - 1000, kind: "save" }],
      states: [{ name: "Golden Sun.pstate.auto", bytes: 317440, modified: NOW - 500, slot: "continue", n: 0 }],
      folder: false, bytes: 382976, max: {},
    },
    "/api/pockets": { enabled: true, now: NOW, pockets: [], recent: [], questions: [] },
    "/api/stats": {
      history: {
        counting_since: "2026-03-03",
        all_time: { secs: 150000, sessions: 120, carried_secs: 7200 },
        damaged: false,
        week: {
          label: "This week", first: "2026-11-14", last: "2026-11-20", secs: 17700, sessions: 9, days_played: 5, new_games: 1,
          current_streak: 3, longest_streak: { days: 3, from: "2026-11-18", to: "2026-11-20" },
          bars: weekBars,
          games: [{ id: "g1", name: "Golden Sun", system: "gba", secs: 10800, new: true }],
          systems: [{ system: "gba", secs: 10800 }, { system: "snes", secs: 6300 }],
          came_back: [], completed: [],
        },
        month: { label: "November 2026", first: "2026-11-01", last: "2026-11-20", secs: 17700, sessions: 9, days_played: 5, new_games: 1, current_streak: 3, longest_streak: null, bars: weekBars, games: [], systems: [], came_back: [], completed: [] },
        year: { label: "2026", first: "2026-01-01", last: "2026-11-20", secs: 17700, sessions: 9, days_played: 5, new_games: 1, current_streak: 3, longest_streak: null, bars: weekBars, games: [], systems: [], came_back: [], completed: [] },
        recent: [],
      },
      systems: [{ name: "Game Boy Advance", stripe: "#4a7", secs: 10800 }, { name: "Super Nintendo", stripe: "#c44", secs: 6300 }],
      totalSecs: 17700, sessions: 9, played: 2, games: [],
    },
  }, extraData || {});

  const reply = (status, json) => ({
    ok: status === 200, status,
    headers: { get: (k) => (k === "Content-Type" ? "application/json" : null) },
    json: async () => json,
  });

  const ctx = {
    console, URLSearchParams, Date, Math, Promise, Object, String, Number, Array, JSON, Intl, Error, Set, Boolean,
    encodeURIComponent, decodeURIComponent, Node,
    document: {
      documentElement: { lang: "en" },
      visibilityState: "visible",
      body: new El("body"),
      getElementById: (id) => els[id] || null,
      createElement: (tag) => new El(tag),
      createElementNS: (_ns, tag) => new El(tag),
      createTextNode: (s) => new Text(s),
      querySelectorAll: (sel) => {
        if (sel.startsWith("#tabs")) return els.tabs.querySelectorAll(sel.replace(/^#tabs\s*/, ""));
        if (sel.startsWith("#main")) return els.main.querySelectorAll(sel.replace(/^#main\s*/, ""));
        return [];
      },
      querySelector: (sel) => ctx.document.querySelectorAll(sel)[0] || null,
      addEventListener() {},
    },
    location: { hash: "#/library", pathname: "/", reload() {} },
    history: { replaceState() {}, back() {} },
    setTimeout: () => 0,
    clearTimeout() {},
    addEventListener() {},
    scrollTo() {},
    confirm: () => true,
    prompt: () => null,
    URL: { createObjectURL: () => "blob:" },
    fetch: async (p) => {
      if (p === "/api/texts") return reply(200, { lang, texts: TABLES[lang] });
      const key = Object.keys(data).sort((a, b) => b.length - a.length).find((k) => p.startsWith(k));
      if (!key) return reply(404, { error: "not found: " + p });
      return reply(200, data[key]);
    },
  };
  ctx.window = ctx;
  ctx.globalThis = ctx;
  vm.createContext(ctx);
  vm.runInContext(SAVES_JS, ctx);
  vm.runInContext(COVER_JS, ctx);
  vm.runInContext(APP_JS, ctx);
  vm.runInContext(MEDIA_JS, ctx);
  return { ctx, els };
}

(async () => {
  const failures = [];
  const check = (cond, msg) => { if (!cond) failures.push(msg); };

  // 1. Mobile 390 px tab bar: no horizontal overflow/clipping, wraps cleanly so all 6 tabs stay visible in all 6 languages.
  const VIEWPORT = 390;
  const INNER_WIDTH = VIEWPORT - 32; // 16 px left + 16 px right body padding
  const rules390 = collectCssRules(APP_CSS + "\n" + MEDIA_CSS, VIEWPORT);
  const tabsCss = computedFor(rules390, ".tabs");
  const tabLinkCss = computedFor(rules390, ".tabs a");
  const tabWraps = tabsCss["flex-wrap"] === "wrap";
  const tabClips = ["auto", "scroll", "hidden"].includes(tabsCss["overflow-x"] || "");
  check(tabWraps, "390px .tabs must wrap (flex-wrap: wrap), got flex-wrap=" + (tabsCss["flex-wrap"] || "nowrap"));
  check(!tabClips, "390px .tabs must not clip/scroll horizontally, got overflow-x=" + (tabsCss["overflow-x"] || "visible"));

  const tabIds = [...INDEX_HTML.matchAll(/data-tab="([^"]+)"/g)].map((m) => m[1]);
  check(tabIds.length === 6, "expected 6 tabs in index.html, got " + tabIds.join(","));
  const TAB_EN = { library: "Library", shelves: "Shelves", upload: "Upload", saves: "Saves", stats: "Stats", more: "More" };
  const fontPx = parseFontPx(tabLinkCss["font-size"], 17);
  const padX = parsePaddingX(tabLinkCss["padding"]);
  const gapX = parseGapX(tabsCss["gap"]);

  for (const lang of LANGS) {
    let x = 0, rows = 1, maxRight = 0;
    for (const id of tabIds) {
      const label = tr(lang, TAB_EN[id]);
      const dotExtra = id === "more" ? 13 : 0;
      const w = Math.ceil(estimateTextWidth(label, fontPx) + padX + dotExtra);
      if (tabWraps && x > 0 && x + gapX + w > INNER_WIDTH) {
        x = 0;
        rows++;
      }
      const left = x > 0 ? x + gapX : 0;
      const right = left + w;
      if (right > maxRight) maxRight = right;
      check(
        left >= 0 && right <= INNER_WIDTH,
        `390px [${lang}] tab "${label}" cut off (left=${left}, right=${right} > ${INNER_WIDTH}px)`
      );
      x = right;
    }
    check(rows <= 2, `390px [${lang}] tabs take ${rows} rows (expected <= 2)`);
  }

  // 2. No triple update badge on #/more and #/updates (all 6 languages).
  const activeDotRule = computedFor(rules390, ".tabs a.on.has-update::after");
  check(
    activeDotRule["display"] === "none",
    "active More tab should hide its redundant update dot (.tabs a.on.has-update::after { display: none })"
  );

  for (const lang of LANGS) {
    const app = makeApp(lang);
    await app.ctx.start();
    await app.ctx.PocketMediaUpdate.refreshBadge();
    const pillText = tr(lang, "new: {s}", { s: "0.6.1" });

    // On #/library: More tab signals the update, and footer has the single update pill.
    check(app.els["tab-more"].classList.contains("has-update"), `[${lang}] #/library should mark More tab with has-update`);
    check(app.els.version.text.includes(pillText), `[${lang}] #/library footer should link to update (${pillText}), got "${app.els.version.text}"`);

    // On #/more: card shows pill ONCE; footer MUST NOT repeat it.
    app.ctx.location.hash = "#/more";
    await app.ctx.route();
    const moreMainCount = app.els.main.text.split(pillText).length - 1;
    const moreFootCount = app.els.version.text.split(pillText).length - 1;
    check(moreMainCount === 1, `[${lang}] #/more main should show "${pillText}" once, got ${moreMainCount}`);
    check(moreFootCount === 0, `[${lang}] #/more footer must not repeat "${pillText}", got "${app.els.version.text}"`);

    // On #/updates: headline shows version once; footer MUST NOT repeat the pill.
    app.ctx.location.hash = "#/updates";
    await app.ctx.route();
    const updFootCount = app.els.version.text.split(pillText).length - 1;
    check(updFootCount === 0, `[${lang}] #/updates footer must not repeat "${pillText}", got "${app.els.version.text}"`);

    // Back to #/stats: footer pill comes back once.
    app.ctx.location.hash = "#/stats";
    await app.ctx.route();
    check(app.els.version.text.includes(pillText), `[${lang}] #/stats footer should restore update link (${pillText}), got "${app.els.version.text}"`);
  }

  // 3. Unambiguous download labels in every language (never German "Laden" / "laden").
  for (const lang of LANGS) {
    const app = makeApp(lang);
    await app.ctx.start();

    app.ctx.location.hash = "#/photos";
    await app.ctx.route();
    const photoLinks = app.els.main.all().filter((e) => e.tag === "a" && "download" in e.attrs);
    check(photoLinks.length >= 3, `[${lang}] expected download links on #/photos, got ${photoLinks.length}`);
    for (const link of photoLinks) {
      const txt = link.text.trim();
      check(txt.length > 0, `[${lang}] empty download button on #/photos`);
      if (lang === "de") {
        check(txt !== "Laden" && !/\bladen\b/i.test(txt), `[de] ambiguous download label on #/photos: "${txt}"`);
      } else if (lang !== "en") {
        check(txt !== "Download file" && txt !== "Download", `[${lang}] untranslated download label on #/photos: "${txt}"`);
      }
    }

    const box = new El("section");
    await app.ctx.savesBlock("g1", box);
    const saveLinks = box.all().filter((e) => e.tag === "a" && "download" in e.attrs);
    check(saveLinks.length >= 2, `[${lang}] expected download links in savesBlock, got ${saveLinks.length}`);
    for (const link of saveLinks) {
      const txt = link.text.trim();
      if (lang === "de") {
        check(txt !== "Laden" && !/\bladen\b/i.test(txt), `[de] ambiguous download label on savesBlock: "${txt}"`);
      } else if (lang !== "en") {
        check(txt !== "Download file" && txt !== "Download", `[${lang}] untranslated download label on savesBlock: "${txt}"`);
      }
    }

    app.ctx.location.hash = "#/saves";
    await app.ctx.route();
    if (lang === "de") {
      const zipLinks = app.els.main.all().filter((e) => e.tag === "a" && (e.attrs.href || "").includes("/api/saves/zip"));
      for (const link of zipLinks) {
        check(!/\bladen\b/i.test(link.text), `[de] ambiguous ZIP button on #/saves: "${link.text}"`);
      }
    }
  }

  // 4. Stats week chart at 390 px: 7 unambiguous, pairwise distinct weekday labels in all 6 languages.
  for (const lang of LANGS) {
    const app = makeApp(lang);
    await app.ctx.start();
    app.ctx.location.hash = "#/stats/week";
    await app.ctx.route();
    const chartEl = app.els.main.all().find((e) => e.tag === "svg" && e.className.includes("chart"));
    check(Boolean(chartEl), `[${lang}] missing week chart SVG on #/stats/week`);
    if (chartEl) {
      const labels = chartEl.all()
        .filter((e) => e.tag === "text" && e.attrs["text-anchor"] === "middle")
        .map((e) => e.text.trim());
      check(labels.length === 7, `[${lang}] expected 7 weekday labels, got ${labels.length}: ${labels.join(", ")}`);
      const unique = new Set(labels);
      check(unique.size === 7, `[${lang}] ambiguous weekday labels (only ${unique.size}/7 unique): ${labels.join(" ")}`);
      for (const l of labels) {
        check(l.length >= 2 && l.length <= 4, `[${lang}] weekday label "${l}" should be 2-4 chars to stay clear and fit 390px`);
      }
    }
  }

  // 5. Stats mobile tile duration & system bar layout at 390 px.
  const tileBig = computedFor(rules390, ".tile-stat .big");
  check(
    tileBig["white-space"] === "nowrap",
    "390px .tile-stat .big should use white-space: nowrap so '4 h 55 min' never splits awkwardly"
  );
  const barTrack = computedFor(rules390, ".bar-row .track");
  check(
    (barTrack["grid-column"] || "").includes("1 / -1"),
    "390px .bar-row should stack the track full-width so long system names like 'Game Boy Advance' are not clipped"
  );

  if (failures.length) {
    console.error("web mobile polish FAILED (" + failures.length + "):\n - " + failures.join("\n - "));
    process.exit(1);
  }
  console.log("web mobile polish (390px, 6 languages): ok");
})().catch((e) => { console.error(e); process.exit(1); });
