// Drives the demo in headless Chromium (docs/WEB-DEMO.md): loads it as a phone or a computer, runs steps, writes
// JPEG screenshots of the canvas (or the page) and measures what a visitor waits for and how smooth it is.
//
//   PLAYWRIGHT=<dir of playwright-core> CHROME=<chromium> node web/demo/test/drive.mjs <url> <out dir> <phone|desktop> <step…>
// (without PLAYWRIGHT the playwright-core package is imported by name; without CHROME, Playwright's own Chromium).
//
// Steps: key:<key>[*<n>], btn:<pad button>[*<n>], tap:<x>,<y> (fractions of the canvas), wait:<ms>, until:<status regex>,
// shot:<name>, page:<name> (the whole page), fps:<ms> (frames the page drew in that time, printed), hold:<code>:<ms>
// (a key held while frames are counted), walkpause:<rows> (every pause-menu row, then Quit), card:yes|no (the page's card is up or not), status.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const pw = await import(process.env.PLAYWRIGHT ? join(process.env.PLAYWRIGHT, "index.mjs") : "playwright-core");
const [url, out, profile, ...steps] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const browser = await pw.chromium.launch({ executablePath: process.env.CHROME || undefined,
  args: ["--no-sandbox", "--enable-unsafe-swiftshader"] });
