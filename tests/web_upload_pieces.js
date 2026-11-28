// Drives the upload of src/web/page/app.js (`sendOne`) in Node against a stand-in of the Pocket's side
// (`src/web/api.rs`, `/api/upload/start` + `/api/upload/part`): a large file goes in pieces, each read here first,
// a piece without an answer goes again, and each failure says what really happened.
// Putting a save or state back (src/web/page/saves.js, `putBack`) goes the same way, from `/api/saves/upload/start`.
// Usage: node tests/web_upload_pieces.js <app.js> [<saves.js>]
"use strict";
const assert = require("assert");
const fs = require("fs");
const vm = require("vm");

const MB = 1 << 20;

// The Pocket: `serve(path, query, data)` may answer first ("none": no answer at all; null: as the Pocket would).
function pocket(serve) {
  const ups = {};
  const log = [];
  let n = 0;
  const answer = (path, q, data) => {
    if (path === "/api/upload/start") {
      const id = "u" + ++n;
      ups[id] = { total: Number(q.get("size")), received: 0 };
      return { status: 200, body: { ok: true, id, piece: 8 * MB, path: "roms/ps2/Game.iso" } };
    }
    // A save or state put back: the last piece answers as `/api/saves/upload` does (no path).
    if (path === "/api/saves/upload/start") {
      const id = "s" + ++n;
      ups[id] = { total: Number(q.get("size")), received: 0, save: { replace: q.get("replace") === "1" } };
      return { status: 200, body: { ok: true, id, piece: 8 * MB } };
    }
    if (path === "/api/upload/cancel") { delete ups[q.get("id")]; return { status: 200, body: { ok: true } }; }
    if (path === "/api/upload/part") {
      const up = ups[q.get("id")];
      if (!up) return { status: 404, body: { ok: false, gone: true, error: "That upload is not on the Pocket any more. Nothing was kept." } };
      if (Number(q.get("offset")) !== up.received) return { status: 416, body: { ok: false, received: up.received, error: "wrong place" } };
      up.received += data.byteLength;
      if (up.received >= up.total && up.save) { delete ups[q.get("id")]; return { status: 200, body: { ok: true, name: "Game (USA).state", game: "Game", replaced: up.save.replace, backup: up.save.replace ? "2026-11-26_1200" : null } }; }
      if (up.received >= up.total) { delete ups[q.get("id")]; return { status: 200, body: { ok: true, path: "roms/ps2/Game.iso", bytes: up.total, replaced: false } }; }
      return { status: 200, body: { ok: true, received: up.received } };
    }
    return { status: 404, body: { error: "Not found." } };
  };
  return {
    ups,
    log,
    answer,
    call(url, data) {
      const [path, query] = url.split("?");
      const q = new URLSearchParams(query || "");
      log.push(path.replace("/api/upload/", "").replace("/api/saves/upload/", "save-") + (q.get("offset") !== null ? "@" + q.get("offset") : ""));
      const r = (serve && serve(path, q, data, answer)) || answer(path, q, data);
      return r;
    },
  };
}

