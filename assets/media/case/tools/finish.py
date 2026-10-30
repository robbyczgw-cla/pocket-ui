"""finish.py <specs.json> <kind> <front|spine> <raw.png> <out.png>: the PixelLab picture (resized nearest to `size` if
needed) gets the case outline (rounded rectangle `radius`, outside alpha 0, inside 255), the `fixed` rectangles flat in
their colours again and `label` / `band` exactly flat grey (200,200,200)."""
import json, sys
from PIL import Image, ImageDraw
spec, kind, part, raw, out = json.load(open(sys.argv[1])), sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
sp = spec[kind][part]; W, H = sp["size"]
im = Image.open(raw).convert("RGB")
if im.size != (W, H):
    im = im.resize((W, H), Image.NEAREST)
mask = Image.new("L", (W, H), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, H - 1], radius=sp.get("radius", 0), fill=255)
im.putalpha(mask)
d = ImageDraw.Draw(im)
R = lambda rc: [rc[0], rc[1], rc[0] + rc[2] - 1, rc[1] + rc[3] - 1]
for op in sp.get("fixed", []):
    d.rectangle(R(op["rect"]), fill=tuple(op["colour"]) + (255,))
for key in ("label", "band"):
    if sp.get(key):
        d.rectangle(R(sp[key]), fill=(200, 200, 200, 255))
im.save(out); print(out, im.size)
