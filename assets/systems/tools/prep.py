"""prep.py <photo.jpg> <out.png>: crop a white-background product photo to the object (+margin),
scale so the longer side is 1280, and write the matching object mask next to it (<out>_mask.png:
white = object, holes filled, so a white console on white stays solid)."""
import sys
from PIL import Image, ImageChops, ImageDraw, ImageFilter

src, out = sys.argv[1], sys.argv[2]
im = Image.open(src).convert("RGB")
# Object = noticeably darker than the white backdrop in any channel.
diff = ImageChops.difference(im, Image.new("RGB", im.size, (255, 255, 255))).convert("L")
# The crop box uses the threshold the conversions were made with (18); the mask a finer one (8).
mask = diff.point(lambda v: 255 if v > 18 else 0).filter(ImageFilter.MaxFilter(5)).filter(ImageFilter.MinFilter(5))
fine = diff.point(lambda v: 255 if v > 8 else 0).filter(ImageFilter.MaxFilter(5)).filter(ImageFilter.MinFilter(5))
# Fill holes: flood the background from the border, everything not reached is object.
pad = Image.new("L", (mask.width + 2, mask.height + 2), 0)
pad.paste(mask, (1, 1))
ImageDraw.floodfill(pad, (0, 0), 128)
filled = pad.crop((1, 1, mask.width + 1, mask.height + 1)).point(lambda v: 0 if v == 128 else 255)
bbox = filled.getbbox()
m = int(0.03 * max(bbox[2] - bbox[0], bbox[3] - bbox[1]))
box = (max(0, bbox[0] - m), max(0, bbox[1] - m), min(im.width, bbox[2] + m), min(im.height, bbox[3] + m))
if "--nocrop" in sys.argv:
    box = (0, 0, im.width, im.height)
im, filled = im.crop(box), filled.crop(box)
s = 1280 / max(im.size)
size = (round(im.width * s), round(im.height * s))
if "--maskonly" not in sys.argv:
    im.resize(size, Image.LANCZOS).save(out)
fine.crop(box).resize(size, Image.BOX).save(out.replace(".png", "_mask.png"))
print(out, size)