function run(serve, opts) {
  opts = opts || {};
  const pk = pocket(serve);
  class Xhr {
    constructor() { this.upload = {}; this.headers = {}; }
    open(method, url) { this.url = url; }
    setRequestHeader(k, v) { this.headers[k] = v; }
    send(data) {
      setImmediate(() => {
        const r = pk.call(this.url, data);
        if (r === "none") return this.onerror();
        if (this.upload.onprogress && data && !opts.noProgress) this.upload.onprogress({ loaded: data.byteLength, lengthComputable: true });
        this.status = r.status;
        this.responseText = JSON.stringify(r.body);
        this.onload();
      });
    }
  }
  const reply = (status, json) => ({ ok: status === 200, status, headers: { get: () => "application/json" }, json: async () => json });
  const ctx = {
    console, URLSearchParams, Date, Math, Promise, Object, String, Number, Array, JSON, Intl, Error, Set,
    document: { addEventListener() {}, documentElement: { lang: "en" }, getElementById: () => null },
    window: { addEventListener() {} },
    XMLHttpRequest: Xhr,
    setTimeout: (f) => setImmediate(f),
    clearTimeout() {},
    confirm: () => true,
    fetch: async (path) => {
      if (path.startsWith("/api/saves/check")) return reply(200, opts.saveCheck || { ok: true, kind: "state", name: "Game (USA).state", replaces: null });
      if (path.startsWith("/api/upload/check")) return reply(200, opts.check || { ok: true, exists: false, path: "roms/ps2/Game.iso" });
      if (path === "/api/session") {
        if (opts.gone) throw new Error("down");
        return reply(200, { signedIn: true });
      }
      return reply(404, { error: "Not found." });
    },
  };
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(process.argv[2], "utf8"), ctx);
  if (process.argv[3]) vm.runInContext(fs.readFileSync(process.argv[3], "utf8"), ctx);
  return { ctx, pk };
}

// A file as the browser hands it: `slice().arrayBuffer()` reads a piece (or can't).
const file = (size, unreadable) => ({
  name: "Game.iso",
  size,
  slice(from, to) {
    return { arrayBuffer: async () => { if (unreadable) throw new Error("NotReadableError"); return { byteLength: to - from }; } };
  },
});

function row() {
  return { el: {}, state: { className: "", textContent: "" }, bar: { value: 0, remove() { this.removed = true; } } };
}

async function send(p, f) {
  const r = row();
  const ok = await p.ctx.sendOne(f, { kind: "rom", system: "ps2", name: "Game.iso" }, r);
  return { ok, r, parts: p.pk.log.filter((l) => l.startsWith("part")) };
}

