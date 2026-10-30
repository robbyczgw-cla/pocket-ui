"""persp.py <in> <out> <W> <H> x0,y0 x1,y1 x2,y2 x3,y3 [margin]: map the quad (TL, TR, BR, BL in the photo) onto a
straight W x H rectangle on white, with a white margin around it (default 4 % of the long side)."""
import sys
from PIL import Image
src, out, W, H = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
q = [tuple(float(v) for v in a.split(",")) for a in sys.argv[5:9]]
m = int(sys.argv[9]) if len(sys.argv) > 9 else round(0.04 * max(W, H))
dst = [(m, m), (m + W, m), (m + W, m + H), (m, m + H)]
# solve for coefficients mapping output (dst) -> input (q): x = (a u + b v + c)/(g u + h v + 1) ...
A, B = [], []
for (u, v), (x, y) in zip(dst, q):
    A.append([u, v, 1, 0, 0, 0, -u * x, -v * x]); B.append(x)
    A.append([0, 0, 0, u, v, 1, -u * y, -v * y]); B.append(y)
n = 8
M = [A[i] + [B[i]] for i in range(n)]
for c in range(n):
    p = max(range(c, n), key=lambda r: abs(M[r][c])); M[c], M[p] = M[p], M[c]
    for r in range(n):
        if r != c:
            f = M[r][c] / M[c][c]
            M[r] = [a - f * b for a, b in zip(M[r], M[c])]
coef = [M[i][n] / M[i][i] for i in range(n)]
im = Image.open(src).convert("RGB")
o = im.transform((W + 2 * m, H + 2 * m), Image.PERSPECTIVE, coef, Image.BICUBIC, fillcolor=(255, 255, 255))
# white outside the quad (the transform also pulls in backdrop / other faces there)
from PIL import ImageDraw
mask = Image.new("L", o.size, 255); ImageDraw.Draw(mask).rectangle((m, m, m + W - 1, m + H - 1), fill=0)
o.paste((255, 255, 255), mask=mask)
o.save(out); print(out, o.size)
