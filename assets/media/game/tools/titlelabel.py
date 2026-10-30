"""titlelabel.py <out.png> <r,g,b> <title>: a plain stand-in label (no real game's art) for the compare images."""
import sys
from PIL import Image, ImageDraw, ImageFont
out, rgb, title = sys.argv[1], tuple(int(v) for v in sys.argv[2].split(",")), sys.argv[3]
im = Image.new("RGB", (320, 320), rgb)
d = ImageDraw.Draw(im)
for y in range(320):
    k = y / 320
    d.line([(0, y), (320, y)], fill=tuple(int(c * (1 - 0.45 * k)) for c in rgb))
d.ellipse((90, 120, 230, 260), fill=(250, 210, 80))
try:
    f = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", 34)
except OSError:
    f = ImageFont.load_default()
d.text((160, 70), title, font=f, fill=(255, 255, 255), anchor="mm")
im.save(out)
