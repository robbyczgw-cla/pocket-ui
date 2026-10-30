"""paint_nintendo.py <in.png> <out.png> <x0,y0,x1,y1> [<box> ...] [--keep-dark T] [--color R,G,B] [--vertical]

Paints embossed or printed lettering on the plastic over before the conversion: every box is filled with the median
colour of the pixels just around it (or --color), with a little of the photo's grain kept so the plastic does not
turn into a flat patch. --vertical fills each column with a blend from the pixel above the box to the
one below it instead (keeps a shading gradient across the box). --keep-dark T leaves pixels darker than T (luminance) as they are, for a box that overlaps a
deep groove (the GBA's arched recess)."""
import random, sys
from PIL import Image

a = sys.argv[1:]
opt = lambda n, d: a[a.index(n) + 1] if n in a else d
im = Image.open(a[0]).convert("RGB")
px = im.load()
KD = int(opt("--keep-dark", -1))
boxes = [tuple(int(v) for v in s.split(",")) for s in a[2:] if s.count(",") == 3 and not s.startswith("-")]
random.seed(1)


def avg(x, ya, yb):
    # Mean of a small patch beside the box (5 rows, 9 columns): the plastic's grain does not turn into streaks.
    cs = [px[min(im.width - 1, max(0, x + dx)), min(im.height - 1, max(0, y))] for dx in range(-4, 5) for y in range(ya, yb)]
    return tuple(sum(c[i] for c in cs) / len(cs) for i in range(3))

for x0, y0, x1, y1 in boxes:
    if "--color" in a:
        col = tuple(int(v) for v in opt("--color", "").split(","))
    else:
        ring = [px[x, y] for x in range(x0 - 4, x1 + 4) for y in (y0 - 4, y1 + 3) if 0 <= x < im.width and 0 <= y < im.height]
        ring += [px[x, y] for y in range(y0, y1) for x in (x0 - 4, x1 + 3) if 0 <= x < im.width and 0 <= y < im.height]
        col = tuple(sorted(c[i] for c in ring)[len(ring) // 2] for i in range(3))
    for y in range(y0, y1):
        for x in range(x0, x1):
            c = px[x, y]
            if "--vertical" in a:
                f = (y - y0 + 1) / (y1 - y0 + 1)
                top, bot = avg(x, y0 - 7, y0 - 2), avg(x, y1 + 1, y1 + 6)
                col = tuple(round(top[i] * (1 - f) + bot[i] * f) for i in range(3))
            if KD >= 0 and 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] < KD:
                continue
            n = random.randint(-3, 3)
            px[x, y] = tuple(max(0, min(255, v + n)) for v in col)
    print("box", (x0, y0, x1, y1), "colour", col)
im.save(a[1])
