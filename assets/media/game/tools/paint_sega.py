"""paint_sega.py <in> <out> <ops.json> <key>: background to white (dark object on light backdrop, or white backdrop given),
then paint polygons/boxes with a flat colour or the median plastic colour sampled from a box.
ops: {"bg": "dark"|"none", "bg_thresh": L, "ops": [{"poly": [[x,y],...] | "box": [x0,y0,x1,y1], "color": [r,g,b] | "sample": [x0,y0,x1,y1]}]}
"""
import json, sys
from PIL import Image, ImageDraw, ImageFilter, ImageChops
src, out, opsf, key = sys.argv[1:5]
cfg = json.load(open(opsf))[key]
im = Image.open(src).convert("RGB")
if cfg.get("rotate"):
    im = im.rotate(cfg["rotate"], resample=Image.BICUBIC, expand=False, fillcolor=tuple(cfg.get("rot_fill", [255, 255, 255])))
if cfg.get("crop"):
    im = im.crop(tuple(cfg["crop"]))
def median(box):
    px = sorted(im.crop(tuple(box)).get_flattened_data(), key=lambda c: sum(c))
    return px[len(px) // 2]
def inpaint(mask):
    """Fill the masked pixels from their known neighbours, outside in (a smooth diffusion fill)."""
    W, H = im.size
    px, mk = im.load(), mask.load()
    todo = {(x, y) for x in range(W) for y in range(H) if mk[x, y]}
    while todo:
        ring = []
        for (x, y) in todo:
            nb = [px[x + dx, y + dy] for dx in (-1, 0, 1) for dy in (-1, 0, 1)
                  if (dx or dy) and 0 <= x + dx < W and 0 <= y + dy < H and (x + dx, y + dy) not in todo]
            if len(nb) >= 2:
                ring.append(((x, y), tuple(sum(c[i] for c in nb) // len(nb) for i in range(3))))
        if not ring:
            break
        for p, c in ring:
            px[p] = c
            todo.discard(p)
for op in cfg.get("ops", []):
    if op.get("hinterp"):
        # each row: linear blend between the pixels just left and right of the box (median of 5 px each side)
        x0, y0, x1, y1 = op["box"]
        px = im.load()
        for y in range(y0, y1 + 1):
            def side(xs):
                c = sorted((px[x, y] for x in xs), key=sum)
                return c[len(c) // 2]
            a, b = side(range(x0 - 6, x0 - 1)), side(range(x1 + 2, x1 + 7))
            for x in range(x0, x1 + 1):
                t = (x - x0) / (x1 - x0)
                px[x, y] = tuple(round(a[i] * (1 - t) + b[i] * t) for i in range(3))
        continue
    if op.get("inpaint"):
        mask = Image.new("L", im.size, 0)
        dm = ImageDraw.Draw(mask)
        if "poly" in op:
            dm.polygon([tuple(p) for p in op["poly"]], fill=255)
        else:
            dm.rectangle(tuple(op["box"]), fill=255)
        inpaint(mask)
        continue
    col = tuple(op["color"]) if "color" in op else median(op["sample"])
    d = ImageDraw.Draw(im)
    if "rrect" in op:
        # rounded rectangle: [x0,y0,x1,y1], "r": radius, "corners": [tl,tr,br,bl] (booleans)
        d.rounded_rectangle(tuple(op["rrect"]), radius=op["r"], fill=col, corners=tuple(op.get("corners", [True] * 4)))
    elif "poly" in op:
        d.polygon([tuple(p) for p in op["poly"]], fill=col)
    else:
        d.rectangle(tuple(op["box"]), fill=col)
if cfg.get("bg") == "light":
    # a light object on a black backdrop: the backdrop becomes white, the object's white plastic a light grey so the
    # object mask (difference from white) still holds it
    L = im.convert("L")
    m = L.point(lambda v: 255 if v > cfg.get("bg_thresh", 120) else 0)
    pad = Image.new("L", (m.width + 2, m.height + 2), 0); pad.paste(m, (1, 1))
    ImageDraw.floodfill(pad, (0, 0), 128)
    m = pad.crop((1, 1, m.width + 1, m.height + 1)).point(lambda v: 0 if v == 128 else 255)
    px = im.load()
    pc = tuple(cfg.get("plastic", [232, 232, 228]))
    for x in range(im.width):
        for y in range(im.height):
            if min(px[x, y]) > 200:
                px[x, y] = pc
    im = Image.composite(im, Image.new("RGB", im.size, (255, 255, 255)), m)
    if cfg.get("outline"):
        # a thin grey edge round the white object, so it stands apart from the white backdrop in the conversion
        n = cfg["outline"]
        edge = ImageChops.subtract(m, m.filter(ImageFilter.MinFilter(2 * n + 1)))
        im.paste(tuple(cfg.get("outline_color", [120, 120, 118])), mask=edge)
if cfg.get("bg") == "dark":
    # object = darker than threshold; fill holes; largest blob
    L = im.convert("L").filter(ImageFilter.MedianFilter(5))
    m = L.point(lambda v: 255 if v < cfg.get("bg_thresh", 130) else 0).filter(ImageFilter.MinFilter(3)).filter(ImageFilter.MaxFilter(3))
    pad = Image.new("L", (m.width + 2, m.height + 2), 0); pad.paste(m, (1, 1))
    ImageDraw.floodfill(pad, (0, 0), 128)
    m = pad.crop((1, 1, m.width + 1, m.height + 1)).point(lambda v: 0 if v == 128 else 255)
    white = Image.new("RGB", im.size, (255, 255, 255))
    im = Image.composite(im, white, m)
for op in cfg.get("ops_after", []):
    d = ImageDraw.Draw(im)
    col = tuple(op["color"])
    if "rrect" in op:
        d.rounded_rectangle(tuple(op["rrect"]), radius=op["r"], fill=col, corners=tuple(op.get("corners", [True] * 4)))
    elif "poly" in op:
        d.polygon([tuple(p) for p in op["poly"]], fill=col)
    else:
        d.rectangle(tuple(op["box"]), fill=col)
if cfg.get("grey"):
    # neutral plastic: drop a colour cast of the photo (the backdrop light), keep the brightness
    im = im.convert("L").convert("RGB")
if cfg.get("lift"):
    # black plastic made a mid grey (so it can be tinted at run time): v -> a + b * v below the label grey,
    # the label (200) and the white backdrop unchanged
    a_, b_ = cfg["lift"]
    im = im.point(lambda v: v if v >= 195 else min(194, round(a_ + b_ * v)))
im.save(out)
print(out, im.size)
