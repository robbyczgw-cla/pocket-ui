"""case_finish.py <specs.json> <kind> <front|spine> <raw.png> <out.png>

After the PixelLab conversion: the raw picture (already the final size; resized nearest if not) gets the case's outline
(a rectangle with `radius` px rounded corners, everything outside fully transparent, everything inside fully opaque),
the label and band rectangles become exactly one flat grey (200, 200, 200), and `fixed` rectangles (a colour the real
case always has, e.g. the Xbox's green band) are painted flat again."""
import json, sys
from PIL import Image, ImageDraw

spec_all, kind, part, raw, out = json.load(open(sys.argv[1])), sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
sp = spec_all[kind][part]
W, H = sp["size"]
im = Image.open(raw).convert("RGB")
if im.size != (W, H):
    im = im.resize((W, H), Image.NEAREST)
mask = Image.new("L", (W, H), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, H - 1], radius=sp.get("radius", 0), fill=255)
for rc in sp.get("cut", []):  # notches etc.
    ImageDraw.Draw(mask).rectangle([rc[0], rc[1], rc[0] + rc[2] - 1, rc[1] + rc[3] - 1], fill=0)
im.putalpha(mask)
d = ImageDraw.Draw(im)
R = lambda rc: [rc[0], rc[1], rc[0] + rc[2] - 1, rc[1] + rc[3] - 1]
for op in sp.get("fixed", []):
    d.rectangle(R(op["rect"]), fill=tuple(op["colour"]) + (255,))
for key in ("label", "band"):
    if sp.get(key):
        d.rectangle(R(sp[key]), fill=(200, 200, 200, 255))
im.save(out)
print(out, im.size)
