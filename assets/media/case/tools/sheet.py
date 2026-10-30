"""sheet.py <out.jpg> <img>...: contact sheet, 3 per row, 400px cells, labelled."""
import sys
from PIL import Image, ImageDraw
fs = sys.argv[2:]; C = 400; n = 3
sh = Image.new("RGB", (C * n, ((len(fs) + n - 1) // n) * (C + 14)), (90, 90, 90)); d = ImageDraw.Draw(sh)
for i, f in enumerate(fs):
    try: im = Image.open(f).convert("RGB")
    except Exception: continue
    im.thumbnail((C - 4, C - 4)); x, y = (i % n) * C, (i // n) * (C + 14)
    sh.paste(im, (x + 2, y + 14)); d.text((x + 2, y + 1), f.split("/")[-1][:60], fill=(255, 255, 0))
sh.save(sys.argv[1], quality=70)
