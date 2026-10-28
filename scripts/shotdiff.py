#!/usr/bin/env python3
"""Mean absolute difference between screenshots (0 = identical, 255 = opposite), to judge save states:
    shotdiff.py A.png B.png C.png    prints d(A,B) and d(A,C); after "save at A, move on to B, load, look at C"
                                     d(A,C) should be clearly smaller than d(A,B).  (Needs Pillow.)"""
import sys
from PIL import Image, ImageChops, ImageStat


def diff(a, b):
    ia = Image.open(a).convert("L").resize((310, 270))
    ib = Image.open(b).convert("L").resize((310, 270))
    return ImageStat.Stat(ImageChops.difference(ia, ib)).mean[0]


a, b, c = sys.argv[1:4]
dab, dac = diff(a, b), diff(a, c)
print(f"d(save,moved on) {dab:.2f}   d(save,after load) {dac:.2f}   {'OK' if dac < 0.5 * dab else 'CHECK'}")
