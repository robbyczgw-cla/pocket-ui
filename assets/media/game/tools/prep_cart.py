"""prep_cart.py <spec.json> : straightened photo -> clean PixelLab input.
spec: src, out, outline (list of [x,y] polygon in src px or 'bands'), label (x0,y0,x1,y1,r), paint (boxes, vertical fill),
plastic target median luma + tint, out_size (w,h) real proportions."""
import json, sys, random
from PIL import Image, ImageDraw, ImageFilter, ImageStat
s = json.load(open(sys.argv[1]))
im = Image.open(s["src"]).convert("RGB")
W, H = im.size
mask = Image.new("L", (W, H), 0)
d = ImageDraw.Draw(mask)
for r in s["outline"]:  # rounded rects [x0,y0,x1,y1,radius,corners]
    d.rounded_rectangle(r[:4], radius=r[4], fill=255, corners=tuple(r[5]) if len(r) > 5 else None)
for poly in s.get("polys", []):
    d.polygon([tuple(p) for p in poly], fill=255)
if "clip" in s:
    c = Image.new("L", (W, H), 0); r = s["clip"]
    ImageDraw.Draw(c).rounded_rectangle(r[:4], radius=r[4], fill=255)
    from PIL import ImageChops
    mask = ImageChops.multiply(mask, c); d = ImageDraw.Draw(mask)
for poly in s.get("cut", []):
    d.polygon([tuple(p) for p in poly], fill=0)
px = im.load()
random.seed(1)
# paint out lettering: fill each column between the rows above and below (keeps shading), with grain
for x0, y0, x1, y1 in s.get("paint", []):
    for x in range(x0, x1):
        top = [px[x, y] for y in range(y0 - 6, y0 - 1)]
        bot = [px[x, y] for y in range(y1 + 1, y1 + 6)]
        t = tuple(sum(c[i] for c in top) / len(top) for i in range(3)); b = tuple(sum(c[i] for c in bot) / len(bot) for i in range(3))
        for y in range(y0, y1):
            f = (y - y0 + 1) / (y1 - y0 + 1); n = random.randint(-2, 2)
            px[x, y] = tuple(max(0, min(255, round(t[i] * (1 - f) + b[i] * f) + n)) for i in range(3))
if s.get("paint"):
    bm = Image.new("L", (W, H), 0)
    for x0, y0, x1, y1 in s["paint"]:
        ImageDraw.Draw(bm).rectangle([x0 - 4, y0 - 4, x1 + 4, y1 + 4], fill=255)
    bm = bm.filter(ImageFilter.GaussianBlur(4))
    im = Image.composite(im.filter(ImageFilter.GaussianBlur(s.get("paint_blur", 5))), im, bm); px = im.load()
# recolour plastic: grey, then scale so the plastic's median luma hits the target, add tint
g = im.convert("L")
lab = s.get("label")
lmask = Image.new("L", (W, H), 0)
if lab:
    ImageDraw.Draw(lmask).rounded_rectangle(lab[:4], radius=lab[4], fill=255, corners=tuple(lab[5]) if len(lab) > 5 else None)
plastic = Image.composite(g, Image.new("L", (W, H), 0), Image.eval(Image.composite(Image.new("L", (W, H), 0), mask, lmask), lambda v: v))
vals = sorted(v for v, m, l in zip(g.get_flattened_data(), mask.get_flattened_data(), lmask.get_flattened_data()) if m and not l)
med = vals[len(vals) // 2]
k = s["plastic_luma"] / med
tint = s.get("tint", [0, 0, 0])
gamma = s.get("gamma", 1.0)
def rec(v):
    return v
out = Image.new("RGB", (W, H), (255, 255, 255))
op = out.load(); gp = g.load(); mp = mask.load(); lp = lmask.load()
for y in range(H):
    for x in range(W):
        if not mp[x, y]:
            continue
        if lp[x, y]:
            op[x, y] = tuple(s.get("label_rgb", [200, 200, 200])); continue
        v = (gp[x, y] / 255) ** gamma * 255 * k / ((med / 255) ** gamma * 255 / med)
        op[x, y] = tuple(max(0, min(255, round(v + tint[i]))) for i in range(3))
box = mask.getbbox()
m = s.get("margin", 30)
out = out.crop((box[0] - m, box[1] - m, box[2] + m, box[3] + m)); mask = mask.crop((box[0] - m, box[1] - m, box[2] + m, box[3] + m))
if "out_size" in s:
    ow, oh = s["out_size"]
    out = out.resize((ow + 2 * m, oh + 2 * m), Image.LANCZOS); mask = mask.resize((ow + 2 * m, oh + 2 * m), Image.NEAREST)
out.save(s["out"]); mask.save(s["out"].replace(".png", "_mask.png"))
print(s["out"], out.size, "median plastic", med, "->", s["plastic_luma"])
