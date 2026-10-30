#!/usr/bin/env python3
"""A realistic made-up library for Curator's snapshots (docs/CURATOR.md, "Developer notes").

    python3 -I scripts/curator-fake-library.py OUT [dark|light] [ROMS]

Writes OUT/roms/<system>/<file> (one byte each, No-Intro and Redump names), OUT/es_systems.cfg, and the player's
files in OUT/data (settings, library.json with statuses, ratings, stickers and a preferred version, favourites, recent games,
curator.json with two smart shelves). ROMS (default OUT/roms) is where the player's files say the games are: give
/storage/roms for the virtual device, whose ROM folders are links to OUT/roms (VDEV_ROMS=OUT/roms). Then, for example:

    POCKETUI_MOCK=1 POCKETUI_ES_SYSTEMS=OUT/es_systems.cfg POCKETUI_DATA=OUT/data \\
      target/release/pocket-ui --snapshot curator:echoes:ps2:revenge out.png

Includes games owned on several systems (Chrono Trigger on SNES, PS1 and DS; Final Fantasy VI on SNES as a fan
translation and on GBA; Burnout Dominator on PS2 and PSP; Link's Awakening on GB and GBC), a remake (Ocarina of
Time on N64 and 3DS) and a PortMaster port without data (Celeste Classic).
"""

import datetime
import json
import os
import sys
import time

SYSTEMS = [
    ("3ds", "Nintendo 3DS", ".3ds .cia"),
    ("gamecube", "GameCube", ".rvz .iso"),
    ("gb", "Game Boy", ".gb .zip"),
    ("gba", "Game Boy Advance", ".gba .zip"),
    ("gbc", "Game Boy Color", ".gbc .zip"),
    ("megadrive", "Mega Drive", ".md .bin .gen .zip"),
    ("n64", "Nintendo 64", ".n64 .z64 .zip"),
    ("nds", "Nintendo DS", ".nds .zip"),
    ("nes", "Nintendo Entertainment System", ".nes .zip"),
    ("ports", "Ports", ".sh"),
    ("ps2", "PlayStation 2", ".iso .chd"),
    ("psp", "PlayStation Portable", ".iso .cso"),
    ("ps3", "PlayStation 3", ".iso .ps3"),
    ("psx", "PlayStation", ".chd .cue .m3u"),
    ("snes", "Super Nintendo", ".sfc .smc .zip"),
    ("wiiu", "Wii U", ".wua .wud .rpx"),
    ("xbox", "Xbox", ".iso .xiso"),
]

