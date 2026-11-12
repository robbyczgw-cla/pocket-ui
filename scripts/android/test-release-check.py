#!/usr/bin/env python3
"""Regression test of scripts/android/apk_audit.py (the part of release-check.sh that needs no Android SDK), on small APKs built
here: the 0.3.1 QA build had every core linked for 4 KB pages and no source archive named in cores.json, and nothing looked.

    python3 -I scripts/android/test-release-check.py
"""
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

AUDIT = Path(__file__).resolve().parent / "apk_audit.py"
LISTING = Path(__file__).resolve().parent / "fastlane_check.py"


def elf(align, offset_skew=0):
    """A minimal AArch64 ELF64 with two LOAD segments of the given alignment."""
    phoff, phnum = 64, 2
    header = b"\x7fELF" + bytes([2, 1, 1, 0]) + bytes(8) + struct.pack("<HHIQQQIHHHHHH", 3, 183, 1, 0, phoff, 0, 0, 64, 56, phnum, 0, 0, 0)
    phdrs = b""
    for i in range(phnum):
        off = i * align
        phdrs += struct.pack("<IIQQQQQQ", 1, 5, off, off + offset_skew, off + offset_skew, 100, 100, align)
    return header + phdrs + bytes(64)


def make_apk(path, *, align=16384, skew=0, extra=None, source=True, core_hash_ok=True, stored_at_odd_offset=False, rom=False, abi="arm64-v8a", psp=False, psp_assets=None, licences=True, notices=True):
    core = elf(align, skew)
    cid = "ppsspp" if psp else "mgba"
    cores = {"cores": [{"id": cid, "sha256": hashlib.sha256(core).hexdigest() if core_hash_ok else "0" * 64}]}
    if notices:
        cores["cores"][0]["notices"] = [{"asset": cid + "--LICENSE", "sha256": "b" * 64}]
    if source:
        cores["cores"][0].update(corresponding_source="mgba-x.tar.gz", source_sha256="a" * 64)
    if psp_assets is not None:
        cores["cores"][0]["system_assets"] = [{"path": name, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)} for name, data in psp_assets.items()]
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name in ["classes.dex", "AndroidManifest.xml", "assets/licences/LICENSE", "assets/licences/NOTICE.md", "assets/licences/ANDROID-CORES.md",
                     "assets/licences/RUST-LICENSES.txt", "assets/licences/PLAYER-RUST-LICENSES.txt", "assets/licences/SOURCE-OFFER.txt",
                     "assets/core-channel.url"]:
            z.writestr(name, b"x")
        if licences:
            z.writestr("assets/licences/CORES.tsv", cid + "\tgba\tMPL-2.0\thttps://x\t" + "a" * 40 + "\tapk\t\n")
            for n in cores["cores"][0].get("notices", []):
                z.writestr("assets/licences/" + n["asset"], b"licence")
        z.writestr("assets/cores.json", json.dumps(cores))
        z.writestr("lib/%s/libpocket_android.so" % abi, elf(align, skew))
        z.writestr("lib/%s/libpocket_core_android.so" % abi, elf(align, skew))
        z.writestr("lib/%s/lib%s_libretro.so" % (abi, cid), core)
        for name, data in (psp_assets or {}).items():
            z.writestr("assets/system/PPSSPP/" + name, data)
        if stored_at_odd_offset:
            z.writestr(zipfile.ZipInfo("lib/arm64-v8a/libstored.so"), elf(align, skew), compress_type=zipfile.ZIP_STORED)
        if rom:
            z.writestr("assets/game.gba", b"rom")
        for name, data in (extra or {}).items():
            z.writestr(name, data)


def audit(apk, *args):
    r = subprocess.run([sys.executable, "-I", str(AUDIT), str(apk), *args], capture_output=True, text=True)
    return r.returncode, r.stdout


