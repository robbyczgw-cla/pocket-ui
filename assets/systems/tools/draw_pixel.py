"""draw_pixel.py <out_dir>: draws the pictures of the systems that have no hardware to photograph, straight in pixels.

dos.png        two 3.5" floppy disks (DOS)
scummvm.png    a CRT monitor showing a plain adventure scene with a pointer (ScummVM)
tic-80.png     a small handheld computer whose screen shows a 16-colour palette (TIC-80)
streaming.png  a monitor with radio waves and a handheld (the "PC streaming" row)

Nothing is copied from a product or a photo: no logos, no lettering. The look follows the converted photos next to
them: a dark outline of one pixel, three or four tones per material, binary transparency. Run rim.py afterwards
(it gives a picture that is dark all over a light rim)."""
import math, os, sys
from PIL import Image

OUT = (0x16, 0x17, 0x1f)  # outline


class Pic:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [[None] * w for _ in range(h)]

    def rect(self, x0, y0, x1, y1, c):
        for y in range(max(0, y0), min(self.h, y1)):
            for x in range(max(0, x0), min(self.w, x1)):
                self.px[y][x] = c

    def put(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = c

    def disc(self, cx, cy, r, c):
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r + r:
                    self.put(x, y, c)

    def line(self, x0, y0, x1, y1, c, t=1):
        n = max(abs(x1 - x0), abs(y1 - y0), 1)
        for i in range(n + 1):
            x, y = x0 + (x1 - x0) * i // n, y0 + (y1 - y0) * i // n
            self.rect(x, y, x + t, y + t, c)

    def outline(self):
        """One dark pixel round every solid one (the silhouette), 8-connected."""
        solid = [[p is not None for p in row] for row in self.px]
        for y in range(self.h):
            for x in range(self.w):
                if solid[y][x]:
                    continue
                if any(0 <= x + dx < self.w and 0 <= y + dy < self.h and solid[y + dy][x + dx] for dx in (-1, 0, 1) for dy in (-1, 0, 1)):
                    self.px[y][x] = OUT

    def image(self):
        im = Image.new("RGBA", (self.w, self.h), (0, 0, 0, 0))
        for y in range(self.h):
            for x in range(self.w):
                if self.px[y][x] is not None:
                    im.putpixel((x, y), self.px[y][x] + (255,))
        return im


def shade(c, k):
    return tuple(max(0, min(255, int(v * k))) for v in c)


def floppy(p, x, y, body, label=(0xe8, 0xe4, 0xd6), stripe=(0xd0, 0x4a, 0x3c)):
    """Front of a 3.5 inch disk, 100 px square, at x, y."""
    s = 100
    p.rect(x, y, x + s, y + s, body)
    p.rect(x, y, x + s, y + 3, shade(body, 1.25))            # light top edge
    p.rect(x + s - 3, y, x + s, y + s, shade(body, 0.75))    # shaded right edge
    p.rect(x, y + s - 3, x + s, y + s, shade(body, 0.75))
    p.rect(x + 20, y, x + 74, y + 34, (0xb8, 0xbc, 0xc6))    # metal shutter
    p.rect(x + 20, y, x + 74, y + 3, (0xe2, 0xe5, 0xec))
    p.rect(x + 20, y + 31, x + 74, y + 34, (0x8a, 0x8f, 0x9c))
    p.rect(x + 54, y + 6, x + 66, y + 28, (0x3b, 0x3f, 0x4a))  # the window in it
    p.rect(x + 56, y + 6, x + 58, y + 28, (0x5a, 0x5f, 0x6c))
    p.rect(x + 14, y + 44, x + 86, y + 92, label)            # blank label
    p.rect(x + 14, y + 44, x + 86, y + 48, stripe)
    for r in range(3):
        p.rect(x + 22, y + 60 + r * 9, x + 78 - r * 8, y + 62 + r * 9, shade(label, 0.72))
    p.rect(x + 14, y + 90, x + 86, y + 92, shade(label, 0.8))
    p.rect(x + 8, y + 84, x + 14, y + 92, shade(body, 0.6))  # write protect notch


def dos():
    p = Pic(150, 130)
    floppy(p, 38, 6, (0x8d, 0x92, 0x9f), stripe=(0x3d, 0x7b, 0xc4))       # the one behind, grey
    floppy(p, 8, 24, (0x2d, 0x3a, 0x6b), stripe=(0xd0, 0x4a, 0x3c))        # the one in front, navy
    p.outline()
    return p


def crt(p, x, y, w, h, screen_draw):
    """A beige CRT monitor with a deep back, its screen drawn by screen_draw(x0, y0, x1, y1)."""
    beige = (0xc9, 0xc2, 0xa8)
    # the tube's back, in perspective behind the case
    p.rect(x + 20, y + h - 4, x + w - 18, y + h + 16, shade(beige, 0.78))
    p.rect(x + 30, y + h + 14, x + w - 30, y + h + 22, shade(beige, 0.62))
    p.rect(x, y, x + w, y + h, beige)
    p.rect(x, y, x + w, y + 4, shade(beige, 1.12))
    p.rect(x, y, x + 4, y + h, shade(beige, 1.06))
    p.rect(x + w - 5, y + 4, x + w, y + h, shade(beige, 0.8))
    p.rect(x, y + h - 5, x + w, y + h, shade(beige, 0.74))
    # bezel and glass
    bz = 11
    p.rect(x + bz - 3, y + bz - 3, x + w - bz + 3, y + h - 20 + 3, (0x4a, 0x47, 0x40))
    p.rect(x + bz, y + bz, x + w - bz, y + h - 20, (0x1b, 0x1f, 0x2b))
    screen_draw(x + bz + 3, y + bz + 3, x + w - bz - 3, y + h - 23)
    # a glint on the glass
    for i in range(10):
        p.put(x + bz + 6 + i, y + bz + 5, (0xf2, 0xf4, 0xfa))
        p.put(x + bz + 5, y + bz + 6 + i, (0xf2, 0xf4, 0xfa))
    # controls under the screen
    p.rect(x + w - 40, y + h - 14, x + w - 14, y + h - 8, shade(beige, 0.7))
    p.disc(x + w - 52, y + h - 11, 3, (0x4f, 0xb3, 0x6a))


def scummvm():
    p = Pic(180, 150)

    def scene(x0, y0, x1, y1):
        h = y1 - y0
        for y in range(y0, y1):  # sky, dusk to light
            t = (y - y0) / h
            p.rect(x0, y, x1, y + 1, (int(0x4a + 0x7a * t), int(0x6d + 0x72 * t), int(0xc4 - 0x20 * t)))
        p.disc(x1 - 22, y0 + 16, 8, (0xff, 0xe0, 0x7a))
        # far and near hills
        for x in range(x0, x1):
            far = y0 + int(h * 0.52) + int(7 * math.sin((x - x0) / 17.0))
            p.rect(x, far, x + 1, y1, (0x3d, 0x7a, 0x5e))
        for x in range(x0, x1):
            near = y0 + int(h * 0.78) - int(10 * abs(((x - x0) % 80) - 40) / 40)
            p.rect(x, near, x + 1, y1, (0x2a, 0x5c, 0x3f))
        p.rect(x0, y1 - 4, x1, y1, (0x1f, 0x44, 0x30))
        # a plain row of verb bars (no words), as an adventure's interface has
        for i in range(4):
            p.rect(x0 + 4 + i * 28, y1 - 11, x0 + 28 + i * 28, y1 - 6, (0xe6, 0xd0, 0x78))
            p.rect(x0 + 4 + i * 28, y1 - 7, x0 + 28 + i * 28, y1 - 6, (0xa8, 0x90, 0x40))
        # the pointer: a white arrow with a dark edge
        ax, ay = x0 + 56, y0 + 34
        arrow = [(0, 0), (0, 15), (4, 11), (7, 17), (10, 16), (7, 10), (12, 10)]
        mask = [[0] * 14 for _ in range(19)]
        for row in range(17):
            for col in range(13):
                # inside the polygon, by even-odd on its edges
                pts = arrow
                n, inside = len(pts), False
                j = n - 1
                for i in range(n):
                    xi, yi = pts[i]
                    xj, yj = pts[j]
                    if (yi > row) != (yj > row) and col < (xj - xi) * (row - yi) / (yj - yi + 1e-9) + xi:
                        inside = not inside
                    j = i
                mask[row][col] = inside
        for row in range(19):
            for col in range(14):
                if mask[row][col]:
                    for dx in (-1, 0, 1):
                        for dy in (-1, 0, 1):
                            if not (0 <= row + dy < 19 and 0 <= col + dx < 14 and mask[row + dy][col + dx]):
                                p.put(ax + col + dx, ay + row + dy, (0x14, 0x14, 0x1c))
        for row in range(19):
            for col in range(14):
                if mask[row][col]:
                    p.put(ax + col, ay + row, (0xfa, 0xfa, 0xfc))

    crt(p, 6, 4, 168, 112, scene)
    p.outline()
    return p


def tic80():
    p = Pic(170, 130)
    body = (0x4b, 0x4e, 0x78)
    p.rect(8, 8, 162, 118, body)
    p.rect(8, 8, 162, 12, shade(body, 1.3))
    p.rect(8, 8, 12, 118, shade(body, 1.15))
    p.rect(158, 12, 162, 118, shade(body, 0.72))
    p.rect(8, 114, 162, 118, shade(body, 0.66))
    # screen with a 16 colour palette strip, like a fantasy computer's
    p.rect(22, 18, 148, 82, (0x12, 0x14, 0x24))
    pal = [(0x1a, 0x1c, 0x2c), (0x5d, 0x27, 0x5d), (0xb1, 0x3e, 0x53), (0xef, 0x7d, 0x57), (0xff, 0xcd, 0x75), (0xa7, 0xf0, 0x70), (0x38, 0xb7, 0x64), (0x25, 0x71, 0x79),
           (0x29, 0x36, 0x6f), (0x3b, 0x5d, 0xc9), (0x41, 0xa6, 0xf6), (0x73, 0xef, 0xf7), (0xf4, 0xf4, 0xf4), (0x94, 0xb0, 0xc2), (0x56, 0x6c, 0x86), (0x33, 0x3c, 0x57)]
    for i, c in enumerate(pal):
        cx, cy = 26 + (i % 8) * 15, 22 + (i // 8) * 14
        p.rect(cx, cy, cx + 13, cy + 12, c)
    # a little sprite under the palette: a plain smiling block
    p.rect(62, 52, 92, 78, (0xff, 0xcd, 0x75))
    p.rect(68, 58, 72, 63, (0x1a, 0x1c, 0x2c))
    p.rect(82, 58, 86, 63, (0x1a, 0x1c, 0x2c))
    p.rect(68, 70, 86, 73, (0xb1, 0x3e, 0x53))
    # d-pad and two buttons
    p.rect(28, 92, 52, 100, (0x23, 0x24, 0x3a))
    p.rect(36, 86, 44, 110, (0x23, 0x24, 0x3a))
    p.rect(36, 92, 44, 100, (0x35, 0x37, 0x55))
    p.disc(122, 100, 6, (0xb1, 0x3e, 0x53))
    p.disc(140, 94, 6, (0xef, 0x7d, 0x57))
    p.rect(72, 100, 100, 104, (0x2b, 0x2d, 0x46))
    p.outline()
    return p


def streaming():
    p = Pic(190, 130)
    # monitor on a stand
    bez = (0x2c, 0x30, 0x42)
    p.rect(10, 12, 118, 82, bez)
    p.rect(10, 12, 118, 15, shade(bez, 1.6))
    p.rect(115, 15, 118, 82, shade(bez, 0.7))
    for y in range(17, 78):  # screen, a cool gradient
        t = (y - 17) / 60
        p.rect(14, y, 114, y + 1, (int(0x3c + 0x30 * t), int(0x7c - 0x20 * t), int(0xd0 - 0x30 * t)))
    for i in range(14):  # a play triangle
        p.rect(52 + i, 33 + i, 53 + i + 1, 33 + 30 - i, (0xf6, 0xf7, 0xfb))
    p.rect(52, 33, 54, 63, (0xf6, 0xf7, 0xfb))
    p.rect(55, 82, 73, 98, shade(bez, 1.25))
    p.rect(40, 98, 88, 105, shade(bez, 1.35))
    p.rect(40, 98, 88, 100, shade(bez, 1.8))
    # radio waves, three arcs going right
    for r, c in ((12, (0xe6, 0xd0, 0x78)), (20, (0xe6, 0xd0, 0x78)), (28, (0xe6, 0xd0, 0x78))):
        for a in range(-52, 53, 2):
            x = 128 + int(r * math.cos(math.radians(a)))
            y = 52 + int(r * math.sin(math.radians(a)))
            p.rect(x, y, x + 3, y + 3, c)
    p.disc(124, 52, 3, (0xe6, 0xd0, 0x78))
    # a small handheld at the right
    body = (0x3a, 0x3d, 0x52)
    p.rect(162, 34, 188, 100, body)
    p.rect(162, 34, 188, 37, shade(body, 1.5))
    p.rect(185, 37, 188, 100, shade(body, 0.7))
    p.rect(165, 40, 185, 62, (0x4c, 0x9a, 0xe0))
    p.disc(170, 80, 4, (0x22, 0x24, 0x34))
    p.disc(180, 78, 3, (0xd0, 0x4a, 0x3c))
    p.disc(180, 88, 3, (0xe6, 0xd0, 0x78))
    p.outline()
    return p


if __name__ == "__main__":
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    for name, f in (("pc", dos), ("scummvm", scummvm), ("tic-80", tic80), ("streaming", streaming)):
        pic = f()
        pic.image().save(os.path.join(out, name + ".png"))
        print(name, pic.w, pic.h)
