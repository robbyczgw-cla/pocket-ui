#!/usr/bin/env python3
"""Host test of scripts/release-routes.py without root, chroot or a device: how each payload is laid out, the hakuX seed,
the refusal of a program without the probe, the test tree never leaving its root, where each emulator's files are looked
for, the verdict per route, and that the expected table is well formed (scripts/release-routes.json).

The 25.11.2026 class: a payload without hakuX (or with bsnes-hd only in ROCKNIX's base) must be a red route, not a quiet
fallback that "still starts something"."""
import importlib.util
import io
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("routes", os.path.join(HERE, "release-routes.py"))
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)

fails = 0


def check(name, cond, detail=""):
    global fails
    print(("ok   " if cond else "FAIL ") + name + ("" if cond else f": {detail}"))
    fails += 0 if cond else 1


ELF = b"\x7fELF" + b"\0" * 64
PROBE = ELF + b"... --emulator-routes ..."


def sums(files):
    import hashlib
    return "".join(f"{hashlib.sha256(d).hexdigest()}  ./{n}\n" for n, d in files.items()).encode()


def tar_bytes(members):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as t:
        for name, data in members.items():
            info = tarfile.TarInfo(name)
            if isinstance(data, tuple):  # ("link", target)
                info.type, info.linkname = tarfile.SYMTYPE, data[1]
                t.addfile(info)
            else:
                info.size = len(data)
                t.addfile(info, io.BytesIO(data))
    return buf.getvalue()