def rebuild_old_make_cache(d):
    """Run the actual builder against a tiny pinned source tree and an old 4 KB Make output."""
    tree = d / "core-builder"
    script = tree / "scripts/android/build-cores.py"
    script.parent.mkdir(parents=True)
    shutil.copyfile(AUDIT.parent / "build-cores.py", script)
    cache = d / "shared-core-cache"
    src = cache / "source/test"
    src.mkdir(parents=True)
    (tree / "android/build").mkdir(parents=True)
    (tree / "android/build/cores").symlink_to(cache, target_is_directory=True)
    (src / "LICENSE").write_text("Test source: permission to use, copy and distribute.\n")
    # Make's target predates the new linker flags. Without cache invalidation it remains 4 KB and the builder fails.
    (src / "Makefile").write_text("core.so: Makefile\n\t@printf '%s' '$(CC)' > link-command\n\t@cp aligned-template core.so\n")
    (src / "aligned-template").write_bytes(elf(16384))
    for args in (["init", "-q"], ["add", "LICENSE", "Makefile", "aligned-template"],
                 ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-qm", "fixture"]):
        subprocess.run(["git", "-C", str(src), *args], check=True)
    revision = subprocess.check_output(["git", "-C", str(src), "rev-parse", "HEAD"], text=True).strip()
    (src / "core.so").write_bytes(elf(4096))
    ndk = d / "ndk"
    tools = ndk / "toolchains/llvm/prebuilt" / ("darwin-x86_64" if sys.platform == "darwin" else "linux-x86_64") / "bin"
    tools.mkdir(parents=True)
    (ndk / "source.properties").write_text("Pkg.Revision = fixture\n")
    for name in ["NOTICE", "NOTICE.toolchain"]:
        (ndk / name).write_text("Test notice\n")
    reader = tools / "llvm-readelf"
    reader.write_text("#!" + sys.executable + "\nimport struct,sys\nfrom pathlib import Path\n"
                      "b=Path(sys.argv[-1]).read_bytes()\n"
                      "if '-h' in sys.argv: print('AArch64')\n"
                      "else:\n for i in range(2): print(' LOAD segment 0x%x' % struct.unpack_from('<Q',b,64+i*56+48)[0])\n")
    stripper = tools / "llvm-strip"
    stripper.write_text("#!/bin/sh\nexit 0\n")
    reader.chmod(0o755)
    stripper.chmod(0o755)
    lock = {"format": 1, "abi": "arm64-v8a", "api": 26, "ndk": "fixture", "cores": [
        {"id": "test", "source": "https://example.invalid/test", "revision": revision, "builder": "make",
         "binary": "core.so", "licence": "Test", "evidence": ["LICENSE"], "options": []}]}
    (tree / "android/cores.lock.json").write_text(json.dumps(lock))
    env = {**os.environ, "ANDROID_NDK_HOME": str(ndk), "ANDROID_RELEASE_SOURCES": "1", "ANDROID_CORE_JOBS": "1"}
    run = subprocess.run([sys.executable, "-I", str(script)], env=env, capture_output=True, text=True)
    if run.returncode:
        print(run.stdout + run.stderr)
        return False
    manifest = json.loads((cache / "manifest.json").read_text())["cores"][0]
    archive = cache / "corresponding-source" / manifest["corresponding_source"]
    lib = cache / "lib/libtest_libretro.so"
    # A released core starts on 16 KB devices, its binary hash agrees, and the exact source remains available.
    return (lib.read_bytes() == elf(16384) and "max-page-size=16384" in (src / "link-command").read_text()
            and hashlib.sha256(lib.read_bytes()).hexdigest() == manifest["sha256"]
            and hashlib.sha256(archive.read_bytes()).hexdigest() == manifest["source_sha256"])


def main():
    failures = []

    def expect(label, condition):
        print(("ok   " if condition else "FAIL ") + label)
        if not condition:
            failures.append(label)

    with tempfile.TemporaryDirectory() as d:
        d = Path(d)
        app = "    E: application (line=15)\n"
        flag = "      A: android:allowNativeHeapPointerTagging(0x01010603)=(type 0x12)"
        for label, tree, accepted in [
            ("fast-memory cores can start with the compiled heap policy", app + flag + "0x0\n", True),
            ("default heap tagging would break fast-memory fault addresses", app, False),
            ("explicit heap tagging is refused", app + flag + "0xffffffff\n", False),
            ("a child activity cannot opt the application out", app + "      E: activity (line=16)\n" + flag + "0x0\n", False),
        ]:
            run = subprocess.run([sys.executable, "-I", str(AUDIT.with_name("native-heap-policy.py"))], input=tree, text=True, capture_output=True)
            expect(label, (run.returncode == 0) == accepted)
        expect("an old Make cache is relinked for 16 KB devices with matching source evidence", rebuild_old_make_cache(d))
        make_apk(d / "good.apk")
        rc, out = audit(d / "good.apk", "--max-bytes", "30000000")
        expect("a 16 KB aligned APK with sources named passes", rc == 0 and "FAIL" not in out)
        make_apk(d / "old.apk", align=4096)
        rc, out = audit(d / "old.apk")
        expect("4 KB aligned libraries are refused, naming the library", rc == 1 and "libmgba_libretro.so" in out and "16 KB" in out)
        make_apk(d / "skew.apk", skew=4096)
        rc, out = audit(d / "skew.apk")
        expect("a segment whose offset and address disagree is refused", rc == 1 and "modulo" in out)
        make_apk(d / "nosrc.apk", source=False)
        rc, out = audit(d / "nosrc.apk")
        expect("a build without ANDROID_RELEASE_SOURCES=1 is refused", rc == 1 and "ANDROID_RELEASE_SOURCES=1" in out)
        make_apk(d / "hash.apk", core_hash_ok=False)
        rc, out = audit(d / "hash.apk")
        expect("cores.json that disagrees with a packed library is refused", rc == 1 and "disagree" in out)
        make_apk(d / "abi.apk", abi="armeabi-v7a")
        rc, out = audit(d / "abi.apk")
        expect("another ABI folder is refused", rc == 1)
        make_apk(d / "rom.apk", rom=True)
        rc, out = audit(d / "rom.apk")
        expect("a game inside the APK is refused", rc == 1 and "games or BIOS" in out)
        for extension in ["cdi", "gdi", "m3u"]:
            make_apk(d / "disc.apk", extra={"assets/game." + extension: b"disc"})
            rc, out = audit(d / "disc.apk")
            expect("a Sega disc or playlist inside the APK is refused: " + extension,
                   rc == 1 and "games or BIOS" in out)
        make_apk(d / "odd.apk", stored_at_odd_offset=True)
        rc, out = audit(d / "odd.apk")
        expect("a stored library off the 16 KB zip boundary is refused", rc == 1 and "boundary" in out)
        make_apk(d / "stray.apk", extra={"lib/arm64-v8a/libextra_libretro.so": elf(16384)})
        rc, out = audit(d / "stray.apk")
        expect("a core library that cores.json does not list is refused", rc == 1 and "does not list" in out)
        make_apk(d / "nolist.apk", licences=False)
        rc, out = audit(d / "nolist.apk")
        expect("an APK without the licence list of its cores is refused", rc == 1 and "CORES.tsv" in out and "licence texts missing" in out)
        make_apk(d / "nonotice.apk", notices=False)
        rc, out = audit(d / "nonotice.apk")
        expect("a core without any licence text is refused", rc == 1 and "licence texts missing from the APK for ['mgba']" in out)
        make_apk(d / "nooffer.apk")
        with zipfile.ZipFile(d / "nooffer.apk") as zin, zipfile.ZipFile(d / "nooffer2.apk", "w") as zout:
            for item in zin.infolist():
                if item.filename != "assets/licences/SOURCE-OFFER.txt":
                    zout.writestr(item, zin.read(item.filename))
        rc, out = audit(d / "nooffer2.apk")
        expect("an APK without the written source offer is refused", rc == 1 and "SOURCE-OFFER.txt is missing" in out)
        rc, out = audit(d / "good.apk", "--max-bytes", "100")
        expect("an APK over the size limit is refused", rc == 1 and "size" in out)
        (d / "src").mkdir()
        rc, out = audit(d / "good.apk", "--sources", str(d / "src"))
        expect("a missing source archive is refused", rc == 1 and "source archive missing" in out)
        (d / "src" / "mgba-x.tar.gz").write_bytes(b"not the recorded archive")
        rc, out = audit(d / "good.apk", "--sources", str(d / "src"))
        expect("a source archive with another hash is refused", rc == 1 and "source archive missing or changed" in out)
        make_apk(d / "psp-missing.apk", psp=True)
        rc, out = audit(d / "psp-missing.apk")
        expect("PSP without its dialog fonts, atlases and tables is refused", rc == 1 and "PPSSPP system assets" in out)
        psp = {name: b"pinned asset" for name in ["compat.ini", "font_atlas.zim", "ppge_atlas.zim", "flash0/font/ltn0.pgf", "vfpu/vfpu_sin_lut8192.dat"]}
        make_apk(d / "psp-complete.apk", psp=True, psp_assets=psp)
        rc, out = audit(d / "psp-complete.apk")
        expect("PSP with its complete verified system assets passes", rc == 0 and "PPSSPP system assets" in out)
        make_apk(d / "psp-damaged.apk", psp=True, psp_assets=psp, extra={"assets/system/PPSSPP/compat.ini": b"damaged"})
        rc, out = audit(d / "psp-damaged.apk")
        expect("a damaged PSP system asset is refused", rc == 1 and "PPSSPP system assets" in out)
        for name in ["assets/system/PPSSPP/lang/README.md", "assets/system/PPSSPP/shaders/smiley_16x16_rgba.bin"]:
            make_apk(d / "psp-fake-resource.apk", psp=True, psp_assets=psp, extra={name: b"a ROM disguised as a resource"})
            rc, out = audit(d / "psp-fake-resource.apk")
            expect("an unpinned file at %s is still refused" % name, rc == 1 and "games or BIOS" in out)
        # the store listing
        def listing(tree, code="40001"):
            r = subprocess.run([sys.executable, "-I", str(LISTING), str(tree), code], capture_output=True, text=True)
            return r.returncode, r.stdout
        tree = d / "tree"
        en = tree / "fastlane/metadata/android/en-US"
        (en / "changelogs").mkdir(parents=True)
        (en / "images/phoneScreenshots").mkdir(parents=True)
        (en / "title.txt").write_text("pocket-ui\n")
        (en / "short_description.txt").write_text("A shelf for retro games.\n")
        (en / "full_description.txt").write_text("Text.\n")
        (en / "changelogs/40001.txt").write_text("First preview.\n")
        for n in ("1.png", "2.png"):
            (en / "images/phoneScreenshots" / n).write_bytes(b"\x89PNG\r\n\x1a\n" + bytes(8) + struct.pack(">II", 1280, 720) + bytes(100))
        rc, out = listing(tree)
        expect("a complete store listing passes", rc == 0 and "FAIL" not in out)
        rc, out = listing(tree, "40002")
        expect("a versionCode without its changelog file is refused", rc == 1 and "40002.txt is missing" in out)
        (en / "short_description.txt").write_text("x" * 81)
        rc, out = listing(tree)
        expect("a short description over 80 characters is refused", rc == 1 and "short description" in out)
        (en / "short_description.txt").write_text("ok")
        (en / "images/phoneScreenshots/2.png").unlink()
        rc, out = listing(tree)
        expect("fewer than two screenshots are refused", rc == 1 and "screenshots" in out)
        (en / "images/phoneScreenshots/2.png").write_bytes(b"\x89PNG\r\n\x1a\n" + bytes(8) + struct.pack(">II", 2000, 720))
        rc, out = listing(tree)
        expect("a screenshot wider than 1280 px is refused", rc == 1 and "2.png" in out)
    if failures:
        print("%d check(s) failed" % len(failures))
        return 1
    print("release-check tests: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
