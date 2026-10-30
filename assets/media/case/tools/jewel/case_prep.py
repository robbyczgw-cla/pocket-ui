"""case_prep.py <specs.json> <kind> <front|spine> <out.png>

Prepares a straight-on photo of one face of a game case for PixelLab image-to-pixelart. The face's four corners in the
photo (`quad`: top-left, top-right, bottom-right, bottom-left, photo pixels) are mapped onto a rectangle of exactly the
case's template proportions (`size`, the final PNG's pixels) times an integer factor (prep long side <= 1280 px), so the
conversion comes back pixel-aligned with the final picture. Then, in this order: `flip` mirrors it (the inside of a
front cover seen from the outside), `tone` maps the plastic's brightness (lum -> lum * mul + add, per channel offsets in
`tint`), `paint` fills rectangles (final-PNG pixels) with a colour or with the plastic sampled at `from` (logos,
lettering, clips), `vgrad` fills with a vertical blend of the rows above and below, and the label (`label`) and the
band (`band`) become the flat grey (200, 200, 200) where the cover / title / accent colour goes at run time."""
import json, sys
from PIL import Image, ImageDraw

spec_all, kind, part, out = json.load(open(sys.argv[1])), sys.argv[2], sys.argv[3], sys.argv[4]
sp = spec_all[kind][part]
W, H = sp["size"]
s = max(1, 1280 // max(W, H))
q = sp["quad"]


def coeffs(dst, srcp):
    A, B = [], []
    for (x, y), (u, v) in zip(dst, srcp):
        A.append([x, y, 1, 0, 0, 0, -u * x, -u * y]); B.append(u)
        A.append([0, 0, 0, x, y, 1, -v * x, -v * y]); B.append(v)
    n = len(B)
    m = [row[:] + [b] for row, b in zip(A, B)]
    for c in range(n):
        p = max(range(c, n), key=lambda r: abs(m[r][c]))
        m[c], m[p] = m[p], m[c]
        for r in range(n):
            if r != c:
                f = m[r][c] / m[c][c]
                m[r] = [a - f * b for a, b in zip(m[r], m[c])]
    return [m[i][n] / m[i][i] for i in range(n)]


src = Image.open(sp["photo"]).convert("RGB")
PW, PH = W * s, H * s
dst = [(0, 0), (PW, 0), (PW, PH), (0, PH)]
im = src.transform((PW, PH), Image.PERSPECTIVE, coeffs(dst, [tuple(q[i:i + 2]) for i in range(0, 8, 2)]), Image.BICUBIC)
if sp.get("flip"):
    im = im.transpose(Image.FLIP_LEFT_RIGHT)
if "tone" in sp:
    mul, add = sp["tone"]
    t = sp.get("tint", [0, 0, 0])
    r, g, b = im.split()
    im = Image.merge("RGB", [ch.point(lambda v, o=o: max(0, min(255, int(v * mul + add + o)))) for ch, o in zip((r, g, b), t)])
d = ImageDraw.Draw(im)
S = lambda rc: [rc[0] * s, rc[1] * s, (rc[0] + rc[2]) * s - 1, (rc[1] + rc[3]) * s - 1]
for op in sp.get("paint", []):
    rc = S(op["rect"])
    if "vgrad" in op:
        px = im.load()
        for x in range(rc[0], rc[2] + 1):
            a, b = px[x, max(0, rc[1] - 2)], px[x, min(PH - 1, rc[3] + 2)]
            for y in range(rc[1], rc[3] + 1):
                f = (y - rc[1]) / max(1, rc[3] - rc[1])
                px[x, y] = tuple(int(a[i] * (1 - f) + b[i] * f) for i in range(3))
        continue
    col = tuple(op["colour"]) if "colour" in op else im.getpixel((op["from"][0] * s, op["from"][1] * s))
    d.rectangle(rc, fill=col)
for key in ("label", "band"):
    if sp.get(key):
        d.rectangle(S(sp[key]), fill=(200, 200, 200))
im.save(out)
print(out, im.size, "factor", s)
