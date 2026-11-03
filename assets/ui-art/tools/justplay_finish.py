"""justplay_finish.py <raw.png> <out.png>: a PixelLab conversion of a white-background photo as pocket-ui ships it.

The backdrop (the light colour PixelLab made of the white) is flooded away from the border, inwards up to the object's
dark outline, so the cream label inside stays; the result is cut to the picture with a 1 px clear margin and every
pixel is fully opaque or fully clear (finalize.py's rule for the other emblems)."""
import sys
from collections import deque
from PIL import Image

raw, out = sys.argv[1], sys.argv[2]
im = Image.open(raw).convert("RGBA")
w, h = im.size
px = im.load()
bg = px[0, 0][:3]


def near(c):
    return sum(abs(c[i] - bg[i]) for i in range(3)) < 60


seen = [[False] * w for _ in range(h)]
q = deque()
for x in range(w):
    q.extend([(x, 0), (x, h - 1)])
for y in range(h):
    q.extend([(0, y), (w - 1, y)])
while q:
    x, y = q.popleft()
    if not (0 <= x < w and 0 <= y < h) or seen[y][x] or not near(px[x, y]):
        continue
    seen[y][x] = True
    q.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)])
for y in range(h):
    for x in range(w):
        r, g, b, a = px[x, y]
        px[x, y] = (0, 0, 0, 0) if seen[y][x] or a < 128 else (r, g, b, 255)
# Specks left by the flood (the joystick's cable end): only pieces of 60 pixels or more stay.
keep = {}
pieces = []
for y0 in range(h):
    for x0 in range(w):
        if px[x0, y0][3] and (x0, y0) not in keep:
            piece, st = [], [(x0, y0)]
            keep[(x0, y0)] = 1
            while st:
                x, y = st.pop()
                piece.append((x, y))
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)):
                    n = (x + dx, y + dy)
                    if 0 <= n[0] < w and 0 <= n[1] < h and px[n][3] and n not in keep:
                        keep[n] = 1
                        st.append(n)
            pieces.append(piece)
big = {p for piece in pieces if len(piece) >= 60 for p in piece}
for y in range(h):
    for x in range(w):
        if px[x, y][3] and (x, y) not in big:
            px[x, y] = (0, 0, 0, 0)
bb = im.getbbox()
im = im.crop(bb)
cv = Image.new("RGBA", (im.width + 2, im.height + 2), (0, 0, 0, 0))
cv.paste(im, (1, 1))
cv.save(out)
print(out, cv.size)
