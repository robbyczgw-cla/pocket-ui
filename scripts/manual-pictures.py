#!/usr/bin/env python3
"""The screenshots of the user guide (docs/user/pictures/*.webp): pocket-ui's own renderer (--snapshot) at 640 x 480, the
screen of the H700 handhelds, on the made-up Curator library with real box art of official games only.

    python3 -I scripts/manual-pictures.py POCKET_UI_BINARY [OUT]      (OUT default: docs/user/pictures)

POCKET_UI_BINARY is a pocket-ui built for this computer from this checkout (`cargo build --profile gate --bin pocket-ui`).
Needs ffmpeg (libwebp) and bwrap. Box art and screenshots come from the libretro thumbnail collection
(thumbnails.libretro.com); a game without box art, hacks and test entries are taken out, so no shelf has an empty cover.
Each snapshot runs in a bwrap sandbox where the library really is /storage/roms, so no picture shows a path of this
computer. The device's manual leaves these pictures out (`manual::parse`); GitHub and the web show them.
Look at every picture afterwards: no raw file names, no test entries, nothing cut off."""
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
PLAYLISTS = {
    "gb": "Nintendo - Game Boy", "gbc": "Nintendo - Game Boy Color", "gba": "Nintendo - Game Boy Advance",
    "snes": "Nintendo - Super Nintendo Entertainment System", "n64": "Nintendo - Nintendo 64", "nds": "Nintendo - Nintendo DS",
    "megadrive": "Sega - Mega Drive - Genesis", "psx": "Sony - PlayStation", "psp": "Sony - PlayStation Portable",
    "gamecube": "Nintendo - GameCube", "ps2": "Sony - PlayStation 2", "3ds": "Nintendo - Nintendo 3DS",
}
KINDS = {"covers": "Named_Boxarts", "snaps": "Named_Snaps", "titles": "Named_Titles"}
DROP = ["xbox", "nes", "ports", "ps3", "wiiu"]  # no box art in the made-up library
GAME = "snes|Chrono Trigger|Chrono Trigger (USA).sfc"  # the game behind the pause menu
RECENT = [("snes", "Chrono Trigger", "Chrono Trigger (USA).sfc", 20, 7200),
          ("snes", "Super Mario World", "Super Mario World (USA).sfc", 30, 5400),
          ("snes", "Super Metroid", "Super Metroid (Japan, USA) (En,Ja).sfc", 50, 9000),
          ("gba", "Golden Sun", "Golden Sun (USA, Europe).gba", 80, 12000)]
# file name, snapshot state, extra environment (THEME: the look in settings.json, "big" is Easy), Chrono Trigger's
# screenshot as the game's picture
SHOTS = [
    ("first-start", "setup:first", {}, False),
    ("shelf", "shelf:snes:shelf:games:0", {}, False),
    ("easy", "big-shelf", {}, False),
    ("pause-menu", "menu:pocket-core", {"POCKETUI_SNAPSHOT_GAME": GAME}, True),
    ("where-was-i", "where-was-i", {}, True),
    ("controls-card", "controls-card:snes", {}, False),
    ("add-games", "setup:games", {}, False),
    ("game-check", "game-check", {}, False),
    ("settings", "settings", {}, False),
    ("updates", "settings:5:4", {}, False),
    ("bluetooth", "bluetooth:found", {}, False),
    ("backup", "backup", {}, False),
]


def thumb_name(stem):
    return "".join("_" if c in '&*/:`<>?\\|"' else c for c in stem)


def fetch(playlist, kind, stem, dest):
    if dest.exists() and dest.stat().st_size > 0:
        return True
    url = "https://thumbnails.libretro.com/" + urllib.parse.quote(f"{playlist}/{kind}/{thumb_name(stem)}.png")
    try:
        with urllib.request.urlopen(url, timeout=20) as r:
            data = r.read()
        if not data.startswith(b"\x89PNG"):
            return False
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)
        return True
    except Exception:
        return False


