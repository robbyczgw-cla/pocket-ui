"""Draws the pixel-art templates of the game media (assets/media/<kind>-spine.png, <kind>-front.png) and prints the
table `src/media.rs` keeps (label and band rectangles, the spine's caps for 9-slicing).

Usage: python3 -I draw_media.py <out dir>   (needs Pillow)

One template pixel is two screen pixels on the Cabinet's shelf. Cartridges are drawn in a neutral grey (GREY) and
take their system's plastic colour at run time; cases are drawn in their own colours. The label of a spine and the
cover of a front are left as a plain rectangle: pocket-ui paints the cover's colour, the title and the cover into
them. Everything here is plain shapes: no logo, no brand name, no lettering."""
import sys
from PIL import Image, ImageDraw

OUT = sys.argv[1]
GREY = (156, 156, 156)
PAPER = (236, 233, 225, 255)
# kind: (spine, front, height) in template pixels (half the shelf size).
SIZES = {
    "snes": (22, 88, 78), "n64": (21, 88, 74), "nes": (22, 75, 88), "gb": (18, 58, 66), "gba": (18, 75, 56),
    "cart": (22, 85, 80), "md": (20, 83, 118), "sms": (20, 83, 118), "ps1": (13, 98, 88), "segadisc": (16, 93, 100),
    "disc": (15, 98, 88), "ps2": (19, 84, 120), "gc": (18, 81, 111), "wii": (19, 84, 120), "keep": (19, 84, 120),
    "umd": (15, 66, 103), "ds": (16, 85, 95), "3ds": (15, 85, 93),
}
CARTS = ("snes", "n64", "nes", "gb", "gba", "cart")


def c3(v):
    return (v >> 16 & 255, v >> 8 & 255, v & 255, 255)


def k(c, f):
    return tuple(max(0, min(255, int(v * f))) for v in c[:3]) + (255,)


def bevel(d, x0, y0, x1, y1, c, hi=1.16, lo=0.72):
    d.line([x0, y0, x1, y0], fill=k(c, hi))
    d.line([x0, y0, x0, y1], fill=k(c, hi * 0.97))
    d.line([x0, y1, x1, y1], fill=k(c, lo))
    d.line([x1, y0, x1, y1], fill=k(c, lo + 0.06))


def grooves(d, x0, x1, y0, n, gap, c):
    for i in range(n):
        y = y0 + i * gap
        d.line([x0, y, x1, y], fill=k(c, 0.68))
        d.line([x0, y + 1, x1, y + 1], fill=k(c, 1.14))


def sides(im, x0, x1, y0, y1, light=1.08, dark=0.86, n=2):
    """The plastic rounds off at its edges: lighter columns at the left, darker at the right."""
    px = im.load()
    for y in range(y0, y1 + 1):
        for i in range(n):
            for x, f in ((x0 + i, light - (light - 1) * i / n), (x1 - i, dark + (1 - dark) * i / n)):
                r, g, b, a = px[x, y]
                if a:
                    px[x, y] = (min(255, int(r * f)), min(255, int(g * f)), min(255, int(b * f)), a)


def recess(d, lab, c):
    x, y, w, h = lab
    d.line([x - 1, y - 1, x + w, y - 1], fill=k(c, 0.55))
    d.line([x - 1, y - 1, x - 1, y + h], fill=k(c, 0.6))
    d.line([x - 1, y + h, x + w, y + h], fill=k(c, 1.2))
    d.line([x + w, y - 1, x + w, y + h], fill=k(c, 1.12))
    d.rectangle([x, y, x + w - 1, y + h - 1], fill=PAPER)


# ---- cartridges ---------------------------------------------------------------------------------------------

