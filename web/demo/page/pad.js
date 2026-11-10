// The page's side of the browser demo (docs/WEB-DEMO.md): the canvas, keyboard and on-screen pad, the frame loop.
// Button numbers as in web/app_web_demo.rs. A page brings <canvas id="screen">, an element #status for words while it
// loads, and #pad: its own buttons with data-b="up|down|…|home", or empty (a plain pad is made). It hears how loading goes
// as "pocket-demo" events on document: detail {state: "loading", done, total} | {state: "ready"} | {state: "stopped"}.
"use strict";
const HERE = document.currentScript ? document.currentScript.src : location.href;
const B = { up: 0, down: 1, left: 2, right: 3, a: 4, b: 5, x: 6, y: 7, l1: 8, r1: 9, l2: 10, r2: 11, select: 12, start: 13, home: 14 };
// Keys by what is printed on them (a German keyboard's Y is its Y), the arrows and the rest by place.
const KEYS = {
  ArrowUp: B.up, ArrowDown: B.down, ArrowLeft: B.left, ArrowRight: B.right,
  Enter: B.a, " ": B.a, Backspace: B.b, Escape: B.b,
  x: B.x, y: B.y, q: B.l1, e: B.r1, "1": B.l2, "3": B.r2, Shift: B.select, s: B.start, m: B.home, Home: B.home,
};
const keyOf = (e) => (e.key.length === 1 ? e.key.toLowerCase() : e.key);
const PAD = [["l2", "L2"], ["l1", "L1"], ["select", "SELECT"], ["home", "HOME"], ["start", "START"], ["r1", "R1"], ["r2", "R2"],
  [null], ["up", "▲"], [null], [null], [null], ["x", "X"], [null],
  ["left", "◀"], [null], ["right", "▶"], [null], ["y", "Y"], [null], ["a", "A"],
  [null], ["down", "▼"], [null], [null], [null], ["b", "B"], [null]];
const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");
const statusEl = document.getElementById("status");
// What the canvas shows now: "card" or "frame" (pocket-ui's), for the tests (window.pocketDemo.cardShown).
let onCanvas = "";
let ready = false, seen = 0, image = null, frames = 0;
function status() {
  try { return Module.FS.readFile("/tmp/pocket-ui.status", { encoding: "utf8" }); } catch { return ""; }
}
function field(s, key) { const m = s.match(new RegExp("(?:^|\t)" + key + "=([^\t\n]*)")); return m ? m[1] : ""; }
function paintStats() {
  const at = Module._malloc(12); Module._pocket_web_paint_stats(at);
  const [n, mean, max] = Module.HEAPU32.subarray(at >> 2, (at >> 2) + 3); Module._free(at);
  return { frames: n, meanMs: mean / 1000, maxMs: max / 1000 };
}
window.pocketDemo = { frames: () => frames, ready: () => ready, status, paintStats, cardShown: () => onCanvas === "card",
  setSound: (on) => setSound(on), sound: () => ({ on: sound.on, started: !!sound.gain, state: sound.ctx ? sound.ctx.state : "none", ...sound.stats }),
  soundReset: () => { sound.stats = { frames: 0, loud: 0, peak: 0, underruns: 0 }; } };

