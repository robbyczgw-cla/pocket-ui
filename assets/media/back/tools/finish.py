"""finish.py <starting dir> <pixellab dir> <out dir>: cuts PixelLab's pictures (px-<kind>.png) to the outline of their
starting pictures (<kind>.png from draw_backs.py): outside the case clear, the middle (the paper pocket-ui paints)
clear, only fully clear or fully solid pixels. Needs Pillow."""
import sys
from PIL import Image
sys.path.insert(0, __file__.rsplit("/", 1)[0])
src, px, out = sys.argv[1:4]
BAND = {"cartbox": 6, "cartbox-front": 6, "dvd": 6, "jewel": 7, "clamshell": 6, "umd": 7, "card": 6}
for kind, b in BAND.items():
    start = Image.open(f"{src}/{kind}.png").convert("RGBA")
    made = Image.open(f"{px}/px-{kind}.png").convert("RGBA").resize(start.size, Image.NEAREST)
    w, h = start.size
    s, m = start.load(), made.load()
    res = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    r = res.load()
    for y in range(h):
        for x in range(w):
            inside = b <= x < w - b and b <= y < h - b
            if s[x, y][3] < 128 or inside:
                continue
            c = m[x, y]
            r[x, y] = (c[0], c[1], c[2], 255)
    res.save(f"{out}/{kind}.png", optimize=True)
    print(kind, w, h)
