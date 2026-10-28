#!/bin/sh
# A large made-up game library for snapshots and the virtual device: an es_systems.cfg and ROM folders with empty
# files named like real dumps (region tags, scene numbers, accents, "The" moved to the end), and plain covers for
# some of them.
#
#   scripts/fake-library.sh <dir> [games] [data dir for covers]
#   POCKETUI_ES_SYSTEMS=<dir>/es_systems.cfg POCKETUI_DATA=<data dir> cargo run --bin pocket-ui -- --snapshot search:typing:mar out.png
#
# Nothing outside <dir> (and the data dir's covers/ folder, when given) is written. Same input, same library.
set -eu
dir=${1:?usage: fake-library.sh <dir> [games] [data dir]}
games=${2:-5000}
data=${3:-}
exec python3 -I - "$dir" "$games" "$data" <<'EOF'
import os, random, struct, sys, zlib

out, total, data = sys.argv[1], int(sys.argv[2]), sys.argv[3]
rng = random.Random(7)
systems = [
    ("nes", "Nintendo Entertainment System", "nes"), ("snes", "Super Nintendo", "sfc"), ("gb", "Game Boy", "gb"),
    ("gbc", "Game Boy Color", "gbc"), ("gba", "Game Boy Advance", "gba"), ("n64", "Nintendo 64", "z64"),
    ("megadrive", "Mega Drive", "md"), ("mastersystem", "Master System", "sms"), ("gamegear", "Game Gear", "gg"),
    ("psx", "PlayStation", "chd"), ("psp", "PlayStation Portable", "iso"), ("nds", "Nintendo DS", "nds"),
    ("pcengine", "PC Engine", "pce"), ("ngp", "Neo Geo Pocket", "ngp"),
]
real = [
    "Super Mario Kart", "Mario Kart 64", "Mario Kart - Super Circuit", "Super Mario World", "Super Mario Bros. 3",
    "Legend of Zelda, The - A Link to the Past", "Legend of Zelda, The - Link's Awakening DX", "Metroid Fusion",
    "Pokémon - Crystal Version", "Pokemon - Emerald Version", "Sonic the Hedgehog 2", "Castlevania - Aria of Sorrow",
    "Mega Man X", "Final Fantasy VI", "Chrono Trigger", "Kirby's Dream Land", "Tetris", "Dr. Mario",
    "Donkey Kong Country", "Street Fighter II Turbo", "Yoshi's Island", "Golden Sun", "Advance Wars",
    "F-Zero", "Ōkami Den", "Pac-Man", "1942", "'98 Koshien", "Earthbound", "Secret of Mana",
]
first = ["Super", "Mega", "Final", "Legend", "Dragon", "Street", "Metal", "Star", "Castle", "Crash", "Tomb", "Silent",
         "Ninja", "Golden", "Bubble", "Battle", "Space", "Shadow", "Wonder", "Thunder", "Ultra", "Pixel", "Turbo",
         "Crystal", "Iron", "Neon", "Lunar", "Wild", "Brave", "Royal", "Zero", "Atomic", "Echo", "Velvet", "Quantum"]
second = ["Fighter", "Quest", "Fantasy", "Kart", "Man", "Gear", "Wars", "Kong", "Raider", "Hill", "Gaiden", "Sun",
          "Adventure", "Party", "Racing", "Soccer", "Tennis", "Golf", "Island", "Knight", "Hunter", "Saga", "Odyssey",
          "Rally", "Arena", "Legends", "Tactics", "Heroes", "Blaster", "Dash", "Pinball", "Puzzle", "Garden", "Tower"]
tail = ["", "", "", " II", " III", " 2", " 64", " Advance", " DX", " Deluxe", " - The Lost Levels", " Gold", " Turbo"]
region = [" (USA)", " (Europe)", " (Japan)", " (USA, Europe)", " (World)", " (Europe) (En,Fr,De)", " (USA) (Rev 1)", ""]

def png(path, w, h, color):
    """A plain box-art stand-in: the colour with a darker band at the top, as covers often have."""
    r, g, b = color
    rows = []
    for y in range(h):
        k = 0.55 if y < h // 6 else (0.8 if y > h - h // 5 else 1.0)
        rows.append(b"\x00" + bytes((int(r * k), int(g * k), int(b * k))) * w)
    raw = zlib.compress(b"".join(rows))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", raw) + chunk(b"IEND", b""))

cfg = ["<?xml version=\"1.0\"?>", "<systemList>"]
per = max(1, total // len(systems))
made = 0
for i, (sid, full, ext) in enumerate(systems):
    folder = os.path.join(out, "roms", sid)
    os.makedirs(folder, exist_ok=True)
    cfg.append(f"  <system><name>{sid}</name><fullname>{full}</fullname><path>{folder}</path>"
               f"<extension>.{ext}</extension><command>true</command></system>")
    names = set(real[i::3] if i < 9 else [])
    while len(names) < per and made + len(names) < total:
        names.add(f"{rng.choice(first)} {rng.choice(second)}{rng.choice(tail)}")
    for k, name in enumerate(sorted(names)):
        stem = name + rng.choice(region)
        if sid == "gba" and k % 3 == 0:
            stem = f"{(k * 37) % 9999:04d} - {stem}"
        open(os.path.join(folder, f"{stem}.{ext}"), "w").close()
        if data and rng.random() < 0.6:
            covers = os.path.join(data, "covers", sid)
            os.makedirs(covers, exist_ok=True)
            png(os.path.join(covers, f"{stem}.png"), 60, 84, (rng.randint(40, 230), rng.randint(40, 230), rng.randint(40, 230)))
    made += len(names)
cfg.append("</systemList>")
with open(os.path.join(out, "es_systems.cfg"), "w") as f:
    f.write("\n".join(cfg) + "\n")
print(f"{made} games in {len(systems)} systems: {out}/es_systems.cfg")
EOF