def cart_front(kind, W, H):
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = GREY + (255,)
    o = k(c, 0.42)
    if kind in ("snes", "cart"):
        # A body with two grips standing up at the top corners, grooved down the sides.
        d.rounded_rectangle([0, 4, W - 1, H - 1], radius=4, fill=c, outline=o)
        bevel(d, 1, 5, W - 2, H - 2, c)
        for x0 in (2, W - 12):
            d.rounded_rectangle([x0, 0, x0 + 9, 13], radius=2, fill=k(c, 0.94), outline=o)
            grooves(d, x0 + 2, x0 + 7, 3, 3, 3, c)
        grooves(d, 3, 9, 17, (H - 24) // 3, 3, c)
        grooves(d, W - 10, W - 4, 17, (H - 24) // 3, 3, c)
        lab = (13, 14, W - 26, H - 21)
        if kind == "cart":
            d.rectangle([W // 2 - 10, H - 4, W // 2 + 10, H - 2], fill=k(c, 0.6))
    elif kind == "n64":
        # Wide and short, a ribbed slope at the top.
        d.polygon([(7, 0), (W - 8, 0), (W - 1, 9), (W - 1, H - 1), (0, H - 1), (0, 9)], fill=c, outline=o)
        for x in range(9, W - 9, 3):
            d.line([x, 2, x, 9], fill=k(c, 0.7))
        d.line([1, 10, W - 2, 10], fill=k(c, 1.2))
        bevel(d, 1, 11, W - 2, H - 2, c)
        lab = (8, 16, W - 16, H - 22)
    elif kind == "nes":
        # Tall, a grooved grip at the top and a ribbed end below the label.
        d.rectangle([0, 0, W - 1, H - 1], fill=c, outline=o)
        bevel(d, 1, 1, W - 2, H - 2, c)
        grooves(d, 4, W - 5, 4, 4, 3, c)
        lab = (7, 18, W - 14, int(H * 0.52))
        grooves(d, 4, W - 5, lab[1] + lab[3] + 5, (H - (lab[1] + lab[3] + 8)) // 3, 3, c)
    elif kind == "gb":
        # The cut corner at the top right, the pressed arrow, a grooved strip under the label.
        d.polygon([(0, 2), (2, 0), (W - 9, 0), (W - 1, 8), (W - 1, H - 1), (0, H - 1)], fill=c, outline=o)
        bevel(d, 1, 9, W - 2, H - 2, c)
        d.line([1, 1, W - 10, 1], fill=k(c, 1.16))
        d.polygon([(W // 2 - 5, 3), (W // 2 + 4, 3), (W // 2, 7)], fill=k(c, 0.78))
        lab = (6, 12, W - 12, int(H * 0.6))
        grooves(d, 6, W - 7, lab[1] + lab[3] + 4, 3, 3, c)
    else:  # gba: wide, a notch at the top
        d.rounded_rectangle([0, 0, W - 1, H - 1], radius=3, fill=c, outline=o)
        bevel(d, 1, 1, W - 2, H - 2, c)
        d.rectangle([W // 2 - 10, 0, W // 2 + 10, 2], fill=k(c, 0.62))
        lab = (7, 7, W - 14, int(H * 0.66))
        grooves(d, 7, W - 8, lab[1] + lab[3] + 3, 3, 3, c)
    recess(d, lab, c)
    sides(im, 1, W - 2, 12, H - 2, n=3)
    return im, lab


def cart_spine(kind, W, H):
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = GREY + (255,)
    d.rounded_rectangle([0, 0, W - 1, H - 1], radius=3 if kind != "gba" else 2, fill=c, outline=k(c, 0.42))
    bevel(d, 1, 1, W - 2, H - 2, c)
    n, top, bottom = {"snes": (5, 21, 5), "cart": (5, 21, 5), "n64": (4, 17, 5), "nes": (5, 21, 6), "gb": (2, 11, 5), "gba": (2, 10, 4)}[kind]
    grooves(d, 3, W - 4, 4, n, 3, c)
    lab = (3, top, W - 6, H - top - bottom)
    recess(d, lab, c)
    # A little foot under the label: the edge where the two shell halves meet.
    d.line([3, H - bottom + 1, W - 4, H - bottom + 1], fill=k(c, 0.8))
    sides(im, 1, W - 2, 1, H - 2)
    # The top end (grooves) and the bottom end stay as they are when the spine is stretched.
    return im, lab, (top - 1, bottom - 1)


# ---- cases ----------------------------------------------------------------------------------------------------

RIM = {"md": 0x151517, "sms": 0x151517, "ps1": 0x1c1d22, "segadisc": 0xd9d9dc, "disc": 0x1c1d22, "ps2": 0x0e0f11,
       "gc": 0x1b1b20, "wii": 0xeaeaeb, "keep": 0x16171a, "umd": 0x111113, "ds": 0xbfc2c7, "3ds": 0xe6e6e8}
# The band at the top of the spine: (height, colour or None for the cover's accent colour).
BAND = {"md": (13, None), "sms": (12, 0xf2f2f0), "ps1": (11, 0x0e0e10), "segadisc": (11, None), "disc": (10, None),
        "ps2": (16, 0x0e0f11), "gc": (12, 0x2a2a31), "wii": (14, 0xf4f4f4), "keep": (14, None), "umd": (12, 0x111113),
        "ds": (11, 0xf6f6f6), "3ds": (11, 0xf6f6f6)}


def case_front(kind, W, H):
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    rc = c3(RIM[kind])
    light = RIM[kind] > 0x808080
    r = {"umd": 7, "wii": 3, "ds": 3, "3ds": 3, "gc": 2}.get(kind, 2)
    d.rounded_rectangle([0, 0, W - 1, H - 1], radius=r, fill=rc, outline=k(rc, 0.6) if light else (5, 5, 7, 255))
    if kind in ("ps1", "segadisc", "disc"):
        # A jewel case: the ribbed hinge at the left.
        hw = 6
        d.rectangle([1, 1, hw, H - 2], fill=k(rc, 1.35 if not light else 0.92))
        for y in range(2, H - 2, 2):
            d.point((2, y), fill=k(rc, 2.4 if not light else 1.05))
            d.point((4, y), fill=k(rc, 0.6))
        lab = (hw + 1, 1, W - hw - 2, H - 2)
    elif kind == "umd":
        # The slim case with a rounded top and the latch.
        d.rectangle([W // 2 - 8, 0, W // 2 + 8, 1], fill=(48, 48, 52, 255))
        lab = (3, 5, W - 6, H - 9)
    elif kind in ("md", "sms"):
        lab = (3, 3, W - 6, H - 6)
        if kind == "sms":
            # The white grid border of the old Sega covers.
            d.rectangle([2, 2, W - 3, H - 3], fill=(240, 240, 238, 255))
            for x in range(3, W - 3, 4):
                d.line([x, 2, x, H - 3], fill=(170, 170, 172, 255))
            for y in range(3, H - 3, 4):
                d.line([2, y, W - 3, y], fill=(170, 170, 172, 255))
            lab = (8, 8, W - 16, H - 16)
    else:
        lab = (3, 3, W - 6, H - 6)
        if kind == "ds":
            # The DS box's clear case: a light inner rim.
            d.rectangle([2, 2, W - 3, H - 3], outline=k(rc, 1.12))
    x, y, w, h = lab
    # The rim's bevel: light along the top and left, darker at the bottom and right.
    d.line([1, 1, W - 2, 1], fill=k(rc, 1.6 if not light else 1.04))
    d.line([1, 1, 1, H - 2], fill=k(rc, 1.4 if not light else 1.02))
    d.line([1, H - 2, W - 2, H - 2], fill=k(rc, 0.7))
    d.line([W - 2, 1, W - 2, H - 2], fill=k(rc, 0.75))
    d.rectangle([x, y, x + w - 1, y + h - 1], fill=(220, 218, 212, 255))
    return im, lab


def case_spine(kind, W, H):
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    rc = c3(RIM[kind])
    light = RIM[kind] > 0x808080
    d.rounded_rectangle([0, 0, W - 1, H - 1], radius=2, fill=rc, outline=k(rc, 0.6) if light else (5, 5, 7, 255))
    r = 1 if W < 16 else 2
    band_h, band_c = BAND[kind]
    hinge = kind in ("ps1", "segadisc", "disc")
    top = r + (3 if hinge else 0)
    bot = H - 1 - r - (3 if hinge else 0)
    d.rectangle([r, top, W - 1 - r, bot], fill=PAPER)
    band = (r, top, W - 2 * r, band_h)
    d.rectangle([r, top, W - 1 - r, top + band_h - 1], fill=c3(band_c) if band_c is not None else (200, 200, 200, 255))
    d.line([r, top + band_h, W - 1 - r, top + band_h], fill=k(rc, 0.55) if light else (8, 8, 10, 255))
    if kind == "ps2" or kind == "gc":
        # A small light mark in the dark band, where the console's own band has its emblem.
        d.rectangle([W // 2 - 2, top + band_h // 2 - 1, W // 2 + 1, top + band_h // 2], fill=(120, 122, 130, 255))
    if kind == "sms":
        for y in range(top + 1, top + band_h - 1, 3):
            d.line([r, y, W - 1 - r, y], fill=(170, 170, 172, 255))
    if hinge:
        for y in (1, 3, H - 4, H - 2):
            d.line([1, y, W - 2, y], fill=k(rc, 1.5) if not light else k(rc, 0.8))
    lab = (r, top + band_h + 1, W - 2 * r, bot - (top + band_h + 1) + 1)
    # Gloss of the clear sleeve down the spine, the rim rounding off at both sides.
    # (opaque: pocket-ui reads a pixel as there or not, so a see-through gloss would be a hole; the label's own
    # highlight is painted at run time)
    if band_c is not None:
        bc = c3(band_c)
        d.line([r + 1, top, r + 1, top + band_h - 1], fill=k(bc, 1.12) if band_c < 0xe0e0e0 else k(bc, 0.94))
    sides(im, 0, W - 1, 1, H - 2, light=1.25 if not light else 1.03, dark=0.7, n=1)
    return im, lab, band if band_c is None else None, (top + band_h + 1, H - bot)


rows = []
for kind, (sw, fw, h) in SIZES.items():
    if kind in CARTS:
        f, flab = cart_front(kind, fw, h)
        s, slab, caps = cart_spine(kind, sw, h)
        band = None
    else:
        f, flab = case_front(kind, fw, h)
        s, slab, band, caps = case_spine(kind, sw, h)
    assert f.size == (fw, h) and s.size == (sw, h)
    f.save(f"{OUT}/{kind}-front.png", optimize=True)
    s.save(f"{OUT}/{kind}-spine.png", optimize=True)
    rows.append((kind, sw, fw, h, slab, flab, band, caps))

for kind, sw, fw, h, slab, flab, band, caps in rows:
    b = f"Some(({band[0]}, {band[1]}, {band[2]}, {band[3]}))" if band else "None"
    print(f'    // {kind}\n    Template {{ spine: ({sw}, {h}), front: ({fw}, {h}), spine_label: {slab}, front_label: {flab}, band: {b}, caps: {caps}, tint: {str(kind in CARTS).lower()} }},')
