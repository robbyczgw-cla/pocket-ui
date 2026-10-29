"""Draws the starting pictures of the game boxes' backs (and the cardboard box's front) that PixelLab turns into the
templates of assets/media/back (see pixellab.py next to this file): a frame per kind of box, its middle left plain for
pocket-ui to paint the back's paper into. The frame's band is `BAND` template pixels on every side; its corners stay
as drawn and its edges are stretched (nine-slice, `game_box.rs`), so an edge is the same all along.

Usage: python3 -I draw_backs.py <out dir>   (needs Pillow)

Cartridge boxes are drawn in a neutral grey and take the label's colour at run time; DVD cases too (each system's
case colour). Plain shapes only: no logo, no lettering."""
import sys
from PIL import Image, ImageDraw

OUT = sys.argv[1]
W, H = 80, 112
GREY = (156, 156, 156, 255)
PAPER = (236, 233, 225, 255)
CLEAR = (0, 0, 0, 0)
# kind: band (template pixels on each side)
BAND = {"cartbox": 6, "cartbox-front": 6, "dvd": 6, "jewel": 7, "clamshell": 6, "umd": 7, "card": 6}


def k(c, f):
    return tuple(max(0, min(255, int(v * f))) for v in c[:3]) + (255,)


def rounded(d, box, r, fill):
    d.rounded_rectangle(box, r, fill=fill)


def frame(kind):
    im = Image.new("RGBA", (W, H), CLEAR)
    d = ImageDraw.Draw(im)
    b = BAND[kind]
    if kind in ("cartbox", "cartbox-front"):
        # Printed cardboard: a darker outline, a lighter fold along the top (the flap), a printed border.
        d.rectangle([0, 0, W - 1, H - 1], fill=k(GREY, 0.45))
        d.rectangle([1, 1, W - 2, H - 2], fill=GREY)
        d.rectangle([1, 1, W - 2, 3], fill=k(GREY, 1.25))
        d.line([1, 4, W - 2, 4], fill=k(GREY, 0.7))
        d.rectangle([3, b - 1, W - 4, H - 4], outline=k(GREY, 1.18))
    elif kind == "dvd":
        rounded(d, [0, 0, W - 1, H - 1], 4, k(GREY, 0.4))
        rounded(d, [1, 1, W - 2, H - 2], 3, GREY)
        # The hinge on the right of the back: two ridges.
        for x in (W - 5, W - 3):
            d.line([x, 3, x, H - 4], fill=k(GREY, 0.72))
        d.line([W - 4, 3, W - 4, H - 4], fill=k(GREY, 1.2))
        # The sleeve's lip all round the paper.
        d.rectangle([b - 2, b - 2, W - b + 1, H - b + 1], outline=k(GREY, 1.25))
    elif kind == "jewel":
        # Clear plastic over the tray card, whose folded ends show left and right.
        clear = (196, 206, 214, 255)
        d.rectangle([0, 0, W - 1, H - 1], fill=k(clear, 0.5))
        d.rectangle([1, 1, W - 2, H - 2], fill=clear)
        for x0 in (2, W - 7):
            d.rectangle([x0, 2, x0 + 4, H - 3], fill=k(PAPER, 0.92))
            d.line([x0 + (4 if x0 == 2 else 0), 2, x0 + (4 if x0 == 2 else 0), H - 3], fill=k(PAPER, 0.6))
        # Grooves at the corners.
        for y in (3, H - 5):
            d.line([1, y, 2, y], fill=k(clear, 0.6))
            d.line([W - 3, y, W - 2, y], fill=k(clear, 0.6))
    elif kind == "clamshell":
        black = (34, 34, 38, 255)
        rounded(d, [0, 0, W - 1, H - 1], 3, (10, 10, 12, 255))
        rounded(d, [1, 1, W - 2, H - 2], 2, black)
        for y in range(4, H - 4, 3):
            d.line([W - 5, y, W - 3, y], fill=k(black, 1.8))
        d.rectangle([b - 2, b - 2, W - b + 1, H - b + 1], outline=k(black, 2.2))
    elif kind == "umd":
        dark = (60, 64, 72, 255)
        rounded(d, [0, 0, W - 1, H - 1], 8, (16, 16, 20, 255))
        rounded(d, [1, 1, W - 2, H - 2], 7, dark)
        d.rectangle([W // 2 - 6, H - 3, W // 2 + 5, H - 2], fill=(16, 16, 20, 255))
        d.rectangle([b - 2, b - 2, W - b + 1, H - b + 1], outline=k(dark, 1.6))
    elif kind == "card":
        white = (206, 206, 210, 255)
        rounded(d, [0, 0, W - 1, H - 1], 3, k(white, 0.45))
        rounded(d, [1, 1, W - 2, H - 2], 2, white)
        d.rectangle([W - 4, H // 2 - 6, W - 2, H // 2 + 5], fill=k(white, 0.7))
        d.rectangle([b - 2, b - 2, W - b + 1, H - b + 1], outline=k(white, 1.12))
    # The middle: the paper pocket-ui paints (left plain here).
    d.rectangle([b, b, W - b - 1, H - b - 1], fill=PAPER)
    return im


for kind in BAND:
    frame(kind).save(f"{OUT}/{kind}.png")
    print(kind, BAND[kind])
