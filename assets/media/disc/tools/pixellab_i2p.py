"""pixellab_i2p.py <in.png> <out.png> <out_w> <out_h> [--strength N] [--fixer] [--seed N]: one PixelLab v2
image-to-pixelart conversion of a prepared photo (the way assets/systems/*.png were made, tools/prep.py first). Appends
the request (without the picture and never the key) and its usage to pixellab-log.jsonl next to <out.png>, and the credits
to ~/pocket-review/PIXELLAB-BUDGET.tsv's line `discs`. The key is read from ~/.config/pixellab/api-key."""
import base64, json, os, sys, time, urllib.error, urllib.request
from PIL import Image

args = sys.argv[1:]
src, out, ow, oh = args[0], args[1], int(args[2]), int(args[3])
opt = lambda name, default=None: args[args.index(name) + 1] if name in args else default
key = open(os.path.expanduser("~/.config/pixellab/api-key")).read().strip()
w, h = Image.open(src).size
body = {"image": {"type": "base64", "base64": base64.b64encode(open(src, "rb").read()).decode()},
        "image_size": {"width": w, "height": h}, "output_size": {"width": ow, "height": oh}, "text_guidance_scale": 8.0}
if opt("--strength"):
    body["init_image_strength"] = int(opt("--strength"))
if "--fixer" in args:
    body["fixer"] = True
if opt("--seed"):
    body["seed"] = int(opt("--seed"))
for attempt in range(8):
    try:
        req = urllib.request.Request("https://api.pixellab.ai/v2/image-to-pixelart", data=json.dumps(body).encode(),
                                     headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"})
        r = json.load(urllib.request.urlopen(req, timeout=300))
        break
    except urllib.error.HTTPError as e:
        if e.code in (429, 502, 503) and attempt < 7:
            time.sleep(30)
            continue
        sys.exit(f"HTTP {e.code}: {e.read()[:300]!r}")
r = r.get("last_response", r)
open(out, "wb").write(base64.b64decode(r["image"]["base64"].split(",", 1)[-1]))
body["image"] = os.path.basename(src)
usage = r.get("usage")
with open(os.path.join(os.path.dirname(os.path.abspath(out)), "pixellab-log.jsonl"), "a") as f:
    f.write(json.dumps({"out": os.path.basename(out), "time": time.strftime("%Y-%m-%dT%H:%M:%S"), "endpoint": "image-to-pixelart",
                        "request": body, "usage": usage}) + "\n")
# The budget: credits this call cost (usage's credits, else 1).
cost = 1
if isinstance(usage, dict):
    cost = usage.get("credits") or usage.get("generations") or 1
budget = os.path.expanduser("~/pocket-review/PIXELLAB-BUDGET.tsv")
import fcntl
with open(budget, "r+") as f:
    fcntl.flock(f, fcntl.LOCK_EX)
    lines = f.read().splitlines()
    lines = [("\t".join(p[:2] + [str(int(p[2]) + int(cost))]) if (p := l.split("\t"))[0] == "discs" else l) for l in lines]
    f.seek(0); f.truncate(); f.write("\n".join(lines) + "\n")
print(out, usage)
