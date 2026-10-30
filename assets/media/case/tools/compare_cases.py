"""compare_cases.py <media_dir> <out.jpg> <kind> <photo> <cover|-> <title> [<kind> <photo> <cover|-> <title> ...]

One row per case kind: the photo | front + spine scaled up nearest | the front with a cover of that system in
front_label (or a plain stand-in label in the system's colours when no cover is at hand) and the spine with the title
in spine_label, the band in the cover's accent colour. <= 1280 px wide, JPEG q75."""
import json, sys
from PIL import Image, ImageDraw, ImageFont

media, out, rest = sys.argv[1], sys.argv[2], sys.argv[3:]
Z = int(__import__("os").environ.get("ZOOM", "2"))
BG = (120, 128, 138)
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"


def standin(w, h, title):
    im = Image.new("RGB", (w, h), (40, 90, 160))
    d = ImageDraw.Draw(im)
    d.rectangle((0, h * 0.62, w, h), fill=(150, 60, 110))
    d.ellipse((w * 0.25, h * 0.4, w * 0.75, h * 0.4 + w * 0.5), fill=(250, 210, 80))
    f = ImageFont.truetype(FONT, max(8, w // 9))
    d.text((w / 2, h * 0.18), title, font=f, fill=(255, 255, 255), anchor="mm")
    return im


def fill(g, rect, src):
    x, y, w, h = rect
    c = src.convert("RGBA")
    k = max(w / c.width, h / c.height)
    c = c.resize((max(w, round(c.width * k)), max(h, round(c.height * k))), Image.LANCZOS)
    l, t = (c.width - w) // 2, (c.height - h) // 2
    g.paste(c.crop((l, t, l + w, t + h)), (x, y))


rows = []
for i in range(0, len(rest), 4):
    kind, photo, cover, title = rest[i:i + 4]
    js = json.load(open(f"{media}/case/{kind}.json"))
    fr = Image.open(f"{media}/{kind}-front.png").convert("RGBA")
    sp = Image.open(f"{media}/{kind}-spine.png").convert("RGBA")
    p = Image.open(photo).convert("RGB")
    p.thumbnail((260, fr.height * Z))
    pair = lambda a, b: (lambda im: (im.paste(a, (0, 0)), im.paste(b, (a.width + 6, 0)), im)[2])(
        Image.new("RGBA", (a.width + b.width + 6, max(a.height, b.height)), BG + (255,)))
    up = pair(Image.alpha_composite(Image.new("RGBA", fr.size, BG + (255,)), fr),
              Image.alpha_composite(Image.new("RGBA", sp.size, BG + (255,)), sp))
    cf, cs = fr.copy(), sp.copy()
    cov = Image.open(cover) if cover != "-" else standin(js["front_label"][2] * 4, js["front_label"][3] * 4, title)
    fill(cf, js["front_label"], cov)
    accent = cov.convert("RGB").resize((1, 1), Image.LANCZOS).getpixel((0, 0))
    x, y, w, h = js["spine_label"]
    lab = Image.new("RGB", (h, w), (235, 235, 235))
    sz = max(7, int(w * 0.6))
    while sz > 7 and ImageFont.truetype(FONT, sz).getlength(title) > h - 12:
        sz -= 1
    f = ImageFont.truetype(FONT, sz)
    ImageDraw.Draw(lab).text((6, w / 2), title, font=f, fill=(20, 20, 20), anchor="lm")
    cs.paste(lab.rotate(-90, expand=True), (x, y))
    if js.get("front_band"):
        bx, by, bw, bh = js["front_band"]
        ImageDraw.Draw(cf).rectangle([bx, by, bx + bw - 1, by + bh - 1], fill=(48, 110, 190, 255))
        ImageDraw.Draw(cf).text((bx + bw / 2, by + bh / 2), "SYSTEM", font=ImageFont.truetype(FONT, max(8, bh // 2)), fill=(255, 255, 255), anchor="mm")
    if js.get("band"):
        bx, by, bw, bh = js["band"]
        ImageDraw.Draw(cs).rectangle([bx, by, bx + bw - 1, by + bh - 1], fill=((48, 110, 190) if js.get("front_band") else accent) + (255,))
    comp = pair(Image.alpha_composite(Image.new("RGBA", cf.size, BG + (255,)), cf),
                Image.alpha_composite(Image.new("RGBA", cs.size, BG + (255,)), cs))
    up = up.resize((up.width * Z, up.height * Z), Image.NEAREST)
    comp = comp.resize((comp.width * Z, comp.height * Z), Image.NEAREST)
    rows.append((p, up, comp, kind))
W = min(1280, max(4 + r[0].width + 12 + r[1].width + 12 + r[2].width + 4 for r in rows))
H = sum(max(r[0].height, r[1].height) + 24 for r in rows)
sh = Image.new("RGB", (W, H), BG)
d = ImageDraw.Draw(sh)
y = 0
for p, up, comp, name in rows:
    sh.paste(p, (4, y + 20))
    x = 4 + 260 + 12
    for im in (up, comp):
        sh.paste(im.convert("RGB"), (x, y + 20)); x += im.width + 12
    d.text((4, y + 4), name, fill=(255, 255, 255))
    y += max(p.height, up.height) + 24
sh.save(out, quality=int(__import__("os").environ.get("Q", "75")), optimize=True)
print(out, sh.size)
