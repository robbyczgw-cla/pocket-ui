#!/usr/bin/env python3
"""Checks of a finished Android APK that need no Android SDK (scripts/android/release-check.sh calls this beside apksigner/aapt).

    apk_audit.py APK [--sources DIR] [--max-bytes N]

Prints `ok: ...` / `FAIL: ...` lines; exit 0 when every check passed, 1 otherwise.
  - every packaged native library is an ARM64 ELF whose LOAD segments are aligned to 16 KB (Android 15+ may use 16 KB pages)
    and whose offsets agree with its addresses modulo that alignment; no other ABI folder, no shared C++ runtime
  - a stored (not deflated) library starts on a 16 KB boundary of the zip
  - the size is within the budget
  - assets/cores.json: each core names the sha256 of the library in the APK, and was built with ANDROID_RELEASE_SOURCES=1
    (it carries the sha256 of its corresponding-source archive); with --sources every archive exists with that hash
  - the licence files are packed, with the written source offer and the list of cores the app's Licences page shows: it names
    every core of cores.json and of core-downloads.json, and each bundled core's licence texts are in the APK
  - no game, BIOS or disc image is inside
"""
import hashlib
import json
import struct
import sys
import zipfile
from pathlib import Path

PAGE = 16384
ROM_SUFFIXES = {".gb", ".gbc", ".gba", ".nes", ".sfc", ".smc", ".md", ".gen", ".bin", ".iso", ".cue", ".chd", ".z64", ".n64",
                ".v64", ".nds", ".sms", ".gg", ".pce", ".ws", ".wsc", ".ngp", ".ngc", ".lnx", ".a26", ".pbp", ".cso", ".rom",
                ".cdi", ".gdi", ".m3u"}
REQUIRED = ["classes.dex", "AndroidManifest.xml", "assets/cores.json", "assets/licences/LICENSE", "assets/licences/NOTICE.md",
            "assets/licences/ANDROID-CORES.md", "assets/licences/RUST-LICENSES.txt", "assets/licences/PLAYER-RUST-LICENSES.txt",
            "assets/licences/SOURCE-OFFER.txt", "assets/licences/CORES.tsv", "assets/core-channel.url",
            "lib/arm64-v8a/libpocket_android.so", "lib/arm64-v8a/libpocket_core_android.so"]
PSP_REQUIRED = {"compat.ini", "font_atlas.zim", "ppge_atlas.zim", "flash0/font/ltn0.pgf", "vfpu/vfpu_sin_lut8192.dat"}
# Pinned source assets whose suffix also names a ROM format: an RGBA texture and translator notes.
# Modified files or another path remain forbidden.
PSP_NON_ROM_ASSETS = {
    "assets/system/PPSSPP/shaders/smiley_16x16_rgba.bin": "898a2064aeac496e82c18d9fa86547a69a069152154a6134ad12680623f0a0df",
    "assets/system/PPSSPP/lang/README.md": "5bce8cf4a2bd04da57dddfc2eacd27c9e01173baad2c1f3ed0d3a3859e4843d7",
}
problems = []


def ok(msg):
    print("ok: " + msg)


def fail(msg):
    problems.append(msg)
    print("FAIL: " + msg)


def load_segments(data):
    """(p_align, p_offset, p_vaddr) of every PT_LOAD of an ELF64 little-endian image; raises ValueError otherwise."""
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise ValueError("not a 64-bit little-endian ELF")
    machine = struct.unpack_from("<H", data, 18)[0]
    if machine != 183:
        raise ValueError("machine %d, not AArch64" % machine)
    phoff, = struct.unpack_from("<Q", data, 0x20)
    phentsize, phnum = struct.unpack_from("<HH", data, 0x36)
    loads = []
    for i in range(phnum):
        at = phoff + i * phentsize
        p_type, _flags, p_offset, p_vaddr, _paddr, _filesz, _memsz, p_align = struct.unpack_from("<IIQQQQQQ", data, at)
        if p_type == 1:
            loads.append((p_align, p_offset, p_vaddr))
    if not loads:
        raise ValueError("no LOAD segment")
    return loads


def check_elf(name, data):
    try:
        loads = load_segments(data)
    except (ValueError, struct.error) as e:
        fail("%s: %s" % (name, e))
        return False
    small = [hex(a) for a, _, _ in loads if a < PAGE]
    skew = [hex(v) for a, o, v in loads if a >= PAGE and (o % a) != (v % a)]
    if small:
        fail("%s: LOAD alignment %s is below 16 KB (link with -Wl,-z,max-page-size=16384)" % (name, ", ".join(small)))
    elif skew:
        fail("%s: LOAD segment at %s: file offset and address differ modulo the alignment" % (name, ", ".join(skew)))
    return not small and not skew


def data_offset(path, info):
    with path.open("rb") as f:
        f.seek(info.header_offset)
        header = f.read(30)
    name_len, extra_len = struct.unpack_from("<HH", header, 26)
    return info.header_offset + 30 + name_len + extra_len