def make_library(lib, cache):
    shutil.rmtree(lib, ignore_errors=True)
    subprocess.run([sys.executable, "-I", str(ROOT / "scripts/curator-fake-library.py"), str(lib), "dark", "/storage/roms"],
                   check=True, stdout=subprocess.DEVNULL)
    for sysdir in sorted((lib / "roms").iterdir()):
        playlist = PLAYLISTS.get(sysdir.name)
        for rom in sorted(sysdir.iterdir()):
            if rom.name == "gamelist.xml" or rom.is_dir() or playlist is None:
                continue
            if "[T-" in rom.name or "[h" in rom.name or "Hack" in rom.name or rom.name.startswith("Chrono Cross"):
                rom.unlink()  # official games only; Chrono Cross is a playlist with a missing disc
                continue
            stem = rom.stem
            if not fetch(playlist, KINDS["covers"], stem, cache / sysdir.name / "covers" / f"{stem}.png"):
                rom.unlink()
                continue
            for kind in KINDS:
                src = cache / sysdir.name / kind / f"{stem}.png"
                if kind != "covers":
                    fetch(playlist, KINDS[kind], stem, src)
                if src.exists():
                    dst = lib / "data" / kind / sysdir.name / f"{stem}.png"
                    dst.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(src, dst)
    for s in DROP:
        shutil.rmtree(lib / "roms" / s, ignore_errors=True)
        for k in KINDS:
            shutil.rmtree(lib / "data" / k / s, ignore_errors=True)
    cfg = lib / "es_systems.cfg"
    cfg.write_text("\n".join(l for l in cfg.read_text().splitlines() if not any(f"<name>{s}</name>" in l for s in DROP)) + "\n")
    now = int(time.time())
    (lib / "data/recents.json").write_text(json.dumps([
        {"system": s, "name": n, "path": f"/storage/roms/{s}/{f}", "last_played": now - h * 3600, "play_secs": p}
        for s, n, f, h, p in RECENT]))
    favs = lib / "data/favorites.json"
    favs.write_text(json.dumps([e for e in json.loads(favs.read_text()) if (lib / e["path"].replace("/storage/", "", 1)).exists()]))


def snapshot(binary, lib, wl, state, png, extra, frame):
    extra = dict(extra)
    settings = lib / "data/settings.json"
    before = settings.read_text()
    cfg = json.loads(before)
    cfg["theme"] = extra.pop("THEME", cfg.get("theme"))
    settings.write_text(json.dumps(cfg))
    cmd = ["bwrap", "--tmpfs", "/", "--ro-bind", "/usr", "/usr", "--symlink", "usr/lib", "/lib", "--symlink", "usr/lib64", "/lib64",
           "--symlink", "usr/bin", "/bin", "--ro-bind", "/etc", "/etc", "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/run",
           "--bind", str(lib.parent), str(lib.parent), "--bind", str(lib / "roms"), "/storage/roms",
           "--bind", str(lib / "data"), "/storage/pocket-data", "--ro-bind", str(lib / "es_systems.cfg"), "/storage/es_systems.cfg",
           "--ro-bind", str(binary.resolve()), "/pocket-ui",
           "env", "POCKETUI_MOCK=1", "POCKETUI_SNAPSHOT_WIFI=1", "POCKETUI_SNAPSHOT_SIZE=640x480", f"WAYLAND_DISPLAY={wl}",
           "POCKETUI_ES_SYSTEMS=/storage/es_systems.cfg", "POCKETUI_DATA=/storage/pocket-data", "HOME=/storage"]
    cmd += [f"{k}={v}" for k, v in extra.items()]
    cmd += ["nice", "-n", "19", "/pocket-ui", "--snapshot", state, str(png)] + ([str(frame)] if frame else [])
    try:
        subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
    finally:
        settings.write_text(before)


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    binary = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "docs/user/pictures"
    out.mkdir(parents=True, exist_ok=True)
    only = set(os.environ.get("ONLY", "").split()) - {""}
    cache = pathlib.Path(os.environ.get("THUMBS", pathlib.Path(tempfile.gettempdir()) / "pocketui-manual-thumbs"))
    # A short folder: the Wayland socket's path must stay under 108 bytes.
    work = pathlib.Path(tempfile.mkdtemp(prefix="pum-"))
    lib = work / "lib"
    make_library(lib, cache)
    frame = work / "frame.png"
    shutil.copyfile(cache / "snes/snaps/Chrono Trigger (USA).png", frame)
    wl = str(work / "wl")
    stub = subprocess.Popen([sys.executable, "-I", str(ROOT / "scripts/wayland-stub.py"), wl])
    time.sleep(1)
    try:
        for name, state, extra, uses_frame in SHOTS:
            if only and name not in only:
                continue
            png = work / f"{name}.png"
            snapshot(binary, lib, wl, state, png, extra, frame if uses_frame else None)
            webp = out / f"{name}.webp"
            # A game's own picture is busier than a menu: a lower quality keeps it under 60 KB.
            quality = "60" if uses_frame else "82"
            subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(png), "-c:v", "libwebp", "-quality", quality, str(webp)], check=True)
            print("ok", webp.name, webp.stat().st_size // 1024, "KB")
    finally:
        stub.terminate()
        stub.wait()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
