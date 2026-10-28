"""fix.py <ops.json> <in_dir> <out_dir> [id ...]

Per-system clean-up of the cut-out pixel art: paint over logos and brand text with the surrounding
plastic, strip halo left from the photo backdrop. ops.json maps a system id to a list of operations:

  {"op": "key",    "box": [x0,y0,x1,y1], "sel": SEL}   repaint the selected pixels from their neighbours
  {"op": "clear",  "box": [...], "sel": SEL}            make the selected pixels transparent
  {"op": "dehalo", "box": [...], "min": 200, "sat": 40} flood from transparency over light, grey pixels
  {"op": "set",    "box": [...], "sel": SEL, "color": [r,g,b]} the selected pixels take one colour
  {"op": "specks"}

"key", "clear" and "set" take an optional "poly": [[x,y], ...]: only pixels inside it count (a slanted strip).                                      drop pixels hanging on by one neighbour, and tiny islands

SEL: "red", "blue", "yellow", "teal", "light:T" (every channel >= T), "dark:T" (every channel <= T),
"grey:T" (a neutral grey or white, every channel >= T), "odd:T" (differs from the box's most common colour by more than T), "near:R,G,B:T", "all".
Boxes are inclusive-exclusive: x0 <= x < x1."""
import json, os, sys
from collections import Counter, deque
from PIL import Image, ImageDraw

N4 = ((1, 0), (-1, 0), (0, 1), (0, -1))
N8 = N4 + ((1, 1), (1, -1), (-1, 1), (-1, -1))


def d(a, b):
    return max(abs(a[0] - b[0]), abs(a[1] - b[1]), abs(a[2] - b[2]))


def selector(sel, px, box):
    kind, _, arg = sel.partition(":")
    if kind == "red":
        return lambda c: c[0] > c[1] + 60 and c[0] > c[2] + 50
    if kind == "blue":
        return lambda c: c[2] > c[0] + 40 and c[2] > c[1] + 15
    if kind == "yellow":
        return lambda c: c[0] > 150 and c[1] > 110 and c[2] < 110
    if kind == "teal":
        return lambda c: c[1] > c[0] + 30 and c[2] > c[0] + 30
    if kind == "light":
        t = int(arg)
        return lambda c: min(c[:3]) >= t
    if kind == "dark":
        t = int(arg)
        return lambda c: max(c[:3]) <= t
    if kind == "odd":
        t = int(arg)
        x0, y0, x1, y1 = box
        mode = Counter(px[x, y][:3] for x in range(x0, x1) for y in range(y0, y1) if px[x, y][3]).most_common(1)[0][0]
        return lambda c: d(c, mode) > t
    if kind == "near":
        rgb, t = arg.split(":")
        ref, t = tuple(int(v) for v in rgb.split(",")), int(t)
        return lambda c: d(c, ref) <= t
    if kind == "all":
        return lambda c: True
    if kind == "warm":
        return lambda c: c[0] > c[1] + 25 and c[0] > c[2] + 20
    if kind == "bluish":
        return lambda c: c[2] > c[0] + 12
    if kind == "lum":
        t = int(arg)
        return lambda c: (c[0] + c[1] + c[2]) / 3 >= t
    if kind == "grey":
        t = int(arg)
        return lambda c: max(c[:3]) - min(c[:3]) < 30 and min(c[:3]) >= t
    if kind == "screen":
        return lambda c: 80 <= c[1] <= 170 and max(c[:3]) - min(c[:3]) < 60 and max(c[:3]) < 190
    raise ValueError(sel)


def boxed(im, box):
    x0, y0, x1, y1 = box
    return max(0, x0), max(0, y0), min(im.width, x1), min(im.height, y1)


def poly_mask(im, o):
    """With "poly": [[x, y], ...] in the op, only pixels inside that polygon (a slanted strip) are touched."""
    if "poly" not in o:
        return lambda x, y: True
    m = Image.new("L", im.size, 0)
    ImageDraw.Draw(m).polygon([tuple(p) for p in o["poly"]], fill=255)
    mp = m.load()
    return lambda x, y: mp[x, y] > 0


