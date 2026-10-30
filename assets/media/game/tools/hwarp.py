"""hwarp.py <in.png> <out.png> <x0> <x1> <a1> <a2>

Second, horizontal-only correction after rectify.py when the face's corners in the photo were rounded off and the
straightened face came out with one side stretched: a projective remap of the columns x0..x1 (the face's left and right
edge) onto themselves, g(t) = t(1+k)/(1+kt) with t = (x - x0)/(x1 - x0), with k solved so that two features that sit
symmetric on the real cartridge (columns a1 and a2, e.g. the N64's two grooves) end up symmetric about the middle.
Rows are not touched."""
import sys
from PIL import Image

src, out = sys.argv[1], sys.argv[2]
x0, x1, a1, a2 = (float(v) for v in sys.argv[3:7])
t1, t2 = (a1 - x0) / (x1 - x0), (a2 - x0) / (x1 - x0)
g = lambda t, k: t * (1 + k) / (1 + k * t)
lo, hi = -0.9, 5.0
for _ in range(100):
    k = (lo + hi) / 2
    if g(t1, k) + g(t2, k) > 1:
        hi = k
    else:
        lo = k
# Inverse: t = s / (1 + k - k s)
im = Image.open(src).convert("RGB")
W, H = im.size
sp, dp = im.load(), None
res = im.copy()
dp = res.load()
for X in range(int(x0), int(x1) + 1):
    s = (X - x0) / (x1 - x0)
    t = s / (1 + k - k * s)
    xs = x0 + t * (x1 - x0)
    xa = min(W - 1, max(0, int(xs)))
    xb = min(W - 1, xa + 1)
    f = xs - xa
    for y in range(H):
        a, b = sp[xa, y], sp[xb, y]
        dp[X, y] = tuple(round(a[i] * (1 - f) + b[i] * f) for i in range(3))
res.save(out)
print("k", round(k, 4), "a1 ->", round(x0 + g(t1, k) * (x1 - x0)), "a2 ->", round(x0 + g(t2, k) * (x1 - x0)))
