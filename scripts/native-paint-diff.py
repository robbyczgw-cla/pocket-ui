#!/usr/bin/env python3
"""scripts/native-paint-compare.sh's comparison: OUT/scaled/<font>/<tag>/*.png against OUT/native/<font>/<tag>/*.png.

Writes OUT/diff.tsv (share of pixels more than 32 steps apart in any channel, largest step; largest share first),
OUT/problems.diff (the findings files that differ) and two contact sheets of the 40 states that differ most
(old | new | difference x4). Prints a summary. Needs PIL.
"""
import difflib
import sys
from pathlib import Path

from PIL import Image, ImageChops

OUT = Path(sys.argv[1])
FAR = 32


def compare(a, b):
    """(share of pixels more than FAR apart, largest step, difference image) of two RGB pictures of one size."""
    if a.size != b.size:
        return 1.0, 255, None
    d = ImageChops.difference(a, b)
    # The largest of the three channels per pixel.
    r, g, bl = d.split()
    m = ImageChops.lighter(ImageChops.lighter(r, g), bl)
    hist = m.histogram()
    far = sum(hist[FAR + 1:])
    top = max((i for i, n in enumerate(hist) if n), default=0)
    return far / (a.size[0] * a.size[1]), top, m


rows = []
missing = []
for old_dir in sorted((OUT / "scaled").glob("*/*")):
    if not old_dir.is_dir():
        continue
    font, tag = old_dir.parent.name, old_dir.name
    new_dir = OUT / "native" / font / tag
    for old in sorted(old_dir.glob("*.png")):
        new = new_dir / old.name
        if not new.exists():
            missing.append(f"{font}/{tag}/{old.name}")
            continue
        a, b = Image.open(old).convert("RGB"), Image.open(new).convert("RGB")
        share, top, _ = compare(a, b)
        rows.append((share, top, font, tag, old.stem, old, new))

rows.sort(key=lambda r: (-r[0], -r[1]))
with open(OUT / "diff.tsv", "w") as f:
    f.write("share_over_32\tmax_step\tfont\tsetting\tstate\n")
    for share, top, font, tag, name, _, _ in rows:
        f.write(f"{share:.5f}\t{top}\t{font}\t{tag}\t{name}\n")

# Findings: the same lines for the same states, else the layout moved.
changed = []
for old in sorted((OUT / "scaled").glob("*/*.problems")):
    new = OUT / "native" / old.parent.name / old.name
    a = old.read_text().splitlines()
    b = new.read_text().splitlines() if new.exists() else []
    if a != b:
        changed += list(difflib.unified_diff(a, b, f"scaled/{old.parent.name}/{old.name}", f"native/{old.parent.name}/{old.name}", lineterm=""))
(OUT / "problems.diff").write_text("\n".join(changed) + ("\n" if changed else ""))

# Contact sheets: the 40 that differ most, each old | new | difference (x4, so small steps show).
worst = [r for r in rows if r[0] > 0 or r[1] > 0][:40]
for sheet in range(2):
    part = worst[sheet * 20:(sheet + 1) * 20]
    if not part:
        break
    cell_w, cell_h, label = 320, 240, 16
    img = Image.new("RGB", (cell_w * 3, (cell_h + label) * len(part)), (24, 24, 24))
    from PIL import ImageDraw

    draw = ImageDraw.Draw(img)
    for k, (share, top, font, tag, name, old, new) in enumerate(part):
        a, b = Image.open(old).convert("RGB"), Image.open(new).convert("RGB")
        _, _, m = compare(a, b)
        y = k * (cell_h + label)
        draw.text((4, y + 2), f"{share * 100:.2f}% >32, max {top}  {font} {tag}  {name}"[:150], fill=(230, 230, 230))
        diff = m.point(lambda v: min(255, v * 4)).convert("RGB") if m else Image.new("RGB", a.size, (255, 0, 0))
        for col, pic in enumerate([a, b, diff]):
            img.paste(pic.resize((cell_w, cell_h)), (col * cell_w, y + label))
    w, h = img.size
    if w > 1280:
        img = img.resize((1280, h * 1280 // w))
    img.save(OUT / f"sheet-{sheet + 1}.jpg", quality=75)

over = [r for r in rows if r[0] >= 0.02]
print(f"native-paint: {len(rows)} pictures compared, {len(missing)} missing in native, {sum(1 for r in rows if r[1] == 0)} identical, "
      f"{len(over)} with 2 % or more of their pixels over {FAR} steps apart; findings differ in {len([l for l in changed if l.startswith('+++')])} files")
for share, top, font, tag, name, _, _ in rows[:10]:
    print(f"  {share * 100:6.2f} %  max {top:3}  {font} {tag} {name}")
for m in missing[:10]:
    print(f"  missing: {m}")
