"""text_art.py: the pictures made from text alone (supergrafx, pokemini), start to finish.

    text_art.py gen <raw_dir> <name> "<description>" <seed> <width> <height>   one PixelLab create-image-pixflux call
    text_art.py make <raw_dir> <out_dir>                                        raw picks -> cut out, cleaned, rimmed, palette PNG

No photo goes in (unlike prep.py -> image-to-pixelart for the other pictures). The key is read from
~/.config/pixellab/api-key and is never printed. Every request is in ../pixellab-assets.json; PICKS says which raw
picture became which file. Needs Pillow."""
import base64, json, os, sys, time, urllib.error, urllib.request
from collections import Counter, deque
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rim

PICKS = {"supergrafx": "t_sg_15", "pokemini": "t_pmL_24"}


def lum(p):
    return 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]


def gen(raw, name, description, seed, w, h):
    key = open(os.path.expanduser("~/.config/pixellab/api-key")).read().strip()
    body = dict(description=description, image_size=dict(width=w, height=h), no_background=True,
                outline="single color black outline", shading="medium shading", detail="medium detail",
                view="high top-down", direction="south", text_guidance_scale=8.0, seed=seed)
    for _ in range(4):
        req = urllib.request.Request("https://api.pixellab.ai/v2/create-image-pixflux", json.dumps(body).encode(),
                                     {"Authorization": "Bearer " + key, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=180) as r:
                data = json.loads(r.read())
            break
        except urllib.error.HTTPError as e:
            if e.code != 429:  # "maximum number of concurrent jobs": wait and retry, the key is shared
                sys.exit(f"HTTP {e.code}")
            time.sleep(30)
    else:
        sys.exit("still busy")
    data = data.get("last_response", data)
    os.makedirs(raw, exist_ok=True)
    open(os.path.join(raw, name + ".png"), "wb").write(base64.b64decode(data["image"]["base64"].split(",")[-1]))
    print(name, data.get("usage"))


def cut_backdrop(im):
    """The big picture comes back with an opaque white backdrop and a light grey drop shadow: flood both away from the border."""
    w, h = im.size
    px = im.load()
    q = deque((x, y) for x in range(w) for y in (0, h - 1))
    q.extend((x, y) for y in range(h) for x in (0, w - 1))
    seen = set()
    while q:
        x, y = q.popleft()
        if not (0 <= x < w and 0 <= y < h) or (x, y) in seen:
            continue
        p = px[x, y]
        if not (lum(p) > 115 and abs(p[0] - p[1]) < 14 and abs(p[1] - p[2]) < 20):
            continue
        seen.add((x, y))
        q.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)])
    for xy in seen:
        px[xy] = (0, 0, 0, 0)
    return im


def inpaint(im, boxes, flag):
    """Repaint the flagged pixels in the boxes with the colour most common round them (the plastic), logos and lettering gone."""
    px = im.load()
    todo = {(x, y) for x0, y0, x1, y1 in boxes for y in range(y0, y1) for x in range(x0, x1) if px[x, y][3] and flag(px[x, y])}
    for _ in range(12):
        new = {}
        for x, y in todo:
            ns = [px[x + dx, y + dy] for dx in (-1, 0, 1) for dy in (-1, 0, 1)
                  if (dx or dy) and 0 <= x + dx < im.width and 0 <= y + dy < im.height and (x + dx, y + dy) not in todo and px[x + dx, y + dy][3]]
            if ns:
                new[(x, y)] = Counter(ns).most_common(1)[0][0]
        for xy, c in new.items():
            px[xy] = c
            todo.discard(xy)
        if not todo:
            break
    return im


def supergrafx(im):
    im = cut_backdrop(im)
    px = im.load()
    # Lettering on the console's lid (white, red) and red specks of it; the pad's two red buttons (x > 180) stay.
    inpaint(im, [(80, 40, 190, 110)], lambda p: (p[0] > 130 and p[1] < 110 and p[2] < 110) or min(p) > 150)
    inpaint(im, [(130, 40, 200, 70)], lambda p: lum(p) > 95)
    im = im.crop(im.getbbox())
    px = im.load()
    for y in range(im.height):  # white left inside the loop of the cable
        for x in range(im.width):
            if px[x, y][3] and min(px[x, y][:3]) > 225:
                px[x, y] = (0, 0, 0, 0)
    inpaint(im, [(60, 30, 140, 85)], lambda p: p[0] - max(p[1], p[2]) > 18)
    return im.crop(im.getbbox())


def pokemini(im):
    """The 256x224 canvas came back on a white backdrop too (the 192x160 ones were transparent but only 134 px big)."""
    return cut_backdrop(im).crop(cut_backdrop(im).getbbox())


def make(raw, out):
    os.makedirs(out, exist_ok=True)
    for sid, pick in PICKS.items():
        im = Image.open(os.path.join(raw, pick + ".png")).convert("RGBA")
        im = {"supergrafx": supergrafx, "pokemini": pokemini}[sid](im)
        if sid == "supergrafx":  # 56 % dark: below rim.py's 85 % rule, but its black outline vanishes on the spine, so by name
            im = _grow(im)
        rim.save_palette(im, os.path.join(out, sid + ".png"))
        print(sid, im.size)


def _grow(im):
    big = Image.new("RGBA", (im.width + 2, im.height + 2), (0, 0, 0, 0))
    big.paste(im, (1, 1))
    return rim.with_rim(big)


if __name__ == "__main__":
    if sys.argv[1] == "gen":
        gen(sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5]), int(sys.argv[6]), int(sys.argv[7]))
    else:
        make(sys.argv[2], sys.argv[3])
