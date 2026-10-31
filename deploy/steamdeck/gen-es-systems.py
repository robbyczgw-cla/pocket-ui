#!/usr/bin/env python3
"""Writes an es_systems.cfg for pocket-ui on a Steam Deck from the ROM folders found (docs/STEAMDECK.md).

Looks for, in this order: $POCKETUI_ROMS, EmuDeck's ~/Emulation/roms, ES-DE's ROMs folder (~/ES-DE/ROMs, then
the path in ~/ES-DE/settings/es_settings.xml), the SD card's /run/media/*/Emulation/roms. EmuDeck and ES-DE name
some folders differently from ROCKNIX (genesis, gc, psx, ...): FOLDERS maps them. A system with no games is simply
not written (pocket-ui hides empty systems anyway).

usage: gen-es-systems.py <out es_systems.cfg> [roms dir]
"""
import glob, os, sys
from xml.sax.saxutils import escape

# pocket-ui system id: (full name, extensions, [roms folder names], libretro core or None, standalone Flatpak or None)
# Cores are the ones pocket-core is tested with on the handhelds; a standalone system has no core and starts its Flatpak.
SYSTEMS = {
    "nes": ("Nintendo Entertainment System", "nes fds unf unif zip 7z", ["nes", "famicom"], "nestopia", None),
    "snes": ("Super Nintendo", "sfc smc fig swc zip 7z", ["snes", "sfc", "snesna"], "snes9x", None),
    "gb": ("Game Boy", "gb zip 7z", ["gb"], "gambatte", None),
    "gbc": ("Game Boy Color", "gbc zip 7z", ["gbc"], "gambatte", None),
    "gba": ("Game Boy Advance", "gba zip 7z", ["gba"], "mgba", None),
    "n64": ("Nintendo 64", "n64 v64 z64 zip 7z", ["n64"], "mupen64plus_next", None),
    "megadrive": ("Mega Drive", "md gen smd bin zip 7z", ["genesis", "megadrive"], "genesis_plus_gx", None),
    "mastersystem": ("Master System", "sms zip 7z", ["mastersystem"], "genesis_plus_gx", None),
    "gamegear": ("Game Gear", "gg zip 7z", ["gamegear"], "genesis_plus_gx", None),
    "pcengine": ("PC Engine", "pce sgx cue chd zip 7z", ["pcengine", "tg16"], "mednafen_pce_fast", None),
    "psx": ("PlayStation", "cue chd pbp m3u iso bin", ["psx"], "pcsx_rearmed", None),
    "nds": ("Nintendo DS", "nds zip 7z", ["nds"], "melonds", None),
    "psp": ("PlayStation Portable", "iso cso pbp chd", ["psp"], None, "org.ppsspp.PPSSPP"),
    "ps2": ("PlayStation 2", "iso chd cso bin gz", ["ps2"], None, "net.pcsx2.PCSX2"),
    "gamecube": ("Nintendo GameCube", "iso gcm gcz rvz ciso wbfs", ["gc", "gamecube"], None, "org.DolphinEmu.dolphin-emu"),
    "wii": ("Nintendo Wii", "iso wbfs rvz gcz wad", ["wii"], None, "org.DolphinEmu.dolphin-emu"),
    "wiiu": ("Nintendo Wii U", "wua wud wux rpx wup", ["wiiu"], None, "info.cemu.Cemu"),
    "ps3": ("PlayStation 3", "iso ps3 sfb", ["ps3"], None, "net.rpcs3.RPCS3"),
    "xbox": ("Xbox", "iso xiso", ["xbox"], None, "app.xemu.xemu"),
    "3ds": ("Nintendo 3DS", "3ds cci cxi cia", ["n3ds", "3ds"], None, "org.azahar_emu.Azahar"),
    "psvita": ("PlayStation Vita", "vpk zip", ["psvita", "vita"], None, "org.vita3k.Vita3K"),
}


def roms_root(arg):
    cands = [arg, os.environ.get("POCKETUI_ROMS"), os.path.expanduser("~/Emulation/roms"), os.path.expanduser("~/ES-DE/ROMs")]
    settings = os.path.expanduser("~/ES-DE/settings/es_settings.xml")
    if os.path.exists(settings):
        import re
        m = re.search(r'name="ROMDirectory" value="([^"]+)"', open(settings, errors="replace").read())
        if m:
            cands.append(m.group(1).replace("%ESPATH%", os.path.expanduser("~/ES-DE")))
    cands += sorted(glob.glob("/run/media/*/Emulation/roms")) + sorted(glob.glob("/run/media/*/*/Emulation/roms"))
    return next((c for c in cands if c and os.path.isdir(c)), None)


def main():
    out = sys.argv[1]
    root = roms_root(sys.argv[2] if len(sys.argv) > 2 else None)
    if root is None:
        print("no ROM folder found (looked for ~/Emulation/roms, ~/ES-DE/ROMs, the SD card)", file=sys.stderr)
        sys.exit(3)
    cfg = ['<?xml version="1.0"?>', "<systemList>"]
    found = 0
    for sid, (full, exts, folders, core, flatpak) in SYSTEMS.items():
        folder = next((os.path.join(root, f) for f in folders if os.path.isdir(os.path.join(root, f))), None)
        if folder is None:
            continue
        found += 1
        ext = " ".join("." + e for e in exts.split()) + " " + " ".join("." + e.upper() for e in exts.split())
        if core:
            emu = f'<emulators><emulator name="pocket-core"><cores><core default="true">{core}</core></cores></emulator></emulators>'
        else:
            emu = f'<emulators><emulator name="{escape(flatpak)}"><cores><core default="true">{escape(flatpak)}</core></cores></emulator></emulators>'
        cfg.append(f"  <system><name>{sid}</name><fullname>{escape(full)}</fullname><path>{escape(folder)}</path>"
                   f"<extension>{ext}</extension>{emu}</system>")
    cfg.append("</systemList>")
    open(out, "w").write("\n".join(cfg) + "\n")
    print(f"{found} systems from {root} -> {out}")


main()
