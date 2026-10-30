"""maskcut.py <pixelart.png> <photo_mask.png> <out.png> [--cov C]

Background removal for a straightened cartridge: the object mask of the prepared photo (prep.py's <out>_mask.png, the
exact outline that rectify.py gave the face) is scaled to the pixel art's size; a pixel is kept where the mask covers
at least C (0..255, default 128) of it, everything else becomes transparent. The flood fill of the consoles' cutout.py
is not needed (the backdrop is pure white and the outline known) and would eat the light top rim of a cartridge."""
import sys
from PIL import Image, ImageDraw

a = sys.argv[1:]
C = int(a[a.index("--cov") + 1]) if "--cov" in a else 128
im = Image.open(a[0]).convert("RGBA")
mask = Image.open(a[1]).convert("L")
# Fill the mask's holes (a white screw head or highlight inside the face is still the face).
pad = Image.new("L", (mask.width + 2, mask.height + 2), 0)
pad.paste(mask.point(lambda v: 255 if v >= 128 else 0), (1, 1))
ImageDraw.floodfill(pad, (0, 0), 128)
mask = pad.crop((1, 1, mask.width + 1, mask.height + 1)).point(lambda v: 0 if v == 128 else 255)
m = mask.resize(im.size, Image.BOX).load()
px = im.load()
for x in range(im.width):
    for y in range(im.height):
        r, g, b, _ = px[x, y]
        px[x, y] = (r, g, b, 255) if m[x, y] >= C else (0, 0, 0, 0)
im = im.crop(im.getbbox())
im.save(a[2])
print(a[2], im.size)
