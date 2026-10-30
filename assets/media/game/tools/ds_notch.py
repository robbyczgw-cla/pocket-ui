"""ds_notch.py <lab.png> <out.png> [--cut C] [--keep-colour]

Second pass for the DS card (ds.png). The photo is of the Acekard 2.1, a third-party card in the DS game card's shell,
straightened to 780 x 740 with the face at x 40..740, y 40..700 and the label already painted flat (label.py). On an
official DS game card the top right corner of the label side is cut off at 45 degrees; that cut is what tells the card
apart from a plain square, so it is painted in before the conversion, large enough to survive at about 150 px:
* the corner triangle (C px along both edges, default 80, about 11 % of the width) becomes white backdrop;
* the cut edge gets a light bevel line and a dark line inside it, like the shell's other edges in the photo;
* the Acekard's dark blue plastic is made the dark grey of Nintendo's DS cards (luminance kept, colour dropped),
  unless --keep-colour; the label stays as it is."""
import sys
from PIL import Image, ImageDraw

a = sys.argv[1:]
C = int(a[a.index("--cut") + 1]) if "--cut" in a else 80
im = Image.open(a[0]).convert("RGB")
X0, Y0, X1, Y1 = 40, 40, 740, 700
px = im.load()
if "--keep-colour" not in a:
    for y in range(im.height):
        for x in range(im.width):
            r, g, b = px[x, y]
            if min(r, g, b) > 235 or (abs(r - g) < 6 and abs(g - b) < 6 and r > 150):
                continue  # backdrop or the flat grey label
            lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
            v = max(0, min(255, round(lum * 1.45 + 26)))
            px[x, y] = (v, v, v + 3)
d = ImageDraw.Draw(im)
# White corner triangle (with a little overshoot past the face so no dark pixels stay outside the cut).
d.polygon([(X1 - C, Y0 - 2), (X1 + 40, Y0 - 2), (X1 + 40, Y0 + C + 40), (X1 + 2, Y0 + C + 2)], fill=(255, 255, 255))
d.polygon([(X1 - C, Y0 - 40), (X1 + 40, Y0 - 40), (X1 + 40, Y0)], fill=(255, 255, 255))
# Bevel along the cut: a light rim on the edge, a dark groove just inside it.
d.line([(X1 - C, Y0), (X1, Y0 + C)], fill=(150, 150, 154), width=7)
d.line([(X1 - C - 9, Y0 + 2), (X1 - 2, Y0 + C + 9)], fill=(40, 40, 44), width=6)
im.save(a[1])
print(a[1], im.size, "cut", C)
