"""rectify.py <photo> <out.png> <quad> <W,H> [--margin M] [--radius R] [--keep]

Straightens the cartridge's front face: the four corners of the face in the photo (quad = x0,y0,x1,y1,x2,y2,x3,y3
in the order top-left, top-right, bottom-right, bottom-left) are mapped onto a W x H rectangle (W:H = the real
cartridge's millimetres), placed with a white margin M. Everything outside the rectangle (rounded by R pixels at the
corners) is painted white, so the photo's backdrop, shadow and side faces are gone, and the object mask
(<out>_mask.png, white = object) is written next to it for cutout.py. --keep keeps the photo outside the rectangle
(for a face whose outline is not a rectangle; then make the outline with mask.py)."""
import sys
from PIL import Image, ImageDraw

args = sys.argv[1:]
opt = lambda n, d: args[args.index(n) + 1] if n in args else d
src, out = args[0], args[1]
q = [float(v) for v in args[2].split(",")]
W, H = (int(v) for v in args[3].split(","))
M, R = int(opt("--margin", 40)), int(opt("--radius", 0))


def coeffs(dst, srcp):
    # Perspective coefficients mapping output (dst) points to input (src) points.
    A, B = [], []
    for (x, y), (u, v) in zip(dst, srcp):
        A.append([x, y, 1, 0, 0, 0, -u * x, -u * y]); B.append(u)
        A.append([0, 0, 0, x, y, 1, -v * x, -v * y]); B.append(v)
    return solve(A, B)


def solve(A, B):
    # Gaussian elimination with partial pivoting (8 x 8).
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


dst = [(M, M), (M + W, M), (M + W, M + H), (M, M + H)]
srcp = [(q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7])]
im = Image.open(src).convert("RGB")
size = (W + 2 * M, H + 2 * M)
rect = im.transform(size, Image.PERSPECTIVE, coeffs(dst, srcp), Image.BICUBIC, fillcolor=(255, 255, 255))
mask = Image.new("L", size, 0)
ImageDraw.Draw(mask).rounded_rectangle([M, M, M + W - 1, M + H - 1], radius=R, fill=255)
if "--keep" not in args:
    rect = Image.composite(rect, Image.new("RGB", size, (255, 255, 255)), mask)
rect.save(out)
mask.save(out.replace(".png", "_mask.png"))
print(out, size)