def op_key(im, px, o, clear=False):
    box = boxed(im, o["box"])
    f, inside = selector(o["sel"], px, box), poly_mask(im, o)
    x0, y0, x1, y1 = box
    S = {(x, y) for x in range(x0, x1) for y in range(y0, y1) if px[x, y][3] and f(px[x, y]) and inside(x, y)}
    if clear:
        for p in S:
            px[p] = (0, 0, 0, 0)
        return len(S)
    n = len(S)
    while S:
        done = {}
        for x, y in S:
            nb = [px[x + dx, y + dy] for dx, dy in N8 if 0 <= x + dx < im.width and 0 <= y + dy < im.height
                  and (x + dx, y + dy) not in S and px[x + dx, y + dy][3]]
            if nb:
                done[(x, y)] = Counter(nb).most_common(1)[0][0]
        if not done:
            break
        for p, c in done.items():
            px[p] = c
            S.discard(p)
    return n


def onion(im, px, S):
    """Repaint the pixels in S from the outside in, each with the most common colour of its settled neighbours."""
    while S:
        done = {}
        for x, y in S:
            nb = [px[x + dx, y + dy] for dx, dy in N8 if 0 <= x + dx < im.width and 0 <= y + dy < im.height
                  and (x + dx, y + dy) not in S and px[x + dx, y + dy][3]]
            if nb:
                done[(x, y)] = Counter(nb).most_common(1)[0][0]
        if not done:
            break
        for p, c in done.items():
            px[p] = c
            S.discard(p)


def op_above(im, px, o):
    """Per column: find the anchor surface (e.g. a screen), the line run directly above it (its frame),
    and repaint every `line`-coloured pixel above that frame within the box (text printed above a frame)."""
    x0, y0, x1, y1 = boxed(im, o["box"])
    anchor, line = selector(o["anchor"], px, (x0, y0, x1, y1)), selector(o["line"], px, (x0, y0, x1, y1))
    S = set()
    for x in range(x0, x1):
        ys = next((y for y in range(y0, y1) if px[x, y][3] and anchor(px[x, y])), None)
        if ys is None:
            continue
        ft = ys
        while ft - 1 >= y0 and line(px[x, ft - 1]):
            ft -= 1
        S |= {(x, y) for y in range(y0, ft) if px[x, y][3] and line(px[x, y])}
    n = len(S)
    onion(im, px, S)
    return n


def op_band(im, px, o):
    """Per column: the first run of `stripe` pixels from the top, then the band down to its last `band` pixel.
    Everything inside the band that is not band-coloured is repainted (text printed on a stripe).
    With "above", pixels between the box top and the stripe that differ from `plain` are repainted too."""
    x0, y0, x1, y1 = boxed(im, o["box"])
    stripe, band = selector(o["stripe"], px, (x0, y0, x1, y1)), selector(o["band"], px, (x0, y0, x1, y1))
    plain = selector(o["plain"], px, (x0, y0, x1, y1)) if "plain" in o else None
    S = set()
    for x in range(x0, x1):
        col = [y for y in range(y0, y1) if px[x, y][3]]
        rs = next((y for y in col if stripe(px[x, y])), None)
        if rs is None:
            continue
        re_ = rs
        while re_ + 1 < y1 and stripe(px[x, re_ + 1]):
            re_ += 1
        bl = [y for y in col if y > re_ and band(px[x, y])]
        if bl:
            S |= {(x, y) for y in range(re_ + 1, max(bl) + 1) if not band(px[x, y])}
        if plain:
            S |= {(x, y) for y in range(y0, rs) if px[x, y][3] and not plain(px[x, y])}
    n = len(S)
    onion(im, px, S)
    return n


