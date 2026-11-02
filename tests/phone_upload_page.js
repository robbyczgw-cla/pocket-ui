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

function page(systems, language = "en") {
  const byId = {};
  const get = (id) => (byId[id] = byId[id] || new Node_("div", id));
  get("data").textContent = JSON.stringify({ systems, saves: ["srm"], disc: ["cue", "bin", "gdi", "raw", "iso"], prefer: { gdi: "dreamcast" } });
  get("folder").webkitdirectory = true;
  const sent = [];
  class Xhr {
    constructor() { this.upload = {}; this.status = 200; }
    open(_method, url) { this.url = url; }
    send() {
      const url = this.url;
      setImmediate(() => {
        if (url.includes("/upload?")) sent.push(url.split("?")[1]);
        this.responseText = JSON.stringify({ ok: true, path: "p" });
        this.onload();
      });
    }
  }
  class Reader {
    readAsText(f) { setImmediate(() => { this.result = f.text; this.onload(); }); }
  }
  const sandbox = {
    document: { getElementById: get, querySelectorAll: () => [], createElement: (t) => new Node_(t), documentElement: {} },
    navigator: { language, userAgent: "test" },
    location: { pathname: "/code/" },
    XMLHttpRequest: Xhr,
    FileReader: Reader,
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
    // Until nothing more is sent for a while (a fixed wait was too short on a loaded build machine).
    async settle() {
      for (let last = -1; last !== this.sent.length; ) {
        last = this.sent.length;
        await new Promise((done) => setTimeout(done, 40));
      }
    },
  };
}

const file = (name, text = "x", rel = "") => ({ name, text, size: text.length, webkitRelativePath: rel });
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
  console.log("ok");
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