# The same library as src/curator/tests.rs (LIBRARY), plus Final Fantasy III on the NES.
GAMES = {
    "3ds": ["Legend of Zelda, The - Ocarina of Time 3D (USA) (En,Fr,Es).3ds"],
    "gamecube": ["Animal Crossing (USA).rvz", "Mario Kart - Double Dash!! (USA).rvz", "F-Zero GX (USA).rvz", "Super Smash Bros. Melee (USA) (En,Ja) (Rev 2).rvz", "Burnout 2 - Point of Impact (USA).rvz"],
    "gb": ["Tetris (World) (Rev 1).gb", "Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2).gb", "Kirby's Dream Land (USA, Europe).gb"],
    "gbc": ["Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2) (SGB Enhanced) (GB Compatible).gbc", "Pokemon - Crystal Version (USA, Europe) (Rev 1).gbc"],
    "gba": [
        "Advance Wars (USA) (Rev 1).gba",
        "Astro Boy - Omega Factor (USA).gba",
        "Castlevania - Aria of Sorrow (USA).gba",
        "F-Zero - Maximum Velocity (USA, Europe).gba",
        "Final Fantasy VI Advance (USA).gba",
        "Golden Sun (USA, Europe).gba",
        "Gunstar Super Heroes (USA).gba",
        "Kirby - Nightmare in Dream Land (USA).gba",
        "Legend of Zelda, The - The Minish Cap (USA).gba",
        "Mario & Luigi - Superstar Saga (USA).gba",
        "Mario Kart - Super Circuit (USA).gba",
        "Mega Man Zero (USA).gba",
        "Metroid - Zero Mission (USA).gba",
        "Metroid Fusion (USA).gba",
        "Mother 3 (Japan) [T-En by Chewy v1.3].gba",
        "Pokemon - Emerald Version (USA, Europe).gba",
        "WarioWare, Inc. - Mega Microgame$! (USA).gba",
    ],
    "megadrive": ["Sonic The Hedgehog 2 (World) (Rev A).md", "Streets of Rage 2 (USA).md", "Phantasy Star IV (USA).md", "Gunstar Heroes (USA).md"],
    "n64": ["Legend of Zelda, The - Ocarina of Time (USA) (Rev 2).z64", "Mario Kart 64 (USA).z64", "F-Zero X (USA).z64", "Super Mario 64 (USA).z64", "GoldenEye 007 (USA).z64"],
    "nds": ["Chrono Trigger (USA) (En,Fr).nds", "Mario Kart DS (USA, Australia) (En,Fr,De,Es,It).nds", "New Super Mario Bros. (USA, Australia).nds", "Advance Wars - Dual Strike (USA, Australia).nds"],
    "nes": ["Final Fantasy III (Japan) [T-En by Neill Corlett v1.1].nes"],
    "ports": ["Celeste Classic.sh"],
    "ps2": [
        "Burnout (USA).iso",
        "Burnout 2 - Point of Impact (USA).iso",
        "Burnout 3 - Takedown (USA).iso",
        "Burnout Dominator (USA).iso",
        "Burnout Revenge (USA).iso",
        "Gran Turismo 4 (USA).iso",
        "Katamari Damacy (USA).iso",
        "Need for Speed - Hot Pursuit 2 (USA).iso",
        "OutRun 2006 - Coast 2 Coast (USA).iso",
        "Shadow of the Colossus (USA).iso",
    ],
    "psp": ["Burnout Legends (USA).iso", "Burnout Dominator (USA).iso", "Ridge Racer (USA).iso", "Lumines (USA).iso"],
    # The newer guests (Cemu, RPCS3, hakuX): no save states, their own menus; two games each for their system pages.
    "ps3": ["Demon's Souls (USA).iso", "Gran Turismo 6 (USA).iso"],
    "wiiu": ["Mario Kart 8 (USA).wua", "Xenoblade Chronicles X (USA).wua"],
    "xbox": ["Halo 2 (USA).iso", "Jet Set Radio Future (USA).iso"],
    "psx": ["Castlevania - Symphony of the Night (USA).chd", "Chrono Cross (USA).m3u", "Chrono Trigger (Japan).chd", "Crash Team Racing (USA).chd", "R4 - Ridge Racer Type 4 (USA).chd", "Wipeout 2097 (Europe).chd"],
    "snes": [
        "Chrono Trigger (USA).sfc",
        "Donkey Kong Country (USA) (Rev 2).sfc",
        "EarthBound (USA).sfc",
        "F-Zero (USA).sfc",
        "Final Fantasy VI (Japan) [T-En by RPGe v1.1].sfc",
        "Legend of Zelda, The - A Link to the Past (USA).sfc",
        "Secret of Mana (USA).sfc",
        "Street Fighter II - The World Warrior (USA).sfc",
        "Super Bomberman (USA).sfc",
        "Super Mario All-Stars (USA).sfc",
        "Super Mario Kart (USA).sfc",
        "Super Mario World (USA).sfc",
        "Super Metroid (Japan, USA) (En,Ja).sfc",
    ],
}


