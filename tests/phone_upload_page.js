// Drives assets/phone-upload/app.js in Node with just enough of a page: which file is sent where, and what still asks.
// Usage: node tests/phone_upload_page.js <app.js>
"use strict";
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

class Node_ {
  constructor(tag, id) { this.tag = tag; this.id = id; this.children = []; this.listeners = {}; this.hidden = false; this.textContent = ""; this.className = ""; this.value = ""; this.parent = null; this.selectedIndex = 0; this.files = []; }
  appendChild(c) { c.parent = this; this.children.push(c); return c; }
  remove() { if (this.parent) this.parent.children = this.parent.children.filter((c) => c !== this); this.parent = null; }
  addEventListener(name, f) { this.listeners[name] = f; }
  fire(name) { this.listeners[name].call(this); }
  get options() { return this.children; }
  all(tag, out = []) { for (const c of this.children) { if (c.tag === tag) out.push(c); c.all(tag, out); } return out; }
}

// The Pocket's side as `src/phone_upload.rs` answers it: a file starts (`start`), comes in pieces that must follow each
// other (`part`), and is in place after the last. `serve(url, data)` may answer first (null: no answer at all).
function pocket() {
  const ups = {};
  const log = [];
  let n = 0;
  return {
    log,
    ups,
    // Every request the page makes, answered or not: "part@<offset>".
    note(url) {
      const [path, query] = url.split("?");
      const q = new URLSearchParams(query || "");
      log.push(path.split("/").pop() + (q.get("offset") !== null ? "@" + q.get("offset") : ""));
    },
    answer(url, data) {
      const [path, query] = url.split("?");
      const q = new URLSearchParams(query || "");
      if (path.endsWith("/check")) return { status: 200, body: { ok: true, path: "p" } };
      if (path.endsWith("/start")) {
        const id = "u" + ++n;
        ups[id] = { total: Number(q.get("size")), received: 0 };
        return { status: 200, body: { ok: true, id, piece: 8 << 20, path: "p" } };
      }
      if (path.endsWith("/cancel")) { delete ups[q.get("id")]; return { status: 200, body: { ok: true } }; }
      if (path.endsWith("/part")) {
        const up = ups[q.get("id")];
        if (!up) return { status: 404, body: { ok: false, code: "gone", error: "gone" } };
        if (Number(q.get("offset")) !== up.received) return { status: 416, body: { ok: false, received: up.received } };
        up.received += data.size;
        if (up.received >= up.total) { delete ups[q.get("id")]; return { status: 200, body: { ok: true, path: "p" } }; }
        return { status: 200, body: { ok: true, received: up.received } };
      }
      if (path.endsWith("/app.css")) return { status: 200, body: "" };
      return { status: 404, body: "Not found" };
    },
  };
}

