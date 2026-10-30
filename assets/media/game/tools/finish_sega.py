"""finish_sega.py <raw.png> <prep.png> <out.png> [--label-min T] [--label-near R,G,B:T] [--label-box x0,y0,x1,y1] [--no-label] [--blob] [--dehalo T]
Cut-out (assets/systems/tools/cutout.py, guarded by prep's object mask), then the label: the largest 4-connected blob of
light pixels (every channel >= T, default 150) or the given box (in output pixels, after the crop) becomes one flat
light grey (LABEL) over its bounding rectangle. Every pixel alpha 0 or 255, palette PNG. Prints the label rect as JSON."""
import json, os, subprocess, sys, tempfile
from collections import deque
from PIL import Image
LABEL = (200, 200, 200)
a = sys.argv[1:]
raw, prep, out = a[:3]
opt = lambda n, d=None: a[a.index(n) + 1] if n in a else d
here = os.path.dirname(os.path.abspath(__file__))
# cutout.py and rim.py live in assets/systems/tools (the console pictures' pipeline); a scratch copy may sit next to this file.
if not os.path.exists(os.path.join(here, "cutout.py")):
    here = os.path.normpath(os.path.join(here, "../../../systems/tools"))
cut = tempfile.mktemp(suffix=".png")
if opt("--simple-cut"):
    # flood from the border over near-white backdrop pixels only (for a light object with its own outline, where
    # cutout.py's halo step would bite into the light plastic)
    t = int(opt("--simple-cut"))
    r = Image.open(raw).convert("RGBA"); W0, H0 = r.size; rp = r.load()
    q = deque([(x, y) for x in range(W0) for y in (0, H0 - 1)] + [(x, y) for y in range(H0) for x in (0, W0 - 1)])
    seen = set()
    while q:
        p = q.popleft()
        if p in seen or not (0 <= p[0] < W0 and 0 <= p[1] < H0):
            continue
        c = rp[p]
        if c[3] and min(c[:3]) < 255 - t:
            continue
        seen.add(p)
        q.extend(((p[0] + 1, p[1]), (p[0] - 1, p[1]), (p[0], p[1] + 1), (p[0], p[1] - 1)))
    for p in seen:
        rp[p] = (0, 0, 0, 0)
    r.crop(r.getbbox()).save(cut)
else:
  subprocess.run([sys.executable, "-I", os.path.join(here, "cutout.py"), raw, prep, prep.replace(".png", "_mask.png"), cut], check=True,
               stdout=subprocess.DEVNULL)
im = Image.open(cut).convert("RGBA")
W, H = im.size
px = im.load()
for x in range(W):
    for y in range(H):
        r, g, b, al = px[x, y]
        px[x, y] = (r, g, b, 255) if al >= 128 else (0, 0, 0, 0)
if opt("--dehalo"):
    # light backdrop/shadow pixels left on the edge: flood from the transparency over pixels with every channel >= T
    t = int(opt("--dehalo"))
    q = deque((x, y) for x in range(W) for y in range(H) if px[x, y][3] == 0 or x in (0, W - 1) or y in (0, H - 1))
    seen = set()
    while q:
        p = q.popleft()
        if p in seen:
            continue
        seen.add(p)
        c = px[p]
        if c[3] and min(c[:3]) < t:
            continue
        px[p] = (0, 0, 0, 0)
        for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            n = (p[0] + dx, p[1] + dy)
            if 0 <= n[0] < W and 0 <= n[1] < H and n not in seen:
                q.append(n)
    bb = im.getbbox()
    im = im.crop(bb); W, H = im.size; px = im.load()
rect = None
if "--no-label" not in a:
    if opt("--label-box"):
        x0, y0, x1, y1 = (int(v) for v in opt("--label-box").split(","))
    else:
        t = int(opt("--label-min", 150))
        light = lambda x, y: px[x, y][3] and min(px[x, y][:3]) >= t
        if opt("--label-near"):
            # the label by its colour (when the plastic round it is light too): within T of R,G,B in every channel
            ref, tol = opt("--label-near").split(":")
            ref, tol = [int(v) for v in ref.split(",")], int(tol)
            light = lambda x, y: px[x, y][3] and max(abs(px[x, y][i] - ref[i]) for i in range(3)) <= tol
        seen, best = set(), []
        for x in range(W):
            for y in range(H):
                if (x, y) in seen or not light(x, y):
                    continue
                comp, q = [], deque([(x, y)]); seen.add((x, y))
                while q:
                    p = q.popleft(); comp.append(p)
                    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                        n = (p[0] + dx, p[1] + dy)
                        if 0 <= n[0] < W and 0 <= n[1] < H and n not in seen and light(*n):
                            seen.add(n); q.append(n)
                if len(comp) > len(best):
                    best = comp
        xs, ys = [p[0] for p in best], [p[1] for p in best]
        x0, y0, x1, y1 = min(xs), min(ys), max(xs) + 1, max(ys) + 1
    if "--blob" in a:
        # keep the label's own outline (rounded corners): only the blob and the holes inside it become LABEL
        inside = set(best)
        for y in range(y0, y1):
            row = [x for x in range(x0, x1) if (x, y) in inside]
            if row:
                for x in range(row[0], row[-1] + 1):
                    inside.add((x, y))
        for p in inside:
            px[p] = LABEL + (255,)
    else:
        for x in range(x0, x1):
            for y in range(y0, y1):
                px[x, y] = LABEL + (255,)
    rect = [x0, y0, x1 - x0, y1 - y0]
sys.path.insert(0, os.path.join(here))
from rim import save_palette
save_palette(im, out)
os.unlink(cut)
print(json.dumps({"size": [W, H], "label": rect}))
