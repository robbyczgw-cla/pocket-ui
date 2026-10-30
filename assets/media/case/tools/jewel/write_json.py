"""write_json.py <specs.json> <kind> <front_variant> <spine_variant> <out.json>: the case's <kind>.json (rects in the
PNGs' own pixels, res 2) from the spec and the PixelLab variants kept."""
import json, sys
spec, kind, fv, sv, out = json.load(open(sys.argv[1])), sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5]
k = spec[kind]
names = {1: "default init-image strength", 2: "init_image_strength 150", 3: "init_image_strength 250 + fixer"}
f, s, m = k["front"], k["spine"], k["meta"]
j = {"res": 2, "front_label": f["label"], "spine_label": s["label"], "band": s.get("band"), "tint": False,
     "source_front": m["source_front"], "source_spine": m.get("source_spine", m["source_front"]),
     "licence": m["licence"], "author": m["author"],
     "pixellab": f"image-to-pixelart v2, output at the final size; front: front-{fv}.png ({names[fv]}), "
                 f"spine: spine-{sv}.png ({names[sv]})",
     "logos_removed": m["logos_removed"]}
if "note" in m:
    j["note"] = m["note"]
open(out, "w").write(json.dumps(j, indent=1) + "\n")
print(out)
