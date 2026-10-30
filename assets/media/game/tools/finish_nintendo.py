"""finish_nintendo.py <cut.png> <out.png> --seed x,y [--thr T] [--grey G] [--inset N] [--no-dehalo]

Last step after cutout.py: the label becomes one flat colour and the picture gets on/off transparency.

The label is the 4-connected region of light pixels (every channel >= T) grown from the seed (a point inside the
label; with --near only pixels within D of the seed's colour, for light plastic), with its holes filled; it is painted G (a flat light grey, the cover goes there at run time). Its bounding box,
shrunk by N px (the rounded corners stay plastic), is printed as the label rectangle for <kind>.json. Edge pixels
that are light leftovers of the photo backdrop (every channel > L, default 200) are dropped (unless --no-dehalo; only from row Y down with
--halo-from, so a light top rim that belongs to the cartridge stays), and every pixel ends fully
opaque or fully transparent."""
import json, sys
from collections import deque
from PIL import Image

a = sys.argv[1:]
opt = lambda n, d: a[a.index(n) + 1] if n in a else d
im = Image.open(a[0]).convert("RGBA")
W, H = im.size
px = im.load()
T, G, N = int(opt("--thr", 170)), int(opt("--grey", 214)), int(opt("--inset", 0))
sx, sy = (int(v) for v in opt("--seed", "").split(","))
N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))
for x in range(W):
    for y in range(H):
        r, g, b, al = px[x, y]
        px[x, y] = (r, g, b, 255) if al >= 128 else (0, 0, 0, 0)
if "--no-dehalo" not in a:
    for _ in range(2):
        kill = [(x, y) for x in range(W) for y in range(int(opt("--halo-from", 0)), H) if px[x, y][3] and min(px[x, y][:3]) > int(opt("--halo", 200))
                and any(not (0 <= x + dx < W and 0 <= y + dy < H) or not px[x + dx, y + dy][3] for dx, dy in N4)]
        for p in kill:
            px[p] = (0, 0, 0, 0)
lab = set()
seed = px[sx, sy][:3]
NEAR = int(opt("--near", 999))
q = deque([(sx, sy)])
while q:
    x, y = q.popleft()
    if (x, y) in lab or not (0 <= x < W and 0 <= y < H) or px[x, y][3] == 0 or min(px[x, y][:3]) < T \
            or max(abs(px[x, y][i] - seed[i]) for i in range(3)) > NEAR:
        continue
    lab.add((x, y))
    q.extend((x + dx, y + dy) for dx, dy in N4)
xs, ys = [p[0] for p in lab], [p[1] for p in lab]
x0, y0, x1, y1 = min(xs), min(ys), max(xs) + 1, max(ys) + 1
# Holes: whatever inside the bbox is not reachable from the bbox border through non-label pixels.
out = set()
q = deque([(x, y) for x in range(x0, x1) for y in (y0, y1 - 1)] + [(x, y) for y in range(y0, y1) for x in (x0, x1 - 1)])
while q:
    p = q.popleft()
    if p in out or p in lab or not (x0 <= p[0] < x1 and y0 <= p[1] < y1):
        continue
    out.add(p)
    q.extend((p[0] + dx, p[1] + dy) for dx, dy in N4)
for x in range(x0, x1):
    for y in range(y0, y1):
        if (x, y) not in out:
            px[x, y] = (G, G, G, 255)
bb = im.getbbox()
im = im.crop(bb)
im.save(a[1])
rect = [x0 - bb[0] + N, y0 - bb[1] + N, x1 - x0 - 2 * N, y1 - y0 - 2 * N]
print(json.dumps({"size": im.size, "label": rect}))
