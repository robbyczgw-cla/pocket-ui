"""tiles.py <out.jpg> <zoom> <png>...: RGBA pngs side by side on grey, nearest zoom, labelled."""
import sys
from PIL import Image, ImageDraw
out, z, fs = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
ims = [Image.open(f).convert("RGBA") for f in fs]
W = sum(i.width * z + 8 for i in ims) + 8; H = max(i.height for i in ims) * z + 24
sh = Image.new("RGBA", (W, H), (120, 128, 138, 255)); d = ImageDraw.Draw(sh); x = 8
for f, i in zip(fs, ims):
    sh.alpha_composite(i.resize((i.width * z, i.height * z), Image.NEAREST), (x, 20)); d.text((x, 4), f.split("/")[-1][:18], fill=(255, 255, 255)); x += i.width * z + 8
sh.convert("RGB").save(out, quality=80)