def main(argv):
    args = list(argv)
    sources = None
    max_bytes = None
    if "--sources" in args:
        i = args.index("--sources")
        sources = Path(args[i + 1])
        del args[i:i + 2]
    if "--max-bytes" in args:
        i = args.index("--max-bytes")
        max_bytes = int(args[i + 1])
        del args[i:i + 2]
    if len(args) != 1:
        print(__doc__)
        return 2
    apk = Path(args[0])
    if not apk.is_file():
        print("FAIL: no such file: %s" % apk)
        return 1
    size = apk.stat().st_size
    if max_bytes is not None:
        (ok if size <= max_bytes else fail)("size %.1f MB (limit %.1f MB)" % (size / 1e6, max_bytes / 1e6))
    with zipfile.ZipFile(apk) as z:
        names = z.namelist()
        for need in REQUIRED:
            if need not in names:
                fail("%s is missing from the APK" % need)
        libs = [n for n in names if n.startswith("lib/") and n.endswith(".so")]
        abis = sorted({n.split("/")[1] for n in libs})
        (ok if abis == ["arm64-v8a"] else fail)("native libraries: %d, ABIs %s" % (len(libs), abis))
        if any(n.endswith("libc++_shared.so") for n in libs):
            fail("the shared C++ runtime is packed (every library must carry its own)")
        bad = 0
        for n in libs:
            if not check_elf(n, z.read(n)):
                bad += 1
            info = z.getinfo(n)
            if info.compress_type == 0 and data_offset(apk, info) % PAGE:
                fail("%s is stored at zip offset %d, not on a 16 KB boundary (zipalign -P 16)" % (n, data_offset(apk, info)))
                bad += 1
        if not bad and libs:
            ok("%d native libraries are 16 KB aligned (LOAD segments)" % len(libs))
        roms = [n for n in names if Path(n).suffix.lower() in ROM_SUFFIXES and not n.startswith(("res/", "assets/licences/"))
                and not (n in PSP_NON_ROM_ASSETS and hashlib.sha256(z.read(n)).hexdigest() == PSP_NON_ROM_ASSETS[n])]
        (ok if not roms else fail)("no game, BIOS or disc image in the APK" if not roms else "files that look like games or BIOS: %s" % roms[:5])
        try:
            manifest = json.loads(z.read("assets/cores.json"))
        except (KeyError, ValueError) as e:
            fail("assets/cores.json: %s" % e)
            manifest = {"cores": []}
        cores = manifest.get("cores", [])
        for c in cores:
            if c.get("id") != "ppsspp":
                continue
            assets = c.get("system_assets", [])
            missing = PSP_REQUIRED - {a.get("path") for a in assets}
            changed = []
            for asset in assets:
                path = asset.get("path", "")
                name = "assets/system/PPSSPP/" + path
                if not path or Path(path).is_absolute() or ".." in Path(path).parts or name not in names:
                    changed.append(path)
                elif hashlib.sha256(z.read(name)).hexdigest() != asset.get("sha256") or z.getinfo(name).file_size != asset.get("bytes"):
                    changed.append(path)
            (ok if assets and not missing and not changed else fail)("PPSSPP system assets: %d verified files" % len(assets)
                    if assets and not missing and not changed else "PPSSPP system assets missing or damaged: %s" % sorted(missing | set(changed)))
        mismatched = [c["id"] for c in cores
                      if hashlib.sha256(z.read("lib/arm64-v8a/lib%s_libretro.so" % c["id"])).hexdigest() != c["sha256"]
                      ] if all(("lib/arm64-v8a/lib%s_libretro.so" % c["id"]) in names for c in cores) else ["a core library is missing"]
        (ok if cores and not mismatched else fail)("%d cores in cores.json, each with the hash of its packed library" % len(cores)
                                                   if cores and not mismatched else "cores.json and the packed libraries disagree: %s" % mismatched)
        stray = sorted(n for n in libs if n.endswith("_libretro.so") and n.split("/")[-1][3:-len("_libretro.so")] not in {c["id"] for c in cores})
        (ok if not stray else fail)("no core library outside cores.json" if not stray else "libraries cores.json does not list: %s" % stray)
        # The Licences page lists every core, the optional downloads too; each bundled core's own texts are packed.
        try:
            listed = {line.split("\t")[0] for line in z.read("assets/licences/CORES.tsv").decode().splitlines() if line.strip()}
            offered = {p.get("core") for p in json.loads(z.read("assets/core-downloads.json")).get("packages", [])} if "assets/core-downloads.json" in names else set()
        except KeyError:
            listed, offered = set(), set()
        unlisted = sorted(({c["id"] for c in cores} | offered) - listed)
        (ok if not unlisted and listed else fail)("the licence list names all %d cores" % len(listed) if not unlisted and listed
                                                 else "assets/licences/CORES.tsv lacks %s" % (unlisted or "every core"))
        no_text = sorted(c["id"] for c in cores if not c.get("notices") or any("assets/licences/" + n.get("asset", "?") not in names for n in c["notices"]))
        (ok if not no_text else fail)("every bundled core's licence texts are packed" if not no_text else "licence texts missing from the APK for %s" % no_text)
        no_source = [c["id"] for c in cores if not c.get("corresponding_source") or not c.get("source_sha256")]
        (ok if not no_source else fail)("every core names its corresponding-source archive (built with ANDROID_RELEASE_SOURCES=1)"
                                        if not no_source else "no source archive recorded for %s (rebuild with ANDROID_RELEASE_SOURCES=1)" % no_source)
        if sources is not None:
            missing = []
            for c in cores:
                f = sources / c.get("corresponding_source", "?")
                if not f.is_file() or hashlib.sha256(f.read_bytes()).hexdigest() != c.get("source_sha256"):
                    missing.append(c["id"])
            (ok if not missing else fail)("all %d corresponding-source archives are in %s with the recorded hash" % (len(cores), sources)
                                          if not missing else "source archive missing or changed for %s" % missing)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
