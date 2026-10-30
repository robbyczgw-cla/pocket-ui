"""prep.py <specs.json> <kind> <front|spine> <out.png>: a straight-on photo texture of the case face (photo `quad`
TL,TR,BR,BL mapped onto `size` x integer factor, long side <= 1280), optional `flip`, `tone` [mul, add] and `tint`
[r,g,b] offsets, a `bevel` (the plastic rim: a light line `bevel` px in and a dark outer line, final-PNG px), then the
`fixed` rectangles (colours the real case always has) and the `label` / `band` rectangles flat grey (200,200,200)."""
import json, sys
from PIL import Image, ImageDraw

spec, kind, part, out = json.load(open(sys.argv[1])), sys.argv[2], sys.argv[3], sys.argv[4]
sp = spec[kind][part]
W, H = sp["size"]
s = max(1, 1280 // max(W, H))


def coeffs(dst, srcp):
    A, B = [], []
    for (x, y), (u, v) in zip(dst, srcp):
        A.append([x, y, 1, 0, 0, 0, -u * x, -u * y]); B.append(u)
        A.append([0, 0, 0, x, y, 1, -v * x, -v * y]); B.append(v)
    n = len(B); m = [r[:] + [b] for r, b in zip(A, B)]
    for c in range(n):
        p = max(range(c, n), key=lambda r: abs(m[r][c])); m[c], m[p] = m[p], m[c]
        for r in range(n):
            if r != c:
                f = m[r][c] / m[c][c]; m[r] = [a - f * b for a, b in zip(m[r], m[c])]
    return [m[i][n] / m[i][i] for i in range(n)]


q = sp["quad"]
src = Image.open(sp["photo"]).convert("RGBA")
src = Image.alpha_composite(Image.new("RGBA", src.size, (255, 255, 255, 255)), src).convert("RGB")  # transparent = white backdrop
PW, PH = W * s, H * s
im = src.transform((PW, PH), Image.PERSPECTIVE, coeffs([(0, 0), (PW, 0), (PW, PH), (0, PH)],
                                                       [tuple(q[i:i + 2]) for i in range(0, 8, 2)]), Image.BICUBIC)
if sp.get("flip"):
    im = im.transpose(Image.FLIP_LEFT_RIGHT)
if "tone" in sp:
    mul, add = sp["tone"]; t = sp.get("tint", [0, 0, 0])
    im = Image.merge("RGB", [ch.point(lambda v, o=o: max(0, min(255, int(v * mul + add + o)))) for ch, o in zip(im.split(), t)])
d = ImageDraw.Draw(im)
S = lambda rc: [rc[0] * s, rc[1] * s, (rc[0] + rc[2]) * s - 1, (rc[1] + rc[3]) * s - 1]
if "bevel" in sp:
    b, (lc, dc) = sp["bevel"], sp["bevel_colours"]
    d.rounded_rectangle([0, 0, PW - 1, PH - 1], radius=sp.get("radius", 0) * s, outline=tuple(dc), width=s)
    d.rectangle([b * s, b * s, PW - b * s - 1, PH - b * s - 1], outline=tuple(lc), width=s)
for op in sp.get("fixed", []):
    d.rectangle(S(op["rect"]), fill=tuple(op["colour"]))
for key in ("label", "band"):
    if sp.get(key):
        d.rectangle(S(sp[key]), fill=(200, 200, 200))
im.save(out)
print(out, im.size, "factor", s)
