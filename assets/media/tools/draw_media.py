"""Draws the pixel-art templates of the game media (assets/media/<kind>-spine.png, <kind>-front.png) and prints the
table `src/media.rs` keeps (label and band rectangles, the spine's caps for 9-slicing).

Usage: python3 -I draw_media.py <out dir>   (needs Pillow)

One template pixel is 2.5 screen pixels on the Cabinet's shelf. Every game stands as a case: the cartridge systems'
cardboard boxes are drawn in a neutral grey (GREY) and take the cover's colour at run time (printed cardboard); cases
are drawn in their own colours. The label of a spine and the cover of a front are left as a plain rectangle:
pocket-ui paints the cover's colour, the title and the cover into them.

What is inside a box (assets/media/game/<kind>.png): the cartridge or game card, drawn in GREY too and coloured with
the system's plastic; its label takes the cover. Discs and UMDs are painted by pocket-ui itself.
Everything here is plain shapes: no logo, no brand name, no lettering."""
import sys
from PIL import Image, ImageDraw

OUT = sys.argv[1]
GREY = (156, 156, 156)
PAPER = (236, 233, 225, 255)
# kind: (spine, front, height) in template pixels (half the shelf size).
# Real sizes at one scale: 0.63 template px per mm (a DVD case's 190 mm is 120 px); docs/DESIGN.md "Media" has the
# table in mm. Spines are thicker than life so a title fits, in the real order. In the order of `Medium::ALL`.
# kind: (spine, front, height) in template pixels.
SIZES = {
    "snes": (24, 116, 84), "n64": (24, 116, 82), "nes": (24, 84, 116), "gb": (20, 76, 81), "gba": (20, 74, 74),
    "generic-cart": (22, 82, 113), "md": (20, 85, 120), "sms": (20, 85, 120), "ps1": (13, 90, 79), "segadisc": (16, 90, 80),
    "generic-disc": (19, 85, 120), "ps2": (19, 85, 120), "gc": (18, 85, 116), "wii": (19, 85, 120), "keep": (19, 85, 120),
    "umd": (15, 66, 110), "ds": (16, 79, 86), "3ds": (15, 79, 86), "xbox": (19, 85, 120), "vita": (14, 66, 86),
    "generic-digital": (14, 66, 86),
}
# The boxes that lie wide stand on their short side on a shelf, as collectors keep them: the spine is as long as the box
# is wide (184 mm), the title runs along it.
SPINE_H = {"snes": 116, "n64": 116}
BOXES = ("snes", "n64", "nes", "gb", "gba")
GENERIC = ("generic-cart", "generic-disc", "generic-digital")
# What is inside, (width, height): the cartridges as they were on the shelf until 0.1.4, the Mega Drive's cartridge,
# a game card (DS, 3DS, Vita).
# In their real proportions, at the size their details need (`Game::DRAWN`'s order); pocket-ui scales each to its real
# size beside its case (`Game::mm`).
GAMES = {"snes": (88, 78), "n64": (88, 57), "nes": (75, 88), "gb": (58, 66), "gba": (75, 46), "cart": (85, 80),
         "md": (72, 88), "card": (32, 30), "memory": (24, 32)}


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
    elif kind == "md":
        # Square shoulders, a grooved grip along the top, the label taking most of the front.
        d.rectangle([0, 0, W - 1, H - 1], fill=c, outline=o)
        bevel(d, 1, 1, W - 2, H - 2, c)
        grooves(d, 4, W - 5, 3, 3, 3, c)
        d.rectangle([2, 12, 4, H - 4], fill=k(c, 0.8))
        d.rectangle([W - 5, 12, W - 3, H - 4], fill=k(c, 0.8))
        lab = (7, 13, W - 14, H - 22)
        grooves(d, 9, W - 10, H - 7, 2, 2, c)
    elif kind == "card":
        # A small game card: the notched corner at the top left, the contacts' ridge at the bottom.
        d.polygon([(0, 3), (3, 0), (W - 1, 0), (W - 1, H - 1), (0, H - 1)], fill=c, outline=o)
        bevel(d, 1, 3, W - 2, H - 2, c)
        d.line([4, 1, W - 2, 1], fill=k(c, 1.16))
        lab = (3, 5, W - 6, H - 11)
        for x in range(4, W - 4, 3):
            d.line([x, H - 4, x + 1, H - 4], fill=k(c, 0.6))
    elif kind == "memory":
        # A memory card: the cut corner at the top right, the contacts' ridges along the top.
        d.polygon([(0, 0), (W - 6, 0), (W - 1, 5), (W - 1, H - 1), (0, H - 1)], fill=c, outline=o)
        bevel(d, 1, 6, W - 2, H - 2, c)
        for x in range(3, W - 7, 3):
            d.line([x, 1, x, 4], fill=k(c, 0.66))
        lab = (3, 8, W - 6, H - 11)
    else:  # gba: wide, a notch at the top
        d.rounded_rectangle([0, 0, W - 1, H - 1], radius=3, fill=c, outline=o)
        bevel(d, 1, 1, W - 2, H - 2, c)
        d.rectangle([W // 2 - 10, 0, W // 2 + 10, 2], fill=k(c, 0.62))
        lab = (7, 7, W - 14, int(H * 0.6))
        grooves(d, 7, W - 8, lab[1] + lab[3] + 3, 3, 3, c)
    recess(d, lab, c)
    sides(im, 1, W - 2, min(12, H // 3), H - 2, n=3 if W > 40 else 1)
    return im, lab


# ---- cardboard boxes ------------------------------------------------------------------------------------------

def worn(px, W, H):
    """Rubbed corners: the card shows lighter where a box has been taken off the shelf a hundred times."""
    for (x, y) in ((1, 1), (2, 1), (1, 2), (W - 2, 1), (W - 3, 1), (W - 2, 2), (1, H - 2), (2, H - 2), (1, H - 3),
                   (W - 2, H - 2), (W - 3, H - 2), (W - 2, H - 3)):
        r, g, b, a = px[x, y]
        px[x, y] = (min(255, int(r * 1.3)), min(255, int(g * 1.3)), min(255, int(b * 1.3)), a)


def box_front(kind, W, H):
    """A game box's front is its printed cover, edge to edge: one pixel of card all round (pocket-ui paints it the same
    warm grey on every box, media::card_edge); the tuck flap's fold and the rubbed corners are drawn over the cover at
    run time (media::box_wear)."""
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = GREY + (255,)
    d.rectangle([0, 0, W - 1, H - 1], fill=c)
    lab = (1, 1, W - 2, H - 2)
    x, y, w, h = lab
    d.rectangle([x, y, x + w - 1, y + h - 1], fill=PAPER)
    return im, lab


def box_spine(kind, W, H):
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    c = GREY + (255,)
    d.rectangle([0, 0, W - 1, H - 1], fill=c, outline=k(c, 0.45))
    # The folds of the flaps at both ends, the card rounding off at the sides.
    for y, f in ((1, 1.22), (2, 1.08), (3, 0.72), (H - 4, 0.72), (H - 3, 1.08)):
        d.line([1, y, W - 2, y], fill=k(c, f))
    d.line([1, 4, 1, H - 5], fill=k(c, 1.12))
    d.line([W - 2, 4, W - 2, H - 5], fill=k(c, 0.8))
    worn(im.load(), W, H)
    # A printed band at the top (the cover's accent), the title's label under it.
    bh = max(8, H // 9)
    band = (2, 5, W - 4, bh)
    lab = (2, 5 + bh + 2, W - 4, H - (5 + bh + 2) - 6)
    d.rectangle([band[0], band[1], band[0] + band[2] - 1, band[1] + band[3] - 1], fill=(200, 200, 200, 255))
    d.line([2, 5 + bh, W - 3, 5 + bh], fill=k(c, 0.8))
    d.rectangle([lab[0], lab[1], lab[0] + lab[2] - 1, lab[1] + lab[3] - 1], fill=PAPER)
    return im, lab, band, (5 + bh + 2, 6)


# ---- cases ----------------------------------------------------------------------------------------------------

RIM = {"md": 0x151517, "sms": 0x151517, "ps1": 0x1c1d22, "segadisc": 0xd9d9dc, "ps2": 0x0e0f11,
       "gc": 0x1b1b20, "wii": 0xeaeaeb, "keep": 0x16171a, "xbox": 0x0f1411, "umd": 0x111113, "ds": 0xbfc2c7, "3ds": 0xe6e6e8, "vita": 0xcfd3da,
       "generic-cart": 0x2b2724, "generic-disc": 0x2b2724, "generic-digital": 0x2b2724}
# The band at the top of the spine: (height, colour or None for the cover's accent colour).
BAND = {"md": (13, None), "sms": (12, 0xf2f2f0), "ps1": (11, 0x0e0e10), "segadisc": (11, None),
        "ps2": (16, 0x0e0f11), "gc": (12, 0x2a2a31), "wii": (14, 0xf4f4f4), "keep": (14, None), "xbox": (16, 0x2a8a1f), "umd": (12, 0x111113),
        "ds": (11, 0xf6f6f6), "3ds": (11, 0xf6f6f6), "vita": (13, 0x1a55b8),
        "generic-cart": (14, None), "generic-disc": (14, None), "generic-digital": (12, None)}


def case_front(kind, W, H):
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    rc = c3(RIM[kind])
    light = RIM[kind] > 0x808080
    r = {"umd": 7, "wii": 3, "ds": 3, "3ds": 3, "vita": 3, "gc": 2}.get(kind, 2)
    d.rounded_rectangle([0, 0, W - 1, H - 1], radius=r, fill=rc, outline=k(rc, 0.6) if light else (5, 5, 7, 255))
    front_band = None
    if kind in ("ps1", "segadisc"):
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
        if kind in GENERIC:
            # pocket-ui's own case for every other system: warm dark plastic, a band at the top that pocket-ui paints in
            # the system's colour with its console and name, the cover under it.
            bh = BAND[kind][0] + 4
            front_band = (3, 3, W - 6, bh)
            d.rectangle([3, 3, W - 4, 3 + bh - 1], fill=(200, 200, 200, 255))
            d.line([3, 3 + bh, W - 4, 3 + bh], fill=(14, 12, 11, 255))
            lab = (3, 4 + bh, W - 6, H - 7 - bh)
    if kind == "xbox":
        # The green top band of the original Xbox case; the cover sits below it.
        lab = (3, 17, W - 6, H - 20)
        d.rectangle([2, 2, W - 3, 15], fill=c3(BAND["xbox"][1]))
        d.line([2, 2, W - 3, 2], fill=k(c3(BAND["xbox"][1]), 1.25))
        d.line([2, 16, W - 3, 16], fill=(8, 10, 8, 255))
    if kind == "vita":
        # The PS Vita case's blue top band (a plain band, no lettering); the cover sits below it.
        lab = (3, 15, W - 6, H - 18)
        d.rectangle([2, 2, W - 3, 13], fill=c3(BAND["vita"][1]))
        d.line([2, 2, W - 3, 2], fill=k(c3(BAND["vita"][1]), 1.3))
        d.line([2, 14, W - 3, 14], fill=(10, 20, 40, 255))
    x, y, w, h = lab
    # The rim's bevel: light along the top and left, darker at the bottom and right.
    d.line([1, 1, W - 2, 1], fill=k(rc, 1.6 if not light else 1.04))
    d.line([1, 1, 1, H - 2], fill=k(rc, 1.4 if not light else 1.02))
    d.line([1, H - 2, W - 2, H - 2], fill=k(rc, 0.7))
    d.line([W - 2, 1, W - 2, H - 2], fill=k(rc, 0.75))
    d.rectangle([x, y, x + w - 1, y + h - 1], fill=(220, 218, 212, 255))
    return im, lab, front_band


def case_spine(kind, W, H):
    im = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    rc = c3(RIM[kind])
    light = RIM[kind] > 0x808080
    d.rounded_rectangle([0, 0, W - 1, H - 1], radius=2, fill=rc, outline=k(rc, 0.6) if light else (5, 5, 7, 255))
    r = 1 if W < 16 else 2
    band_h, band_c = BAND[kind]
    hinge = kind in ("ps1", "segadisc")
    top = r + (3 if hinge else 0)
    bot = H - 1 - r - (3 if hinge else 0)
    d.rectangle([r, top, W - 1 - r, bot], fill=PAPER)
    band = (r, top, W - 2 * r, band_h)
    d.rectangle([r, top, W - 1 - r, top + band_h - 1], fill=c3(band_c) if band_c is not None else (200, 200, 200, 255))
    d.line([r, top + band_h, W - 1 - r, top + band_h], fill=k(rc, 0.55) if light else (8, 8, 10, 255))
    if kind in ("ps2", "gc", "xbox"):
        # A small light mark in the dark band, where the console's own band has its emblem.
        d.rectangle([W // 2 - 2, top + band_h // 2 - 1, W // 2 + 1, top + band_h // 2],
                    fill=(120, 122, 130, 255) if kind != "xbox" else (120, 196, 104, 255))
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


import os
os.makedirs(f"{OUT}/game", exist_ok=True)
games = []
for kind, (w, h) in GAMES.items():
    g, glab = cart_front(kind, w, h)
    assert g.size == (w, h)
    g.save(f"{OUT}/game/{kind}.png", optimize=True)
    games.append((kind, w, h, glab))

rows = []
for kind, (sw, fw, h) in SIZES.items():
    if kind in BOXES:
        f, flab = box_front(kind, fw, h)
        fband = None
        s, slab, band, caps = box_spine(kind, sw, SPINE_H.get(kind, h))
    else:
        f, flab, fband = case_front(kind, fw, h)
        s, slab, band, caps = case_spine(kind, sw, h)
    assert f.size == (fw, h) and s.size == (sw, SPINE_H.get(kind, h))
    f.save(f"{OUT}/{kind}-front.png", optimize=True)
    s.save(f"{OUT}/{kind}-spine.png", optimize=True)
    rows.append((kind, sw, fw, h, SPINE_H.get(kind, h), slab, flab, band, fband, caps))


def opt(r):
    return f"Some(({r[0]}, {r[1]}, {r[2]}, {r[3]}))" if r else "None"


for kind, sw, fw, h, sh, slab, flab, band, fband, caps in rows:
    print(f'    // {kind}\n    Template {{ spine: ({sw}, {sh}), front: ({fw}, {h}), spine_label: {slab}, front_label: {flab}, band: {opt(band)}, front_band: {opt(fband)}, caps: {caps}, tint: {str(kind in BOXES).lower()} }},')
print()
for kind, w, h, lab in games:
    print(f'    // {kind}\n    GameTemplate {{ size: ({w}, {h}), label: {lab} }},')
