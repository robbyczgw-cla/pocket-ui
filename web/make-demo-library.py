#!/usr/bin/env python3
"""The browser demo's library (docs/WEB-DEMO.md): a handful of official games with their real pictures, no ROMs.

    python3 -I web/make-demo-library.py <thumbs dir> <out dir>

<thumbs dir> holds libretro-thumbnails style folders (<system>/covers|snaps|titles/<name>.png). <out dir> gets the
tree the page mounts at /storage: roms/<system>/<name>.<ext> (one line of text: the shelf needs a file, no game is in it),
.config/emulationstation/es_systems.cfg and .config/pocket-ui/ (settings and the pictures, made smaller with
ImageMagick so the download stays small). Nothing outside <out dir> is written.
"""
import json, os, shutil, subprocess, sys

GAMES = {
    "gba": ("Game Boy Advance", "gba", [
        "Metroid Fusion (USA)",
        "Mario Kart - Super Circuit (USA)",
        "F-Zero - Maximum Velocity (USA, Europe)",
        "Castlevania - Aria of Sorrow (USA)",
        "Golden Sun (USA, Europe)",
    ]),
    "snes": ("Super Nintendo", "sfc", [
        "Super Mario World (USA)",
        "Super Mario Kart (USA)",
        "F-Zero (USA)",
        "Donkey Kong Country (USA) (Rev 2)",
    ]),
    "n64": ("Nintendo 64", "z64", [
        "Super Mario 64 (USA)",
        "Mario Kart 64 (USA)",
        "F-Zero X (USA)",
    ]),
    "psx": ("PlayStation", "chd", [
        "Castlevania - Symphony of the Night (USA)",
        "R4 - Ridge Racer Type 4 (USA)",
    ]),
}
ROCKNIX = {"n64": '<emulators><emulator name="retroarch"><cores><core default="true">mupen64plus_next</core></cores></emulator></emulators>'}
# Covers at the size the 640x480 shelf and the opened box use; screenshots at the consoles' own size.
SIZES = {"covers": "360x360>", "snaps": "320x240>", "titles": "320x240>"}


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    thumbs, out = sys.argv[1], sys.argv[2]
    if os.path.exists(out):
        shutil.rmtree(out)
    data = os.path.join(out, ".config", "pocket-ui")
    systems = []
    for system, (full, ext, names) in GAMES.items():
        roms = os.path.join(out, "roms", system)
        os.makedirs(roms)
        for name in names:
            # Not empty (pocket-ui says so instead of starting an empty file), and plainly no game.
            with open(os.path.join(roms, f"{name}.{ext}"), "w") as f:
                f.write("pocket-ui browser demo: no game is in this file.\n")
            for kind, size in SIZES.items():
                src = os.path.join(thumbs, system, kind, f"{name}.png")
                if not os.path.exists(src):
                    continue
                dst_dir = os.path.join(data, kind, system)
                os.makedirs(dst_dir, exist_ok=True)
                subprocess.run(["convert", src, "-resize", size, "-strip", "-colors", "255", "-define", "png:compression-level=9",
                                os.path.join(dst_dir, f"{name}.png")], check=True)
        # The emulator ROCKNIX starts where pocket-core doesn't play the system on these handhelds (N64 on the H700s).
        emulator = ROCKNIX.get(system, "")
        systems.append(f"<system><name>{system}</name><fullname>{full}</fullname><path>/storage/roms/{system}</path>"
                       f"<extension>.{ext}</extension><command>true</command><platform>{system}</platform><theme>{system}</theme>{emulator}</system>")
    # pocket-core "is installed", so the games start the way they do on a handheld (and the pause menu is pocket-core's);
    # the page's stand-in answers its commands (web/app_web_demo.rs).
    os.makedirs(os.path.join(out, "pocket-ui"))
    open(os.path.join(out, "pocket-ui", "pocket-core"), "w").close()
    es = os.path.join(out, ".config", "emulationstation")
    os.makedirs(es)
    with open(os.path.join(es, "es_systems.cfg"), "w") as f:
        f.write("<systemList>\n" + "\n".join(systems) + "\n</systemList>\n")
    # Past the first start's setup, on the Shelf look with the playful animations: the demo starts on the shelf.
    # "New in this version" seen: the tour's cards are about updates, not for a first look.
    version = next(l.split('"')[1] for l in open(os.path.join(os.path.dirname(__file__), "..", "Cargo.toml")) if l.startswith("version"))
    settings = {"onboarded": True, "appearance": "dark", "theme": "shelf", "animations": "cozy",
                "extras": {"playful_animations": True}, "seen_version": version}
    with open(os.path.join(data, "settings.json"), "w") as f:
        json.dump(settings, f)
    total = sum(os.path.getsize(os.path.join(d, n)) for d, _, ns in os.walk(out) for n in ns)
    print(f"demo library: {sum(len(g[2]) for g in GAMES.values())} games, {total / 1e6:.1f} MB in {out}")


if __name__ == "__main__":
    main()