// phone: a mid Android phone; iphone: an iPhone's size and Safari's name (Chromium underneath); both on a slow line.
const phone = profile === "phone" || profile === "iphone";
const context = await browser.newContext(profile === "iphone"
  ? { viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true,
      userAgent: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Mobile/15E148 Safari/604.1" }
  : phone
  ? { viewport: { width: 393, height: 852 }, deviceScaleFactor: 2.75, isMobile: true, hasTouch: true,
      userAgent: "Mozilla/5.0 (Linux; Android 14; Pixel 7a) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Mobile Safari/537.36" }
  : { viewport: { width: 1280, height: 900 } });
const page = await context.newPage();
const cdp = await context.newCDPSession(page);
// A mid phone: about a quarter of this machine's single-core speed, and a 4G-ish line.
const slowdown = Number(process.env.CPU_SLOWDOWN || 4);
let throttled = 0;
if (phone) {
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: slowdown });
  // The App paints on its own thread (a Web Worker): throttled too, each one as it starts, or the "phone" is a desktop.
  cdp.on("Target.attachedToTarget", async (e) => {
    if (e.targetInfo.type !== "worker") return;
    const send = (id, method, params) => cdp.send("Target.sendMessageToTarget", { sessionId: e.sessionId, message: JSON.stringify({ id, method, params }) }).catch(() => {});
    await send(1, "Emulation.setCPUThrottlingRate", { rate: slowdown });
    await send(2, "Runtime.runIfWaitingForDebugger", {});
    throttled++;
  });
  await cdp.send("Target.setAutoAttach", { autoAttach: true, waitForDebuggerOnStart: true, flatten: false });
  await cdp.send("Network.enable");
  await cdp.send("Network.emulateNetworkConditions", { offline: false, latency: 70, downloadThroughput: 12e6 / 8, uploadThroughput: 3e6 / 8 });
}
let bytes = 0;
await cdp.send("Network.enable");
cdp.on("Network.loadingFinished", (e) => { bytes += e.encodedDataLength; });
page.on("console", (m) => { const t = m.text(); if (!/^(library|first frame|pad|launch|web demo|menu|pause|system controls)/.test(t) && !process.env.VERBOSE) return; console.log("  [page]", t.slice(0, 200)); });
page.on("pageerror", (e) => console.log("  [error]", e.message));
// Every request the page and its workers make: anything not from the page's own server is listed at the end.
const requests = [];
context.on("request", (r) => requests.push(r.url()));
let failed = false;
const t0 = Date.now();
await page.goto(url, { waitUntil: "commit" });
// LOADING_SHOT=<name>: the page while it loads (the loading shelf), before the first frame.
if (process.env.LOADING_SHOT) {
  await page.waitForSelector("#load", { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(Number(process.env.LOADING_AFTER || 1500));
  await page.screenshot({ path: join(out, `${process.env.LOADING_SHOT}.jpg`), type: "jpeg", quality: 75, scale: "css" });
  console.log(`  shot ${join(out, process.env.LOADING_SHOT + ".jpg")}`);
}
await page.waitForFunction(() => window.pocketDemo && window.pocketDemo.frames() > 0, null, { timeout: 120000 });
console.log(`first frame after ${Date.now() - t0} ms, ${(bytes / 1e6).toFixed(2)} MB transferred (gzip)${phone ? `, CPU ${slowdown}x slower on the page and ${throttled} workers` : ""}`);
const status = () => page.evaluate(() => window.pocketDemo.status());
const canvas = page.locator("#screen");
for (const step of steps) {
  const [kind, arg = ""] = step.split(/:(.*)/s);
  if (kind === "key") {
    const [code, n = "1"] = arg.split("*");
    for (let i = 0; i < Number(n); i++) { await page.keyboard.down(code); await page.waitForTimeout(60); await page.keyboard.up(code); await page.waitForTimeout(220); }
  } else if (kind === "tap") {
    const [x, y] = arg.split(",").map(Number); const b = await canvas.boundingBox();
    if (phone) await page.touchscreen.tap(b.x + x * b.width, b.y + y * b.height); else await page.mouse.click(b.x + x * b.width, b.y + y * b.height);
    await page.waitForTimeout(300);
  } else if (kind === "btn") {
    // btn:<name>[*<n>]: the on-screen pad's button, by finger (data-b="<name>").
    const [name, n = "1"] = arg.split("*"); const b = await page.locator(`#pad [data-b="${name}"]`).boundingBox();
    for (let i = 0; i < Number(n); i++) {
      await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: b.x + b.width / 2, y: b.y + b.height / 2 }] });
      await page.waitForTimeout(70);
      await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
      await page.waitForTimeout(250);
    }
  } else if (kind === "wait") await page.waitForTimeout(Number(arg));
  else if (kind === "until") {
    const re = new RegExp(arg); const end = Date.now() + 15000; let s = "";
    while (Date.now() < end && !re.test(s = await status())) await page.waitForTimeout(100);
    console.log(`  until ${arg}: ${re.test(s) ? "yes" : "NO"} (${s.replace(/\t/g, " ").slice(0, 120)})`);
    if (!re.test(s)) failed = true;
  } else if (kind === "shot" || kind === "page") {
    const file = join(out, `${arg}.jpg`);
    await (kind === "shot" ? canvas : page).screenshot({ path: file, type: "jpeg", quality: 75, scale: "css" });
    console.log(`  shot ${file}`);
  } else if (kind === "fps") {
    const ms = Number(arg); await page.evaluate(() => window.pocketDemo.paintStats()); const a = await page.evaluate(() => window.pocketDemo.frames());
    const long = await page.evaluate((ms) => new Promise((done) => { let worst = 0, last = performance.now(), n = 0;
      const f = (t) => { worst = Math.max(worst, t - last); last = t; n++; if (t - start < ms) requestAnimationFrame(f); else done({ worst, n }); };
      const start = performance.now(); requestAnimationFrame(f); }), ms);
    const b = await page.evaluate(() => window.pocketDemo.frames());
    const p = await page.evaluate(() => window.pocketDemo.paintStats());
    console.log(`  fps: ${((b - a) * 1000 / ms).toFixed(1)} frames/s on the canvas (App painted ${p.frames}, mean ${p.meanMs.toFixed(1)} ms, max ${p.maxMs.toFixed(1)} ms), page ${(long.n * 1000 / ms).toFixed(1)} rAF/s, worst gap ${long.worst.toFixed(0)} ms`);
  } else if (kind === "hold") {
    // hold:<code>:<ms>: the key held down (the App repeats it) while the frames are counted.
    const [code, ms] = arg.split(":"); await page.evaluate(() => window.pocketDemo.paintStats());
    const a = await page.evaluate(() => window.pocketDemo.frames());
    await page.keyboard.down(code); await page.waitForTimeout(Number(ms)); await page.keyboard.up(code);
    const b = await page.evaluate(() => window.pocketDemo.frames());
    const p = await page.evaluate(() => window.pocketDemo.paintStats());
    console.log(`  hold ${code} ${ms} ms: ${((b - a) * 1000 / Number(ms)).toFixed(1)} frames/s on the canvas, App painted ${p.frames} (mean ${p.meanMs.toFixed(1)} ms, max ${p.maxMs.toFixed(1)} ms)`);
  } else if (kind === "card") {
    // card:yes|no: whether the page shows its "would start here" card (no: pocket-ui's own frame is on the canvas).
    await page.waitForTimeout(250);
    const shown = await page.evaluate(() => window.pocketDemo.cardShown());
    const ok = shown === (arg === "yes");
    console.log(`  card ${arg}: ${ok ? "yes" : "NO"} (card ${shown ? "shown" : "not shown"})`);
    if (!ok) failed = true;
  } else if (kind === "walkpause") {
    // walkpause:<max rows>: from the open pause menu, every row in turn: A on it; a row that closes the menu (Continue,
    // Save, Load) gets HOME to open it again, a row that opens a page gets B until the menu's main page is back. The
    // last row (Quit) must reach the shelf. Any step that doesn't come within its time fails the run.
    const press = async (k) => { await page.keyboard.down(k); await page.waitForTimeout(60); await page.keyboard.up(k); await page.waitForTimeout(160); };
    const wait = async (re, ms) => { const end = Date.now() + ms; let st = ""; while (Date.now() < end) { st = await status(); if (re.test(st)) return st; await page.waitForTimeout(80); } return null; };
    const where = (st) => (st.match(/place=([^\t]*)/) || [])[1] + " / game=" + (st.match(/game=([^\t]*)/) || [])[1];
    let done = false;
    for (let row = 0; row < Number(arg || 10) && !done; row++) {
      if (!(await wait(/place=pause:Main/, 4000))) { console.log(`  walkpause row ${row}: NO, the menu's main page isn't up (${where(await status())})`); failed = true; break; }
      for (let i = 0; i < row; i++) await press("ArrowDown");
      await canvas.screenshot({ path: join(out, `walk-${row}-on.jpg`), type: "jpeg", quality: 70, scale: "css" });
      await press("Enter");
      await page.waitForTimeout(900);
      await canvas.screenshot({ path: join(out, `walk-${row}-after.jpg`), type: "jpeg", quality: 70, scale: "css" });
      let st = await status();
      if (/game=quitting|game=none/.test(st)) {
        const back = await wait(/game=none/, 4000);
        console.log(`  walkpause row ${row}: quit ${back ? "back on the shelf (" + where(back) + ")" : "NO, still " + where(await status())}`);
        if (!back) failed = true;
        done = true;
      } else if (/game=playing/.test(st)) {
        await press("m");
        const again = await wait(/game=paused/, 3000);
        console.log(`  walkpause row ${row}: closed the menu, HOME ${again ? "opened it again" : "NO"}`);
        if (!again) failed = true;
      } else if (/place=pause:Main/.test(st)) {
        console.log(`  walkpause row ${row}: stayed on the menu (${where(st)})`);
        // The selection stays on this row (a row that does nothing now, as Earlier without moments): back to the top.
        for (let i = 0; i < row; i++) await press("ArrowUp");
      } else {
        let back = null;
        for (let b = 0; b < 4 && !back; b++) { await press("Backspace"); back = await wait(/place=pause:Main|game=playing/, 1200); }
        if (back && /game=playing/.test(back)) { await press("m"); back = await wait(/place=pause:Main/, 3000); }
        console.log(`  walkpause row ${row}: opened ${where(st)}, B ${back ? "back to the menu" : "NO"}`);
        if (!back) failed = true;
      }
      if (/game=quitting/.test(st) && !done) { console.log(`  walkpause row ${row}: NO, hangs quitting`); failed = true; break; }
    }
    if (!done) { console.log("  walkpause: NO, never reached Quit"); failed = true; }
  } else if (kind === "status") console.log("  status:", (await status()).replace(/\t/g, " "));
}
const own = new URL(url).origin;
const foreign = [...new Set(requests.filter((u) => !u.startsWith(own) && !u.startsWith("data:") && !u.startsWith("blob:")))];
console.log(`requests: ${requests.length}, all from ${own}${foreign.length ? "" : " (none elsewhere)"}`);
for (const u of foreign) { console.log(`  FOREIGN request: ${u}`); failed = true; }
await browser.close();
if (failed) { console.log("FAILED: a step (until: or card:) did not hold"); process.exit(1); }
