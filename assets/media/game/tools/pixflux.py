"""pixflux.py <out.png> <w> <h> <seed> "<description>" [--view side] [--outline ...]: one PixelLab v2 create-image-pixflux
call from a text prompt alone (no input image). Logs request + usage to pixellab-log.jsonl next to out, counts credits in
~/pocket-review/PIXELLAB-BUDGET.tsv line cases-photo. Key from ~/.config/pixellab/api-key, never printed."""
import base64, fcntl, json, os, sys, time, urllib.error, urllib.request
a = sys.argv[1:]
out, w, h, seed, desc = a[0], int(a[1]), int(a[2]), int(a[3]), a[4]
opt = lambda n, d: a[a.index(n) + 1] if n in a else d
key = open(os.path.expanduser("~/.config/pixellab/api-key")).read().strip()
body = dict(description=desc, image_size=dict(width=w, height=h), no_background=True,
            outline=opt("--outline", "single color black outline"), shading=opt("--shading", "medium shading"),
            detail=opt("--detail", "medium detail"), view=opt("--view", "side"), direction="south",
            text_guidance_scale=float(opt("--guidance", "8")), seed=seed)
for attempt in range(8):
    try:
        req = urllib.request.Request("https://api.pixellab.ai/v2/create-image-pixflux", json.dumps(body).encode(),
                                     {"Authorization": "Bearer " + key, "Content-Type": "application/json"})
        r = json.load(urllib.request.urlopen(req, timeout=300)); break
    except urllib.error.HTTPError as e:
        if e.code in (429, 502, 503) and attempt < 7:
            time.sleep(30); continue
        sys.exit(f"HTTP {e.code}: {e.read()[:300]!r}")
r = r.get("last_response", r)
open(out, "wb").write(base64.b64decode(r["image"]["base64"].split(",", 1)[-1]))
usage = r.get("usage")
with open(os.path.join(os.path.dirname(os.path.abspath(out)), "pixellab-log.jsonl"), "a") as f:
    f.write(json.dumps({"out": os.path.basename(out), "time": time.strftime("%Y-%m-%dT%H:%M:%S"), "endpoint": "create-image-pixflux",
                        "request": body, "usage": usage}) + "\n")
cost = 1
if isinstance(usage, dict):
    cost = usage.get("credits") or usage.get("generations") or 1
with open(os.path.expanduser("~/pocket-review/PIXELLAB-BUDGET.tsv"), "r+") as f:
    fcntl.flock(f, fcntl.LOCK_EX)
    lines = f.read().splitlines()
    lines = [("\t".join(p[:2] + [str(int(p[2]) + int(cost))]) if (p := l.split("\t"))[0] == "cases-photo" else l) for l in lines]
    f.seek(0); f.truncate(); f.write("\n".join(lines) + "\n")
print(out, usage)
