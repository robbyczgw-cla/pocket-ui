"""table.py: prints the cases' table of src/media.rs (between "// BEGIN CASE TABLE" and "// END CASE TABLE"). A case with
assets/media/case/<kind>.json is a photo converted by PixelLab (case/SOURCES.md): its pictures are `res` times the
template's size and its rectangles are in their pixels. The others are drawn by assets/media/tools/draw_media.py: their
rows are kept as src/media.rs has them (res 1). Run from the repository's root."""
import json, os, re
from PIL import Image

# In the order of `Medium::ALL`.
KINDS = ["snes", "n64", "nes", "gb", "gba", "generic-cart", "md", "sms", "ps1", "segadisc", "generic-disc", "ps2", "gc", "wii",
         "keep", "umd", "ds", "3ds", "xbox", "vita", "generic-digital"]
src = open("src/media.rs").read()
block = src[src.index("// BEGIN CASE TABLE"):src.index("// END CASE TABLE")]
rows = dict(re.findall(r"    // (\S+)\n    (Template \{.*\}),", block))
opt = lambda r: f"Some(({r[0]}, {r[1]}, {r[2]}, {r[3]}))" if r else "None"
out = []
for kind in KINDS:
    js = f"assets/media/case/{kind}.json"
    if os.path.exists(js):
        d = json.load(open(js))
        res = d["res"]
        sw, sh = Image.open(f"assets/media/{kind}-spine.png").size
        fw, fh = Image.open(f"assets/media/{kind}-front.png").size
        assert sw % res == 0 and sh % res == 0 and fw % res == 0 and fh % res == 0, kind
        row = (f"Template {{ spine: ({sw // res}, {sh // res}), front: ({fw // res}, {fh // res}), spine_label: {tuple(d['spine_label'])}, "
               f"front_label: {tuple(d['front_label'])}, band: {opt(d.get('band'))}, front_band: {opt(d.get('front_band'))}, caps: (1, 1), "
               f"tint: {str(bool(d.get('tint'))).lower()}, res: {res} }}")
    else:
        row = rows[kind]
        if "res:" not in row:
            row = row[:-2] + ", res: 1 }"
    out.append((kind, row))
print("// BEGIN CASE TABLE (assets/media/case/tools/table.py)")
print("/// The cases, in the order of `Medium::ALL`: drawn by `assets/media/tools/draw_media.py` (res 1) or converted from photos")
print("/// (`assets/media/case/SOURCES.md`, their pictures `res` times the size, their rectangles in their pixels).")
print("#[rustfmt::skip]")
print(f"const TEMPLATES: [Template; {len(out)}] = [")
for kind, row in out:
    print(f"    // {kind}\n    {row},")
print("];")
print("// END CASE TABLE")
