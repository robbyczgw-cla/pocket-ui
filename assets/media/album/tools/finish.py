"""finish.py <px-frame.png> <px-case-plastic.png> <px-case-cardboard.png> <out dir>: the album's art from PixelLab's
conversions (SOURCES.md): the instant photo's frame with its picture window cut out (see-through, where the game's
picture goes) and its window printed for album.rs; the two open cases with their last rows (the photo's background
under the case) cleaned to the case's own edge."""
import sys
from PIL import Image
import numpy as np

frame, plastic, cardboard, out = sys.argv[1:5]
f = np.asarray(Image.open(frame).convert('RGBA')).copy()
h, w = f.shape[:2]
lum = f[..., :3].astype(int).sum(2)
# The window: the dark block in the middle; grown from the middle while the next row/column is still dark.
dark = lum < 200
cy, cx = h * 2 // 5, w // 2
x0 = x1 = cx
y0 = y1 = cy
while x0 > 0 and dark[cy, x0 - 1]: x0 -= 1
while x1 < w - 1 and dark[cy, x1 + 1]: x1 += 1
while y0 > 0 and dark[y0 - 1, cx]: y0 -= 1
while y1 < h - 1 and dark[y1 + 1, cx]: y1 += 1
f[y0:y1 + 1, x0:x1 + 1, 3] = 0
# Rows under the frame's bottom edge that are the photo's background (cream, all alike): cut off.
bg = f[h - 1, 0, :3].astype(int)
while h > y1 + 4 and (np.abs(f[h - 1, :, :3].astype(int) - bg).sum(1) < 8).all():
    h -= 1
f = f[:h]
Image.fromarray(f).save(f'{out}/polaroid.png')
print(f'window x={x0} y={y0} w={x1 - x0 + 1} h={y1 - y0 + 1} of {w}x{h}')

for src, name in ((plastic, 'case-plastic'), (cardboard, 'case-cardboard')):
    a = np.asarray(Image.open(src).convert('RGBA')).copy()
    hh = a.shape[0]
    # Rows at the bottom that are mostly light (the white under the photo): copy the last clean row over them.
    l = a[..., :3].astype(int).sum(2)
    bad = [y for y in range(hh - 6, hh) if (l[y] > 600).mean() > 0.1]
    if bad:
        good = min(bad) - 1
        for y in range(min(bad), hh):
            a[y] = a[good]
    if name == 'case-plastic':
        # White left over from the photo's background where the flattened halves didn't reach (the top rows, the
        # bottom of the spine): the plastic of the same column, from the nearest clean row inward.
        l = a[..., :3].astype(int).sum(2)
        for x in range(a.shape[1]):
            for y in list(range(0, 5)) + list(range(hh - 14, hh)):
                if l[y, x] > 560:
                    step = 1 if y < hh // 2 else -1
                    yy = y
                    while 0 <= yy < hh and l[yy, x] > 560:
                        yy += step
                    if 0 <= yy < hh:
                        a[y, x] = a[yy, x]
    Image.fromarray(a).save(f'{out}/{name}.png')
    print(name, a.shape[1], a.shape[0], 'cleaned rows', bad)
