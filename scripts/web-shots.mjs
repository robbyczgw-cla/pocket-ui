// Screenshots of the web companion as a phone and a computer see it, dark and light, through the local Chrome's
// DevTools protocol (headless Chrome alone can't make a window narrower than 500 px). Needs Node 22 or later (its
// built-in WebSocket) and Google Chrome; start the development server first:
//
//   cargo run --release --example web_dev -- --port 8737 &
//   node scripts/web-shots.mjs http://127.0.0.1:8737 /tmp/web-shots library system/gba upload stats
//
// Writes <out>/<route>-<phone|desktop>-<dark|light>.png, the whole page each. CHROME names another Chrome or Chromium;
// CHROME_ARGS adds switches (on a Linux host without user namespaces: CHROME_ARGS=--no-sandbox).
import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";

const [base, out, ...routes] = process.argv.slice(2);
if (!base || !out) {
  console.error("usage: node scripts/web-shots.mjs <base url> <out dir> [route…]");
  process.exit(2);
}
mkdirSync(out, { recursive: true });
const chrome = process.env.CHROME || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const port = 9333 + Math.floor(Math.random() * 500);
const extra = (process.env.CHROME_ARGS || "").split(" ").filter(Boolean);
const proc = spawn(chrome, [...extra, "--headless=new", "--disable-gpu", "--hide-scrollbars", `--remote-debugging-port=${port}`, `--user-data-dir=/tmp/web-shots-chrome-${port}`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let ws;
for (let i = 0; i < 50 && !ws; i++) {
  await sleep(200);
  try {
    const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const page = list.find((t) => t.type === "page");
    if (page) ws = new WebSocket(page.webSocketDebuggerUrl);
  } catch (e) { /* not up yet */ }
}
await new Promise((r) => ws.addEventListener("open", r));
let seq = 0;
const waiting = new Map();
ws.addEventListener("message", (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && waiting.has(msg.id)) { waiting.get(msg.id)(msg); waiting.delete(msg.id); }
});
const send = (method, params = {}) => new Promise((r) => { const id = ++seq; waiting.set(id, r); ws.send(JSON.stringify({ id, method, params })); });

const sizes = { phone: { width: 390, height: 844, deviceScaleFactor: 2, mobile: true }, desktop: { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false } };
for (const route of routes.length ? routes : ["library"]) {
  for (const [kind, size] of Object.entries(sizes)) {
    for (const scheme of ["dark", "light"]) {
      await send("Emulation.setDeviceMetricsOverride", size);
      await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
      // A fresh load each time: the same address with another size would only change the hash, and the page would keep
      // what it drew for the size before.
      await send("Page.navigate", { url: "about:blank" });
      await sleep(200);
      await send("Page.navigate", { url: `${base}/#/${route}` });
      await sleep(1500);
      const { result } = await send("Runtime.evaluate", { expression: "Math.max(document.documentElement.scrollHeight, innerHeight)", returnByValue: true });
      const height = Math.min(6000, result.result.value);
      await send("Emulation.setDeviceMetricsOverride", { ...size, height });
      await sleep(300);
      const shot = await send("Page.captureScreenshot", { format: "png" });
      const file = `${out}/${route.replaceAll("/", "-") || "home"}-${kind}-${scheme}.png`;
      writeFileSync(file, Buffer.from(shot.result.data, "base64"));
      console.log(file);
    }
  }
}
ws.close();
proc.kill();
