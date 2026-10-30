"""compare.py <out.jpg> <photo> <png> <json> <cover> [<photo> <png> <json> <cover> ...]
One row per kind: photo | PNG scaled up nearest | PNG with the cover in its label rect. <=1280 wide, JPEG q75."""
import json, sys
from PIL import Image, ImageDraw
out, rest = sys.argv[1], sys.argv[2:]
RH = 300
rows = []
for i in range(0, len(rest), 4):
    photo, png, js, cover = rest[i:i + 4]
    p = Image.open(photo).convert("RGB"); p.thumbnail((400, RH))
    g = Image.open(png).convert("RGBA")
    s = max(1, min(400 // g.width, RH // g.height))
    up = g.resize((g.width * s, g.height * s), Image.NEAREST)
    lab = json.load(open(js))["label"]
    comp = g.copy()
    if lab and cover != "-":
        x, y, w, h = lab
        c = Image.open(cover).convert("RGBA")
        k = max(w / c.width, h / c.height)
        c = c.resize((max(w, round(c.width * k)), max(h, round(c.height * k))), Image.LANCZOS)
        l, t = (c.width - w) // 2, (c.height - h) // 2
        c = c.crop((l, t, l + w, t + h))
        # only where the template is label-grey: the label's own outline (rounded corners) stays
        gp = g.load()
        mask = Image.new("L", (w, h), 0)
        mp = mask.load()
        for i in range(w):
            for j in range(h):
                if gp[x + i, y + j] == (200, 200, 200, 255):
                    mp[i, j] = 255
        comp.paste(c, (x, y), mask)
    comp = comp.resize((g.width * s, g.height * s), Image.NEAREST)
    rows.append((p, up, comp, png.split("/")[-1]))
W = 1280
H = sum(max(r[0].height, r[1].height) + 24 for r in rows)
sh = Image.new("RGB", (W, H), (120, 128, 138))
d = ImageDraw.Draw(sh)
y = 0
for p, up, comp, name in rows:
    h = max(p.height, up.height)
    sh.paste(p, (4, y + 20))
    x = 412
    for im in (up, comp):
        bg = Image.new("RGBA", im.size, (120, 128, 138, 255)); bg.alpha_composite(im)
        if x + im.width > W:
            im2 = bg.copy(); im2.thumbnail((W - x - 4, h)); bg = im2
        sh.paste(bg.convert("RGB"), (x, y + 20)); x += bg.width + 12
    d.text((4, y + 4), name, fill=(255, 255, 255))
    y += h + 24
sh.save(out, quality=75, optimize=True)
print(out, sh.size)
