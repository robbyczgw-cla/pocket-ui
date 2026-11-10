// A static server for web/demo/dist with the headers the demo needs (docs/WEB-DEMO.md): cross-origin isolation, so
// the App's threads (SharedArrayBuffer) work, and gzip like a CDN.  node web/demo/test/serve.mjs <dir> [port]
import { createServer } from "node:http";
import { readFileSync, existsSync, statSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { join, extname, normalize } from "node:path";
const [dir, port = "8790"] = process.argv.slice(2);
const types = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".data": "application/octet-stream", ".png": "image/png", ".jpg": "image/jpeg", ".svg": "image/svg+xml", ".webp": "image/webp" };
const cache = new Map();
createServer((req, res) => {
  let path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^(\.\.[/\\])+/, "");
  let file = join(dir, path);
  if (existsSync(file) && statSync(file).isDirectory()) file = join(file, "index.html");
  if (!existsSync(file)) { res.writeHead(404); return res.end("not found"); }
  const headers = { "Content-Type": types[extname(file)] || "application/octet-stream",
    "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp", "Cache-Control": "no-store" };
  let body = readFileSync(file);
  if ((req.headers["accept-encoding"] || "").includes("gzip")) {
    const key = file + statSync(file).mtimeMs;
    if (!cache.has(key)) cache.set(key, gzipSync(body, { level: 9 }));
    body = cache.get(key); headers["Content-Encoding"] = "gzip";
  }
  res.writeHead(200, headers); res.end(body);
}).listen(Number(port), "127.0.0.1", () => console.log(`serving ${dir} on http://127.0.0.1:${port}`));
