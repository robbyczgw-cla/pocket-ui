"""compare.py <out.jpg> <photo.png>:<kind.png>:<kind.json>:<cover.png or "title text"> ...

One row per cartridge: the straightened photo | the pixel art scaled up nearest | the pixel art with a sample cover
fitted (crop to fill) into the label rectangle from <kind>.json. Rows are about 260 px high (the pixel art scaled by a whole number), on a mid grey, at most 1280 px
wide, JPEG quality 75 (the review pictures)."""
import json, os, sys
from PIL import Image, ImageDraw

RH, PAD = 260, 12
rows = []
for spec in sys.argv[2:]:
    photo, png, js, cover = spec.split(":", 3)
    art = Image.open(png).convert("RGBA")
    k = max(1, round(RH / art.height))
    ph = Image.open(photo).convert("RGB")
    ph = ph.resize((round(ph.width * art.height * k / ph.height), art.height * k), Image.LANCZOS)
    up = art.resize((art.width * k, art.height * k), Image.NEAREST)
    lx, ly, lw, lh = json.load(open(js))["label"]
    withc = art.copy()
    if os.path.exists(cover):
        c = Image.open(cover).convert("RGB")
        s = max(lw / c.width, lh / c.height)
        c = c.resize((max(lw, round(c.width * s)), max(lh, round(c.height * s))), Image.LANCZOS)
        c = c.crop(((c.width - lw) // 2, (c.height - lh) // 2, (c.width - lw) // 2 + lw, (c.height - lh) // 2 + lh))
    else:
        c = Image.new("RGB", (lw, lh), (236, 233, 225))
        ImageDraw.Draw(c).text((3, lh // 2 - 5), cover, fill=(30, 30, 30))
    withc.paste(c, (lx, ly))
    withc.putalpha(art.getchannel("A"))
    wc = withc.resize((art.width * k, art.height * k), Image.NEAREST)
    rows.append([ph.convert("RGBA"), up, wc])
W = max(sum(i.width for i in r) + PAD * (len(r) + 1) for r in rows)
H = sum(max(i.height for i in r) + PAD for r in rows) + PAD
sheet = Image.new("RGB", (W, H), (110, 112, 120))
y = PAD
for r in rows:
    x = PAD
    for i in r:
        sheet.paste(i, (x, y), i)
        x += i.width + PAD
    y += max(i.height for i in r) + PAD
if sheet.width > 1280:
    sheet = sheet.resize((1280, round(sheet.height * 1280 / sheet.width)), Image.LANCZOS)
sheet.save(sys.argv[1], quality=75)
print(sys.argv[1], sheet.size, os.path.getsize(sys.argv[1]))