function tell(detail) { document.dispatchEvent(new CustomEvent("pocket-demo", { detail })); }
var Module = {
  canvas,
  // The library beside this script; the program in the folder the page chose (window.pocketDemoCode: simd/ or base/,
  // by what the browser can run), else beside this script too.
  locateFile: (file) => new URL(file, file.endsWith(".data") ? HERE : new URL(window.pocketDemoCode || "./", HERE)).href,
  preRun: [() => {
    Object.assign(Module.ENV || ENV, { POCKETUI_DEVICE: "rg35xx-h", POCKETUI_LANG: "en", TZ: "UTC", HOME: "/storage" });
    // pocket-core's command FIFO, as a stream: what pocket-ui writes waits here until the stand-in game reads it
    // (web/app_web_demo.rs). A plain file lost a command written right after another ("save 1", then "resume").
    const fs = Module.FS, queue = [], dev = fs.makedev(64, 1);
    fs.registerDevice(dev, {
      read(stream, buffer, offset, length) { const n = Math.min(length, queue.length); for (let i = 0; i < n; i++) buffer[offset + i] = queue.shift(); return n; },
      write(stream, buffer, offset, length) { for (let i = 0; i < length; i++) queue.push(buffer[offset + i] & 255); return length; },
    });
    fs.mkdev("/tmp/pocket-core.cmd", dev);
  }],
  onRuntimeInitialized() {
    ready = true;
    Module._pocket_web_resize(canvas.width, canvas.height);
    requestAnimationFrame(tick);
  },
  print: (t) => console.log(t),
  printErr: (t) => console.warn(t),
  // The program itself, fetched here so the page can show how far it is (emscripten tells only the library's progress).
  // `done` counts unpacked bytes; `total` is the page's guess (window.pocketDemoExpect) when the server compresses.
  instantiateWasm(imports, done) {
    (async () => {
      const res = await fetch(Module.locateFile("pocket_ui.wasm"));
      if (!res.ok) throw new Error("pocket_ui.wasm: " + res.status);
      const plain = !res.headers.get("content-encoding");
      const total = (plain && Number(res.headers.get("content-length"))) || window.pocketDemoExpect || 0;
      let bytes;
      if (res.body && res.body.getReader) {
        const reader = res.body.getReader(), parts = []; let got = 0;
        for (;;) {
          const { done: end, value } = await reader.read();
          if (end) break;
          parts.push(value); got += value.length;
          tell({ state: "loading", done: got, total: Math.max(total, got) });
        }
        bytes = new Uint8Array(got); let at = 0;
        for (const p of parts) { bytes.set(p, at); at += p.length; }
      } else bytes = new Uint8Array(await res.arrayBuffer());
      tell({ state: "starting" });
      const { instance, module } = await WebAssembly.instantiate(bytes, imports);
      done(instance, module);
    })().catch((e) => { statusEl.hidden = false; statusEl.textContent = "pocket-ui couldn't load. Reload the page to try again."; tell({ state: "stopped", why: String(e) }); });
    return {};
  },
  onAbort: (w) => { statusEl.hidden = false; statusEl.textContent = "pocket-ui stopped. Reload the page to start it again."; tell({ state: "stopped", why: String(w) }); },
};

// The honest card while a "game runs": pocket-ui paints it in its own look when the start is asked for
// (web/app_web_demo.rs, pocket_web_card); the page puts it up once the start animation is over.
let cardSeen = -1, cardImage = null;
function showCard() {
  const p = Module._pocket_web_card();
  if (p) {
    const h32 = Module.HEAPU32, at = p >> 2;
    const w = h32[at], h = h32[at + 1], n = h32[at + 2], ptr = h32[at + 3];
    if (n !== cardSeen || !cardImage) {
      cardImage = new ImageData(w, h);
      cardImage.data.set(Module.HEAPU8.subarray(ptr, ptr + w * h * 4));
      cardSeen = n;
    }
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    ctx.putImageData(cardImage, 0, 0);
  }
  onCanvas = "card";
}
// The card comes when the game "runs" and pocket-ui's start animation is over (no new frame for a moment): on a handheld
// the game's own picture takes over there.
let playing = false, statusAt = 0, frameAt = 0;
function tick(now) {
  if (now - statusAt > 100) {
    statusAt = now;
    const s = status();
    const was = playing;
    playing = field(s, "game") === "playing" && field(s, "place") === "game" && now - frameAt > 250;
    // Leaving the card (the pause menu opened): the newest frame again, even one that came while the card was up.
    if (was && !playing) seen = 0;
  }
  if (playing) {
    showCard();
    const p = Module._pocket_web_frame(seen);
    if (p) seen = Module.HEAPU32[(p >> 2) + 2];
    requestAnimationFrame(tick);
    return;
  }
  const p = Module._pocket_web_frame(seen);
  if (p) {
    const h32 = Module.HEAPU32, at = p >> 2;
    const w = h32[at], h = h32[at + 1], ptr = h32[at + 3];
    seen = h32[at + 2];
    if (!image || image.width !== w || image.height !== h) {
      image = new ImageData(w, h);
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
    }
    image.data.set(Module.HEAPU8.subarray(ptr, ptr + w * h * 4));
    ctx.putImageData(image, 0, 0);
    onCanvas = "frame";
    frames++;
    frameAt = performance.now();
    if (!statusEl.hidden) { statusEl.hidden = true; tell({ state: "ready" }); }
  }
  requestAnimationFrame(tick);
}

