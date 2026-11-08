#!/usr/bin/env python3
"""Keeps look-alikes of Pixelify Sans (SIL OFL 1.1) apart at every size.
Pixelify's C is an O with a one-row notch and its c an o with one: "Chrono" reads "Ohrono", "description" "desoription".
Here the capital C keeps one short lip at top and bottom and a three-row opening, the small c loses its lips.
Z gets a diagonal (it read as 2) and 5 a flat top (it read as S). Every other glyph, and every advance width and
side bearing, is untouched: nothing in a layout moves. Accented C, c and Z (Ç, ç, Č, Ž ...) are composites of these
glyphs and follow.
Rewrites assets/fonts/PixelifySans-Regular.ttf and -SemiBold.ttf in place (running it again changes nothing).
Needs fontTools:  python3 -m venv v && v/bin/pip install fonttools
Usage: scripts/pixelify-glyphs.py [fonts folder]   (default: assets/fonts next to scripts/)
"""
import os, sys
from fontTools.ttLib import TTFont
from fontTools.pens.ttGlyphPen import TTGlyphPen

HERE = os.path.dirname(os.path.abspath(__file__))
FONTS = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "assets", "fonts")

# (x, y) outlines, clockwise like the originals. Regular / SemiBold differ by stroke weight.
OUTLINES = {
    "PixelifySans-Regular": {
        "C": [(151, -12), (151, 78), (60, 78), (60, 540), (151, 540), (151, 631), (433, 631), (433, 540), (525, 540),
              (525, 450), (423, 450), (423, 530), (161, 530), (161, 88), (423, 88), (423, 169), (525, 169),
              (525, 78), (433, 78), (433, -12)],
        "c": [(151, -12), (151, 78), (60, 78), (60, 359), (151, 359), (151, 450), (433, 450), (433, 350),
              (161, 350), (161, 88), (433, 88), (433, -12)],
    },
    "PixelifySans-SemiBold": {
        "C": [(150, -11), (150, 75), (61, 75), (61, 548), (150, 548), (150, 636), (446, 636), (446, 548), (536, 548),
              (536, 449), (417, 449), (417, 511), (179, 511), (179, 113), (417, 113), (417, 175), (536, 175),
              (536, 75), (446, 75), (446, -11)],
        "c": [(150, -11), (150, 75), (61, 75), (61, 374), (150, 374), (150, 461), (446, 461), (446, 337),
              (179, 337), (179, 113), (446, 113), (446, -11)],
    },
}

# 5 x 7 bitmaps drawn on the font's own pixel grid; `e` grows every pixel to the stroke weight
BITMAPS = {
    "Z": ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"],
    "5": ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
}
GRID = {  # x0, x1, y0, y1 of the pixel centres' box (outer box shrunk by e), e
    "PixelifySans-Regular": (65, 520, -7, 626, 5),
    "PixelifySans-SemiBold": (79, 518, 7, 618, 18),
}


def bitmap_glyph(rows, name):
    x0, x1, y0, y1, e = GRID[name]
    pw, ph = (x1 - x0) / 5, (y1 - y0) / 7
    pen = TTGlyphPen(None)
    for r, row in enumerate(rows):
        for c, ch in enumerate(row):
            if ch != "#":
                continue
            l, rt = round(x0 + c * pw - e), round(x0 + (c + 1) * pw + e)
            top, bot = round(y1 - r * ph + e), round(y1 - (r + 1) * ph - e)
            pen.moveTo((l, bot)); pen.lineTo((l, top)); pen.lineTo((rt, top)); pen.lineTo((rt, bot)); pen.closePath()
    return pen.glyph()


for name, glyphs in OUTLINES.items():
    path = os.path.join(FONTS, name + ".ttf")
    font = TTFont(path)
    cmap, hmtx = font.getBestCmap(), font["hmtx"]
    before = {ch: hmtx[cmap[ord(ch)]] for ch in "CcZ5"}
    new = {}
    for ch, pts in glyphs.items():
        pen = TTGlyphPen(None)
        pen.moveTo(pts[0])
        for p in pts[1:]:
            pen.lineTo(p)
        pen.closePath()
        new[ch] = pen.glyph()
    for ch, rows in BITMAPS.items():
        new[ch] = bitmap_glyph(rows, name)
    for ch, glyph in new.items():
        font["glyf"][cmap[ord(ch)]] = glyph
    font["glyf"].recalcBounds = True
    font.save(path)
    # The advance and the left side bearing must stay what they were.
    check = TTFont(path)
    for ch, (adv, lsb) in before.items():
        g = check["glyf"][cmap[ord(ch)]]
        g.recalcBounds(check["glyf"])
        assert check["hmtx"][cmap[ord(ch)]] == (adv, lsb) and g.xMin == lsb, (name, ch, check["hmtx"][cmap[ord(ch)]], g.xMin)
    print("wrote", os.path.normpath(path), os.path.getsize(path), "bytes")
