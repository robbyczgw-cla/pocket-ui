"""Draw two generic, brand-free 'product photos' on white for PixelLab to convert:
cart.png  - a plain grey game cartridge with a blank colour-striped label (pico-8: a fantasy console's cartridge)
folder.png - a manila folder with three cartridges sticking out (ports: a folder of games)
Both 1280 px on the long side, soft shading, no text."""
import os, sys
from PIL import Image, ImageDraw, ImageFilter

out = sys.argv[1]
S = 2  # supersample


def grad(size, top, bottom):
    w, h = size
    g = Image.new("RGB", (1, h))
    for y in range(h):
        t = y / max(h - 1, 1)
        g.putpixel((0, y), tuple(int(top[i] * (1 - t) + bottom[i] * t) for i in range(3)))
    return g.resize((w, h))


def rrect_mask(size, box, r):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle(box, r, fill=255)
    return m


def cartridge(canvas, x, y, w, h, body=(150, 152, 160), label_cols=((230, 70, 60), (250, 190, 60), (70, 160, 220))):
    """Front view of a cartridge: body with a notched top and grip ridges, inset label."""
    W, H = canvas.size
    shadow = rrect_mask((W, H), (x + 10 * S, y + 14 * S, x + w + 10 * S, y + h + 14 * S), 40 * S).filter(ImageFilter.GaussianBlur(14 * S))
    canvas.paste((215, 215, 218), (0, 0), shadow.point(lambda v: v // 3))
    m = rrect_mask((W, H), (x, y, x + w, y + h), 36 * S)
    # Notch in the top right corner like a real cartridge.
    ImageDraw.Draw(m).polygon([(x + w - 70 * S, y), (x + w + 2, y), (x + w + 2, y + 70 * S)], fill=0)
    lite = tuple(min(255, c + 40) for c in body)
    dark = tuple(max(0, c - 55) for c in body)
    canvas.paste(grad((W, H), lite, dark), (0, 0), m)
    edge = m.filter(ImageFilter.FIND_EDGES).filter(ImageFilter.MaxFilter(5))
    canvas.paste((40, 40, 46), (0, 0), edge)
    d = ImageDraw.Draw(canvas)
    # Grip ridges near the top.
    for i in range(5):
        yy = y + 40 * S + i * 18 * S
        d.rounded_rectangle((x + 70 * S, yy, x + w - 110 * S, yy + 8 * S), 4 * S, fill=dark)
    # Label: cream, with three plain colour bands and a blank picture area.
    lx0, ly0, lx1, ly1 = x + 60 * S, y + 150 * S, x + w - 60 * S, y + h - 60 * S
    d.rounded_rectangle((lx0 - 10 * S, ly0 - 10 * S, lx1 + 10 * S, ly1 + 10 * S), 20 * S, fill=dark)
    d.rounded_rectangle((lx0, ly0, lx1, ly1), 14 * S, fill=(245, 238, 220))
    pic = (lx0 + 24 * S, ly0 + 24 * S, lx1 - 24 * S, ly1 - 90 * S)
    canvas.paste(grad((pic[2] - pic[0], pic[3] - pic[1]), (110, 180, 230), (60, 110, 190)), (pic[0], pic[1]))
    hills = [(pic[0], pic[3]), (pic[0] + (pic[2] - pic[0]) // 3, pic[1] + (pic[3] - pic[1]) // 2), (pic[0] + (pic[2] - pic[0]) * 2 // 3, pic[3] - 30 * S),
             (pic[2] - 40 * S, pic[1] + (pic[3] - pic[1]) // 3), (pic[2], pic[3])]
    d.polygon(hills, fill=(70, 150, 90))
    d.ellipse((pic[2] - 90 * S, pic[1] + 20 * S, pic[2] - 40 * S, pic[1] + 70 * S), fill=(255, 220, 90))
    by = ly1 - 70 * S
    bw = (lx1 - lx0 - 48 * S) // 3
    for i, c in enumerate(label_cols):
        d.rectangle((lx0 + 24 * S + i * bw, by, lx0 + 24 * S + (i + 1) * bw - 6 * S, by + 34 * S), fill=c)


def make_cart():
    W, H = 1100 * S, 1280 * S
    im = Image.new("RGB", (W, H), (255, 255, 255))
    cartridge(im, 190 * S, 110 * S, 720 * S, 1060 * S)
    return im.resize((W // S, H // S), Image.LANCZOS)


def make_folder():
    W, H = 1280 * S, 1000 * S
    im = Image.new("RGB", (W, H), (255, 255, 255))
    d = ImageDraw.Draw(im)
    back = (214, 170, 96)
    front_top, front_bot = (240, 200, 125), (205, 160, 90)
    # Back of the folder with its tab.
    d.rounded_rectangle((120 * S, 260 * S, 1160 * S, 900 * S), 30 * S, fill=back, outline=(90, 65, 30), width=6 * S)
    d.rounded_rectangle((140 * S, 190 * S, 520 * S, 300 * S), 26 * S, fill=back, outline=(90, 65, 30), width=6 * S)
    d.rectangle((146 * S, 262 * S, 514 * S, 300 * S), fill=back)
    # Three cartridges standing in it, slightly fanned.
    for i, (cx, col, lab) in enumerate([(230, (120, 124, 134), ((220, 80, 70), (240, 200, 70), (80, 170, 220))),
                                        (500, (200, 70, 70), ((250, 210, 80), (80, 170, 220), (90, 190, 110))),
                                        (770, (80, 110, 170), ((240, 120, 60), (110, 200, 120), (250, 210, 80)))]):
        layer = Image.new("RGB", (W, H), (255, 255, 255))
        cartridge(layer, cx * S, 120 * S + i * 20 * S, 300 * S, 520 * S, body=col, label_cols=lab)
        mask = Image.new("L", (W, H), 0)
        ImageDraw.Draw(mask).rounded_rectangle((cx * S - 2, 120 * S + i * 20 * S - 2, cx * S + 300 * S + 2, 120 * S + i * 20 * S + 520 * S + 2), 36 * S, fill=255)
        im.paste(layer, (0, 0), mask)
    # Front flap of the folder covering the lower part of the cartridges.
    fm = Image.new("L", (W, H), 0)
    ImageDraw.Draw(fm).polygon([(90 * S, 470 * S), (1190 * S, 470 * S), (1150 * S, 930 * S), (130 * S, 930 * S)], fill=255)
    im.paste(grad((W, H), front_top, front_bot), (0, 0), fm)
    edge = fm.filter(ImageFilter.FIND_EDGES).filter(ImageFilter.MaxFilter(5))
    im.paste((90, 65, 30), (0, 0), edge)
    d = ImageDraw.Draw(im)
    d.line((110 * S, 490 * S, 1170 * S, 490 * S), fill=(250, 220, 160), width=8 * S)
    return im.resize((W // S, H // S), Image.LANCZOS)


os.makedirs(out, exist_ok=True)
make_cart().save(os.path.join(out, "pico-8.png"))
make_folder().save(os.path.join(out, "ports.png"))
print("ok")