// Sound: pocket-ui's own UI sounds (web/web_audio.rs) through an AudioWorklet (sound.js). Browsers let a page play only
// after a press or tap, so the first one starts it; until then nothing sounds and nothing piles up. The page's switch
// (window.pocketDemo.setSound) is kept in localStorage; Settings → Sounds inside pocket-ui work as on a handheld.
const SOUND_KEY = "pocketui-demo-sound";
const sound = { ctx: null, gain: null, on: true, stats: { frames: 0, loud: 0, peak: 0, underruns: 0 } };
try { sound.on = localStorage.getItem(SOUND_KEY) !== "off"; } catch { /* storage off: sound on */ }
function soundRun(on) { if (ready && Module._pocket_web_audio_running) Module._pocket_web_audio_running(on ? 1 : 0); }
function soundStart() {
  if (!ready || sound.ctx || !sound.on) return;
  const AC = window.AudioContext || window.webkitAudioContext;
  if (!AC || !window.AudioWorkletNode) return;
  let ctx;
  try { ctx = new AC({ sampleRate: 48000, latencyHint: "interactive" }); } catch { try { ctx = new AC(); } catch { return; } }
  sound.ctx = ctx;
  ctx.resume();
  ctx.audioWorklet.addModule(new URL("sound.js", HERE).href).then(() => {
    const node = new AudioWorkletNode(ctx, "pocket-ui-sound", { numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [2] });
    const gain = ctx.createGain();
    gain.gain.value = 0.7;
    node.connect(gain).connect(ctx.destination);
    const at = Module._malloc(16);
    Module._pocket_web_audio(at);
    const v = Module.HEAPU32.slice(at >> 2, (at >> 2) + 4);
    Module._free(at);
    node.port.onmessage = (e) => { for (const k in e.data) sound.stats[k] = k === "peak" ? Math.max(sound.stats[k], e.data[k]) : sound.stats[k] + e.data[k]; };
    node.port.postMessage({ memory: Module.HEAPU8.buffer, buf: v[0], frames: v[1], w: v[2], r: v[3] });
    sound.gain = gain;
    soundRun(sound.on && !document.hidden);
    tell({ state: "sound", on: sound.on });
  }).catch(() => { sound.ctx = null; });
}
function setSound(on) {
  sound.on = !!on;
  try { localStorage.setItem(SOUND_KEY, sound.on ? "on" : "off"); } catch { /* not kept */ }
  if (sound.on && !sound.ctx) soundStart();
  else if (sound.ctx) { if (sound.on) sound.ctx.resume(); else sound.ctx.suspend(); soundRun(sound.on); }
  tell({ state: "sound", on: sound.on });
}
document.addEventListener("visibilitychange", () => {
  if (!sound.ctx) return;
  if (document.hidden) { sound.ctx.suspend(); soundRun(false); } else if (sound.on) { sound.ctx.resume(); soundRun(true); }
});

function press(b, down) { if (ready && b !== undefined) { if (down) soundStart(); Module._pocket_web_button(b, down ? 1 : 0); } }
const held = new Set();
// A page may say when the keys are the demo's (window.pocketDemoWantsKeys), so the arrows still scroll it elsewhere.
const wantsKeys = () => !window.pocketDemoWantsKeys || window.pocketDemoWantsKeys();
addEventListener("keydown", (e) => {
  if (e.ctrlKey || e.metaKey || e.altKey || !wantsKeys()) return;
  const k = keyOf(e), b = KEYS[k]; if (b === undefined) return;
  e.preventDefault(); if (e.repeat || held.has(k)) return;
  held.add(k); press(b, true);
});
addEventListener("keyup", (e) => {
  const k = keyOf(e), b = KEYS[k]; if (b === undefined) return;
  e.preventDefault(); held.delete(k); press(b, false);
});
addEventListener("blur", () => { for (const k of held) press(KEYS[k], false); held.clear(); });
document.addEventListener("visibilitychange", () => { if (!document.hidden && ready) Module._pocket_web_redraw(); });

function touch(phase, e) {
  if (!ready) return;
  if (phase === 0) soundStart();
  const r = canvas.getBoundingClientRect();
  Module._pocket_web_touch(phase, (e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
}
canvas.addEventListener("pointerdown", (e) => {
  e.preventDefault();
  if (playing) { press(B.home, true); setTimeout(() => press(B.home, false), 80); return; }
  canvas.setPointerCapture(e.pointerId); touch(0, e);
});
canvas.addEventListener("pointermove", (e) => { if (e.buttons || e.pointerType === "touch") touch(1, e); });
canvas.addEventListener("pointerup", (e) => touch(2, e));
canvas.addEventListener("pointercancel", (e) => touch(2, e));

const pad = document.getElementById("pad");
if (pad && !pad.querySelector("[data-b]")) {
  for (const [name, label] of PAD) {
    const el = document.createElement(name ? "button" : "span");
    if (name) { el.textContent = label; el.dataset.b = name; el.type = "button"; }
    pad.appendChild(el);
  }
}
for (const el of pad ? pad.querySelectorAll("[data-b]") : []) {
  const name = el.dataset.b;
  const down = (e) => { e.preventDefault(); el.setPointerCapture?.(e.pointerId); el.classList.add("down"); press(B[name], true); };
  const up = () => { if (el.classList.contains("down")) { el.classList.remove("down"); press(B[name], false); } };
  el.addEventListener("pointerdown", down);
  el.addEventListener("pointerup", up); el.addEventListener("pointercancel", up); el.addEventListener("lostpointercapture", up);
  el.addEventListener("contextmenu", (e) => e.preventDefault());
}
