"""cutout.py <pixelart.png> <photo.png> <photo_mask.png> <out.png> [--fill x0,y0,x1,y1 ...]

Transparent background for a PixelLab conversion of a white-background photo.

1. Object mask from the photo, scaled to the pixel-art size. Enclosed holes in it are filled unless
   the photo there is the pure white backdrop (a loop of controller cable), so white plastic counts as object.
2. That mask, eroded by a few pixels (the conversion does not line up exactly with the photo), protects
   the object: nothing inside it is ever cleared, so a white console on the light background stays whole.
3. Flood fill from the image border, and from enclosed backdrop regions, over background-coloured pixels.
4. Strip remaining light halo pixels on the edge, drop specks. Crop to the object."""
import sys
from collections import deque
from PIL import Image, ImageFilter

# Optional: --fill x0,y0,x1,y1 (repeatable, in photo-mask pixels) marks a box as object. For white plastic on the
# white backdrop that the mask misses (the wii: the top of the Wiimote, seen against the white console).
args, fills = [], []
it = iter(sys.argv[1:])
for a in it:
    if a == "--fill":
        fills.append(tuple(int(v) for v in next(it).split(",")))
    else:
        args.append(a)
src, photop, maskp, out = args[:4]
im = Image.open(src).convert("RGBA")
W, H = im.size
px = im.load()
mask_im = Image.open(maskp).convert("L")
for box in fills:
    mask_im.paste(255, box)
cov = mask_im.resize((W, H), Image.BOX).load()
photo = Image.open(photop).convert("RGB").resize((W, H), Image.BOX).load()
N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))


def dist(a, b):
    return max(abs(a[0] - b[0]), abs(a[1] - b[1]), abs(a[2] - b[2]))


def components(pred):
    seen, comps = set(), []
    for x in range(W):
        for y in range(H):
            if (x, y) in seen or not pred(x, y):
                continue
            comp, q = [], deque([(x, y)])
            seen.add((x, y))
            while q:
                a, b = q.popleft()
                comp.append((a, b))
                for dx, dy in N4:
                    n = (a + dx, b + dy)
                    if 0 <= n[0] < W and 0 <= n[1] < H and n not in seen and pred(*n):
                        seen.add(n)
                        q.append(n)
            comps.append(comp)
    return comps


ring = [(x, y) for x in range(W) for y in (0, 1, 2, H - 3, H - 2, H - 1)] + [(x, y) for y in range(H) for x in (0, 1, 2, W - 3, W - 2, W - 1)]
border = [px[p][:3] for p in ring if px[p][3] == 255]
bg = tuple(sorted(c[i] for c in border)[len(border) // 2] for i in range(3))

obj = Image.new("L", (W, H), 0)
o = obj.load()
for x in range(W):
    for y in range(H):
        o[x, y] = 255 if cov[x, y] >= 128 else 0
backdrop_holes = []
for comp in components(lambda x, y: o[x, y] == 0):
    if any(x in (0, W - 1) or y in (0, H - 1) for x, y in comp):
        continue
    white = sum(min(photo[p]) for p in comp) / len(comp)
    if white >= 250 and len(comp) >= 12:
        backdrop_holes.append(comp)
    else:
        for p in comp:
            o[p] = 255
protect = obj.filter(ImageFilter.MinFilter(7)).load()

clear = [[False] * H for _ in range(W)]


def clearable(x, y):
    if px[x, y][3] == 0:
        return True
    return not protect[x, y] and dist(px[x, y], bg) < 40


q = deque(ring)
for comp in backdrop_holes:
    q.extend(comp)
while q:
    x, y = q.popleft()
    if clear[x][y] or not clearable(x, y):
        continue
    clear[x][y] = True
    for dx, dy in N4:
        nx, ny = x + dx, y + dy
        if 0 <= nx < W and 0 <= ny < H and not clear[nx][ny]:
            q.append((nx, ny))

# Halo: light, background-ish pixels on the edge outside the protected core.
for _ in range(2):
    kill = []
    for x in range(W):
        for y in range(H):
            if clear[x][y] or protect[x, y]:
                continue
            edge = any(not (0 <= x + dx < W and 0 <= y + dy < H) or clear[x + dx][y + dy] for dx, dy in N4)
            if edge and dist(px[x, y], bg) < 70 and min(px[x, y][:3]) > 150:
                kill.append((x, y))
    for x, y in kill:
        clear[x][y] = True

for comp in components(lambda x, y: not clear[x][y]):
    if len(comp) < 8:
        for a, b in comp:
            clear[a][b] = True

for x in range(W):
    for y in range(H):
        r, g, b, _ = px[x, y]
        px[x, y] = (0, 0, 0, 0) if clear[x][y] else (r, g, b, 255)
im = im.crop(im.getbbox())
im.save(out)
print("bg", bg, "holes", len(backdrop_holes), out, im.size)
