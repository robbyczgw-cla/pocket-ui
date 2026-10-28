"""finalize.py <in_dir> <out_dir> id...: exact (lossless) palette PNGs with binary transparency."""
import os, sys
from PIL import Image

src, dst = sys.argv[1], sys.argv[2]
os.makedirs(dst, exist_ok=True)
total = 0
for sid in sys.argv[3:]:
    im = Image.open(os.path.join(src, sid + ".png")).convert("RGBA")
    px = [(0, 0, 0, 0) if a == 0 else (r, g, b, 255) for r, g, b, a in im.get_flattened_data()]
    cols = sorted(set(px), key=lambda c: c[3])  # transparent first
    assert len(cols) <= 256, (sid, len(cols))
    index = {c: i for i, c in enumerate(cols)}
    p = Image.new("P", im.size)
    p.putdata([index[c] for c in px])
    pal = []
    for c in cols:
        pal += c[:3]
    p.putpalette(pal)
    p.info["transparency"] = bytes(c[3] for c in cols)
    out = os.path.join(dst, sid + ".png")
    p.save(out, optimize=True, transparency=bytes(c[3] for c in cols))
    back = [(0, 0, 0, 0) if a == 0 else (r, g, b, a) for r, g, b, a in Image.open(out).convert("RGBA").get_flattened_data()]
    assert back == px, sid
    total += os.path.getsize(out)
    print(sid, im.size, len(cols), os.path.getsize(out))
print("total", total)
