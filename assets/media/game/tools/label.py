"""label.py <in.png> <out.png> <x0,y0,x1,y1> [--grey G] [--tol T] [--close K] [--rect | --round R]

Paints the cartridge's label flat light grey before the conversion (the cover is composited there at run time).
Inside the box, every pixel that is not the plastic counts as label: the plastic colour is the most common colour of
the box's border ring, a pixel further than T from it (any channel, or saturated) is label. The mask is closed (K px)
and its holes filled, so the label keeps its own outline (rounded corners, notches). --rect paints the whole box
instead, --round R the box with corners rounded by R px. Writes <out> and <out>_label.png (the mask) and prints the mask's bounding box."""
import sys
from collections import Counter
from PIL import Image, ImageDraw, ImageFilter

a = sys.argv[1:]
opt = lambda n, d: a[a.index(n) + 1] if n in a else d
im = Image.open(a[0]).convert("RGB")
x0, y0, x1, y1 = (int(v) for v in a[2].split(","))
G, T, K = int(opt("--grey", 214)), int(opt("--tol", 40)), int(opt("--close", 9))
px = im.load()
mask = Image.new("L", im.size, 0)
if "--rect" in a or "--round" in a:
    ImageDraw.Draw(mask).rounded_rectangle([x0, y0, x1 - 1, y1 - 1], radius=int(opt("--round", 0)), fill=255)
else:
    ring = [px[x, y] for x in range(x0, x1) for y in (y0, y1 - 1)] + [px[x, y] for y in range(y0, y1) for x in (x0, x1 - 1)]
    q = lambda c: tuple(v // 8 * 8 for v in c)
    base = Counter(q(c) for c in ring).most_common(1)[0][0]
    m = mask.load()
    for y in range(y0, y1):
        for x in range(x0, x1):
            c = px[x, y]
            if max(abs(c[i] - base[i] - 4) for i in range(3)) > T or max(c) - min(c) > 30:
                m[x, y] = 255
    mask = mask.filter(ImageFilter.MaxFilter(K)).filter(ImageFilter.MinFilter(K))
    pad = Image.new("L", (im.width + 2, im.height + 2), 0)
    pad.paste(mask, (1, 1))
    ImageDraw.floodfill(pad, (0, 0), 128)
    mask = pad.crop((1, 1, im.width + 1, im.height + 1)).point(lambda v: 0 if v == 128 else 255)
im.paste((G, G, G), mask=mask)
im.save(a[1])
mask.save(a[1].replace(".png", "_label.png"))
print("label bbox", mask.getbbox())
