#!/usr/bin/env python3
"""Puts several screenshots side by side (scaled down) into one image, for checking a run at a glance.
    contact.py OUT.png IN1.png IN2.png ...   (needs Pillow)"""
import sys
from PIL import Image

out, ins = sys.argv[1], sys.argv[2:]
imgs = [Image.open(p).convert("RGB") for p in ins]
h = 540
imgs = [i.resize((int(i.width * h / i.height), h)) for i in imgs]
cols = min(len(imgs), 3)
rows = (len(imgs) + cols - 1) // cols
w = max(i.width for i in imgs)
sheet = Image.new("RGB", (w * cols + 8 * (cols - 1), h * rows + 8 * (rows - 1)), (40, 40, 40))
for n, i in enumerate(imgs):
    sheet.paste(i, ((n % cols) * (w + 8), (n // cols) * (h + 8)))
sheet.save(out)
