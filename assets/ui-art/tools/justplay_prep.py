"""justplay_prep.py <photo_dir> <out_dir>: the three prepared photos behind the Just Play emblem (emblem-justplay.png).

<photo_dir> holds the 1280 px Commons thumbnails cart.jpg (Atari-5200-Cartridge.jpg), joy.jpg (Atari-2600-Joystick.jpg)
and dice.jpg (Colorful dices on wooden background (41903826620).jpg); see ../SOURCES.md. Each candidate is a white
background picture for PixelLab image-to-pixelart (tools/pixellab_i2p.py in assets/media/game/tools):
  a_cart.png      the cartridge, its Atari label and the logo on its underside painted over: a cream label with one
                  big play triangle
  b_cart_dice.png the same cartridge with a blue and a green die in front of it (cut from the dice photo by colour)
  c_joystick.png  the joystick with its one red button, nothing printed on it (the photo has no lettering)
"""
import colorsys, os, sys
from PIL import Image, ImageChops, ImageDraw, ImageFilter

src, out = sys.argv[1], sys.argv[2]
WHITE = (255, 255, 255)
CREAM = (240, 236, 222)
RED = (214, 64, 44)
DARK = (40, 36, 34)


def cart():
    """Left cartridge of the photo with the label repainted. Label corners measured in the 1280 px photo."""
    im = Image.open(os.path.join(src, "cart.jpg")).convert("RGB").crop((0, 0, 655, 604))
    d = ImageDraw.Draw(im)
    d.rectangle((628, 280, 655, 604), fill=WHITE)  # the edge of the photo's second cartridge
    # The logo on the underside: covered with the plain plastic beside it.
    im.paste(im.crop((330, 405, 400, 455)), (225, 400))
    im.paste(im.crop((330, 405, 400, 455)), (285, 400))
    tl, tr, br, bl = (222, 75), (588, 150), (493, 425), (112, 330)
    d.polygon([tl, tr, br, bl], fill=CREAM)

    def at(u, v):  # a point of the label, 0..1 each way, bilinear between the corners
        top = (tl[0] + (tr[0] - tl[0]) * u, tl[1] + (tr[1] - tl[1]) * u)
        bot = (bl[0] + (br[0] - bl[0]) * u, bl[1] + (br[1] - bl[1]) * u)
        return (top[0] + (bot[0] - top[0]) * v, top[1] + (bot[1] - top[1]) * v)

    d.polygon([at(0.30, 0.18), at(0.30, 0.82), at(0.74, 0.50)], fill=RED, outline=DARK, width=5)
    return im


def mask_white(im, thr=18):
    """Object = darker/coloured than the white backdrop; holes filled."""
    diff = ImageChops.difference(im, Image.new("RGB", im.size, WHITE)).convert("L")
    m = diff.point(lambda v: 255 if v > thr else 0).filter(ImageFilter.MaxFilter(5)).filter(ImageFilter.MinFilter(5))
    pad = Image.new("L", (m.width + 2, m.height + 2), 0)
    pad.paste(m, (1, 1))
    ImageDraw.floodfill(pad, (0, 0), 128)
    return pad.crop((1, 1, m.width + 1, m.height + 1)).point(lambda v: 0 if v == 128 else 255)


def die(photo, box, hue_lo, hue_hi):
    """A coloured die cut from the wooden table by hue (wood is orange-brown, the die blue or green)."""
    c = photo.crop(box)
    hsv = c.convert("HSV")
    h, s, v = hsv.split()
    m = Image.eval(h, lambda x: 255 if hue_lo <= x <= hue_hi else 0)
    m = ImageChops.multiply(m, s.point(lambda x: 255 if x > 70 else 0))
    m = m.filter(ImageFilter.MaxFilter(15)).filter(ImageFilter.MinFilter(15))
    pad = Image.new("L", (m.width + 2, m.height + 2), 0)
    pad.paste(m, (1, 1))
    ImageDraw.floodfill(pad, (0, 0), 128)
    m = pad.crop((1, 1, m.width + 1, m.height + 1)).point(lambda x: 0 if x == 128 else 255).filter(ImageFilter.MinFilter(3))
    return c, m


def on_white(canvas_size, parts):
    cv = Image.new("RGB", canvas_size, WHITE)
    for im, m, pos in parts:
        cv.paste(im, pos, m)
    return cv


def fit(im, long_side=1280, margin=40):
    """Crop to the object on white, add a margin, scale the long side to `long_side`."""
    box = mask_white(im).getbbox()
    im = im.crop(box)
    cv = Image.new("RGB", (im.width + 2 * margin, im.height + 2 * margin), WHITE)
    cv.paste(im, (margin, margin))
    s = long_side / max(cv.size)
    return cv.resize((round(cv.width * s), round(cv.height * s)), Image.LANCZOS)


os.makedirs(out, exist_ok=True)
c = cart()
cm = mask_white(c)
fit(on_white(c.size, [(c, cm, (0, 0))])).save(os.path.join(out, "a_cart.png"))

photo = Image.open(os.path.join(src, "dice.jpg")).convert("RGB")
blue, bm = die(photo, (395, 660, 640, 910), 150, 190)   # PIL hue 0..255: blue ~ 170
green, gm = die(photo, (675, 645, 910, 890), 60, 100)  # green ~ 85
big = Image.new("RGB", (c.width + 140, c.height + 130), WHITE)
big.paste(c, (0, 0), cm)
bs, gs = 0.95, 0.95
b2 = blue.resize((round(blue.width * bs), round(blue.height * bs)), Image.LANCZOS)
bm2 = bm.resize(b2.size, Image.LANCZOS)
g2 = green.resize((round(green.width * gs), round(green.height * gs)), Image.LANCZOS)
gm2 = gm.resize(g2.size, Image.LANCZOS)
big.paste(g2, (c.width - 120, big.height - g2.height - 5), gm2)
big.paste(b2, (c.width - 330, big.height - b2.height - 25), bm2)
fit(big).save(os.path.join(out, "b_cart_dice.png"))

j = Image.open(os.path.join(src, "joy.jpg")).convert("RGB")
fit(j).save(os.path.join(out, "c_joystick.png"))
