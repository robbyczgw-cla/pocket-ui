"""usnes_compose.py <sgb_rect.png> <out.png>

Builds the front of a plain US SNES game cartridge from Evan Amos's straightened Super Game Boy photo (rectify.py's
output, 1480 x 1320 with the face at x 70..1430, y 20..1300). The Super Game Boy is built in the US cartridge shell
but is taller: on top of the shell sits the Game Boy slot housing. A game cartridge's face is about 1.5 times as wide
as it is high, so:

* the lower 870 rows of the photo are kept (the plain band, the grip recess with its two ridges, the bottom screws);
  the photo is cut at x 1365 so the side rib columns are equally wide (the photo shows a bit of the right side face);
* the side rib columns get their own top (the photo's left side top and first rib, rows 33..170, mirrored for the right) above the photo's ribs
  from row 567 on, so their grooves stay evenly spaced and the upper screws are gone;
* the centre column above the plain band is rebuilt from the photo's plain plastic (rows 896..949, tiled) and the US
  cartridge's label recess is drawn into it: a rounded recess almost as wide as the column from just below the top edge
  down to the photo's existing step (row 815), the label sticker flat light grey in its upper part and the small round
  window below the label. Proportions from a front photo of a US game cartridge (label 0.215..0.79 of the width and
  0.035..0.335 of the height, recess 0.176..0.83, window at 0.49 / 0.385).
Writes <out> (white backdrop, 40 px margin) and <out>_mask.png (white = object) for prep.py / maskcut.py."""
import sys
from PIL import Image, ImageDraw

src, out = sys.argv[1], sys.argv[2]
ph = Image.open(src).convert("RGB")
X0, X1, BOT, H = 70, 1365, 1300, 870
W = X1 - X0
TOP = BOT - H  # photo row that becomes the face's row 0
M = 40
cv = Image.new("RGB", (W + 2 * M, H + 2 * M), (255, 255, 255))
mask = Image.new("L", cv.size, 0)
COL0, COL1 = 250, 1185  # centre column incl. its edges, photo x
STEP = 815  # the photo's step below its label plate: kept as the bottom edge of the label recess
SIDE_DROP = 30  # side tops sit this far below the column top

# 1. Everything from the step down, as photographed.
cv.paste(ph.crop((X0, STEP - 5, X1, BOT)), (M, M + STEP - 5 - TOP))
# 2. Side rib columns: own top piece + the photo's ribs from row 567.
top = ph.crop((X0, 33, COL0, 170))  # left side top edge at row ~63 -> face row 30; mirrored for the right side
cv.paste(top, (M, M))
cv.paste(top.transpose(Image.FLIP_LEFT_RIGHT), (M + X1 - X0 - top.width, M))
for sx0, sx1 in ((X0, COL0), (COL1, X1)):
    cv.paste(ph.crop((sx0, 567, sx1, STEP - 5)), (M + sx0 - X0, M + 137))
# white above the side tops (the photo's backdrop there is white already, keep it clean)
d = ImageDraw.Draw(cv)
# 3. Centre column above the step: tiled plain plastic (rows 835..945) incl. the column's edge lines.
tile = ph.crop((COL0, 896, COL1, 950))
y = 0
while y < STEP - 5 - TOP:
    cv.paste(tile.crop((0, 0, tile.width, min(tile.height, STEP - 5 - TOP - y))), (M + COL0 - X0, M + y))
    y += tile.height
# column top edge: dark outline + a light rim below it
cx0, cx1 = M + COL0 - X0 + 10, M + COL1 - X0 - 14
d.rectangle([cx0, M, cx1, M + 4], fill=(150, 150, 152))
d.rectangle([cx0, M + 5, cx1, M + 9], fill=(206, 206, 208))
# 4. Label recess, sticker and the round window.
fx = lambda f: M + round(f * W)
fy = lambda f: M + round(f * H)
rx0, rx1, ry0, ry1 = fx(0.176), fx(0.83), fy(0.022), M + STEP - TOP
d.rounded_rectangle([rx0, ry0, rx1, ry1 + 2], radius=22, fill=(176, 176, 178))  # inner edge (shadow)
d.rounded_rectangle([rx0 + 7, ry0 + 9, rx1 - 4, ry1 - 2], radius=18, fill=(184, 184, 186))  # recess floor
d.rounded_rectangle([fx(0.215), fy(0.035), fx(0.79), fy(0.335)], radius=16, fill=(214, 214, 214))  # label
wx, wy, wr = fx(0.49), fy(0.385), round(0.016 * W)
d.ellipse([wx - wr - 3, wy - wr - 3, wx + wr + 3, wy + wr + 3], fill=(150, 150, 152))
d.ellipse([wx - wr, wy - wr, wx + wr, wy + wr], fill=(200, 200, 204))
d.ellipse([wx - wr // 2, wy - wr // 2, wx, wy], fill=(236, 236, 238))
# 5. Object mask: column from row 0, sides from row SIDE_DROP, rounded outer corners.
md = ImageDraw.Draw(mask)
md.rounded_rectangle([M, M + SIDE_DROP, M + W - 1, M + H - 1], radius=14, fill=255)
md.rounded_rectangle([M + COL0 - X0 + 8, M, M + COL1 - X0 - 12, M + 60], radius=10, fill=255)
# white outside the mask (the side pieces' backdrop rows above the side tops)
white = Image.new("RGB", cv.size, (255, 255, 255))
cv = Image.composite(cv, white, mask)
cv.save(out)
mask.save(out.replace(".png", "_mask.png"))
print(out, cv.size, "label box", (fx(0.215), fy(0.035), fx(0.79), fy(0.335)))
