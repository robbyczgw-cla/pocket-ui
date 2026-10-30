"""write_json.py <specs.json> <kind> <variant> <out.json>: the case's <kind>.json (rects in the PNGs' own pixels, res 2)."""
import json, sys
spec, kind, v, out = json.load(open(sys.argv[1])), sys.argv[2], int(sys.argv[3]), sys.argv[4]
names = {1: "default init-image strength", 2: "init_image_strength 150", 3: "init_image_strength 250 + fixer"}
f, s, m = spec[kind]["front"], spec[kind]["spine"], spec[kind]["meta"]
j = {"res": 2, "front_label": f["label"], "spine_label": s["label"], "band": s.get("band"), "front_band": f.get("band"),
     "fixed_spine": [o["rect"] + o["colour"] for o in s.get("fixed", [])], "tint": False,
     "source_front": m["source"], "source_spine": m["source"], "licence": m["licence"], "author": m["author"],
     "pixellab": f"image-to-pixelart v2 at the final size, 3 variants per face; kept front-{v}/spine-{v} ({names[v]})",
     "logos_removed": "none on the plastic used (the Wii case's moulded logo is on the disc half, not used; the DVD logo of the tray photo lies under the label)",
     "note": m["note"]}
open(out, "w").write(json.dumps(j, indent=1) + "\n")