function page(systems, language = "en", serve = null) {
  const byId = {};
  const get = (id) => (byId[id] = byId[id] || new Node_("div", id));
  get("data").textContent = JSON.stringify({ systems, saves: ["srm"], disc: ["cue", "bin", "gdi", "raw", "iso"], prefer: { gdi: "dreamcast" } });
  get("folder").webkitdirectory = true;
  get("stopped").hidden = true;
  const sent = [];
  const pk = pocket();
  const pending = new Set();
  // Readiness is an API completion, not a quiet interval in the upload count:
  // sheet reads and check requests may still be queued before any upload exists.
  function later(callback) {
    const task = {};
    pending.add(task);
    setImmediate(() => {
      try { callback(); }
      finally { pending.delete(task); }
    });
  }
  class Xhr {
    constructor() { this.upload = {}; this.status = 200; }
    open(_method, url) { this.url = url; }
    send(data) {
      const url = this.url;
      later(() => {
        if (url.includes("/start?")) sent.push(url.split("?")[1]);
        pk.note(url);
        const r = (serve && serve(url, data, pk)) || pk.answer(url, data);
        if (r === "none") return this.onerror();
        this.status = r.status;
        this.responseText = typeof r.body === "string" ? r.body : JSON.stringify(r.body);
        this.onload();
      });
    }
  }
  class Reader {
    readAsText(f) { later(() => { this.result = f.text; this.onload(); }); }
    readAsArrayBuffer(b) { later(() => { if (b.unreadable) this.onerror(); else { this.result = b; this.onload(); } }); }
  }
  const sandbox = {
    document: { getElementById: get, querySelectorAll: () => [], createElement: (t) => new Node_(t), documentElement: {} },
    navigator: { language, userAgent: "test" },
    location: { pathname: "/code/" },
    XMLHttpRequest: Xhr,
    FileReader: Reader,
    // A retry's pause: at once here.
    setTimeout: (f) => later(f),
    encodeURIComponent,
    JSON,
    Array,
    String,
    Math,
  };
  vm.runInNewContext(fs.readFileSync(process.argv[2], "utf8"), sandbox);
  return {
    sandbox,
    sent,
    pk,
    stopped: get("stopped"),
    list: get("list"),
    pick(id, files) {
      const input = get(id);
      input.files = files;
      input.fire("change");
    },
    choose(row, id) {
      const sel = row.all("select")[0];
      sel.selectedIndex = sel.options.findIndex((o) => o.value === id);
      sel.value = id;
      sel.fire("change");
    },
    row(name) { return this.list.children.find((li) => li.children[0].textContent === name); },
    async settle() {
      const until = Date.now() + 10000;
      do {
        await new Promise((done) => setImmediate(done));
        assert.ok(Date.now() < until, "phone page did not finish its queued requests/readers");
      } while (pending.size);
    },
  };
}

const file = (name, text = "x", rel = "", size = text.length, unreadable = false) => ({
  name, text, size, webkitRelativePath: rel,
  slice(from, to) { return { size: to - from, unreadable }; },
});
const MB = 1 << 20;
const DISCS = [
  { id: "ps2", name: "PlayStation 2", folder: "ps2", ext: ["iso"], max: 1e10 },
  { id: "psp", name: "PSP", folder: "psp", ext: ["iso"], max: 1e10 },
  { id: "megadrive", name: "Mega Drive", folder: "megadrive", ext: ["bin"], max: 1e10 },
  { id: "dreamcast", name: "Dreamcast", folder: "dreamcast", ext: ["gdi", "cue", "bin", "raw"], max: 1e10 },
  { id: "saturn", name: "Saturn", folder: "saturn", ext: ["cue", "bin"], max: 1e10 },
];