(async () => {
  // Raw Danger, 1.62 GB: 8 MB pieces, each where the Pocket stands; saved after the last.
  {
    const p = run();
    const { ok, r, parts } = await send(p, file(1620 * MB));
    assert.ok(ok, r.state.textContent);
    assert.strictEqual(parts.length, Math.ceil(1620 / 8));
    assert.deepStrictEqual(parts.slice(0, 3), ["part@0", "part@" + 8 * MB, "part@" + 16 * MB]);
    assert.strictEqual(r.state.textContent, "Saved as roms/ps2/Game.iso");
    assert.strictEqual(r.bar.value, 1);
    assert.deepStrictEqual(p.pk.ups, {});
  }
  // A browser that reports no progress while it sends: the bar still moves with every piece the Pocket confirms.
  {
    const seen = [];
    let row_;
    const p = run((path, q, data, answer) => {
      if (path === "/api/upload/part") seen.push(row_.bar.value);
      return null;
    }, { noProgress: true });
    row_ = row();
    await p.ctx.sendOne(file(40 * MB), { kind: "rom", system: "ps2", name: "Game.iso" }, row_);
    assert.deepStrictEqual(seen.map((v) => Math.round(v * 100)), [0, 20, 40, 60, 80], "the bar before each piece");
  }
  // An empty file is one empty piece.
  {
    const p = run();
    const { ok, parts } = await send(p, file(0));
    assert.ok(ok);
    assert.deepStrictEqual(parts, ["part@0"]);
  }
  // A piece without an answer goes again; one that arrived but lost its answer is not written twice.
  {
    const cut = new Set();
    const p = run((path, q, data, answer) => {
      const at = q.get("offset");
      if (path === "/api/upload/part" && at === String(8 * MB) && !cut.has(at)) { cut.add(at); return "none"; }
      if (path === "/api/upload/part" && at === String(16 * MB) && !cut.has(at)) { cut.add(at); answer(path, q, data); return "none"; }
      return null;
    });
    const { ok, r, parts } = await send(p, file(30 * MB));
    assert.ok(ok, r.state.textContent);
    assert.deepStrictEqual(parts, ["part@0", "part@" + 8 * MB, "part@" + 8 * MB, "part@" + 16 * MB, "part@" + 16 * MB, "part@" + 24 * MB]);
  }
  // The browser can't read the file: it says so, and the Pocket throws its part away.
  {
    const p = run();
    const { ok, r } = await send(p, file(20 * MB, true));
    assert.ok(!ok);
    assert.strictEqual(r.state.textContent, "This browser could not read the file. Nothing was kept.");
    assert.ok(p.pk.log.includes("cancel"), p.pk.log.join());
    assert.deepStrictEqual(p.pk.ups, {});
  }
  // The connection breaks off for good while the Pocket answers: "dropped", and its part goes.
  {
    const p = run((path) => (path === "/api/upload/part" ? "none" : null));
    const { ok, r, parts } = await send(p, file(20 * MB));
    assert.ok(!ok);
    assert.strictEqual(r.state.textContent, "The connection dropped. Nothing was kept.");
    assert.strictEqual(parts.length, 9, "the first try and eight more");
    assert.ok(p.pk.log.includes("cancel"));
  }
  // The Pocket answers no more: it says so, not that the connection dropped.
  {
    const p = run((path) => (path === "/api/upload/part" ? "none" : null), { gone: true });
    const { ok, r } = await send(p, file(20 * MB));
    assert.ok(!ok);
    assert.strictEqual(r.state.textContent, "The Pocket did not answer. Nothing was kept.");
  }
  // A refusal of the Pocket (no room) is said as the Pocket says it; nothing is sent.
  {
    const p = run((path) => (path === "/api/upload/start" ? { status: 507, body: { ok: false, error: "Not enough space on the SD card: 1 GB free." } } : null));
    const { ok, r, parts } = await send(p, file(20 * MB));
    assert.ok(!ok);
    assert.strictEqual(r.state.textContent, "Not enough space on the SD card: 1 GB free.");
    assert.deepStrictEqual(parts, []);
  }
  // A 300 MB save state put back (a PS2 state is tens of MB, the Pocket takes up to 512 MB): in pieces too, the old
  // one replaced after asking; the answer of the last piece is the put-back's.
  if (process.argv[3]) {
    const p = run(null, { saveCheck: { ok: true, kind: "state", name: "Game (USA).state", replaces: { name: "Game (USA).state", bytes: 5, modified: 1 } } });
    const r = row();
    const f = Object.assign(file(300 * MB), { name: "Game.state" });
    const ok = await p.ctx.putBack("g1", f, r);
    const parts = p.pk.log.filter((l) => l.startsWith("part"));
    assert.ok(ok, r.state.textContent);
    assert.strictEqual(p.pk.log[0], "save-start", p.pk.log.join());
    assert.ok(!p.pk.log.some((l) => l === "/api/saves/upload"), "never in one request");
    assert.strictEqual(parts.length, Math.ceil(300 / 8));
    assert.strictEqual(r.state.textContent, "Put back as Game (USA).state. The old one is in the backup.");
    assert.strictEqual(r.bar.value, 1);
    assert.deepStrictEqual(p.pk.ups, {});
  }
  // A game that starts on the Pocket ends a put-back: said as the Pocket says it, nothing sent after.
  if (process.argv[3]) {
    const p = run((path, q) => (path === "/api/upload/part" && q.get("offset") === String(16 * MB) ? { status: 500, body: { ok: false, error: "A game started. Nothing was changed." } } : null));
    const r = row();
    const ok = await p.ctx.putBack("g1", Object.assign(file(40 * MB), { name: "Game.state" }), r);
    assert.ok(!ok);
    assert.strictEqual(r.state.textContent, "A game started. Nothing was changed.");
    assert.deepStrictEqual(p.pk.log.filter((l) => l.startsWith("part")), ["part@0", "part@" + 8 * MB, "part@" + 16 * MB]);
  }
  console.log("web upload pieces: ok");
})().catch((e) => { console.error(e); process.exit(1); });
