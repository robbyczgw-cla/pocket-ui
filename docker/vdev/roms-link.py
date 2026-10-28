#!/usr/bin/env python3
"""Links the ROM folders of a host directory into /storage/roms/<system>/.

usage: roms-link.py SRC DEST ES_SYSTEMS_CFG

SRC folders are matched to ES system names case-insensitively, with the usual short names
(FC -> nes, SFC -> snes, MD -> megadrive ...). Only the ROM files are symlinked, so the system
folder itself is writable in the container: saves (.srm) and save states land there, not on the host.
"""
import os, re, sys

ALIASES = {"fc": "nes", "famicom": "nes", "sfc": "snes", "superfamicom": "snes", "md": "megadrive", "genesis": "megadrive",
           "pce": "pcengine", "pcecd": "pcenginecd", "gg": "gamegear", "sms": "mastersystem", "ws": "wonderswan",
           "wsc": "wonderswancolor", "ps": "psx", "ps1": "psx", "poke": "pokemini", "vb": "virtualboy"}

src, dest, cfg = sys.argv[1:4]
names = set(re.findall(r"<name>([^<]+)</name>", open(cfg, encoding="utf-8").read()))
count = 0
for folder in sorted(os.listdir(src)):
    path = os.path.join(src, folder)
    if not os.path.isdir(path) or folder.startswith("."):
        continue
    key = folder.lower()
    system = key if key in names else ALIASES.get(key)
    if system not in names:
        print(f"skip {folder}: no ES system of that name")
        continue
    target = os.path.join(dest, system)
    os.makedirs(target, exist_ok=True)
    for f in sorted(os.listdir(path)):
        if f.startswith(".") or not os.path.isfile(os.path.join(path, f)):
            continue
        link = os.path.join(target, f)
        if not os.path.lexists(link):
            os.symlink(os.path.join(src, folder, f), link)
            count += 1
    print(f"{folder} -> {system}")
print(f"{count} ROM files linked")