GAMELIST = """<?xml version="1.0"?>
<gameList>
  <game>
    <path>./Advance Wars (USA) (Rev 1).gba</path>
    <name>Advance Wars</name>
    <desc>Command an army across 2D battlefields, turn by turn. Capture cities, build units and outwit the enemy commanders. Fog of war and the commanders' special powers keep every map different.</desc>
    <rating>0.8</rating>
    <releasedate>20010921T000000</releasedate>
    <developer>Intelligent Systems</developer>
    <publisher>Nintendo</publisher>
    <genre>Turn-based strategy</genre>
    <players>1-4</players>
    <favorite>true</favorite>
  </game>
  <game>
    <path>./Golden Sun (USA, Europe).gba</path>
    <name>Golden Sun</name>
    <rating>1</rating>
    <favorite>true</favorite>
  </game>
  <game>
    <path>./Metroid Fusion (USA).gba</path>
    <name>Metroid Fusion</name>
    <desc>Samus is hunted on a research station by the parasite that nearly killed her.</desc>
    <rating>0.9</rating>
    <favorite>true</favorite>
  </game>
  <game>
    <path>./Pokemon - Emerald Version (USA, Europe).gba</path>
    <rating>0.6</rating>
  </game>
  <game>
    <path>./Not on this card.gba</path>
    <favorite>true</favorite>
  </game>
</gameList>
"""


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    out = os.path.abspath(sys.argv[1])
    appearance = sys.argv[2] if len(sys.argv) > 2 else "dark"
    files_at = os.path.join(out, "roms")
    roms = sys.argv[3] if len(sys.argv) > 3 else files_at
    data = os.path.join(out, "data")
    os.makedirs(data, exist_ok=True)
    for f in os.listdir(data):
        p = os.path.join(data, f)
        if os.path.isfile(p):
            os.remove(p)
    # The ROM folders hold exactly this library (files of an earlier run go).
    if os.path.isdir(files_at):
        for system in os.listdir(files_at):
            folder = os.path.join(files_at, system)
            for f in os.listdir(folder) if os.path.isdir(folder) else []:
                os.remove(os.path.join(folder, f))
    for system, files in GAMES.items():
        os.makedirs(os.path.join(files_at, system), exist_ok=True)
        for f in files:
            with open(os.path.join(files_at, system, f), "w") as fh:
                fh.write('Chrono Cross (USA) (Disc 1).chd\nChrono Cross (USA) (Disc 2).chd\n' if f.endswith(".m3u") else "x")
    # A scraped gamelist.xml next to the GBA games, as EmulationStation or ES-DE leaves one (src/gamelist.rs): a
    # description, facts, a favourite and a rating for a few of them, and a game the library does not have.
    with open(os.path.join(files_at, "gba", "gamelist.xml"), "w") as fh:
        gamelist = GAMELIST
        if os.environ.get("POCKETUI_TEXT_FIT"):
            # Scraped names replace cryptic filenames. Readable filenames deliberately keep their
            # cleaned title, so changing Advance Wars' <name> would not exercise a long title.
            entries = [
                ("BPEE.gba", "Pokemon (Emerald Rogue v2.2.0 EX)"),
                ("AX4E.gba", "Super Mario Advance 4 - Super Mario Bros. 3 - 38 e-Reader levels"),
            ]
            extra = ""
            for filename, title in entries:
                with open(os.path.join(files_at, "gba", filename), "w") as rom:
                    rom.write("x")
                extra += f"<game><path>./{filename}</path><name>{title}</name></game>"
            gamelist = gamelist.replace("</gameList>", extra + "</gameList>")
        if os.environ.get("POCKETUI_REDISCOVER"):
            # Curator's Rediscover (src/connections.rs): EmulationStation's "last played", as ES writes it (UTC, noon).
            # Metroid Fusion a year ago today (this very time, so the date is the same in the device's zone), Golden Sun two
            # years ago, Advance Wars 400 days ago.
            today = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0)
            def es_date(d):
                return d.strftime("%Y%m%dT%H%M%S")
            def year_ago(d, years):
                try:
                    return d.replace(year=d.year - years)
                except ValueError:
                    return d.replace(year=d.year - years, day=28)
            for name, when in [("Metroid Fusion (USA).gba", year_ago(today, 1)), ("Golden Sun (USA, Europe).gba", year_ago(today, 2)), ("Advance Wars (USA) (Rev 1).gba", today - datetime.timedelta(days=400))]:
                gamelist = gamelist.replace(f"<path>./{name}</path>", f"<path>./{name}</path>\n    <lastplayed>{es_date(when)}</lastplayed>")
        fh.write(gamelist)
    cfg = ["<systemList>"]
    for sid, name, ext in SYSTEMS:
        cfg.append(f"  <system><name>{sid}</name><fullname>{name}</fullname><path>{roms}/{sid}</path><extension>{ext}</extension></system>")
    cfg.append("</systemList>")
    with open(os.path.join(out, "es_systems.cfg"), "w") as fh:
        fh.write("\n".join(cfg) + "\n")

    def path(system, f):
        return f"{roms}/{system}/{f}"

    now = int(time.time())
    hour = 3600
    games = {
        path("ps2", "Burnout 3 - Takedown (USA).iso"): {"status": "completed", "status_set": True, "sessions": 22, "secs": 14 * hour, "stickers": [1], "rating": 5},
        path("ps2", "Gran Turismo 4 (USA).iso"): {"status": "playing", "status_set": True, "sessions": 31, "secs": 30 * hour, "note": "Finish the Sunday Cup", "rating": 4},
        path("ps2", "Burnout Revenge (USA).iso"): {"status": "playing", "status_set": True, "sessions": 4, "secs": 3 * hour, "rating": 3},
        path("gba", "Pokemon - Emerald Version (USA, Europe).gba"): {"status": "dropped", "status_set": True, "sessions": 3, "secs": 2 * hour, "rating": 2},
        path("snes", "Super Mario Kart (USA).sfc"): {"stickers": [1, 3]},
        path("nds", "Mario Kart DS (USA, Australia) (En,Fr,De,Es,It).nds"): {"stickers": [1]},
        path("snes", "Secret of Mana (USA).sfc"): {"stickers": [1]},
        path("nds", "Chrono Trigger (USA) (En,Fr).nds"): {"status": "paused", "status_set": True, "sessions": 6, "secs": 9 * hour, "stickers": [2], "rating": 4},
        path("gb", "Tetris (World) (Rev 1).gb"): {"stickers": [2]},
        path("gba", "Golden Sun (USA, Europe).gba"): {"stickers": [2]},
        path("snes", "Super Metroid (Japan, USA) (En,Ja).sfc"): {"status": "completed", "status_set": True, "sessions": 5, "secs": 8 * hour, "rating": 5},
    }
    library = {
        "version": 2,
        "games": games,
        "stickers": [
            {"id": 1, "name": "Couch co-op", "pinned": True},
            {"id": 2, "name": "Before bed", "pinned": False},
            {"id": 3, "name": "With the kids", "pinned": False},
        ],
        "preferred": {"Q761815": path("snes", "Chrono Trigger (USA).sfc")},
    }
    favorites = [
        {"system": "snes", "name": "Chrono Trigger", "path": path("snes", "Chrono Trigger (USA).sfc")},
        {"system": "nds", "name": "Chrono Trigger", "path": path("nds", "Chrono Trigger (USA) (En,Fr).nds")},
        {"system": "snes", "name": "Super Metroid", "path": path("snes", "Super Metroid (Japan, USA) (En,Ja).sfc")},
        {"system": "ps2", "name": "Burnout 3 - Takedown", "path": path("ps2", "Burnout 3 - Takedown (USA).iso")},
    ]
    recents = [
        {"system": "ps2", "name": "Gran Turismo 4", "path": path("ps2", "Gran Turismo 4 (USA).iso"), "last_played": now - 2 * hour, "play_secs": 30 * hour},
        {"system": "ps2", "name": "Burnout Revenge", "path": path("ps2", "Burnout Revenge (USA).iso"), "last_played": now - 26 * hour, "play_secs": 3 * hour},
        {"system": "nds", "name": "Chrono Trigger", "path": path("nds", "Chrono Trigger (USA) (En,Fr).nds"), "last_played": now - 40 * 24 * hour, "play_secs": 9 * hour},
    ]
    if os.environ.get("POCKETUI_TEXT_FIT"):
        recents.insert(0, {"system": "gba", "name": "Pokemon (Emerald Rogue v2.2.0 EX)", "path": path("gba", "BPEE.gba"), "last_played": now, "play_secs": hour})
    curator = {
        "shelves": [
            {"name": "Racing", "pinned": True, "lens": {"picks": {"family": ["racing"]}}},
            {"name": "Unplayed RPGs", "pinned": False, "lens": {"picks": {"family": ["rpg"], "status": ["not_started"]}}},
        ],
    }
    files = {
        "settings.json": {"onboarded": True, "appearance": appearance, "theme": "shelf"},
        "library.json": library,
        "favorites.json": favorites,
        "recents.json": recents,
        "curator.json": curator,
    }
    for name, value in files.items():
        with open(os.path.join(data, name), "w") as fh:
            json.dump(value, fh, indent=2)
    print(out)


if __name__ == "__main__":
    main()