(async () => {
  // One .iso is PS2 and one PSP: a choice is that file's.
  {
    const p = page(DISCS);
    p.pick("files", [file("Two.iso"), file("Portable.iso")]);
    await p.settle();
    p.choose(p.row("Two.iso"), "ps2");
    await p.settle();
    assert.deepStrictEqual(p.sent, ["name=Two.iso&size=1&system=ps2"]);
    assert.strictEqual(p.row("Portable.iso").all("select").length, 1, "the other file still asks");
    p.choose(p.row("Portable.iso"), "psp");
    await p.settle();
    assert.strictEqual(p.sent[1], "name=Portable.iso&size=1&system=psp");
  }
  // "The same for the rest" only when pressed.
  {
    const p = page(DISCS);
    p.pick("files", [file("A.iso"), file("B.iso"), file("C.iso")]);
    await p.settle();
    p.choose(p.row("A.iso"), "psp");
    await p.settle();
    assert.strictEqual(p.sent.length, 1);
    const same = p.row("A.iso").all("button")[0];
    assert.ok(same.textContent.includes("2 others") && same.textContent.includes("PSP"), same.textContent);
    same.fire("click");
    await p.settle();
    assert.deepStrictEqual(p.sent.map((s) => s.split("&")[0]).sort(), ["name=A.iso", "name=B.iso", "name=C.iso"]);
    assert.ok(p.sent.every((s) => s.includes("system=psp")));
  }
  // A folder with a .gdi: its tracks go with it, the .bin asks nothing though Mega Drive takes .bin too.
  {
    const p = page(DISCS);
    const gdi = '2\n1 0 4 2352 "Track01.bin" 0\n2 600 0 2352 track02.raw 0\n';
    p.pick("folder", [file("track01.bin", "x", "Game/track01.bin"), file("Game.gdi", gdi, "Game/Game.gdi"), file("track02.raw", "x", "Game/track02.raw")]);
    await p.settle();
    assert.strictEqual(p.sent.length, 3, p.sent.join());
    assert.ok(p.sent.every((s) => s.includes("system=dreamcast") && s.includes("dir=Game")), p.sent.join());
    assert.strictEqual(p.list.all("select").length, 0);
  }
  // A .cue that asks: its .bin waits, says whose it is, and goes with the choice; a .bin of no sheet keeps its own question.
  {
    const p = page(DISCS);
    p.pick("files", [file("Disc.cue", 'FILE "Disc.bin" BINARY\n'), file("Disc.bin"), file("Sonic.bin")]);
    await p.settle();
    assert.deepStrictEqual(p.sent, []);
    assert.ok(p.row("Disc.bin").children[1].textContent.includes("Disc.cue"));
    assert.strictEqual(p.row("Disc.bin").all("select").length, 0);
    p.choose(p.row("Disc.cue"), "saturn");
    await p.settle();
    assert.deepStrictEqual(p.sent.map((s) => s.split("&")[0]).sort(), ["name=Disc.bin", "name=Disc.cue"]);
    assert.ok(p.sent.every((s) => s.includes("system=saturn")));
    assert.strictEqual(p.row("Sonic.bin").all("select").length, 1);
  }
  // Two discs in folders of the same name: each sheet takes its own tracks, whichever is read first.
  {
    const p = page(DISCS);
    const cue = 'FILE "track01.bin" BINARY\n';
    p.pick("folder", [
      file("Dreamcast.cue", cue, "Collection/Dreamcast/Game/Dreamcast.cue"),
      file("track01.bin", "dc", "Collection/Dreamcast/Game/track01.bin"),
      file("Saturn.cue", cue, "Collection/Saturn/Game/Saturn.cue"),
      file("track01.bin", "ss", "Collection/Saturn/Game/track01.bin"),
    ]);
    await p.settle();
    p.choose(p.row("Dreamcast.cue"), "dreamcast");
    p.choose(p.row("Saturn.cue"), "saturn");
    await p.settle();
    assert.deepStrictEqual(p.sent.slice().sort(), [
      "name=Dreamcast.cue&size=" + cue.length + "&system=dreamcast&dir=Game",
      "name=Saturn.cue&size=" + cue.length + "&system=saturn&dir=Game",
      "name=track01.bin&size=2&system=dreamcast&dir=Game",
      "name=track01.bin&size=2&system=saturn&dir=Game",
    ]);
  }
  // The words follow the phone's language: German, Spanish, English for everything else; a missing line shows as undefined.
  {
    const words = {};
    for (const lang of ["de", "es-MX", "es", "en-US", "fr", ""]) {
      const p = page(DISCS, lang);
      p.pick("files", [file("Sonic.bin"), file("Game.srm")]);
      await p.settle();
      const lines = [p.row("Sonic.bin").children[1].textContent, p.row("Game.srm").children[1].textContent];
      assert.ok(lines.every((l) => l && !l.includes("undefined")), lang + ": " + lines);
      words[lang] = lines.join(" | ");
    }
    assert.strictEqual(words.es, words["es-MX"]);
    assert.strictEqual(words["en-US"], words.fr);
    assert.ok(words.de.includes("Spielstände") && words.es.includes("partidas") && words["en-US"].includes("Saves"), JSON.stringify(words));
    assert.notStrictEqual(words.de, words.es);
    const es = page(DISCS, "es");
    assert.strictEqual(es.sandbox.document.documentElement.lang, "es");
  }
  // A large disc goes in pieces of 8 MB, each starting where the Pocket stands, and is added after the last.
  {
    const p = page(DISCS);
    p.pick("files", [file("Raw Danger.iso", "", "", 1620 * MB)]);
    await p.settle();
    p.choose(p.row("Raw Danger.iso"), "ps2");
    await p.settle();
    const parts = p.pk.log.filter((l) => l.startsWith("part"));
    assert.strictEqual(parts.length, Math.ceil(1620 / 8), parts.length + " pieces");
    assert.deepStrictEqual(parts.slice(0, 3), ["part@0", "part@" + 8 * MB, "part@" + 16 * MB]);
    assert.ok(p.row("Raw Danger.iso").children[1].textContent.startsWith("Added: "), p.row("Raw Danger.iso").children[1].textContent);
    assert.ok(p.stopped.hidden, "no word of the Pocket closing the page");
  }
  // A piece that gets no answer goes again; one the Pocket got after all is not sent twice (it says where it stands).
  {
    const cut = new Set();
    const once = (at) => !cut.has(at) && cut.add(at);
    const serve = (url, data, pk) => {
      if (url.includes("part?") && url.endsWith("offset=" + 8 * MB) && once(8)) return "none";
      // This one arrives, only its answer is lost.
      if (url.includes("part?") && url.endsWith("offset=" + 16 * MB) && once(16)) { pk.answer(url, data); return "none"; }
      return null;
    };
    const p = page(DISCS, "en", serve);
    p.pick("files", [file("Big.iso", "", "", 30 * MB)]);
    await p.settle();
    p.choose(p.row("Big.iso"), "ps2");
    await p.settle();
    assert.deepStrictEqual(p.pk.log.filter((l) => l.startsWith("part")), ["part@0", "part@" + 8 * MB, "part@" + 8 * MB, "part@" + 16 * MB, "part@" + 16 * MB, "part@" + 24 * MB]);
    assert.ok(p.row("Big.iso").children[1].textContent.startsWith("Added: "), p.row("Big.iso").children[1].textContent);
    assert.ok(p.stopped.hidden);
  }
  // The phone can't read the file: it says so, the Pocket throws its part away, and the page goes on.
  for (const [lang, words] of [["en", "This phone could not read the file"], ["de", "Dieses Handy konnte die Datei nicht lesen"], ["es", "Este teléfono no pudo leer"]]) {
    const p = page(DISCS, lang);
    p.pick("files", [file("Broken.iso", "", "", 20 * MB, true)]);
    await p.settle();
    p.choose(p.row("Broken.iso"), "ps2");
    await p.settle();
    assert.ok(p.row("Broken.iso").children[1].textContent.startsWith(words), p.row("Broken.iso").children[1].textContent);
    assert.ok(p.pk.log.includes("cancel"), p.pk.log.join());
    assert.deepStrictEqual(p.pk.ups, {}, "nothing left half sent");
    assert.ok(p.stopped.hidden, lang + ": the page did not close");
  }
  // The connection breaks off for good while the Pocket still has the page open: that file failed, the page stays.
  {
    const serve = (url) => (url.includes("part?") ? "none" : null);
    const p = page(DISCS, "en", serve);
    p.pick("files", [file("Lost.iso", "", "", 20 * MB)]);
    await p.settle();
    p.choose(p.row("Lost.iso"), "ps2");
    await p.settle();
    assert.ok(/^The connection to the (Pocket|Brick) broke off/.test(p.row("Lost.iso").children[1].textContent), p.row("Lost.iso").children[1].textContent);
    assert.ok(p.stopped.hidden, "the Pocket still answers: no word of it closing the page");
    assert.ok(p.pk.log.includes("cancel"), "the part on the Pocket is thrown away");
  }
  // Only when the Pocket answers no more does the page say it was closed.
  {
    const serve = (url) => (url.includes("part?") || url.endsWith("app.css") ? "none" : null);
    const p = page(DISCS, "en", serve);
    p.pick("files", [file("Asleep.iso", "", "", 20 * MB)]);
    await p.settle();
    p.choose(p.row("Asleep.iso"), "ps2");
    await p.settle();
    assert.ok(!p.stopped.hidden, "the Pocket is gone: the page says so");
  }
  console.log("ok");
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
