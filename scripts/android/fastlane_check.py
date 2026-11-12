#!/usr/bin/env python3
"""The store listing of the Android preview (fastlane/metadata/android/, read by IzzyOnDroid and F-Droid style clients).

    fastlane_check.py TREE VERSIONCODE

Prints `ok:` / `FAIL:` lines, exit 0 when everything fits: title <= 30 characters, short description <= 80, full description <= 4000,
a changelog of <= 500 bytes for exactly this versionCode in every language, a short and a full description in every language, and
screenshots (en-US must have at least two) that are JPEG/PNG files of at most 150 KB and 1280 px wide (the repository's picture rule).
Only real screenshots belong in images/: no generated pictures, no commercial cover art (not checkable here; review by eye).
"""
import struct
import sys
from pathlib import Path

problems = []


def ok(msg):
    print("ok: " + msg)


def fail(msg):
    problems.append(msg)
    print("FAIL: " + msg)


def width(path):
    data = path.read_bytes()
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        return struct.unpack(">I", data[16:20])[0]
    if data[:2] == b"\xff\xd8":
        i = 2
        while i + 9 < len(data):
            if data[i] != 0xFF:
                i += 1
                continue
            marker = data[i + 1]
            if marker in (0xD8, 0x01) or 0xD0 <= marker <= 0xD7:
                i += 2
                continue
            length = struct.unpack(">H", data[i + 2:i + 4])[0]
            if 0xC0 <= marker <= 0xCF and marker not in (0xC4, 0xC8, 0xCC):
                return struct.unpack(">H", data[i + 7:i + 9])[0]
            i += 2 + length
    return None


def main(argv):
    if len(argv) != 2:
        print(__doc__)
        return 2
    root = Path(argv[0]) / "fastlane/metadata/android"
    code = argv[1]
    locales = sorted(p for p in root.glob("*") if p.is_dir()) if root.is_dir() else []
    if not locales or not (root / "en-US").is_dir():
        fail("%s has no en-US listing" % root)
        return 1
    for loc in locales:
        name = loc.name

        def text(f):
            p = loc / f
            return p.read_text(encoding="utf-8").strip() if p.is_file() else None
        title, short, full = text("title.txt"), text("short_description.txt"), text("full_description.txt")
        for label, value, limit in (("title", title, 30), ("short description", short, 80), ("full description", full, 4000)):
            if value is None:
                fail("%s: no %s" % (name, label))
            elif not value or len(value) > limit:
                fail("%s: %s is %d characters (1 to %d)" % (name, label, len(value), limit))
            else:
                ok("%s: %s, %d characters" % (name, label, len(value)))
        log = loc / "changelogs" / ("%s.txt" % code)
        if not log.is_file():
            fail("%s: changelogs/%s.txt is missing (the file is named after the versionCode)" % (name, code))
        elif len(log.read_bytes()) > 500 or not log.read_text(encoding="utf-8").strip():
            fail("%s: changelogs/%s.txt is empty or over 500 bytes" % (name, code))
        else:
            ok("%s: changelog for versionCode %s, %d bytes" % (name, code, len(log.read_bytes())))
        shots = sorted((loc / "images" / "phoneScreenshots").glob("*")) if (loc / "images" / "phoneScreenshots").is_dir() else []
        if name == "en-US" and len(shots) < 2:
            fail("en-US: at least two real phone screenshots are expected (%d found)" % len(shots))
        for shot in shots:
            w = width(shot)
            if shot.suffix.lower() not in (".jpg", ".jpeg", ".png") or w is None:
                fail("%s: %s is not a JPEG or PNG" % (name, shot.name))
            elif shot.stat().st_size > 150 * 1024 or w > 1280:
                fail("%s: %s is %d KB, %s px wide (limit 150 KB, 1280 px; a phone-sized portrait may be up to 1080 px wide)" % (name, shot.name, shot.stat().st_size // 1024, w))
        if shots:
            ok("%s: %d screenshots within the picture rule" % (name, len(shots)))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
