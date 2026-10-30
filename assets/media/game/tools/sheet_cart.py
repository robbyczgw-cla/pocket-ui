"""sheet.py <kind> <out.jpg> <ref.jpg> <variant.png>...: reference | each variant (row): plain + 3 covers on the label."""
import sys, glob
from PIL import Image, ImageDraw, ImageFont
kind, out, ref = sys.argv[1], sys.argv[2], sys.argv[3]; vs = sys.argv[4:]
covers = {"gba": ["Advance Wars (USA) (Rev 1)", "Pokemon - Emerald Version (USA, Europe)", "Golden Sun (USA, Europe)"],
          "n64": ["Super Mario 64 (USA)", "GoldenEye 007 (USA)", "Mario Kart 64 (USA)"]}[kind]
covers = [Image.open(f"covers/{c}.png").convert("RGB") for c in covers]
TH = 230
def label_art(c, lw, lh):
    w, h = c.size
    if kind == "gba": c = c.crop((int(w * 0.18), 0, w, h))
    else: c = c.crop((0, 0, int(w * 0.82), h))
    w, h = c.size
    if w * lh > h * lw:  # too wide: centre crop
        sw = h * lw // lh; c = c.crop(((w - sw) // 2, 0, (w - sw) // 2 + sw, h))
    else:
        sh = w * lh // lw; oy = 0 if kind == "gba" else (h - sh) // 2; c = c.crop((0, oy, w, oy + sh))
    return c.resize((lw, lh), Image.LANCZOS)
def render(v, cover):
    im = Image.open(v).convert("RGBA"); k = TH / im.height
    big = im.resize((round(im.width * k), TH), Image.NEAREST)
    if cover is None: return big
    px = big.load(); W, H = big.size
    lab = [(x, y) for x in range(W) for y in range(H) if px[x, y][:3] == (214, 214, 214) and px[x, y][3]]
    xs, ys = [p[0] for p in lab], [p[1] for p in lab]
    x0, y0 = min(xs), min(ys); art = label_art(cover, max(xs) - x0 + 1, max(ys) - y0 + 1).load()
    for x, y in lab: px[x, y] = art[x - x0, y - y0] + (255,)
    return big
rows = []
r = Image.open(ref).convert("RGB"); r.thumbnail((10000, TH)); rows.append(("reference photo", [r]))
for v in vs:
    rows.append((v, [render(v, None)] + [render(v, c) for c in covers]))
Wd = max(sum(t.width + 16 for t in ts) for _, ts in rows) + 220
S = Image.new("RGB", (Wd, len(rows) * (TH + 24) + 10), (28, 28, 32)); d = ImageDraw.Draw(S)
y = 10
for name, ts in rows:
    d.text((10, y + TH // 2), name.split("/")[-1], fill=(220, 220, 220))
    x = 200
    for t in ts:
        S.paste(t, (x, y), t if t.mode == "RGBA" else None); x += t.width + 16
    y += TH + 24
S.thumbnail((1800, 4000)); S.save(out, quality=82); print(out, S.size)
