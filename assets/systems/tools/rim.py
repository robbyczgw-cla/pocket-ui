"""rim.py <dir> [id...]: gives the pictures that are nearly all dark a thin light rim, in place.

The pictures sit on the paper of the pulled-out label (#ECE9E1) and on dark spines and tiles (#1F1F22, black).
A console that is black or navy all over (Mega Drive, PS2, PSP, Atari 2600, Master System) disappears into the
dark ones. They get a rim: one pixel just outside the silhouette in a mid light grey-blue. The picture itself
is not touched, so it looks the same on paper, only with a fine grey line round it.

Which pictures: those named, or, with no names, every one where at least DARK_SHARE of the pixels are within
reach of the spine colour (luminance below NEAR_SPINE). Running it twice changes nothing more (a picture that
already has the rim is not dark any more by the share it counts, and the rim pixels are skipped by the check).
"""
import glob, os, sys
from PIL import Image

RIM = (0x8C, 0x93, 0xA3)
NEAR_SPINE = 53  # luminance: the spine #1F1F22 is 31, a tile or card on black 0
DARK_SHARE = 0.85


def lum(r, g, b):
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def dark_share(im):
    px = [lum(r, g, b) for r, g, b, a in im.get_flattened_data() if a > 0]
    return sum(1 for v in px if v < NEAR_SPINE) / len(px)


def with_rim(im):
    w, h = im.size
    src = im.load()
    out = im.copy()
    dst = out.load()
    for y in range(h):
        for x in range(w):
            if src[x, y][3] == 0:
                near = [(x + dx, y + dy) for dx in (-1, 0, 1) for dy in (-1, 0, 1) if (dx or dy)]
                if any(0 <= nx < w and 0 <= ny < h and src[nx, ny][3] > 0 for nx, ny in near):
                    dst[x, y] = RIM + (255,)
    return out


def save_palette(im, path):
    px = [(0, 0, 0, 0) if a == 0 else (r, g, b, 255) for r, g, b, a in im.get_flattened_data()]
    cols = sorted(set(px), key=lambda c: c[3])
    assert len(cols) <= 256, (path, len(cols))
    index = {c: i for i, c in enumerate(cols)}
    p = Image.new("P", im.size)
    p.putdata([index[c] for c in px])
    pal = []
    for c in cols:
        pal += c[:3]
    p.putpalette(pal)
    p.save(path, optimize=True, transparency=bytes(c[3] for c in cols))


if __name__ == "__main__":
    d, ids = sys.argv[1], sys.argv[2:]
    for f in sorted(glob.glob(os.path.join(d, "*.png"))):
        sid = os.path.basename(f)[:-4]
        im = Image.open(f).convert("RGBA")
        share = dark_share(im)
        if (ids and sid not in ids) or (not ids and share < DARK_SHARE):
            continue
        # The rim pixels already there (a second run) don't make the picture lighter in the count: skip it.
        if any((r, g, b) == RIM and a for r, g, b, a in im.get_flattened_data()):
            print(sid, "has a rim already")
            continue
        # The frame may need a pixel on each side: grow the canvas by one.
        big = Image.new("RGBA", (im.width + 2, im.height + 2), (0, 0, 0, 0))
        big.paste(im, (1, 1))
        out = with_rim(big)
        save_palette(out, f)
        print(sid, "dark share %.0f%%" % (share * 100), "->", out.size)
