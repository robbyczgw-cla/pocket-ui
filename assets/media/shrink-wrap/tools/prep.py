"""prep.py <Cellofan.jpg> <out glare.png>: the public-domain photo of shrink-wrapped biscuits -> the film's glare as numbers.

The film shows where it is white and grey over the dark brown biscuits: a pixel counts by its blue (the biscuits have
little) and by how grey it is (the biscuits' own shine is pink). Over the biscuits (x 300-1000, y 205-510 of the 1350 x
930 photo) the film has one long glare band, a little uphill to the right. For each photo column from 300 to 540 of that
part the band's middle is found; out of it come two rows of 256 grey values:

- row 0, the band's profile across it, the mean of every column 48 px each side of the middle: it rises slowly, jumps to
  white at one edge and fades out softly at the other (the floor of the film's own glow taken off, 30 %).
- row 1, how far the band's sharp edge strays from a straight line along it (128 = on the line, 1 grey step = 0.25 px):
  the edge is not ruled straight.

Needs numpy and ffmpeg only."""
import subprocess
import sys

import numpy as np

src, out = sys.argv[1:3]
W, H = 1350, 930
raw = subprocess.run(["ffmpeg", "-v", "error", "-i", src, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], check=True, capture_output=True).stdout
a = np.frombuffer(raw, np.uint8).reshape(H, W, 3).astype(np.float64)
lo, hi = a.min(-1), a.max(-1)
sat = (hi - lo) / (hi + 1)
white = (np.clip((a[..., 2] - 95) / 130, 0, 1) * np.clip(1 - (sat - 0.15) / 0.35, 0, 1))[205:510, 300:1000]
prof, edge = [], []
for x in range(300, 540):
    col = np.convolve(white[:, x], np.ones(5) / 5, "same")
    y = int(np.argmax(col[:200]))
    prof.append([white[y + d, x] if 0 <= y + d < white.shape[0] else 0.0 for d in range(-48, 48)])
    # The sharp edge: the steepest rise above the middle.
    g = np.diff(col[max(0, y - 30):y + 1])
    edge.append(max(0, y - 30) + int(np.argmax(g)) if len(g) else y)
p = np.clip((np.mean(prof, 0) - 0.3) / 0.7, 0, 1)
e = np.array(edge, np.float64)
xs = np.arange(len(e))
e = e - np.polyval(np.polyfit(xs, e, 1), xs)
row0 = np.interp(np.linspace(0, len(p) - 1, 256), np.arange(len(p)), p) * 255
row1 = np.clip(128 + np.interp(np.linspace(0, len(e) - 1, 256), xs, e) * 4, 0, 255)
g = np.stack([row0, row1]).astype(np.uint8)
rgb = np.repeat(g[..., None], 3, axis=2)
subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", "256x2", "-i", "-", "-pix_fmt", "rgb24", out],
               input=rgb.tobytes(), check=True)
print("profile", " ".join(f"{v:.0f}" for v in row0[::16]))
print("edge", " ".join(f"{v:.0f}" for v in row1[::16]))