def op_dehalo(im, px, o):
    x0, y0, x1, y1 = boxed(im, o["box"])
    t, s = o.get("min", 200), o.get("sat", 40)
    ok = lambda c: c[3] and min(c[:3]) >= t and max(c[:3]) - min(c[:3]) <= s
    q = deque()
    for x in range(x0, x1):
        for y in range(y0, y1):
            if px[x, y][3] and ok(px[x, y]) and any(
                    not (0 <= x + dx < im.width and 0 <= y + dy < im.height) or px[x + dx, y + dy][3] == 0 for dx, dy in N4):
                q.append((x, y))
    n = 0
    while q:
        x, y = q.popleft()
        if not px[x, y][3] or not ok(px[x, y]):
            continue
        px[x, y] = (0, 0, 0, 0)
        n += 1
        for dx, dy in N4:
            a, b = x + dx, y + dy
            if x0 <= a < x1 and y0 <= b < y1 and px[a, b][3] and ok(px[a, b]):
                q.append((a, b))
    return n


def op_specks(im, px, o):
    W, H = im.size
    n = 0
    for _ in range(2):
        kill = [(x, y) for x in range(W) for y in range(H) if px[x, y][3]
                and sum(1 for dx, dy in N4 if 0 <= x + dx < W and 0 <= y + dy < H and px[x + dx, y + dy][3]) <= 1]
        for p in kill:
            px[p] = (0, 0, 0, 0)
        n += len(kill)
    seen = set()
    for x in range(W):
        for y in range(H):
            if not px[x, y][3] or (x, y) in seen:
                continue
            comp, q = [], deque([(x, y)])
            seen.add((x, y))
            while q:
                a, b = q.popleft()
                comp.append((a, b))
                for dx, dy in N4:
                    m = (a + dx, b + dy)
                    if 0 <= m[0] < W and 0 <= m[1] < H and m not in seen and px[m][3]:
                        seen.add(m)
                        q.append(m)
            if len(comp) < o.get("island", 12):
                for p in comp:
                    px[p] = (0, 0, 0, 0)
                n += len(comp)
    return n


def run(sid, ops, src, dst):
    im = Image.open(src).convert("RGBA")
    px = im.load()
    for o in ops:
        kind = o["op"]
        if kind == "key":
            n = op_key(im, px, o)
        elif kind == "clear":
            n = op_key(im, px, o, clear=True)
        elif kind == "dehalo":
            n = op_dehalo(im, px, o)
        elif kind == "set":
            # Selected pixels take one colour; with "inside", only those whose 8 neighbours all match it too.
            box = boxed(im, o["box"])
            f = selector(o["sel"], px, box)
            g = selector(o["inside"], px, box) if "inside" in o else None
            inp = poly_mask(im, o)
            x0, y0, x1, y1 = box
            S = [(x, y) for x in range(x0, x1) for y in range(y0, y1) if px[x, y][3] and f(px[x, y]) and inp(x, y)
                 and (g is None or all(0 <= x + dx < im.width and 0 <= y + dy < im.height and g(px[x + dx, y + dy]) for dx, dy in N8))]
            for p in S:
                px[p] = tuple(o["color"]) + (255,)
            n = len(S)
        elif kind == "above":
            n = op_above(im, px, o)
        elif kind == "band":
            n = op_band(im, px, o)
        elif kind == "specks":
            n = op_specks(im, px, o)
        else:
            raise ValueError(kind)
        print(f"  {sid} {kind} {o.get('box', '')} {o.get('sel', '')}: {n} px")
    full = os.path.join(os.path.dirname(dst), "_full")
    os.makedirs(full, exist_ok=True)
    im.save(os.path.join(full, os.path.basename(dst)))
    im = im.crop(im.getbbox())
    im.save(dst)
    print(sid, im.size)


if __name__ == "__main__":
    ops = json.load(open(sys.argv[1]))
    src, dst = sys.argv[2], sys.argv[3]
    os.makedirs(dst, exist_ok=True)
    for sid in sys.argv[4:] or list(ops):
        run(sid, ops.get(sid, []), os.path.join(src, sid + ".png"), os.path.join(dst, sid + ".png"))
