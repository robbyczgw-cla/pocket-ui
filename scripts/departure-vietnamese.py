#!/usr/bin/env python3
"""Adds the 76 Vietnamese letters Departure Mono 1.500 lacks to assets/fonts/DepartureMono-Regular.otf, in place.

    python3 scripts/departure-vietnamese.py [font]

Needs fonttools. Departure Mono is drawn on a grid of 50 units (11 pixels to the em, capitals 8, x-height 6): every new
letter is made of its own pixels, from the font's own letters and marks: a base (a ă â e ê i o ô u ư y, capitals too),
the tone mark as the font draws it on that base's plain letter (á minus a: grave, acute, tilde; ạ minus a: dot below),
and over a circumflex or breve that tone mark two pixels higher (as Galmuri and other pixel fonts stack them; the
capitals' second mark reaches two pixels over the ascender). Drawn new, in the font's manner: the hook above (two
pixels across, one down: ả), the horn of ơ and Ơ (as ư's and Ư's, a pixel stub at the top right). ỵ's dot sits right
of the descender. No glyph of the font is changed; the new ones are 350 units wide like all of them. Departure Mono
declares no Reserved Font Name; the change is noted in assets/fonts/OFL.txt.
"""
import os
import sys
import unicodedata

from fontTools.misc.psCharStrings import T2CharString
from fontTools.pens.pointInsidePen import PointInsidePen
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.ttLib import TTFont

U = 50  # units to a pixel
COLS = range(-1, 9)
ROWS = range(-4, 14)


def pixels(gs, name):
    """The pixels of glyph `name` (x, y; y up from the baseline), sampled at their centres: exact on Departure's grid."""
    out = set()
    for y in ROWS:
        for x in COLS:
            p = PointInsidePen(gs, (x * U + U / 2, y * U + U / 2))
            gs[name].draw(p)
            if p.getResult():
                out.add((x, y))
    return out


def charstring(px, width, private, gsubrs):
    pen = T2CharStringPen(width, None)
    # One rectangle per run of pixels in a row, counter-clockwise as CFF outer contours are drawn.
    for y in sorted({y for _, y in px}):
        xs = sorted(x for x, yy in px if yy == y)
        runs, start = [], None
        for i, x in enumerate(xs):
            if start is None:
                start = x
            if i + 1 == len(xs) or xs[i + 1] != x + 1:
                runs.append((start, x))
                start = None
        for a, b in runs:
            x0, x1, y0, y1 = a * U, (b + 1) * U, y * U, (y + 1) * U
            pen.moveTo((x0, y0))
            pen.lineTo((x1, y0))
            pen.lineTo((x1, y1))
            pen.lineTo((x0, y1))
            pen.closePath()
    return pen.getCharString(private=private, globalSubrs=gsubrs)


def shift(px, dy):
    return {(x, y + dy) for x, y in px}


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "fonts", "DepartureMono-Regular.otf")
    font = TTFont(path)
    cmap = font.getBestCmap()
    gs = font.getGlyphSet()
    g = lambda ch: pixels(gs, cmap[ord(ch)])
    # The tone marks as the font draws them, small (over x-height) and capital (over the capitals).
    tones = {}
    for mark, (small, cap) in {"̀": ("à", "À"), "́": ("á", "Á"), "̃": ("ã", "Ã")}.items():
        tones[mark] = (g(small) - g("a"), g(cap) - g("A"))
    dot = g("ạ") - g("a")
    hook_small = {(2, 8), (3, 8), (3, 7)}
    tones["̉"] = (hook_small, shift(hook_small, 2))
    # Bases: the plain letter or the font's own letter with its first mark; ơ and Ơ get a horn like ư and Ư.
    dotless_i = g("i") - {(3, 7)}
    bases = {}
    for ch in "aăâeêoôuưyAĂÂEÊOÔUƯY":
        bases[ch] = g(ch)
    bases["i"] = dotless_i
    bases["I"] = g("I")
    bases["ơ"] = g("o") | {(6, 5), (6, 6)}
    bases["Ơ"] = g("O") | {(6, 7), (6, 8)}
    stacked = set("ăâêôĂÂÊÔ")
    new = {}
    for base in "aăâeêioôơuưyAĂÂEÊIOÔƠUƯY":
        for mark in ["", "̀", "́", "̃", "̉", "̣"]:
            ch = unicodedata.normalize("NFC", base + mark)
            if len(ch) != 1 or ord(ch) in cmap:
                continue
            px = set(bases[base])
            if mark == "̣":
                px |= {(5, -2)} if base == "y" else dot
            elif mark:
                small, cap = tones[mark]
                m = cap if base.isupper() else small
                px |= shift(m, 2) if base in stacked else m
            new[ch] = px
    assert len(new) == 76, len(new)
    cff = font["CFF "].cff
    top = cff.topDictIndex[0]
    strings = top.CharStrings
    private = top.Private
    order = font.getGlyphOrder()
    for ch, px in sorted(new.items()):
        name = "uni%04X" % ord(ch)
        cs = charstring(px, 350, private, cff.GlobalSubrs)
        strings.charStringsIndex.append(cs)
        strings.charStrings[name] = len(strings.charStringsIndex) - 1
        order.append(name)
        font["hmtx"][name] = (350, min(x for x, _ in px) * U)
        for table in font["cmap"].tables:
            if table.isUnicode():
                table.cmap[ord(ch)] = name
    top.charset = order
    font.setGlyphOrder(order)
    font["maxp"].numGlyphs = len(order)
    font.save(path)
    print(f"{len(new)} letters added to {path}")


if __name__ == "__main__":
    main()