with tempfile.TemporaryDirectory(prefix="test-release-routes-") as tmp:
    b = R.Bundle(os.path.join(tmp, "bundle"))
    hakux = {"hakux_libretro.so": ELF, "hakux_libretro.info": b'display_version = "1.0"\n'}
    hakux["SHA256SUMS"] = sums(hakux)

    # install zip: files/ -> releases/<v>, hakux/ -> releases/<v>/hakux, the links install.sh makes
    zp = os.path.join(tmp, "install.zip")
    with zipfile.ZipFile(zp, "w") as z:
        z.writestr("pkg/files/pocket-ui", PROBE)
        z.writestr("pkg/files/cores/gpsp_libretro.so", ELF)
        for n, d in hakux.items():
            z.writestr(f"pkg/hakux/{n}", d)
    d = R.payload_from(b, "rocknix-install", zp, "9.9.9")
    paths = {e[0]: e for e in d["entries"]}
    check("install: programs in releases/<version>", "/storage/pocket-ui/releases/9.9.9/pocket-ui" in paths, sorted(paths))
    check("install: hakux beside them", "/storage/pocket-ui/releases/9.9.9/hakux/hakux_libretro.so" in paths)
    check("install: current and the program links", paths.get("/storage/pocket-ui/current", [0, 0, 0, ""])[3] == "releases/9.9.9"
          and paths.get("/storage/pocket-ui/pocket-ui", [0, 0, 0, ""])[3] == "current/pocket-ui")
    check("install: hakux seeded when it matches its SHA256SUMS (./ names)", d.get("seed", {}).get("ok") is True, d.get("seed"))
    check("install: the program keeps its bytes for the run", R.blob_data(b.out, paths["/storage/pocket-ui/releases/9.9.9/pocket-ui"][4]) == PROBE)
    check("install: a big core is only a placeholder with its ELF header",
          R.blob_data(b.out, paths["/storage/pocket-ui/releases/9.9.9/cores/gpsp_libretro.so"][4])[:4] == b"\x7fELF")
    # 25.11.2026: a 4-byte placeholder hid DSperate's machine from dArkOS's systems.py, so the check saw NDS on drastic
    big = R.AARCH64_HEAD + b"\0" * (R.SMALL + 1)
    p_ = R.Payload(b, "x", "x", zp)
    p_.add_file("/c/dsperate_libretro.so", big)
    check("payload: a big core's placeholder keeps the whole ELF head (class and machine)",
          R.blob_data(b.out, p_.entries["/c/dsperate_libretro.so"][4]) == R.AARCH64_HEAD)

    bad = dict(hakux, **{"hakux_libretro.so": ELF + b"changed"})
    zp2 = os.path.join(tmp, "install-bad.zip")
    with zipfile.ZipFile(zp2, "w") as z:
        z.writestr("pkg/files/pocket-ui", PROBE)
        for n, dd in bad.items():
            z.writestr(f"pkg/hakux/{n}", dd)
    d2 = R.payload_from(b, "rocknix-install", zp2, "9.9.9")
    check("install: a hakux that fails its sums is not seeded, and says so",
          d2.get("seed", {}).get("ok") is False and any("SHA256SUMS" in n for n in d2["notes"]), d2.get("seed"))

    # card seed: a /storage tree inside the card zip
    cz = os.path.join(tmp, "card.zip")
    with zipfile.ZipFile(cz, "w") as z:
        z.writestr("pocket-ui-seed.tar.gz", tar_bytes({"./pocket-ui/releases/9.9.9/pocket-ui": PROBE,
                                                       "./pocket-ui/current": ("link", "releases/9.9.9")}))
    d = R.payload_from(b, "rocknix-card", cz, "9.9.9")
    paths = {e[0]: e for e in d["entries"]}
    check("card: the seed lands in /storage", "/storage/pocket-ui/releases/9.9.9/pocket-ui" in paths and paths["/storage/pocket-ui/current"][1] == "l")

    # OTA and the offline update: the same tarball, its hash recorded for the byte compare
    ota = os.path.join(tmp, "pocket-ui-9.9.9-aarch64.tar.gz")
    with open(ota, "wb") as f:
        f.write(tar_bytes({"pocket-ui": PROBE, "cores/dsperate_libretro.so": ELF}))
    up = os.path.join(tmp, "update.zip")
    with zipfile.ZipFile(up, "w") as z:
        z.write(ota, "pocket-ui-9.9.9-aarch64.tar.gz")
    d_ota = R.payload_from(b, "rocknix-ota", ota, "9.9.9")
    d_up = R.payload_from(b, "rocknix-update", up, "9.9.9")
    check("ota: unpacked into releases/<version>", any(e[0] == "/storage/pocket-ui/releases/9.9.9/cores/dsperate_libretro.so" for e in d_ota["entries"]))
    check("update zip: carries the OTA tarball byte for byte", d_up["inner_sha256"] == d_ota["sha256"])

    # Brick update: every asset its list names counts as there, carried or not
    app = tar_bytes({"bin/pocket-ui": PROBE, "asset-packages.json": json.dumps({"assets": [
        {"path": "cores/gpsp_libretro.so", "size": 9}, {"path": "cores/snes9x_libretro.so", "size": 9}]}).encode()})
    bz = os.path.join(tmp, "trimui.zip")
    with zipfile.ZipFile(bz, "w") as z:
        z.writestr("pocket-ui-brick-9.9.9.tar.gz", app)
        z.writestr("assets/brick-asset-x.tar.gz", tar_bytes({"cores/gpsp_libretro.so": ELF}))
    d = R.payload_from(b, "brick-update", bz, "9.9.9")
    paths = {e[0]: e for e in d["entries"]}
    check("brick update: listed and carried asset", "/mnt/SDCARD/pocket-ui/app/cores/gpsp_libretro.so" in paths)
    check("brick update: listed asset kept from the card, as the port's aarch64 build",
          paths.get("/mnt/SDCARD/pocket-ui/app/cores/snes9x_libretro.so", [0, 0, 0, 0, ""])[4] == "magic:" + R.AARCH64_HEAD.hex())

    # a program without the probe (0.7.1) is refused before anything runs: it would start its whole UI instead
    old = os.path.join(tmp, "old.zip")
    with zipfile.ZipFile(old, "w") as z:
        z.writestr("pkg/files/pocket-ui", ELF + b"no probe here")
    exp = os.path.join(tmp, "expected.json")
    with open(exp, "w") as f:
        json.dump({"families": {}}, f)
    r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "release-routes.py"), "bundle", "--version", "9.9.9", "--bases", tmp,
                        "--out", os.path.join(tmp, "b2"), "--expected", exp, f"rocknix-install={old}"], capture_output=True, text=True)
    check("bundle: a pocket-ui without --emulator-routes is refused (exit 2)", r.returncode == 2 and "no --emulator-routes" in r.stderr, r.stderr[-200:])

    # the tree: absolute links of the base resolve inside it, never on the host
    root = os.path.join(tmp, "root")
    os.makedirs(root)
    t = R.Tree(root)
    t.mkdir("/usr/lib")
    t.link("/lib", "/usr/lib")
    t.write("/lib/libc.so.6", b"x")
    check("tree: a write through /lib -> /usr/lib stays in the tree", os.path.isfile(os.path.join(root, "usr/lib/libc.so.6")))
    t.link("/escape", "/../../../../etc")
    check("tree: .. above the root stays at the root", t.resolve("/escape/passwd") == "/etc/passwd" and t.host("/escape/passwd").startswith(root))

    # where each emulator's files are looked for (src/store.rs core_path, src/emu_choice.rs installed)
    t.write("/storage/pocket-ui/releases/9.9.9/pocket-ui", PROBE)
    t.link("/storage/pocket-ui/current", "releases/9.9.9")
    t.link("/storage/pocket-ui/pocket-ui", "current/pocket-ui")
    t.write("/storage/pocket-ui/releases/9.9.9/pocket-core", ELF)
    t.link("/storage/pocket-ui/pocket-core", "current/pocket-core")
    t.write("/storage/pocket-ui/releases/9.9.9/cores/gpsp_libretro.so", ELF)
    t.write("/usr/lib/libretro/mgba_libretro.so", ELF)
    t.write("/usr/bin/retroarch", ELF)
    t.write("/usr/bin/start_dolphin_gc.sh", b"#!/bin/sh")
    fam = {}
    exe = "/storage/pocket-ui/pocket-ui"
    check("find: gpSP beside the running program", R.find_core(t, fam, exe, "pocket-core", "gpsp", "gba")[1].endswith("releases/9.9.9/cores/gpsp_libretro.so"))
    check("find: a base core", R.find_core(t, fam, exe, "pocket-core", "mgba", "gba")[1] == "/usr/lib/libretro/mgba_libretro.so")
    check("find: a missing core is None", R.find_core(t, fam, exe, "pocket-core", "snes9x", "snes") is None)
    check("find: Dolphin's GameCube script", R.find_core(t, fam, exe, "dolphin", "dolphin-sa-gc", "gamecube") is None,
          "dolphin-emu-nogui is missing, so it must not count")
    t.write("/usr/bin/dolphin-emu-nogui", ELF)
    check("find: Dolphin with its program", R.find_core(t, fam, exe, "dolphin", "dolphin-sa-gc", "gamecube") == ["/usr/bin/start_dolphin_gc.sh", "/usr/bin/dolphin-emu-nogui"])
    check("find: hakux needs core, .info and RetroArch", R.find_core(t, fam, exe, "hakux", "hakux", "xbox") is None)

    # the verdict per route
    check("judge: same route, ready, right source", R.judge(["pocket-core", "gpsp", "payload"], ["pocket-core", "gpsp", "ready"], True, ["x", "y"], "payload") == [])
    w = R.judge(["hakux", "hakux", "payload"], ["hakux", "hakux", "missing"], True, None, None)
    check("judge: Xbox without hakuX in the payload is red (25.11.)", any("missing" in x for x in w) and any("neither" in x for x in w), w)
    w = R.judge(["hakux", "hakux", "payload"], ["xemu", "xemu-sa", "ready"], True, ["/usr/bin/start_xemu.sh"], "base")
    check("judge: a quiet switch to xemu is red even though xemu could start", any("starts xemu/xemu-sa" in x for x in w), w)
    w = R.judge(["pocket-core", "bsnes_hd_beta", "payload"], ["pocket-core", "bsnes_hd_beta", "ready"], True, ["p", "/usr/lib/libretro/b"], "base")
    check("judge: bsnes-hd only in ROCKNIX's base is red (pocket-ui promises it)", w == ["comes from the base, expected from the payload"], w)
    check("judge: a system nobody expects is red", R.judge(None, ["retroarch", "x", "ready"], True, None, None) != [])
    check("judge: an expected system without a route is red", R.judge(["retroarch", "x", "base"], None, True, None, None) != [])
    check("judge: unsupported stays unsupported", R.judge(["hakux", "hakux", "unsupported"], ["hakux", "hakux", "unsupported"], True, None, None) == [])

    # dArkOS: a base from a mounted root partition keeps each core's ELF head (systems.py offers only aarch64 cores) and the
    # card's folders from roms.tar; the run lays the heads back so the port's own systems.py reads them as on the device
    img = os.path.join(tmp, "darkos-root")
    A64 = b"\x7fELF\x02\x01\x01" + b"\0" * 11 + b"\xb7\x00"
    A32 = b"\x7fELF\x01\x01\x01" + b"\0" * 11 + b"\x28\x00"
    for rel, data in {"home/user/.config/retroarch/cores/gpsp_libretro.so": A64 + b"rest",
                      "home/user/.config/retroarch/cores/flycast_libretro.so": A32 + b"rest",
                      "home/user/.config/retroarch/cores/yabasanshiro_libretro.so": A64,
                      "usr/local/bin/saturn.sh": b"#!/bin/sh\n", "usr/local/bin/retroarch": b"#!/bin/sh\n",
                      # the cores sit in ark's home, wherever /etc/passwd puts it (start.sh: getent passwd ark)
                      "etc/passwd": b"root:x:0:0:root:/root:/bin/bash\nark:x:1002:1002::/home/user:/bin/bash\n",
                      "etc/emulationstation/es_systems.cfg": (
                          "<systemList>"
                          "<system><name>gba</name><path>/roms/gba/</path><extension>.gba .zip</extension>"
                          "<command>sudo perfmax %GOVERNOR% %ROM%; nice -n -19 /usr/local/bin/%EMULATOR% -L /home/user/.config/%EMULATOR%/cores/%CORE%_libretro.so %ROM%; sudo perfnorm</command>"
                          "<emulators><emulator name=\"retroarch\"><cores><core>mgba</core><core>gpsp</core></cores></emulator></emulators></system>"
                          "<system><name>saturn</name><path>/roms/saturn/</path><extension>.cue</extension>"
                          "<command>sudo perfmax %GOVERNOR% %ROM%; nice -n -19 /usr/local/bin/saturn.sh %EMULATOR% %CORE% %ROM%; sudo perfnorm</command>"
                          "<emulators><emulator name=\"standalone-nobios\"><cores><core>yabasanshiro</core></cores></emulator></emulators></system>"
                          "<system><name>dreamcast</name><path>/roms/dreamcast/</path><extension>.cdi</extension>"
                          "<command>/usr/local/bin/dreamcast.sh %ROM%</command></system>"
                          "</systemList>").encode()}.items():
        os.makedirs(os.path.dirname(os.path.join(img, rel)), exist_ok=True)
        with open(os.path.join(img, rel), "wb") as f:
            f.write(data)
    os.symlink("/roms/themes/", os.path.join(img, "etc/emulationstation/themes"))
    with tarfile.open(os.path.join(img, "roms.tar"), "w") as tf:
        for name in ("roms", "roms/bios"):
            info = tarfile.TarInfo(name)
            info.type = tarfile.DIRTYPE
            tf.addfile(info)
    ents, files, heads = R.dir_entries(img)
    kinds = {e[0]: e[1] for e in ents}
    check("darkos base: links are recorded, not followed", kinds.get("etc/emulationstation/themes") == "l", kinds.get("etc/emulationstation/themes"))
    check("darkos base: each core's ELF head", R.base64.b64decode(heads["home/user/.config/retroarch/cores/gpsp_libretro.so"]) == A64, heads)
    check("darkos base: ES list and start scripts with their content",
          "etc/emulationstation/es_systems.cfg" in files and "usr/local/bin/saturn.sh" in files, sorted(files))
    out = os.path.join(tmp, "bases")
    r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "release-routes.py"), "base", img, "dk-test", out], capture_output=True, text=True)
    check("darkos base: a folder needs the image it came from (its hash)", r.returncode == 2 and "--source-file" in r.stderr, r.stderr[-200:])
    srcimg = os.path.join(tmp, "image.7z.001")
    with open(srcimg, "wb") as f:
        f.write(b"image")
    r = subprocess.run([sys.executable, "-I", os.path.join(HERE, "release-routes.py"), "base", img, "dk-test", out,
                        "--source-file", srcimg, "--with-tar", "roms.tar"], capture_output=True, text=True)
    base = R.load_json_gz(os.path.join(out, "dk-test.json.gz")) if r.returncode == 0 else {"entries": [], "source": ""}
    check("darkos base: roms.tar's card folders join it, the image's hash recorded",
          any(e[0] == "roms/bios" for e in base["entries"]) and base["source"] == "image.7z.001", r.stderr[-300:])

    droot = os.path.join(tmp, "dtree")
    os.makedirs(droot)
    dt = R.Tree(droot)
    R.lay_base(dt, base, False)
    check("darkos run: the laid core keeps its head", dt.read("/home/user/.config/retroarch/cores/gpsp_libretro.so")[:20] == A64)
    app = "/roms/ports/pocket-ui/app"
    with open(os.path.join(HERE, "..", "deploy", "darkos", "systems.py"), "rb") as f:
        dt.write(f"{app}/port-data/systems.py", f.read())
    dt.write(f"{app}/bin/cores/dsperate_libretro.so", A64)
    dt.write(f"{app}/bin/pocket-core", ELF)
    dt.mkdir("/roms/ports/pocket-ui/storage")
    dt.link("/storage", "/roms/ports/pocket-ui/storage")
    dfam = {"darkos": {"app": app, "es_systems": ["~ark/.emulationstation/es_systems.cfg", "/etc/emulationstation/es_systems.cfg"],
                       "cores": "~ark/.config/retroarch/cores"}}
    probs = []
    env = R.prepare_darkos(dt, dfam, probs, "test")
    check("darkos run: start.sh's steps work in the tree", env is not None and not probs, probs)
    shelf = R.es_systems(dt.read("/storage/.config/emulationstation/es_systems.cfg").decode()) if env else {}
    check("darkos run: the port's systems.py writes the shelf of every folder the card has", set(shelf) == {"gba", "saturn", "dreamcast"}, shelf)
    check("darkos run: each shelf folder shows the card's", dt.resolve("/storage/roms/gba") == "/roms/gba", dt.resolve("/storage/roms/gba"))
    gba_core = (env or {}).get("POCKETUI_GBA_CORE", "")
    check("darkos run: GBA's gpSP is the image's, through the folder start.sh makes",
          dt.resolve(gba_core) == "/home/user/.config/retroarch/cores/gpsp_libretro.so", gba_core)
    check("darkos run: DSperate from the port in the same folder",
          dt.resolve(env["POCKETUI_CORES"] + "/dsperate_libretro.so") == f"{app}/bin/cores/dsperate_libretro.so" if env else False)
    shelf_text = dt.read("/storage/.config/emulationstation/es_systems.cfg").decode() if env else ""
    check("darkos run: a 32-bit core is not offered to pocket-core (systems.py)", "<core>flycast</core>" not in shelf_text, shelf_text[-300:])
    check("darkos run: dArkOS's command as host-launch.py fills it",
          dfam.get("darkos_commands", {}).get("saturn", "").endswith("/usr/local/bin/saturn.sh standalone-nobios yabasanshiro %ROM%; sudo perfnorm"),
          dfam.get("darkos_commands"))
    check("darkos find: dArkOS's own Saturn script must be in the image",
          R.find_core(dt, dfam, app + "/bin/pocket-ui", "darkos", "", "saturn", env) == ["/usr/local/bin/saturn.sh"])
    check("darkos find: a command whose program is missing is None",
          R.find_core(dt, dfam, app + "/bin/pocket-ui", "darkos", "", "dreamcast", env) is None)
    dt.write("/usr/local/bin/dolphin.sh", b"#!/bin/sh\n")
    dfam["darkos_commands"]["gamecube"] = "sudo perfmax %GOVERNOR% %ROM%; nice -n -19 dolphin.sh standalone %ROM%; sudo perfnorm"
    check("darkos find: a program called by its bare name is found in PATH",
          R.find_core(dt, dfam, app + "/bin/pocket-ui", "darkos", "", "gamecube", env) == ["/usr/local/bin/dolphin.sh"])
    check("darkos programs: the governor helpers and placeholders are not files",
          R.darkos_programs("sudo perfmax %GOVERNOR% %ROM%; nice -n -19 /usr/local/bin/retroarch -L /c/x_libretro.so %ROM%; sudo perfnorm")
          == ["/usr/local/bin/retroarch", "/c/x_libretro.so"])
    check("core file: the libretro core, not RetroArch or the script that loads it",
          R.core_file(["/usr/local/bin/retroarch", "/c/x_libretro.so"]) == "/c/x_libretro.so")

    # darkos zip: the port's folder under /roms/ports
    dz = os.path.join(tmp, "darkos.zip")
    with zipfile.ZipFile(dz, "w") as z:
        z.writestr("pocket-ui.sh", b"#!/bin/sh\n")
        z.writestr("pocket-ui/app/bin/pocket-ui", PROBE)
    d = R.payload_from(b, "darkos-zip", dz, "9.9.9")
    check("darkos zip: the port lands in /roms/ports", any(e[0] == "/roms/ports/pocket-ui/app/bin/pocket-ui" for e in d["entries"]))

    # documented exceptions: green, but carried along to be shown
    plain, why = R.split_exceptions({"saturn": ["yabasanshiro", "yabasanshiro-sa", "base", "no BIOS"], "gb": ["pocket-core", "gambatte", "base"], "x": None})
    check("exceptions: the route is the first three, the reason kept apart",
          plain == {"saturn": ["yabasanshiro", "yabasanshiro-sa", "base"], "gb": ["pocket-core", "gambatte", "base"]} and why == {"saturn": "no BIOS"},
          (plain, why))

    # es_systems.cfg: folder and first extension
    es = R.es_systems("<systemList><system><name>gba</name><path>/storage/roms/gba</path><extension>.gba .GBA .zip</extension></system></systemList>")
    check("es_systems: folder and first extension", es == {"gba": ("/storage/roms/gba", ".gba")}, es)

