"""draw_devices.py <out_dir>: the handhelds pocket-ui runs on, drawn straight in pixels, front view.

device-miniv2.png  Retroid Pocket Mini V2 (also the picture of the Controls page, with anchors.json)
device-miniv1.png  Retroid Pocket Mini (V1): the same shell with a 4:3 screen
device-rp5.png     Retroid Pocket 5
device-flip2.png   Retroid Pocket Flip 2, opened
device-rp6.png     Retroid Pocket 6
device-nova.png    Retroid Pocket Nova

Drawn for this project from the devices' published proportions (size, screen size and aspect, where the D-pad,
face buttons and sticks sit). No photo was traced and nothing is copied: no logos, no lettering, no button letters.
The screens show pocket-ui's own shelf. Same look as assets/systems/tools/draw_pixel.py: a dark outline of one pixel,
three or four tones a material, binary transparency, a light rim where the shell is dark.
anchors.json: where each control of the Mini V2 is, in the picture's pixels, for the labels the UI draws."""
import json, os, sys
from PIL import Image

OUT = (0x16, 0x17, 0x1f)
RIM = (0x8c, 0x93, 0xa3)


class Pic:
    def __init__(self, w, h):
        self.w, self.h = w, h
        self.px = [[None] * w for _ in range(h)]

    def put(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[y][x] = c

    def get(self, x, y):
        return self.px[y][x] if 0 <= x < self.w and 0 <= y < self.h else None

    def rect(self, x0, y0, x1, y1, c):
        for y in range(y0, y1):
            for x in range(x0, x1):
                self.put(x, y, c)

    def rrect(self, x0, y0, x1, y1, r, c):
        for y in range(y0, y1):
            for x in range(x0, x1):
                dx = max(x0 + r - x - 0.5, 0, x - (x1 - r) + 0.5)
                dy = max(y0 + r - y - 0.5, 0, y - (y1 - r) + 0.5)
                if dx * dx + dy * dy <= r * r:
                    self.put(x, y, c)

    def disc(self, cx, cy, r, c):
        for y in range(int(cy - r) - 1, int(cy + r) + 2):
            for x in range(int(cx - r) - 1, int(cx + r) + 2):
                if (x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2 <= r * r:
                    self.put(x, y, c)

    def ring_shade(self, cx, cy, r, hi, lo):
        """Light upper-left edge, dark lower-right edge of a disc already painted."""
        for y in range(int(cy - r) - 1, int(cy + r) + 2):
            for x in range(int(cx - r) - 1, int(cx + r) + 2):
                d = ((x + 0.5 - cx) ** 2 + (y + 0.5 - cy) ** 2) ** 0.5
                if r - 1.2 < d <= r:
                    self.put(x, y, hi if (x + 0.5 - cx) + (y + 0.5 - cy) < 0 else lo)

    def outline(self, colour=OUT):
        solid = [[p is not None for p in row] for row in self.px]
        for y in range(self.h):
            for x in range(self.w):
                if not solid[y][x] and any(0 <= x + dx < self.w and 0 <= y + dy < self.h and solid[y + dy][x + dx]
                                           for dx in (-1, 0, 1) for dy in (-1, 0, 1)):
                    self.px[y][x] = colour

    def image(self):
        im = Image.new("RGBA", (self.w, self.h), (0, 0, 0, 0))
        for y in range(self.h):
            for x in range(self.w):
                if self.px[y][x] is not None:
                    im.putpixel((x, y), self.px[y][x] + (255,))
        return im


def shade(c, k):
    return tuple(max(0, min(255, int(v * k))) for v in c)


def shelf_screen(p, x0, y0, x1, y1):
    """pocket-ui's shelf in miniature: black, a pulled-out label, spines with coloured stripes."""
    p.rect(x0, y0, x1, y1, (0x05, 0x05, 0x07))
    w, h = x1 - x0, y1 - y0
    m = max(2, w // 14)
    y = y0 + max(3, h // 9)
    lab_h = max(6, h // 4)
    p.rect(x0 + m, y, x1 - m, y + lab_h, (0xec, 0xe9, 0xe1))
    p.rect(x0 + m, y, x0 + m + max(1, w // 40), y + lab_h, (0x5b, 0x4a, 0xa6))
    p.rect(x0 + m + w // 12, y + lab_h // 3, x0 + m + w // 2, y + lab_h // 3 + max(1, lab_h // 5), (0x15, 0x15, 0x15))
    p.rect(x0 + m + w // 12, y + lab_h * 2 // 3, x0 + m + w // 3, y + lab_h * 2 // 3 + 1, (0x8a, 0x87, 0x7f))
    y += lab_h + max(2, h // 20)
    stripes = [(0xc8, 0x10, 0x2e), (0x2e, 0x8c, 0x9a), (0x8a, 0x9a, 0x3b), (0x7a, 0x6c, 0xb0), (0x9a, 0xa3, 0xad)]
    sp = max(4, h // 9)
    for s in stripes:
        if y + sp - 1 > y1 - 2:
            break
        p.rect(x0 + m, y, x1 - m, y + sp - 1, (0x1f, 0x1f, 0x22))
        p.rect(x0 + m, y, x0 + m + max(1, w // 40), y + sp - 1, s)
        p.rect(x0 + m + w // 12, y + sp // 2 - 1, x0 + m + w // 12 + w // 4 + (len(s) * 7 % 11), y + sp // 2, (0xd9, 0xd6, 0xce))
        y += sp


def dpad(p, cx, cy, s, col):
    a, b = s // 3 + 1, s
    p.rrect(cx - a, cy - b, cx + a + 1, cy + b + 1, 2, col)
    p.rrect(cx - b, cy - a, cx + b + 1, cy + a + 1, 2, col)
    hi, lo = shade(col, 1.35), shade(col, 0.6)
    p.rect(cx - a + 1, cy - b + 1, cx + a, cy - b + 2, hi)
    p.rect(cx - b + 1, cy - a + 1, cx - b + 2, cy + a, hi)
    p.rect(cx - a + 1, cy + b - 1, cx + a, cy + b, lo)
    p.rect(cx + b - 1, cy - a + 1, cx + b, cy + a, lo)
    p.disc(cx + 0.5, cy + 0.5, max(1.5, a / 2), shade(col, 0.8))


def stick(p, cx, cy, r, well, cap, led):
    p.disc(cx, cy, r, well)
    if led:
        p.disc(cx, cy, r - 1, led)
        p.disc(cx, cy, r - 2.5, well)
    p.disc(cx, cy, r * 0.62, cap)
    p.ring_shade(cx, cy, r * 0.62, shade(cap, 1.25), shade(cap, 0.7))
    p.disc(cx - r * 0.12, cy - r * 0.12, r * 0.25, shade(cap, 1.1))


def face(p, cx, cy, gap, r, cols):
    """North, east, south, west: X, A, B, Y in the Nintendo layout the Mini V2 prints."""
    pos = {"X": (cx, cy - gap), "A": (cx + gap, cy), "B": (cx, cy + gap), "Y": (cx - gap, cy)}
    for k, (x, y) in pos.items():
        c = cols[k]
        p.disc(x, y, r, c)
        p.ring_shade(x, y, r, shade(c, 1.3), shade(c, 0.6))
    return pos


def grille(p, x, y, w, h, col):
    for yy in range(y, y + h, 2):
        for xx in range(x + (yy // 2) % 2, x + w, 2):
            p.put(xx, yy, col)


def handheld(spec):
    W, H = spec["w"], spec["h"]
    p = Pic(W + 4, H + 6)
    ox, oy = 2, 4
    body, r = spec["body"], spec["r"]
    # Shoulders peeking over the top edge.
    sh = shade(body, 0.75)
    p.rrect(ox + 6, oy - 3, ox + W * 3 // 10, oy + 4, 3, sh)
    p.rrect(ox + W - W * 3 // 10, oy - 3, ox + W - 6, oy + 4, 3, sh)
    # The shell, with grips that run a little lower at the sides.
    p.rrect(ox, oy, ox + W, oy + H - 3, r, body)
    g = spec.get("grip", 0)
    if g:
        p.rrect(ox, oy + H // 3, ox + W * 9 // 40, oy + H, r, body)
        p.rrect(ox + W - W * 9 // 40, oy + H // 3, ox + W, oy + H, r, body)
    # Light from above: a highlight under the top edge, shadow along the bottom.
    hi, lo = shade(body, 1.22), shade(body, 0.8)
    for y in range(oy, oy + H):
        for x in range(ox, ox + W):
            if p.get(x, y) == body:
                if p.get(x, y - 2) in (None, sh):
                    p.put(x, y, hi)
                elif p.get(x, y + 2) is None:
                    p.put(x, y, lo)
    # Screen: glass bezel and the shelf.
    sx0, sy0, sx1, sy1 = spec["screen"]
    bez = spec.get("bezel", (0x12, 0x13, 0x17))
    p.rrect(ox + sx0 - 3, oy + sy0 - 3, ox + sx1 + 3, oy + sy1 + 3, 3, bez)
    shelf_screen(p, ox + sx0, oy + sy0, ox + sx1, oy + sy1)
    a = {}
    dx, dy, ds = spec["dpad"]
    dpad(p, ox + dx, oy + dy, ds, spec["dpad_col"])
    a["dpad"] = (ox + dx, oy + dy)
    fx, fy, fg, fr = spec["face"]
    for k, v in face(p, ox + fx, oy + fy, fg, fr, spec["face_cols"]).items():
        a[k] = (round(v[0]), round(v[1]))
    for name, (x, y) in (("lstick", spec["lstick"]), ("rstick", spec["rstick"])):
        stick(p, ox + x, oy + y, spec["stick_r"], spec["well"], spec["cap"], spec.get("led"))
        a[name] = (ox + x, oy + y)
    for name, (x, y) in spec["small"].items():
        p.disc(ox + x, oy + y, 2.6, shade(body, 0.55))
        p.disc(ox + x - 0.4, oy + y - 0.4, 1.6, shade(body, 1.45))
        a[name] = (ox + x, oy + y)
    for (x, y, w, h) in spec.get("grilles", []):
        grille(p, ox + x, oy + y, w, h, shade(body, 0.55))
    a["L"] = (ox + W * 3 // 20, oy - 1)
    a["R"] = (ox + W - W * 3 // 20, oy - 1)
    p.outline()
    if spec.get("rim"):
        p.outline(RIM)
    return p, a


def flip(spec):
    """The Flip 2 opened: the lid with the screen above, the base with the controls below, seen straight on."""
    W = spec["w"]
    lid = handheld({**spec["lid"], "w": W})[0]
    base, a = handheld({**spec["base"], "w": W})
    p = Pic(W + 4, lid.h + base.h - 3)
    for y in range(lid.h):
        for x in range(lid.w):
            if lid.px[y][x] is not None:
                p.px[y][x] = lid.px[y][x]
    for y in range(base.h):
        for x in range(base.w):
            if base.px[y][x] is not None:
                p.px[y + lid.h - 3][x] = base.px[y][x]
    # The hinge.
    hy = lid.h - 4
    p.rect(W // 6, hy - 1, W - W // 6, hy + 3, shade(spec["lid"]["body"], 0.6))
    return p, {k: (x, y + lid.h - 3) for k, (x, y) in a.items()}


GREY = (0x4a, 0x4e, 0x58)
ABXY_DARK = {k: (0x2c, 0x2e, 0x35) for k in "XABY"}
CAP = (0x55, 0x59, 0x63)
WELL = (0x24, 0x26, 0x2c)
TEAL = (0x4f, 0xa8, 0xb0)

# Mini V2: 165 x 78 mm, a 3.92" 31:27 screen (75 x 65 mm): 1.45 px a millimetre.
MINI = dict(w=240, h=113, r=20, grip=1, body=GREY, rim=True,
            screen=(66, 9, 174, 103), dpad=(33, 30, 11), dpad_col=(0x2c, 0x2e, 0x35),
            face=(207, 30, 10, 4.6), face_cols=ABXY_DARK,
            lstick=(36, 69), rstick=(204, 69), stick_r=12.5, well=WELL, cap=CAP, led=TEAL,
            small={"select": (21, 10), "start": (219, 10), "home": (21, 99), "back": (219, 99)},
            grilles=[(30, 96, 14, 5), (196, 96, 14, 5)])
# Mini V1: the same shell, a 3.7" 4:3 screen.
MINI1 = {**MINI, "screen": (61, 12, 179, 101), "body": (0x3c, 0x3f, 0x47)}
# Pocket 5: 199 x 78 mm, 5.5" 16:9 (121 x 68 mm).
RP5 = dict(w=290, h=113, r=22, grip=1, body=(0x5c, 0x60, 0x6a), rim=True,
           screen=(57, 7, 233, 106), dpad=(30, 30, 11), dpad_col=(0x3a, 0x3d, 0x45),
           face=(259, 30, 10, 4.6), face_cols={k: (0x3a, 0x3d, 0x45) for k in "XABY"},
           lstick=(30, 72), rstick=(259, 72), stick_r=12.5, well=WELL, cap=CAP, led=TEAL,
           small={"select": (17, 8), "start": (273, 8), "home": (17, 101), "back": (273, 101)},
           grilles=[(24, 99, 12, 5), (254, 99, 12, 5)])
# Pocket 6: 5.5" 16:9, light shell, sticks on top, coloured face buttons.
RP6 = dict(w=290, h=118, r=24, grip=1, body=(0xc9, 0xc4, 0xb8), bezel=(0x26, 0x27, 0x2c),
           screen=(58, 8, 232, 106), dpad=(31, 76, 11), dpad_col=(0x6a, 0x6c, 0x72),
           face=(259, 34, 10, 4.8),
           face_cols={"X": (0x3b, 0x5b, 0xc4), "A": (0xc0, 0x3a, 0x3a), "B": (0xd8, 0xb0, 0x3a), "Y": (0x4a, 0x9a, 0x4e)},
           lstick=(31, 34), rstick=(259, 76), stick_r=12.5, well=(0x8a, 0x86, 0x7c), cap=(0x6a, 0x6c, 0x72), led=(0x8a, 0xa8, 0xe0),
           small={"select": (19, 9), "start": (271, 9), "home": (19, 105), "back": (271, 105)},
           grilles=[(26, 101, 12, 5), (252, 101, 12, 5)])
# Nova: 4.5" 4:3, a compact navy shell, sticks below, coloured face buttons.
NOVA = dict(w=236, h=112, r=16, grip=0, body=(0x3c, 0x45, 0x8a),
            screen=(52, 6, 184, 105), dpad=(27, 28, 10), dpad_col=(0xc9, 0xc6, 0xd2),
            face=(209, 28, 9, 4.4),
            face_cols={"X": (0xc9, 0xc6, 0xd2), "A": (0x3e, 0x9a, 0x7a), "B": (0xc0, 0x3a, 0x3a), "Y": (0xd8, 0xb8, 0x50)},
            lstick=(27, 70), rstick=(209, 70), stick_r=11.5, well=(0x2a, 0x30, 0x60), cap=(0xc9, 0xc6, 0xd2), led=None,
            small={"select": (15, 7), "start": (221, 7), "home": (15, 102), "back": (221, 102)},
            grilles=[(20, 98, 12, 5), (204, 98, 12, 5)])
# Flip 2: 5.5" 16:9 lid over a base with the controls.
FLIP_LID = dict(h=82, r=12, grip=0, body=(0x4a, 0x4e, 0x58), rim=True, screen=(18, 6, 162, 77),
                dpad=(-50, -50, 1), dpad_col=GREY, face=(-50, -50, 1, 1), face_cols=ABXY_DARK,
                lstick=(-50, -50), rstick=(-50, -50), stick_r=1, well=WELL, cap=CAP, small={})
FLIP_BASE = dict(h=82, r=12, grip=0, body=(0x4a, 0x4e, 0x58), rim=True, screen=(-60, -60, -59, -59),
                 dpad=(32, 24, 11), dpad_col=(0x2c, 0x2e, 0x35), face=(148, 24, 10, 4.6), face_cols=ABXY_DARK,
                 lstick=(62, 56), rstick=(118, 56), stick_r=12, well=WELL, cap=CAP, led=TEAL,
                 small={"select": (78, 16), "start": (102, 16), "home": (18, 70), "back": (162, 70)},
                 grilles=[])


def main():
    out = sys.argv[1]
    anchors = {}
    for name, spec in (("miniv2", MINI), ("miniv1", MINI1), ("rp5", RP5), ("rp6", RP6), ("nova", NOVA)):
        p, a = handheld(spec)
        p.image().save(os.path.join(out, f"device-{name}.png"))
        if name == "miniv2":
            anchors = a
    p, _ = flip(dict(w=180, lid=FLIP_LID, base=FLIP_BASE))
    p.image().save(os.path.join(out, "device-flip2.png"))
    json.dump(anchors, open(os.path.join(out, "anchors.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
