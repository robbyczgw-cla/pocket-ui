"""table.py: prints the Rust table of the games' pictures (src/media.rs, between "// BEGIN GAME TABLE" and
"// END GAME TABLE") from assets/media/game: every <kind>.png with a <kind>.json is a photo converted by PixelLab
(SOURCES.md; its own colours, the label rectangle from the .json); the others are drawn by draw_media.py in grey and take
the system's plastic (their label rectangle from draw_media.py's table, LABELS below). Run from the repository's root."""
import json, os
from PIL import Image

DIR = "assets/media/game"
# kind file -> Game variant, in the order of Game::DRAWN.
KINDS = [("snes", "Snes"), ("snes-us", "SnesUs"), ("n64", "N64"), ("nes", "Nes"), ("gb", "Gb"), ("gbc", "Gbc"), ("gba", "Gba"),
         ("cart", "Cart"), ("md", "MegaDrive"), ("sms", "MasterSystem"), ("gg", "GameGear"), ("hucard", "HuCard"),
         ("ngp", "NeoGeoPocket"), ("ws", "WonderSwan"), ("lynx", "Lynx"), ("ds", "Card"), ("3ds", "Card3ds"), ("memory", "Memory")]
# The drawn ones' labels (draw_media.py prints them); "card" is the drawn DS card until ds.png comes.
LABELS = {"snes": (13, 14, 62, 57), "n64": (8, 16, 72, 35), "nes": (7, 18, 61, 45), "gb": (6, 12, 46, 39), "gba": (7, 7, 61, 27),
          "cart": (13, 14, 59, 59), "md": (7, 13, 58, 66), "card": (3, 5, 26, 19), "memory": (3, 8, 18, 21)}
rows = []
for kind, variant in KINDS:
    png, js = f"{DIR}/{kind}.png", f"{DIR}/{kind}.json"
    if kind == "ds" and not os.path.exists(js):
        kind, png, js = "card", f"{DIR}/card.png", f"{DIR}/card.json"
    if not os.path.exists(png):
        continue
    w, h = Image.open(png).size
    if os.path.exists(js):
        meta = json.load(open(js))
        # A photo keeps its colours, unless it was made neutral grey to take each system's plastic ("tint").
        label, tint = tuple(meta["label"]), bool(meta.get("tint", False))
    elif kind in LABELS:
        label, tint = LABELS[kind], True
    else:
        continue
    rows.append((kind, variant, w, h, label, tint))
print("// BEGIN GAME TABLE (assets/media/game/tools/table.py)")
print("/// The games' pictures, in the order of `Game::DRAWN`.")
print("#[rustfmt::skip]")
print(f"const GAME_TEMPLATES: [GameTemplate; {len(rows)}] = [")
for kind, variant, w, h, label, tint in rows:
    print(f"    // {kind}\n    GameTemplate {{ size: ({w}, {h}), label: {label}, tint: {str(tint).lower()} }},")
print("];\n")
print(f"const GAME_PNGS: [&[u8]; {len(rows)}] = [")
for kind, *_ in rows:
    print(f'    include_bytes!("../assets/media/game/{kind}.png"),')
print("];\n")
print("impl Game {")
print("    /// The ones with a picture of their own (the rest stand in for each other, `drawn`, or are painted round).")
print(f"    const DRAWN: [Game; {len(rows)}] = [{', '.join('Game::' + r[1] for r in rows)}];")
print("}")
print("// END GAME TABLE")