# the expected table
with open(os.path.join(HERE, "release-routes.json")) as f:
    table = json.load(f)
keys = set()
for src in ("device_model.rs", "darkos.rs"):
    with open(os.path.join(HERE, "..", "src", src)) as f:
        keys |= set(re.findall(r'board\("([^"]+)"', f.read()))
keys |= {"miniv2", "mini", "rp5", "rp-classic", "flip2", "rp6", "nova", "odin2", "odin2mini", "odin2portal", "rgdsplus", "rgrotate", "brick", "smartpro", "brickpro"}
kinds = {"rocknix-install", "rocknix-card", "rocknix-ota", "rocknix-update", "brick-card", "brick-update", "darkos-zip", "darkos-update"}
problems = []
for name, fam in table["families"].items():
    for g in fam.get("use", []):
        if g not in table["groups"]:
            problems.append(f"{name}: no group {g}")
    routes = {}
    for g in fam.get("use", []):
        routes.update(table["groups"].get(g, {}))
    routes.update(fam.get("routes", {}))
    problems += [f"{name}: no shared variant {v}" for v in fam.get("use_variants", []) if v not in table.get("variants", {})]
    for v in list(fam.get("variants", {}).values()) + [table.get("variants", {}).get(v, {"routes": {}}) for v in fam.get("use_variants", [])]:
        routes.update({f"variant {k}": r for k, r in v["routes"].items()})
    for system, r in routes.items():
        # [emulator, core, source] and, for a documented exception, its reason (a sentence that names where it is documented)
        if r is not None and (len(r) not in (3, 4) or r[2] not in ("payload", "base", "unsupported")
                              or (len(r) == 4 and not (isinstance(r[3], str) and len(r[3]) > 20))):
            problems.append(f"{name}/{system}: {r}")
    if fam.get("darkos") and not fam.get("base"):
        problems.append(f"{name}: a dArkOS family without its base")
    problems += [f"{name}: unknown device {d}" for d in fam["devices"] if d not in keys]
    problems += [f"{name}: unknown payload {k}" for k in fam["payloads"] if k not in kinds]
    problems += [f"{name}: hakux device {d} not tested" for d in fam.get("hakux_devices", []) if d not in fam["devices"]]
    if not fam.get("base") and "rocknix-install" in fam["payloads"]:
        problems.append(f"{name}: a ROCKNIX family without its base")
