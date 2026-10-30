"""mask.py <rect.png> <out.png> [--below Y] [--above Y] [--right X] [--left X] [--drop x0,y0,x1,y1] ... [--paint R,G,B]

Object outline for a face that rectify.py straightened with --keep (its outline is not a plain rectangle: the N64's
arched top, the legs beside the connector). Object = pixels that differ from the white backdrop (any channel by more
than 18), minus everything below Y / above Y / right of X / left of X, minus in each --drop box the pixels that are
not the grey plastic (saturated like the gold contacts, or dark like the shadow in the connector opening). Holes are
filled, specks dropped. With --paint the --drop
pixels stay object but take that plastic colour (a straight bottom edge where the photo showed the contacts). Writes <out> (white outside the object) and <out>_mask.png."""
import sys
from PIL import Image, ImageDraw, ImageFilter

a = sys.argv[1:]
im = Image.open(a[0]).convert("RGB")
W, H = im.size
lim = {"--below": H, "--above": 0, "--right": W, "--left": 0}
drops = []
paint = tuple(int(t) for t in a[a.index("--paint") + 1].split(",")) if "--paint" in a else None
for i, v in enumerate(a):
    if v in lim:
        lim[v] = int(a[i + 1])
    if v == "--drop":
        drops.append(tuple(int(t) for t in a[i + 1].split(",")))
px = im.load()
m = Image.new("L", im.size, 0)
mp = m.load()
for y in range(lim["--above"], lim["--below"]):
    for x in range(lim["--left"], lim["--right"]):
        c = px[x, y]
        if 255 - min(c) <= 18 and not any(b[0] <= x < b[2] and b[1] <= y < b[3] for b in drops if paint):
            continue
        if any(b[0] <= x < b[2] and b[1] <= y < b[3] for b in drops) and (max(c) - min(c) > 25 or sum(c) / 3 < 115):
            if not paint:
                continue
            px[x, y] = paint
        mp[x, y] = 255
m = m.filter(ImageFilter.MinFilter(5)).filter(ImageFilter.MaxFilter(5))
pad = Image.new("L", (W + 2, H + 2), 0)
pad.paste(m, (1, 1))
ImageDraw.floodfill(pad, (0, 0), 128)
m = pad.crop((1, 1, W + 1, H + 1)).point(lambda v: 0 if v == 128 else 255)
Image.composite(im, Image.new("RGB", im.size, (255, 255, 255)), m).save(a[1])
m.save(a[1].replace(".png", "_mask.png"))
print(a[1], m.getbbox())
