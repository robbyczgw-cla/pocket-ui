#!/usr/bin/env python3
"""Adds pocket-ui to Steam as a "Non-Steam game" by editing shortcuts.vdf (docs/STEAMDECK.md). Close Steam first
(it rewrites the file on exit), or restart Steam afterwards. Without arguments it writes for every Steam user found.

usage: add-to-steam.py [--exe PATH] [--args "..."] [--name pocket-ui] [--dry-run]
Default exe: ~/.local/share/pocket-ui/pocket-ui-deck.sh (the bundle's installer puts it there).
"""
import argparse, glob, os, struct, sys, zlib


def parse(b, i=0):
    """Binary VDF map -> (dict, next index). Types: 0 map, 1 string, 2 u32, 8 end."""
    out = {}
    while i < len(b):
        t = b[i]; i += 1
        if t == 8:
            return out, i
        j = b.index(b"\0", i); key = b[i:j].decode("utf8", "replace"); i = j + 1
        if t == 0:
            out[key], i = parse(b, i)
        elif t == 1:
            j = b.index(b"\0", i); out[key] = b[i:j].decode("utf8", "replace"); i = j + 1
        elif t == 2:
            out[key] = struct.unpack("<I", b[i:i + 4])[0]; i += 4
        else:
            raise ValueError(f"unknown VDF type {t}")
    return out, i


def dump(d):
    out = b""
    for k, v in d.items():
        k = k.encode()
        if isinstance(v, dict):
            out += b"\0" + k + b"\0" + dump(v)
        elif isinstance(v, int):
            out += b"\2" + k + b"\0" + struct.pack("<I", v)
        else:
            out += b"\1" + k + b"\0" + v.encode() + b"\0"
    return out + b"\x08"


def app_id(exe, name):
    return (zlib.crc32((exe + name).encode()) & 0xFFFFFFFF) | 0x80000000


def entry(exe, args, name, icon=""):
    return {"appid": app_id('"' + exe + '"', name), "AppName": name, "Exe": '"' + exe + '"',
            "StartDir": '"' + os.path.dirname(exe) + '"', "icon": icon, "ShortcutPath": "", "LaunchOptions": args,
            "IsHidden": 0, "AllowDesktopConfig": 1, "AllowOverlay": 1, "OpenVR": 0, "Devkit": 0,
            "DevkitGameID": "", "DevkitOverrideAppID": 0, "LastPlayTime": 0, "FlatpakAppID": "", "tags": {}}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=os.path.expanduser("~/.local/share/pocket-ui/pocket-ui-deck.sh"))
    ap.add_argument("--args", default="")
    ap.add_argument("--name", default="pocket-ui")
    ap.add_argument("--steam", default=os.path.expanduser("~/.steam/steam"))
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    files = glob.glob(os.path.join(a.steam, "userdata", "*", "config", "shortcuts.vdf"))
    if not files:  # a fresh Steam user has no shortcuts.vdf yet
        files = [os.path.join(d, "shortcuts.vdf") for d in glob.glob(os.path.join(a.steam, "userdata", "*", "config"))]
    if not files:
        print("no Steam user found under", a.steam, file=sys.stderr); sys.exit(3)
    for f in files:
        data = open(f, "rb").read() if os.path.exists(f) else b"\0shortcuts\0\x08\x08"
        root, _ = parse(data)
        sc = root.setdefault("shortcuts", {})
        mine = [k for k, v in sc.items() if isinstance(v, dict) and v.get("AppName") == a.name]
        if mine:
            print(f"{f}: '{a.name}' is already there"); continue
        sc[str(len(sc))] = entry(a.exe, a.args, a.name)
        if a.dry_run:
            print(f"{f}: would add '{a.name}' ({len(sc)} shortcuts)"); continue
        if os.path.exists(f):
            open(f + ".pocket-ui.bak", "wb").write(data)
        open(f, "wb").write(dump(root))
        print(f"{f}: added '{a.name}'. Restart Steam (Game Mode: Power > Restart Steam) to see it.")


main()