check("table: groups, routes, devices and payload kinds are known", not problems, problems)
xbox = {n for n, f in table["families"].items() if "xbox" in f.get("routes", {})}
check("table: Xbox is expected on the Snapdragon families with hakuX from the payload",
      {"ROCKNIX SM8250", "ROCKNIX SM8550"} <= xbox and all(table["families"][n]["routes"]["xbox"][2] == "payload" for n in xbox))
# every family checks the systems of 25.11.2026's second round (Saturn, PICO-8, Ports, DOS, ScummVM) and the dArkOS port is one
fam_routes = {n: {s for g in f.get("use", []) for s, r in table["groups"][g].items() if r} | {s for s, r in f.get("routes", {}).items() if r}
              for n, f in table["families"].items()}
lacking = {n: sorted({"saturn", "pico-8", "ports", "pc", "scummvm"} - r) for n, r in fam_routes.items()}
check("table: Saturn, PICO-8, Ports, DOS and ScummVM in every family", not any(lacking.values()), lacking)
check("table: the dArkOS port is checked (darkos-zip and darkos-update)",
      any({"darkos-zip", "darkos-update"} <= set(f["payloads"]) and f.get("darkos") for f in table["families"].values()))
sat = [f"{n}/{v}" for n, f in table["families"].items() if n.startswith("ROCKNIX") for v in ["saturn-bios"] if v not in f.get("use_variants", [])]
check("table: every ROCKNIX family also checks Saturn with a BIOS (pocket-core)", not sat, sat)
sys.exit(1 if fails else 0)
