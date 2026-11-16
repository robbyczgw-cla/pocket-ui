#!/usr/bin/env python3
"""The screenshots of the user guide (docs/user/pictures/*.webp): pocket-ui's own renderer (--snapshot) at 640 x 480, the
screen of the H700 handhelds, on the made-up Curator library with real box art of official games only, in the Retro style (Departure Mono, Cocoa) and
with Sealed new games as a player has it (a game never played stands in its film). The tape label's picture is the one
exception to "official games only": a freely licensed homebrew game that has no box art, which is the point.

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
    ("where-was-i", "where-was-i", {"POCKETUI_SNAPSHOT_PROGRESS": "chrono"}, True),
    ("case-back", "box:snes:Super Metroid:r", {"POCKETUI_SNAPSHOT_PROGRESS": "metroid"}, False),
    ("sealed-case", "box:snes:Super Mario Kart", {}, False),
    ("tape-label", "box:gbc:Tobu Tobu Girl", {"POCKETUI_SEALED": "0"}, False),
    ("cozy-extras", "cozy:8", {}, False),
    ("extra-dim", "sys:quick", {"DIM": 2}, False),
    ("add-games", "setup:games", {}, False),
    ("game-check", "game-check", {}, False),
    ("settings", "settings", {}, False),
    ("updates", "settings:6:0", {}, False),
    ("bluetooth", "bluetooth:found", {}, False),
    ("test-buttons", "test-buttons:bt", {}, False),
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


def wiki_text(data, title):
    """The card's description as pocket-ui keeps it (`wiki_text.rs`: <data>/wikipedia/<title>.txt, whole sentences up to 700
    characters), loaded once here, because a snapshot ends before the card's own request is answered."""
    cached = pathlib.Path(tempfile.gettempdir()) / "pocketui-manual-thumbs" / "wikipedia" / (title + ".txt")
    if not cached.exists():
        url = "https://en.wikipedia.org/api/rest_v1/page/summary/" + urllib.parse.quote(title.replace(" ", "_"))
        req = urllib.request.Request(url, headers={"User-Agent": "pocket-ui-wiki/0.1"})
        try:
            with urllib.request.urlopen(req, timeout=20) as r:
                extract = " ".join(json.load(r)["extract"].split())
        except Exception:
            return
        text = ""
        for sentence in extract.replace(". ", ".\n").split("\n"):
            if len(text) + len(sentence) > 700 and text:
                break
            text += (" " if text else "") + sentence
        cached.parent.mkdir(parents=True, exist_ok=True)
        cached.write_text(text)
    dest = data / "wikipedia" / (title + ".txt")
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(cached, dest)


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
    # A real homebrew game that has no box art in the collection: the tape label's picture.
    (lib / "roms/gbc/Tobu Tobu Girl (World) (Homebrew).gbc").write_bytes(b"\0" * 32768)
    for s in DROP:
        shutil.rmtree(lib / "roms" / s, ignore_errors=True)
        for k in KINDS:
            shutil.rmtree(lib / "data" / k / s, ignore_errors=True)
    cfg = lib / "es_systems.cfg"
    cfg.write_text("\n".join(l for l in cfg.read_text().splitlines() if not any(f"<name>{s}</name>" in l for s in DROP)) + "\n")
    wiki_text(lib / "data", "Super Metroid")
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
    dim = extra.pop("DIM", 0)
    # Phone games (J2ME) are a folder of their own, only here: the other pictures show the shelf without it.
    j2me = extra.pop("J2ME", 0)
    cfg_file = lib / "es_systems.cfg"
    cfg_before = cfg_file.read_text()
    if j2me:
        (lib / "roms/j2me").mkdir(exist_ok=True)
        (lib / "roms/j2me/Bounce Tales (Nokia).jar").write_bytes(b"PK")
        cfg_file.write_text(cfg_before.replace("</systemList>", "  <system><name>j2me</name><fullname>Phone games (J2ME)</fullname>"
                            "<path>/storage/roms/j2me</path><extension>.jar</extension></system>\n</systemList>"))
    display = lib / "data/display.json"
    display_before = display.read_text() if display.exists() else None
    if dim:
        display.write_text(json.dumps({"dim": dim}))
    # One look for the whole manual: Retro in Departure Mono with the Cocoa colours (the style of the README's pictures).
    cfg.update({"font": "retro", "retro_letters": "departure", "retro_colours": "cocoa"})
    settings.write_text(json.dumps(cfg))
    cmd = ["bwrap", "--tmpfs", "/", "--ro-bind", "/usr", "/usr", "--symlink", "usr/lib", "/lib", "--symlink", "usr/lib64", "/lib64",
           "--symlink", "usr/bin", "/bin", "--ro-bind", "/etc", "/etc", "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/run",
           "--bind", str(lib.parent), str(lib.parent), "--bind", str(lib / "roms"), "/storage/roms",
           "--bind", str(lib / "data"), "/storage/pocket-data", "--ro-bind", str(lib / "es_systems.cfg"), "/storage/es_systems.cfg",
           "--ro-bind", str(binary.resolve()), "/pocket-ui",
           "env", "POCKETUI_MOCK=1", "POCKETUI_SNAPSHOT_WIFI=1", "POCKETUI_SNAPSHOT_SIZE=640x480", "POCKETUI_SEALED=1", f"WAYLAND_DISPLAY={wl}",
           "POCKETUI_ES_SYSTEMS=/storage/es_systems.cfg", "POCKETUI_DATA=/storage/pocket-data", "HOME=/storage"]
    cmd += [f"{k}={v}" for k, v in extra.items()]
    cmd += ["nice", "-n", "19", "/pocket-ui", "--snapshot", state, str(png)] + ([str(frame)] if frame else [])
    try:
        subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
    finally:
        settings.write_text(before)
        cfg_file.write_text(cfg_before)
        shutil.rmtree(lib / "roms/j2me", ignore_errors=True)
        if display_before is None:
            display.unlink(missing_ok=True)
        else:
            display.write_text(display_before)


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
