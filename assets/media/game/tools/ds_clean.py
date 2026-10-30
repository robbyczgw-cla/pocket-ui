"""ds_clean.py <fin.png> <out.png> [--label G]

Clean-up after finish_nintendo.py --no-dehalo for the DS card: the photo's glare on the dark shell comes through the
conversion as single light specks. Every opaque pixel that is not the label (exactly G, default 214) and is lighter
than the shell (luminance > 112) is replaced: on the outline (a transparent 4-neighbour) by one rim grey, so the edge
and the cut corner keep a clean light rim; inside by the median of the darker shell pixels around it."""
import sys
from PIL import Image

a = sys.argv[1:]
G = int(a[a.index("--label") + 1]) if "--label" in a else 214
im = Image.open(a[0]).convert("RGBA")
W, H = im.size
src = im.copy().load()
px = im.load()
lum = lambda c: 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
RIM = (128, 128, 132, 255)
n = 0
for y in range(H):
    for x in range(W):
        c = src[x, y]
        if not c[3] or c[:3] == (G, G, G) or lum(c) <= 112:
            continue
        edge = any(not (0 <= x + dx < W and 0 <= y + dy < H) or not src[x + dx, y + dy][3]
                   for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
        if edge:
            px[x, y] = RIM
        else:
            nb = sorted((src[x + dx, y + dy] for dx in range(-3, 4) for dy in range(-3, 4)
                         if 0 <= x + dx < W and 0 <= y + dy < H and src[x + dx, y + dy][3]
                         and src[x + dx, y + dy][:3] != (G, G, G) and lum(src[x + dx, y + dy]) <= 112), key=lum)
            if nb:
                px[x, y] = nb[len(nb) // 2]
        n += 1
im.save(a[1])
print(a[1], im.size, n, "pixels cleaned")
