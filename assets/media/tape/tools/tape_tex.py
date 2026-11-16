"""tape_tex.py <photo p1.jpg> <out.png>: the masking-tape strip of the CC0 photo, straightened, cut inside its edges,
laid horizontal, recoloured to masking tape's cream (the photo's grey light made it green-grey) keeping the paper's
own light and grain, and mirrored once so it tiles along its length."""
import sys
import numpy as np
from PIL import Image, ImageFilter
im = Image.open(sys.argv[1]).convert('RGB')
strip = im.crop((480, 250, 880, 850)).rotate(-13, resample=Image.BICUBIC).crop((152, 180, 248, 470))
strip = strip.transpose(Image.Transpose.ROTATE_90)  # length along x
a = np.asarray(strip).astype(np.float32)
lum = a.mean(axis=2)
# The light across the strip (large blur) and the paper's grain (what is left).
low = np.asarray(Image.fromarray(lum.astype(np.uint8)).filter(ImageFilter.GaussianBlur(18))).astype(np.float32)
grain = lum - low
shade = (low - low.mean()) * 0.35          # keep a little of the photo's uneven light
cream = np.array([231, 216, 176], np.float32)
out = cream[None, None, :] * (1 + (shade + grain * 1.6)[..., None] / 255.0)
out = np.clip(out, 0, 255).astype(np.uint8)
tile = np.concatenate([out, out[:, ::-1]], axis=1)
Image.fromarray(tile).save(sys.argv[2], optimize=True)
print(tile.shape, 'grain std', grain.std())
