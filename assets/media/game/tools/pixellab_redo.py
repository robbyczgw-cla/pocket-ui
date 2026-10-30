"""pixellab_redo.py <mode> <in.png> <out.png> [opts]: one PixelLab v2 call for the cartridge redo. Budget line `carts-redo` in
~/pocket-review/PIXELLAB-BUDGET.tsv, log to pixellab-log.jsonl next to out. Key from ~/.config/pixellab/api-key, never printed.
modes: i2p (image-to-pixelart: --size WxH --strength N --fixer --seed N), flash (image-to-pixelart-pro-flash: --desc),
       pixflux (create-image-pixflux with init_image: --size WxH --strength N --desc --seed N)"""
import base64, fcntl, json, os, sys, time, urllib.error, urllib.request
from PIL import Image
a = sys.argv[1:]; mode, src, out = a[0], a[1], a[2]
opt = lambda n, d=None: a[a.index(n) + 1] if n in a else d
key = open(os.path.expanduser("~/.config/pixellab/api-key")).read().strip()
H = {"Authorization": "Bearer " + key, "Content-Type": "application/json"}
img = {"type": "base64", "base64": base64.b64encode(open(src, "rb").read()).decode()}
w, h = Image.open(src).size
if mode == "i2p":
    ow, oh = map(int, opt("--size").split("x"))
    body = {"image": img, "image_size": {"width": w, "height": h}, "output_size": {"width": ow, "height": oh}, "text_guidance_scale": 8.0}
    if opt("--strength"): body["init_image_strength"] = int(opt("--strength"))
    if "--fixer" in a: body["fixer"] = True
    ep = "image-to-pixelart"
elif mode == "flash":
    body = {"image": img}
    if opt("--desc"): body["description"] = opt("--desc")
    ep = "image-to-pixelart-pro-flash"
else:
    ow, oh = map(int, opt("--size").split("x"))
    small = Image.open(src).convert("RGB").resize((ow, oh), Image.LANCZOS); small.save("/tmp/pl-init.png")
    body = {"description": opt("--desc"), "image_size": {"width": ow, "height": oh}, "no_background": True, "view": "side",
            "outline": "single color outline", "shading": "medium shading", "detail": "highly detailed",
            "text_guidance_scale": float(opt("--guidance", "8")),
            "init_image": {"type": "base64", "base64": base64.b64encode(open("/tmp/pl-init.png", "rb").read()).decode()},
            "init_image_strength": int(opt("--strength", "400"))}
    ep = "create-image-pixflux"
if opt("--seed"): body["seed"] = int(opt("--seed"))
def call(url, data=None, method="POST"):
    for attempt in range(8):
        try:
            req = urllib.request.Request(url, data=json.dumps(data).encode() if data else None, headers=H, method=method)
            return json.load(urllib.request.urlopen(req, timeout=300))
        except urllib.error.HTTPError as e:
            if e.code in (429, 502, 503) and attempt < 7: time.sleep(30); continue
            sys.exit(f"HTTP {e.code}: {e.read()[:300]!r}")
r = call("https://api.pixellab.ai/v2/" + ep, body)
jid = r.get("background_job_id") or (r.get("data") or {}).get("background_job_id")
while jid:
    time.sleep(5)
    j = call(f"https://api.pixellab.ai/v2/background-jobs/{jid}", method="GET")
    st = j.get("status")
    if st in ("completed", "succeeded", "done"): r = j; break
    if st in ("failed", "error", "cancelled"): sys.exit(f"job {st}: {str(j)[:300]}")
lr = r.get("last_response", r)
im = lr.get("image") or (lr.get("images") or [None])[0]
b64 = im["base64"] if isinstance(im, dict) else im
open(out, "wb").write(base64.b64decode(b64.split(",", 1)[-1]))
for k in ("image", "init_image"):
    if k in body: body[k] = os.path.basename(src)
usage = r.get("usage") or lr.get("usage")
with open(os.path.join(os.path.dirname(os.path.abspath(out)), "pixellab-log.jsonl"), "a") as f:
    f.write(json.dumps({"out": os.path.basename(out), "time": time.strftime("%Y-%m-%dT%H:%M:%S"), "endpoint": ep, "request": body, "usage": usage}) + "\n")
cost = 6 if mode == "flash" else 1
if isinstance(usage, dict): cost = usage.get("credits") or usage.get("generations") or cost
with open(os.path.expanduser("~/pocket-review/PIXELLAB-BUDGET.tsv"), "r+") as f:
    fcntl.flock(f, fcntl.LOCK_EX); lines = f.read().splitlines()
    lines = [("\t".join(p[:2] + [str(int(p[2]) + int(cost))]) if (p := l.split("\t"))[0] == "carts-redo" else l) for l in lines]
    f.seek(0); f.truncate(); f.write("\n".join(lines) + "\n")
print(out, Image.open(out).size, usage)
