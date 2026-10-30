"""finish_cart.py <raw.png> <out.png> <plastic_hex> [--mask prep_mask.png] [--thr T]: background off (prep mask scaled, or
flood fill from the border over the backdrop colour), label = light region grown from the centre, holes filled, painted
flat (214,214,214); plastic luma scaled so its median is the target's, neutral grey (+ the target's hue); on/off alpha.
Prints {"size":..,"label":[x,y,w,h]}."""
import json, sys
from collections import deque
from PIL import Image
a = sys.argv[1:]; opt = lambda n, d=None: a[a.index(n) + 1] if n in a else d
im = Image.open(a[0]).convert("RGBA"); W, H = im.size; px = im.load()
tgt = tuple(int(a[2][i:i + 2], 16) for i in (0, 2, 4))
N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))
L = lambda c: 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
obj = [[px[x, y][3] >= 128 for y in range(H)] for x in range(W)]
if opt("--mask"):
    m = Image.open(opt("--mask")).convert("L").resize((W, H), Image.BILINEAR).load()
    obj = [[m[x, y] >= 128 for y in range(H)] for x in range(W)]
elif all(px[x, y][3] == 255 for x in (0, W - 1) for y in (0, H - 1)):
    bgc = px[0, 0][:3]; seen = set(); q = deque([(x, y) for x in range(W) for y in (0, H - 1)] + [(x, y) for y in range(H) for x in (0, W - 1)])
    while q:
        x, y = q.popleft()
        if (x, y) in seen or not (0 <= x < W and 0 <= y < H): continue
        if max(abs(px[x, y][i] - bgc[i]) for i in range(3)) > int(opt("--bgtol", 40)): continue
        seen.add((x, y)); obj[x][y] = False; q.extend((x + dx, y + dy) for dx, dy in N4)
# label: grow from centre over light pixels
T = int(opt("--thr", 170)); sx, sy = (int(v) for v in opt("--seed", f"{W // 2},{int(H * 0.45)}").split(","))
lab = set(); q = deque([(sx, sy)])
while q:
    x, y = q.popleft()
    if (x, y) in lab or not (0 <= x < W and 0 <= y < H) or not obj[x][y] or min(px[x, y][:3]) < T: continue
    lab.add((x, y)); q.extend((x + dx, y + dy) for dx, dy in N4)
xs, ys = [p[0] for p in lab], [p[1] for p in lab]
x0, y0, x1, y1 = min(xs), min(ys), max(xs) + 1, max(ys) + 1
out = set(); q = deque([(x, y) for x in range(x0, x1) for y in (y0, y1 - 1)] + [(x, y) for y in range(y0, y1) for x in (x0, x1 - 1)])
while q:
    p = q.popleft()
    if p in out or p in lab or not (x0 <= p[0] < x1 and y0 <= p[1] < y1): continue
    out.add(p); q.extend((p[0] + dx, p[1] + dy) for dx, dy in N4)
labset = {(x, y) for x in range(x0, x1) for y in range(y0, y1) if (x, y) not in out}
plast = sorted(L(px[x, y]) for x in range(W) for y in range(H) if obj[x][y] and (x, y) not in labset)
med = plast[len(plast) // 2]; k = L(tgt) / med
for x in range(W):
    for y in range(H):
        if not obj[x][y]: px[x, y] = (0, 0, 0, 0)
        elif (x, y) in labset: px[x, y] = (214, 214, 214, 255)
        else:
            v = L(px[x, y]) * k / L(tgt)
            px[x, y] = tuple(max(0, min(254 if i < 3 else 255, round(tgt[i] * v)))  for i in range(3)) + (255,)
bb = im.getbbox(); im = im.crop(bb)
im.save(a[1])
print(json.dumps({"size": im.size, "label": [x0 - bb[0], y0 - bb[1], x1 - x0, y1 - y0], "plastic_median": round(med)}))
