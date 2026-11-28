#!/usr/bin/env python3
"""Which emulator each release payload really starts, per device family and system, without hardware.

The release's own pocket-ui binary resolves every system in a test tree made of the unpacked payload and the pinned
ROCKNIX base, and the result is compared with the expected table scripts/release-routes.json. A different emulator or
core, a core or start script that is not in the payload or the base, or a core that comes from the wrong one (the
table says `payload` where pocket-ui promises to bundle it) is a red release. Docs: docs/DEVICE-TESTS.md
"Emulator routes of the release payloads".

  release-routes.py base SOURCE LABEL OUTDIR
      SOURCE is ROCKNIX's SYSTEM squashfs or its .img/.img.gz (the SYSTEM file is taken from the image's first
      partition with 7z), a rootfs .tar.gz, or a folder: dArkOS's root partition mounted read only from its image
      (`--source-file` names the downloaded image for the recorded hash). Writes OUTDIR/LABEL.json.gz: every path of
      the base with its type, small config files and start scripts with their content, the ELF head of each libretro
      core (dArkOS's port offers only 64-bit ones). Made once per pin; needs unsquashfs (and 7z).
  release-routes.py bundle --version V --bases DIR --out DIR KIND=PATH...
      Reads the payloads (KIND: rocknix-install, rocknix-card, rocknix-ota, rocknix-update, brick-card, brick-update,
      darkos-zip, darkos-update) and writes their installed layout, small files and the pocket-ui programs (xz) into
      the bundle folder, with the bases the table needs. Seconds; runs where the payloads were built.
  release-routes.py run BUNDLE EXPECTED [--receipt FILE]
      Runs as root in an aarch64 Linux container (tools/air-build.sh -x on the aarch64 build host): builds one tree per family,
      payload and device, runs `pocket-ui --emulator-routes` inside it with chroot and compares. Exit 1 on any
      mismatch, 2 when the inputs are wrong.

Only the Python standard library; the bundle and run steps never write outside their folders and /tmp/release-routes.
"""
import argparse
import base64
import concurrent.futures
import gzip
import hashlib
import io
import json
import lzma
import multiprocessing
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile
from pathlib import PurePosixPath

SMALL = 64 * 1024
# Config files keep their content up to this size (es_systems.cfg is ~100 KB).
CONFIG_MAX = 1024 * 1024
CONFIG = re.compile(r"\.(cfg|json|info|sh|conf|txt)$|/(RELEASE|VERSION|SHA256SUMS)$")
# Base files whose content the resolver or the launchers read (es_systems.cfg, system.cfg, start scripts, .info).
BASE_CONTENT = re.compile(r"^(etc/[^/]+|etc/profile\.d/.*|usr/config/emulationstation/[^/]+|usr/config/system/.*|usr/bin/[^/]+\.sh|usr/lib/libretro/[^/]+\.info"
                          r"|home/ark/\.emulationstation/.*|home/ark/\.config/retroarch/cores/[^/]+\.info|etc/emulationstation/.*"
                          r"|usr/local/bin/[^/]+\.sh|opt/[^/]+/[^/]+\.sh)$")
# The first bytes of a libretro core: ELF class and machine (deploy/darkos/systems.py takes only aarch64 cores).
HEAD = re.compile(r"_libretro\.so$")
HEAD_LEN = 20
AARCH64_HEAD = b"\x7fELF\x02\x01\x01" + b"\0" * 11 + b"\xb7\x00"
TREE_ROOT = "/tmp/release-routes"


def die(msg, code=2):
    print(f"release-routes: {msg}", file=sys.stderr)
    sys.exit(code)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# ---- bases ---------------------------------------------------------------------------------------------------------

def squashfs_entries(system):
    """(path, type, size, link) of every file, directory and link in a squashfs, without unpacking it."""
    out = subprocess.run(["unsquashfs", "-lls", system], check=True, capture_output=True, text=True, errors="replace").stdout
    entries = []
    for line in out.splitlines():
        parts = line.split(None, 5)
        if len(parts) < 6 or parts[0][0] not in "-dl" or not parts[5].startswith("squashfs-root"):
            continue
        rest = parts[5][len("squashfs-root"):].lstrip("/")
        if not rest:
            continue
        kind = {"-": "f", "d": "d", "l": "l"}[parts[0][0]]
        link = ""
        if kind == "l":
            rest, _, link = rest.partition(" -> ")
        entries.append([rest, kind, int(parts[2]) if parts[2].isdigit() else 0, link])
    return entries


def squashfs_content(system, entries):
    want = [p for p, k, size, _ in entries if k == "f" and size <= CONFIG_MAX and BASE_CONTENT.match(p)]
    files = {}
    with tempfile.TemporaryDirectory(prefix="routes-base-") as tmp:
        lst = os.path.join(tmp, "list")
        with open(lst, "w") as f:
            f.write("".join(p + "\n" for p in want))
        subprocess.run(["unsquashfs", "-q", "-n", "-d", os.path.join(tmp, "x"), "-ef", lst, system], check=True,
                       stdout=subprocess.DEVNULL)
        for p in want:
            full = os.path.join(tmp, "x", p)
            if os.path.isfile(full) and not os.path.islink(full):
                with open(full, "rb") as f:
                    files[p] = base64.b64encode(f.read()).decode()
    return files


def system_from_image(image, tmp):
    """ROCKNIX's SYSTEM squashfs from its card image (.img or .img.gz): the first partition is FAT and holds it."""
    img = image
    if image.endswith(".gz"):
        img = os.path.join(tmp, "image.img")
        with gzip.open(image, "rb") as src, open(img, "wb") as dst:
            shutil.copyfileobj(src, dst, 1 << 22)
    parts = os.path.join(tmp, "parts")
    subprocess.run(["7z", "x", "-y", f"-o{parts}", img], check=True, stdout=subprocess.DEVNULL)
    for name in sorted(os.listdir(parts)):
        p = os.path.join(parts, name)
        r = subprocess.run(["7z", "l", p], capture_output=True, text=True)
        if r.returncode == 0 and re.search(r"\sSYSTEM\s*$", r.stdout, re.M):
            subprocess.run(["7z", "e", "-y", f"-o{tmp}", p, "SYSTEM"], check=True, stdout=subprocess.DEVNULL)
            return os.path.join(tmp, "SYSTEM")
    die(f"{image}: no partition with a SYSTEM file")


def dir_entries(top):
    """A mounted root partition (dArkOS): every path without following links, never leaving `top`."""
    entries, files, heads = [], {}, {}
    for d, dirs, names in os.walk(top):
        for n in sorted(dirs + names):
            full = os.path.join(d, n)
            p = os.path.relpath(full, top)
            if os.path.islink(full):
                entries.append([p, "l", 0, os.readlink(full)])
                if n in dirs:
                    dirs.remove(n)
            elif os.path.isdir(full):
                entries.append([p, "d", 0, ""])
            elif os.path.isfile(full):
                size = os.path.getsize(full)
                entries.append([p, "f", size, ""])
                if size <= CONFIG_MAX and BASE_CONTENT.match(p):
                    with open(full, "rb") as f:
                        files[p] = base64.b64encode(f.read()).decode()
                if HEAD.search(p):
                    with open(full, "rb") as f:
                        heads[p] = base64.b64encode(f.read(HEAD_LEN)).decode()
        dirs.sort()
    return entries, files, heads


def rootfs_entries(tar_path):
    entries, files, heads = [], {}, {}
    with tarfile.open(tar_path) as t:
        for m in t:
            p = m.name.lstrip("./")
            if not p or m.ischr() or m.isblk() or m.isfifo():
                continue
            if m.isdir():
                entries.append([p, "d", 0, ""])
            elif m.issym():
                entries.append([p, "l", 0, m.linkname])
            elif m.islnk():
                entries.append([p, "l", 0, "/" + m.linkname.lstrip("./")])
            elif m.isfile():
                entries.append([p, "f", m.size, ""])
                if m.size <= CONFIG_MAX and BASE_CONTENT.match(p):
                    files[p] = base64.b64encode(t.extractfile(m).read()).decode()
                if HEAD.search(p):
                    heads[p] = base64.b64encode(t.extractfile(m).read(HEAD_LEN)).decode()
    return entries, files, heads


def cmd_base(a):
    t0 = time.time()
    src = os.path.abspath(a.source)
    heads = {}
    if os.path.isdir(src) and not a.source_file:
        die("a folder as the base needs --source-file: the image it was mounted from (its hash goes into the base)")
    with tempfile.TemporaryDirectory(prefix="routes-base-", dir=a.tmp) as tmp:
        if os.path.isdir(src):
            entries, files, heads = dir_entries(src)
            # dArkOS unpacks /roms.tar onto its ROMS partition at the first start: the card's folders, BIOS notes, tools.
            for rel in a.with_tar or []:
                have = {e[0] for e in entries}
                more, more_files, more_heads = rootfs_entries(os.path.join(src, rel))
                entries += [e for e in more if e[0] not in have]
                files.update(more_files)
                heads.update(more_heads)
        elif src.endswith((".tar.gz", ".tgz", ".tar")):
            entries, files, heads = rootfs_entries(src)
        else:
            system = system_from_image(src, tmp) if re.search(r"\.img(\.gz)?$", src) else src
            entries = squashfs_entries(system)
            files = squashfs_content(system, entries)
    os.makedirs(a.outdir, exist_ok=True)
    out = os.path.join(a.outdir, f"{a.label}.json.gz")
    origin = os.path.abspath(a.source_file) if a.source_file else src
    doc = {"label": a.label, "source": os.path.basename(origin), "source_sha256": sha256_file(origin),
           "entries": entries, "files": files, "heads": heads}
    with gzip.open(out + ".tmp", "wt") as f:
        json.dump(doc, f, separators=(",", ":"))
    os.replace(out + ".tmp", out)
    print(f"base {a.label}: {len(entries)} paths, {len(files)} with content, {len(heads)} core heads, {time.time() - t0:.0f} s -> {out}")


# ---- payloads ------------------------------------------------------------------------------------------------------

class Payload:
    """The installed layout of one payload: absolute path -> entry, plus the blobs the run needs."""

    def __init__(self, bundle, name, kind, source):
        self.bundle, self.name, self.kind, self.source = bundle, name, kind, source
        self.entries = {}
        self.notes = []

    def add_dir(self, path):
        self.entries.setdefault(path, [path, "d", 0, "", ""])

    def add_link(self, path, target):
        self.entries[path] = [path, "l", 0, target, ""]

    def add_file(self, path, data=None, size=None, promised=False):
        """`data` the bytes (small files and programs keep them), else a placeholder with the file's first bytes."""
        size = len(data) if data is not None else (size or 0)
        blob = ""
        if data is not None:
            keep = size <= SMALL or (size <= CONFIG_MAX and CONFIG.search(path)) \
                or (data[:4] == b"\x7fELF" and PurePosixPath(path).name == "pocket-ui")
            # a placeholder keeps the ELF head: class and machine say whether dArkOS's systems.py takes the core
            blob = self.bundle.blob(data) if keep else "magic:" + data[:HEAD_LEN if data[:4] == b"\x7fELF" else 4].hex()
        elif promised:
            # an asset the update lists but does not carry stays from the card: the port's own build (aarch64)
            blob = "magic:" + AARCH64_HEAD.hex() if path.endswith(".so") else "magic:"
        self.entries[path] = [path, "f", size, "", blob]

    def add_archive(self, members, dest, strip=0):
        """`members`: (relative path, type, data or None, link) of a zip or tar; `strip` leading components."""
        for rel, kind, data, link in members:
            parts = [p for p in rel.split("/") if p not in ("", ".")]
            if len(parts) <= strip:
                continue
            path = dest.rstrip("/") + "/" + "/".join(parts[strip:])
            if kind == "d":
                self.add_dir(path)
            elif kind == "l":
                self.add_link(path, link)
            else:
                self.add_file(path, data)

    def doc(self):
        return {"name": self.name, "kind": self.kind, "source": os.path.basename(self.source),
                "sha256": sha256_file(self.source), "entries": sorted(self.entries.values()), "notes": self.notes}


def zip_members(z):
    for info in z.infolist():
        mode = info.external_attr >> 16
        if info.is_dir():
            yield info.filename, "d", None, ""
        elif stat.S_ISLNK(mode):
            yield info.filename, "l", None, z.read(info).decode()
        else:
            yield info.filename, "f", z.read(info), ""


def tar_members(t):
    for m in t:
        if m.isdir():
            yield m.name, "d", None, ""
        elif m.issym():
            yield m.name, "l", None, m.linkname
        elif m.isfile():
            yield m.name, "f", t.extractfile(m).read(), ""


def rocknix_links(p, version):
    """What install.sh and the updater lay out beside releases/<version> (deploy/package/install.sh "The programs")."""
    app = "/storage/pocket-ui"
    p.add_link(f"{app}/current", f"releases/{version}")
    for f in ("pocket-ui", "pocket-core", "portmaster-setup.sh"):
        p.add_link(f"{app}/{f}", f"current/{f}")


def hakux_seed(p, version):
    """The running release seeds releases/<version>/hakux into /storage/pocket-ui/hakux at its first start
    (src/hakux.rs seed_running_release) when the bundle is usable and matches its SHA256SUMS; the run step copies it."""
    rel = f"/storage/pocket-ui/releases/{version}/hakux/"
    files = {e[0][len(rel):]: e for e in p.entries.values() if e[0].startswith(rel) and e[1] == "f"}
    if not files:
        return
    sums = files.get("SHA256SUMS")
    ok = "hakux_libretro.so" in files and "hakux_libretro.info" in files and sums is not None
    if ok:
        listed = {}
        for line in p.bundle.blob_bytes(sums[4]).decode().splitlines():
            h, _, name = line.partition(" ")
            name = name.strip().lstrip("*")
            listed[name[2:] if name.startswith("./") else name] = h
        ok = all(name in p.real for name in listed) and all(p.real[name] == h for name, h in listed.items())
    p.seed = {"from": rel.rstrip("/"), "to": "/storage/pocket-ui/hakux", "ok": ok}
    if not ok:
        p.notes.append("hakux/ is in the payload but does not match its SHA256SUMS: pocket-ui would not seed it")


def build_rocknix(p, version, members, dest_rel=True):
    rel = f"/storage/pocket-ui/releases/{version}"
    p.add_archive(members, rel)
    rocknix_links(p, version)


def payload_from(bundle, kind, path, version):
    p = Payload(bundle, kind, kind, path)
    p.real = {}  # hakux file -> sha256, for the seed check
    p.seed = None

    def note_hakux(rel, data):
        if data is not None and "/hakux/" in "/" + rel:
            p.real[rel.split("hakux/", 1)[1]] = hashlib.sha256(data).hexdigest()

    if kind == "rocknix-install":
        with zipfile.ZipFile(path) as z:
            members = list(zip_members(z))
        rel = f"/storage/pocket-ui/releases/{version}"
        for name, k, data, link in members:
            parts = name.split("/", 2)
            if len(parts) < 3:
                continue
            if parts[1] == "files":
                p.add_archive([(parts[2], k, data, link)], rel)
            elif parts[1] == "hakux":
                note_hakux(name, data)
                p.add_archive([(parts[2], k, data, link)], rel + "/hakux")
            elif parts[1] == "licences":
                p.add_archive([(parts[2], k, data, link)], "/storage/pocket-ui/licences")
        rocknix_links(p, version)
    elif kind == "rocknix-card":
        with zipfile.ZipFile(path) as z:
            seed = z.read("pocket-ui-seed.tar.gz")
        with tarfile.open(fileobj=io.BytesIO(seed)) as t:
            members = list(tar_members(t))
        for name, _, data, _ in members:
            note_hakux(name, data)
        p.add_archive(members, "/storage")
    elif kind in ("rocknix-ota", "rocknix-update"):
        if kind == "rocknix-update":
            with zipfile.ZipFile(path) as z:
                inner = [n for n in z.namelist() if n.endswith("-aarch64.tar.gz")]
                if len(inner) != 1:
                    die(f"{path}: expected one pocket-ui-*-aarch64.tar.gz inside, found {inner}")
                data = z.read(inner[0])
            p.inner_sha256 = hashlib.sha256(data).hexdigest()
            tf = tarfile.open(fileobj=io.BytesIO(data))
        else:
            tf = tarfile.open(path)
        with tf as t:
            members = list(tar_members(t))
        for name, _, data, _ in members:
            note_hakux(name, data)
        build_rocknix(p, version, members)
    elif kind == "brick-card":
        with zipfile.ZipFile(path) as z:
            p.add_archive(zip_members(z), "/mnt/SDCARD")
    elif kind in ("brick-update", "darkos-update"):
        app = "/mnt/SDCARD/pocket-ui/app" if kind == "brick-update" else "/roms/ports/pocket-ui/app"
        with zipfile.ZipFile(path) as z:
            names = z.namelist()
            inner = [n for n in names if re.fullmatch(r"pocket-ui-(brick|darkos)-[^/]+\.tar\.gz", n)]
            if len(inner) != 1:
                die(f"{path}: expected one app tarball inside, found {inner}")
            with tarfile.open(fileobj=io.BytesIO(z.read(inner[0]))) as t:
                members = list(tar_members(t))
            p.add_archive(members, app)
            listing = next((d for n, k, d, _ in members if n.lstrip("./") == "asset-packages.json"), None)
            shipped = {}
            for n in names:
                if n.startswith("assets/") and n.endswith(".tar.gz"):
                    with tarfile.open(fileobj=io.BytesIO(z.read(n))) as t:
                        for rel, k, data, link in tar_members(t):
                            shipped[rel.lstrip("./")] = (k, data, link)
        # Assets the update lists but does not carry stay as they are on the card or are fetched by the updater
        # (src/update/brick_assets.rs): they count as delivered, with a placeholder of their size.
        assets = json.loads(listing)["assets"] if listing else []
        for a_ in assets:
            rel = a_["path"]
            if rel in shipped and shipped[rel][0] == "f":
                p.add_file(f"{app}/{rel}", shipped[rel][1])
            else:
                p.add_file(f"{app}/{rel}", size=a_.get("size", 0), promised=True)
        for rel, (k, data, link) in shipped.items():
            if f"{app}/{rel}" not in p.entries:
                p.add_archive([(rel, k, data, link)], app)
        if kind == "brick-update":
            p.add_dir("/mnt/SDCARD/Roms")
        p.notes.append(f"{len(assets)} assets listed, {len(shipped)} files carried")
    elif kind == "darkos-zip":
        with zipfile.ZipFile(path) as z:
            p.add_archive(zip_members(z), "/roms/ports")
    else:
        die(f"unknown payload kind {kind}")
    hakux_seed(p, version)
    d = p.doc()
    if p.seed:
        d["seed"] = p.seed
    if getattr(p, "inner_sha256", None):
        d["inner_sha256"] = p.inner_sha256
    return d


class Bundle:
    def __init__(self, out):
        self.out = out
        os.makedirs(os.path.join(out, "blobs"), exist_ok=True)

    def blob(self, data):
        h = hashlib.sha256(data).hexdigest()
        path = os.path.join(self.out, "blobs", h)
        big = len(data) > SMALL
        if not os.path.exists(path + (".xz" if big else "")):
            if big:
                r = subprocess.run(["xz", "-T0", "-3", "-c"], input=data, capture_output=True, check=True)
                with open(path + ".xz.tmp", "wb") as f:
                    f.write(r.stdout)
                os.replace(path + ".xz.tmp", path + ".xz")
            else:
                with open(path, "wb") as f:
                    f.write(data)
        return h

    def blob_bytes(self, h):
        path = os.path.join(self.out, "blobs", h)
        if os.path.exists(path):
            with open(path, "rb") as f:
                return f.read()
        with lzma.open(path + ".xz") as f:
            return f.read()


def cmd_bundle(a):
    t0 = time.time()
    with open(a.expected) as f:
        expected = json.load(f)
    b = Bundle(a.out)
    payloads = {}
    for spec in a.payloads:
        kind, _, path = spec.partition("=")
        if not path or not os.path.isfile(path):
            die(f"payload {spec!r}: give KIND=PATH of an existing file")
        payloads[kind] = payload_from(b, kind, path, a.version)
        print(f"payload {kind}: {len(payloads[kind]['entries'])} paths from {os.path.basename(path)}")
    for kind, d in payloads.items():
        progs = [e for e in d["entries"] if e[1] == "f" and PurePosixPath(e[0]).name == "pocket-ui" and e[4] and not e[4].startswith("magic:")]
        override = a.probe_rocknix if kind.startswith("rocknix") else a.probe_brick
        if override:
            # A test of the check itself with a newer program than the payload's: said in the receipt.
            with open(override, "rb") as f:
                blob = b.blob(f.read())
            for e in progs:
                e[4] = blob
            d["notes"].append(f"probe program replaced by {os.path.basename(override)} (sha256 {blob[:12]}): not the payload's own")
        for e in progs:
            # A pocket-ui without the probe would start its whole UI instead of answering (0.7.1 and older).
            if b"--emulator-routes" not in b.blob_bytes(e[4]):
                die(f"{kind}: {e[0]} has no --emulator-routes probe (older than the route check): check a newer build")
        if not progs:
            die(f"{kind}: no pocket-ui program in the payload")
    os.makedirs(os.path.join(a.out, "payloads"), exist_ok=True)
    for kind, d in payloads.items():
        with gzip.open(os.path.join(a.out, "payloads", kind + ".json.gz"), "wt") as f:
            json.dump(d, f, separators=(",", ":"))
    os.makedirs(os.path.join(a.out, "bases"), exist_ok=True)
    for fam in expected["families"].values():
        label = fam.get("base")
        if not label:
            continue
        src = os.path.join(a.bases, label + ".json.gz")
        if not os.path.isfile(src):
            die(f"no base {label} in {a.bases}: make it with `release-routes.py base <ROCKNIX image> {label} {a.bases}`")
        dst = os.path.join(a.out, "bases", label + ".json.gz")
        if not os.path.exists(dst) or sha256_file(dst) != sha256_file(src):
            shutil.copyfile(src, dst)
    # The blobs of an earlier bundle in the same folder stay for rsync; drop the ones nothing names now.
    used = set()
    for d in payloads.values():
        used.update(e[4] for e in d["entries"] if e[4] and not e[4].startswith("magic:"))
    for name in os.listdir(os.path.join(a.out, "blobs")):
        if name.split(".")[0] not in used:
            os.remove(os.path.join(a.out, "blobs", name))
    with open(os.path.join(a.out, "bundle.json"), "w") as f:
        json.dump({"version": a.version, "payloads": sorted(payloads), "made": time.strftime("%Y-%m-%d %H:%M:%S")}, f)
    print(f"bundle: {a.out} in {time.time() - t0:.0f} s")


# ---- run -----------------------------------------------------------------------------------------------------------

class Tree:
    """A directory standing in for `/` of a device. Every path is resolved inside it, links included, so an absolute
    link of the base never reaches the container's own files."""

    def __init__(self, root):
        self.root = root

    def resolve(self, path, follow_last=True, depth=0):
        if depth > 40:
            raise OSError(f"link loop at {path}")
        parts = [p for p in path.split("/") if p not in ("", ".")]
        cur = []
        for i, part in enumerate(parts):
            if part == "..":
                cur = cur[:-1]
                continue
            cand = cur + [part]
            host = os.path.join(self.root, *cand)
            last = i == len(parts) - 1
            if os.path.islink(host) and (follow_last or not last):
                target = os.readlink(host)
                base = "/" if target.startswith("/") else "/" + "/".join(cur)
                rest = "/".join(parts[i + 1:])
                return self.resolve(os.path.join(base, target, rest) if rest else os.path.join(base, target), follow_last, depth + 1)
            cur = cand
        return "/" + "/".join(cur)

    def host(self, path, follow_last=True):
        return os.path.join(self.root, self.resolve(path, follow_last).lstrip("/"))

    def mkdir(self, path):
        h = self.host(path)
        if os.path.isfile(h) and not os.path.islink(h):
            os.remove(h)
        os.makedirs(h, exist_ok=True)

    def write(self, path, data, mode=0o644):
        parent, name = os.path.split(path)
        self.mkdir(parent or "/")
        h = os.path.join(self.host(parent or "/"), name)
        if os.path.islink(h) or os.path.isfile(h):
            os.remove(h)
        elif os.path.isdir(h):
            shutil.rmtree(h)
        with open(h, "wb") as f:
            f.write(data)
        os.chmod(h, mode)

    def link(self, path, target):
        parent, name = os.path.split(path)
        self.mkdir(parent or "/")
        h = os.path.join(self.host(parent or "/"), name)
        if os.path.islink(h) or os.path.isfile(h):
            os.remove(h)
        elif os.path.isdir(h):
            shutil.rmtree(h)
        os.symlink(target, h)

    def is_file(self, path):
        try:
            return os.path.isfile(self.host(path))
        except OSError:
            return False

    def read(self, path):
        with open(self.host(path), "rb") as f:
            return f.read()


def load_json_gz(path):
    with gzip.open(path, "rt") as f:
        return json.load(f)


def blob_data(bundle, blob):
    if not blob:
        return b""
    if blob.startswith("magic:"):
        return bytes.fromhex(blob[6:])
    path = os.path.join(bundle, "blobs", blob)
    if os.path.exists(path):
        with open(path, "rb") as f:
            return f.read()
    with lzma.open(path + ".xz") as f:
        return f.read()


def lay_base(tree, base, mirror_config):
    files = base["files"]
    heads = base.get("heads", {})
    for path, kind, size, link in base["entries"]:
        p = "/" + path
        try:
            if kind == "d":
                tree.mkdir(p)
            elif kind == "l":
                tree.link(p, link)
            else:
                if path in files:
                    data = base64.b64decode(files[path])
                elif path in heads:
                    data = base64.b64decode(heads[path])
                else:
                    data = b"\x7fELF" if path.endswith(".so") or "/bin/" in p else b"x"
                tree.write(p, data, 0o755 if "/bin/" in p or path.endswith(".sh") else 0o644)
        except OSError as e:
            print(f"  base {path}: {e}", file=sys.stderr)
    if mirror_config:
        # ROCKNIX's first start copies /usr/config to /storage/.config (es_systems.cfg, system.cfg, ...).
        for path, kind, size, link in base["entries"]:
            if not path.startswith("usr/config/"):
                continue
            p = "/storage/.config/" + path[len("usr/config/"):]
            if kind == "d":
                tree.mkdir(p)
            elif kind == "l":
                tree.link(p, link)
            else:
                tree.write(p, base64.b64decode(files[path]) if path in files else b"x")


def lay_payload(tree, bundle, payload):
    for path, kind, size, link, blob in payload["entries"]:
        if kind == "d":
            tree.mkdir(path)
        elif kind == "l":
            tree.link(path, link)
        else:
            tree.write(path, blob_data(bundle, blob), 0o755)


def install_loader(tree):
    """The container's own loader and C library: the release programs need only libc, libm (and libpthread, libdl)."""
    libs = "/.release-routes-libs"
    tree.mkdir(libs)
    for name in ("libc.so.6", "libm.so.6", "libpthread.so.0", "libdl.so.2"):
        for d in ("/lib/aarch64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/lib"):
            if os.path.exists(os.path.join(d, name)):
                with open(os.path.join(d, name), "rb") as f:
                    tree.write(f"{libs}/{name}", f.read(), 0o755)
                break
    with open("/lib/ld-linux-aarch64.so.1", "rb") as f:
        tree.write("/lib/ld-linux-aarch64.so.1", f.read(), 0o755)
    for cache in ("/etc/ld.so.cache", "/etc/ld.so.preload"):
        try:
            os.remove(tree.host(cache))
        except (FileNotFoundError, IsADirectoryError, OSError):
            pass
    return libs


ES_SYSTEM = re.compile(r"<system>(.*?)</system>", re.S)


def es_systems(text):
    """name -> (path, first extension) of an es_systems.cfg."""
    out = {}
    for block in ES_SYSTEM.findall(text):
        name = re.search(r"<name>\s*(.*?)\s*</name>", block, re.S)
        path = re.search(r"<path>\s*(.*?)\s*</path>", block, re.S)
        ext = re.search(r"<extension>\s*(.*?)\s*</extension>", block, re.S)
        if name and path:
            exts = (ext.group(1).split() if ext else []) or [".zip"]
            out.setdefault(name.group(1), (path.group(1), exts[0]))
    return out


def exe_dir(tree, exe):
    return os.path.dirname(tree.resolve(exe))


def darkos_programs(command):
    """The files dArkOS's own ES command needs (deploy/darkos/host-launch.py runs it as it stands): every absolute path in
    the part that starts the game (the one with %ROM%), without the governor helpers around it."""
    need = []
    for part in command.split(";"):
        if "%ROM%" not in part or "perfmax" in part or "perfnorm" in part:
            continue
        toks = [t.strip("\"'") for t in part.split()]
        # the program: the first word after nice/sudo and their options; a bare name is looked up in PATH (find_core)
        rest = list(toks)
        while rest and (rest[0] in ("nice", "sudo", "-n", "-19", "--") or re.fullmatch(r"-?\d+", rest[0])):
            rest.pop(0)
        if rest and "%" not in rest[0] and "/" not in rest[0]:
            need.append(rest[0])
        for tok in toks:
            if tok.startswith("/") and "%" not in tok and tok not in ("/usr/bin/sudo", "/bin/sh", "/bin/bash"):
                need.append(tok)
    return need


def darkos_command(system_xml, settings):
    """dArkOS's command for a system as deploy/darkos/host-launch.py `choose` fills it for a game without its own choice:
    the system's emulator from es_settings.cfg or the first one, its core (defaultCore or the first), its own command."""
    import xml.etree.ElementTree as ET
    try:
        node = ET.fromstring(system_xml)
    except ET.ParseError:
        return ""
    sid = node.findtext("name", "")
    emus = node.findall("emulators/emulator")
    wanted = settings.get(sid + ".emulator")
    emu = next((e for e in emus if e.get("name") == wanted), emus[0] if emus else None)
    name = emu.get("name", "") if emu is not None else ""
    cores = [c.text or "" for c in emu.findall("cores/core")] if emu is not None else []
    wanted = settings.get(sid + ".core")
    core = wanted if wanted in cores else (node.findtext("defaultCore", "") or next(iter(cores), ""))
    command = (emu.get("command") if emu is not None else None) or node.findtext("command", "")
    return " ".join(command.split()).replace("%EMULATOR%", name).replace("%CORE%", core)


def find_core(tree, fam, exe, emulator, core, system, env=None):
    """Where the started emulator's files are, by the rules of src/store.rs core_path and src/emu_choice.rs
    installed/rocknix_external_ready; None when one is missing. Returns the paths that must exist."""
    env = fam.get("env", {}) if env is None else env
    so = f"{core}_libretro.so"
    if emulator == "pocket-core":
        host = env.get("POCKETUI_HOST_BIN", "/storage/pocket-ui/pocket-core")
        cands = []
        if core == "gpsp":
            cands.append(env.get("POCKETUI_GBA_CORE") or f"{exe_dir(tree, exe)}/cores/gpsp_libretro.so")
        cands.append(f"/storage/cores/{so}")
        cands += [f"{d}/{so}" for d in env.get("POCKETUI_CORES", "").split(":") if d]
        if core == "dsperate":
            cands.append(f"{exe_dir(tree, exe)}/cores/dsperate_libretro.so")
        cands.append(f"/usr/lib/libretro/{so}")
        hit = next((c for c in cands if tree.is_file(c)), None)
        return [host, hit] if hit and tree.is_file(host) else None
    if system in ("ports", "windows", "shell", "setup"):
        return []
    if emulator == "retroarch":
        hit = next((f"{d}/{so}" for d in ("/tmp/cores", "/usr/lib/libretro", "/storage/cores") if tree.is_file(f"{d}/{so}")), None)
        return [hit, "/usr/bin/retroarch"] if hit and tree.is_file("/usr/bin/retroarch") else None
    if emulator == "hakux":
        need = ["/storage/pocket-ui/hakux/hakux_libretro.so", "/storage/pocket-ui/hakux/hakux_libretro.info", "/usr/bin/retroarch"]
        return need if all(tree.is_file(p) for p in need) else None
    if emulator == "darkos":
        # dArkOS's own command for the system (systems.py keeps it in systems.json): its program and core must be in the image.
        need = []
        for p in darkos_programs(fam.get("darkos_commands", {}).get(system, "")):
            if not p.startswith("/"):
                p = next((f"{d}/{p}" for d in ("/usr/local/bin", "/usr/bin", "/bin") if tree.is_file(f"{d}/{p}")), f"/usr/local/bin/{p}")
            need.append(p)
        return need if need and all(tree.is_file(p) for p in need) else None
    if emulator == "mednafen":
        script = "start_mednafen.sh"
    elif system in ("gamecube", "triforce"):
        script = "start_dolphin_gc.sh"
    elif system in ("wii", "wiiware"):
        script = "start_dolphin_wii.sh"
    else:
        script = f"start_{core.rsplit('-', 1)[0] if '-' in core else core}.sh"
    exe_of = {"armsx2": "/usr/share/armsx2-sa/armsx2-qt", "azahar": "/usr/bin/azahar", "ppsspp": "/usr/bin/ppsspp",
              "xemu": "/usr/bin/xemu", "dolphin": "/usr/bin/dolphin-emu" if "-qt-" in core else "/usr/bin/dolphin-emu-nogui"}
    need = [f"/usr/bin/{script}"] + ([exe_of[emulator]] if emulator in exe_of else [])
    return need if all(tree.is_file(p) for p in need) else None


def core_file(paths):
    """The file that is the emulator itself: the core or start script, not pocket-core or RetroArch that load it."""
    cores = [p for p in paths if p and p.endswith("_libretro.so")]
    files = cores or [p for p in paths if p and not p.endswith(("/pocket-core", "/retroarch"))] or [p for p in paths if p]
    return files[0] if files else None


def source_of(tree, paths, payload_paths):
    """`payload` when the core came from the payload (or pocket-ui seeded it from there), else `base`."""
    f = core_file(paths)
    return "none" if f is None else ("payload" if tree.resolve(f) in payload_paths else "base")


def judge(exp, act, offered, found, src):
    """What is wrong with one route: `exp` [emulator, core, source] of the table, `act` [emulator, core, state] of the probe,
    `found` the files find_core located (None: missing, []: nothing to look for), `src` where the core came from."""
    why = []
    if exp is None:
        return ["not in the table (a system the payload offers that nobody expects)"]
    if act is None:
        return ["no route: the system is not offered" if offered else "the base has no such system"]
    e_emu, e_core, e_src = exp
    a_emu, a_core, a_state = act
    if (a_emu, a_core) != (e_emu, e_core):
        why.append(f"starts {a_emu}/{a_core}, expected {e_emu}/{e_core}")
    if e_src == "unsupported":
        if a_state != "unsupported":
            why.append(f"state {a_state}, expected unsupported")
        return why
    if a_state != "ready":
        why.append(f"the binary says {a_state}")
    if found is None:
        why.append(f"{a_core or a_emu} is in neither the payload nor the base")
    elif found and e_src in ("payload", "base") and src != e_src:
        why.append(f"comes from the {src}, expected from the {e_src}")
    return why


def routes_for(fam, family_name):
    """system -> [emulator, core, source] and system -> the documented exception a 4th element names (green, but shown)."""
    routes = {}
    for group in fam.get("use", []):
        routes.update(EXPECTED["groups"][group])
    routes.update(fam.get("routes", {}))
    return split_exceptions(routes)


def split_exceptions(routes):
    plain = {s: r[:3] for s, r in routes.items() if r is not None}
    why = {s: r[3] for s, r in routes.items() if r is not None and len(r) > 3}
    return plain, why


def prepare_darkos(tree, fam, problems, label):
    """What deploy/darkos/start.sh does before pocket-ui starts, in the tree (docs/DARKOS.md "How it runs"): a fresh
    card's folder for every system of dArkOS's ES list, the port's own systems.py turns that list into the shelf (only
    64-bit cores the image has go to pocket-core), each shelf folder shows the card's, the image's cores plus the port's
    DSperate in one folder. Returns the environment start.sh exports, or None when a step failed."""
    d = fam["darkos"]
    app, data, run = d["app"], os.path.dirname(d["app"]), "/tmp/pocket-ui-darkos"
    # start.sh: ARK_HOME from `getent passwd ark`; the table writes ~ark/ for it
    ark = next((line.split(":")[5] for line in (tree.read("/etc/passwd").decode(errors="replace").splitlines()
                                                 if tree.is_file("/etc/passwd") else [])
                if line.startswith("ark:") and line.count(":") >= 6), None)
    if ark is None:
        problems.append(f"{label}: no user ark in the base's /etc/passwd")
        return None
    home = lambda p: ark + p[len("~ark"):] if p.startswith("~ark/") else p  # noqa: E731
    es_src = next((home(p) for p in d["es_systems"] if tree.is_file(home(p))), None)
    script = f"{app}/port-data/systems.py"
    if es_src is None or not tree.is_file(script):
        problems.append(f"{label}: {'no ES list of dArkOS in the base' if es_src is None else script + ' is not in the payload'}")
        return None
    for block in ES_SYSTEM.findall(tree.read(es_src).decode(errors="replace")):
        path = re.search(r"<path>\s*(.*?)\s*</path>", block, re.S)
        if path and re.match(r"^/roms2?/[^/]", path.group(1).strip()):
            tree.mkdir("/roms/" + path.group(1).strip().split("/", 2)[2])
    cores = home(d["cores"])
    if tree.is_file(f"{app}/bin/cores/dsperate_libretro.so"):
        # relative links: they resolve the same for systems.py on the build host and for pocket-ui in the chroot
        rc = f"{run}/cores"
        tree.mkdir(rc)
        src_dir = tree.host(cores)
        for name in sorted(os.listdir(src_dir)) if os.path.isdir(src_dir) else []:
            if name.endswith(("_libretro.so", "_libretro.info")):
                tree.link(f"{rc}/{name}", os.path.relpath(f"{cores}/{name}", rc))
        tree.link(f"{rc}/dsperate_libretro.so", os.path.relpath(f"{app}/bin/cores/dsperate_libretro.so", rc))
        cores = rc
    shelf = f"{data}/storage/.config/emulationstation"
    tree.mkdir(shelf)
    tree.mkdir(run)
    r = subprocess.run([sys.executable, "-I", tree.host(script), tree.host(es_src), tree.host("/roms"), tree.host(cores),
                        tree.host(shelf), tree.host(run)], capture_output=True, text=True)
    if r.returncode != 0:
        problems.append(f"{label}: the port's systems.py failed: {(r.stderr or r.stdout).strip()[-300:]}")
        return None
    with open(os.path.join(tree.host(run), "systems.json")) as f:
        inventory = json.load(f)
    settings = {}
    for es_settings in (f"{ark}/.emulationstation/es_settings.cfg",):
        if tree.is_file(es_settings):
            text = tree.read(es_settings).decode(errors="replace")
            settings.update(re.findall(r'<string name="([^"]+)" value="([^"]*)"', text))
    commands = {}
    for sid, item in inventory.items():
        tree.link(f"/storage/roms/{sid}", "/" + os.path.relpath(item["path"], tree.root))
        commands[sid] = darkos_command(item["system"], settings)
    fam["darkos_commands"] = commands
    if tree.is_file("/roms/bios") or os.path.isdir(tree.host("/roms/bios")):
        tree.link("/storage/roms/bios", "/roms/bios")
    env = {"POCKETUI_HOST": "darkos", "POCKET_CORE_SOFTWARE": "1", "POCKETUI_CORES": cores,
           "POCKETUI_HOST_BIN": f"{app}/bin/pocket-core", "POCKET_CORE_SAVE_DIR": "/storage/saves",
           "POCKETUI_BRICK_CARD": data, "HOME": "/storage/home", "POCKETUI_SCUMMVM_SYSTEM": "/storage/.config/scummvm"}
    if tree.is_file(f"{cores}/gpsp_libretro.so"):
        env["POCKETUI_GBA_CORE"] = f"{cores}/gpsp_libretro.so"
    return env


def run_probe(tree, exe, env, libs):
    full = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": "/storage", "LD_LIBRARY_PATH": libs, "LANG": "C.UTF-8"}
    full.update(env)
    r = subprocess.run(["chroot", tree.root, exe, "--emulator-routes"], env=full, capture_output=True, text=True, timeout=60)
    if r.returncode != 0:
        return None, (r.stderr or r.stdout).strip()[-400:]
    try:
        return json.loads(r.stdout), r.stderr.strip()
    except json.JSONDecodeError:
        return None, ("not JSON: " + r.stdout[:200])


def check_tree(bundle, fam_name, kind):
    """One family and payload: its tree, every device and variant. Runs in a worker of its own (cmd_run); returns rows
    and problems. The tree goes when it ends, also on a problem."""
    fam = EXPECTED["families"][fam_name]
    routes, exceptions = routes_for(fam, fam_name)
    base = load_json_gz(os.path.join(bundle, "bases", fam["base"] + ".json.gz")) if fam.get("base") else None
    payloads = PAYLOADS
    rows, problems = [], []
    root = os.path.join(TREE_ROOT, f"{fam_name}-{kind}")
    try:
        return check_tree_in(bundle, fam_name, fam, kind, base, routes, exceptions, payloads, root, rows, problems)
    except Exception as e:  # one broken tree is a red line of its own, the other trees still run
        return rows, problems + [f"{fam_name}/{kind}: the check itself failed: {type(e).__name__}: {e}"]
    finally:
        shutil.rmtree(root, ignore_errors=True)


def check_tree_in(bundle, fam_name, fam, kind, base, routes, exceptions, payloads, root, rows, problems):
    payload = payloads[kind]
    os.makedirs(root)
    tree = Tree(root)
    t1 = time.time()
    if base:
        lay_base(tree, base, fam.get("config_mirror", False))
    lay_payload(tree, bundle, payload)
    for src, dst in fam.get("binds", {}).items():
        # start.sh's bind mounts (Brick: the card's storage, Roms and cores at /storage)
        tree.mkdir(src)
        h = tree.host(dst, follow_last=False)
        if os.path.isdir(h) and not os.path.islink(h):
            for name in os.listdir(h):
                os.rename(os.path.join(h, name), os.path.join(tree.host(src), name)) if not os.path.exists(os.path.join(tree.host(src), name)) else None
            shutil.rmtree(h)
        tree.link(dst, src)
    for src, dst in fam.get("copies", {}).items():
        tree.write(dst, tree.read(src))
    payload_paths = set()
    for path, k, *_ in payload["entries"]:
        if k == "f":
            try:
                payload_paths.add(tree.resolve(path))
            except OSError:
                pass
    libs = install_loader(tree)
    tree.mkdir("/tmp")
    fam_env = dict(fam.get("env", {}))
    if fam.get("darkos"):
        more = prepare_darkos(tree, fam, problems, f"{fam_name}/{kind}")
        if more is None:
            return rows, problems
        fam_env.update(more)
    es_path = fam.get("es_systems", "/storage/.config/emulationstation/es_systems.cfg")
    if not tree.is_file(es_path):
        problems.append(f"{fam_name}/{kind}: no {es_path} in the tree")
        return rows, problems
    es = es_systems(tree.read(es_path).decode(errors="replace"))
    game_of = {}
    games = dict(EXPECTED.get("games", {}), **fam.get("games", {}))
    for system in routes:
        if system not in es:
            continue
        folder, ext = es[system]
        ext = games.get(system, ext)
        game = f"{folder.rstrip('/')}/Route test{ext}"
        tree.write(game, b"route test", 0o644)
        game_of[system] = game
    exe = fam["exe"]
    if not tree.is_file(exe):
        problems.append(f"{fam_name}/{kind}: {exe} is not in the payload")
        return rows, problems
    # No procfs in the chroot: pocket-ui finds its bundled cores beside itself through /proc/self/exe
    # (h700_gba::bundled_path, ds::bundled_path), so that one link stands in for it.
    tree.link("/proc/self/exe", tree.resolve(exe))
    for device in fam["devices"]:
        seed = payload.get("seed")
        hakux_dir = tree.host("/storage/pocket-ui/hakux")
        if os.path.isdir(hakux_dir):
            shutil.rmtree(hakux_dir)
        seeded = set()
        if seed and seed["ok"] and device in fam.get("hakux_devices", []):
            shutil.copytree(tree.host(seed["from"]), hakux_dir, symlinks=True)
            for d, _, names in os.walk(hakux_dir):
                seeded.update("/" + os.path.relpath(os.path.join(d, n), tree.root) for n in names)
        variants = dict(fam.get("variants", {}))
        variants.update({v: EXPECTED["variants"][v] for v in fam.get("use_variants", [])})
        for variant, extra in [("default", {})] + list(variants.items()):
            data = tree.host("/storage/.config/pocket-ui")
            settings = os.path.join(data, "game-settings")
            shutil.rmtree(settings, ignore_errors=True)
            if variant == "default":
                want, why_ok = dict(routes), dict(exceptions)
            else:
                want, why_ok = split_exceptions(extra["routes"])
            # A variant's files: relative to pocket-ui's settings (JSON), or an absolute path with its text (a BIOS file,
            # the official PICO-8); they go again after the variant.
            written = []
            for rel, content in extra.get("files", {}).items():
                path = rel if rel.startswith("/") else f"/storage/.config/pocket-ui/{rel}"
                if tree.is_file(path):
                    problems.append(f"{fam_name}/{kind}/{variant}: {path} is already in the tree; the variant would replace it")
                    continue
                tree.write(path, content.encode() if isinstance(content, str) else json.dumps(content).encode())
                written.append(path)
            env = dict(fam_env)
            env["POCKETUI_DEVICE"] = device
            got, err = run_probe(tree, exe, env, libs)
            for path in written:
                os.remove(tree.host(path))
            if got is None:
                problems.append(f"{fam_name}/{device}/{kind}/{variant}: the probe failed: {err}")
                continue
            systems = sorted(set(want) | (set(got) if variant == "default" else set()))
            for system in systems:
                exp = want.get(system)
                act = got.get(system)
                row = {"family": fam_name, "device": device, "payload": kind, "variant": variant, "system": system,
                       "expected": exp, "actual": act}
                found = src = None
                if exp is not None and act is not None and exp[2] != "unsupported":
                    found = find_core(tree, fam, exe, act[0], act[1], system, env)
                    if found:
                        src = source_of(tree, found, payload_paths | seeded)
                        row["file"] = tree.resolve(core_file(found))
                        row["source"] = src
                why = judge(exp, act, system in es, found, src)
                row["ok"] = not why
                row["why"] = "; ".join(why)
                if system in why_ok:
                    row["exception"] = why_ok[system]
                rows.append(row)
    print(f"{fam_name}/{kind}: tree and {len(fam['devices'])} device(s) in {time.time() - t1:.1f} s", flush=True)
    return rows, problems


def cmd_run(a):
    global EXPECTED
    t0 = time.time()
    if os.geteuid() != 0:
        die("run needs root (chroot): use tools/air-build.sh -x on the aarch64 build host")
    with open(a.expected) as f:
        EXPECTED = json.load(f)
    with open(os.path.join(a.bundle, "bundle.json")) as f:
        meta = json.load(f)
    version = meta["version"]
    payloads = {k: load_json_gz(os.path.join(a.bundle, "payloads", k + ".json.gz")) for k in meta["payloads"]}
    PAYLOADS.update(payloads)
    rows, problems, notes, jobs = [], [], [], []
    shutil.rmtree(TREE_ROOT, ignore_errors=True)
    os.makedirs(TREE_ROOT)
    for kind, p in payloads.items():
        for n in p.get("notes", []):
            notes.append(f"{kind}: {n}")
    if "rocknix-update" in payloads and "rocknix-ota" in payloads:
        if payloads["rocknix-update"].get("inner_sha256") != payloads["rocknix-ota"]["sha256"]:
            problems.append("rocknix-update: its tarball is not the OTA tarball byte for byte")
    for fam_name, fam in EXPECTED["families"].items():
        wanted = [k for k in fam["payloads"] if k in payloads]
        missing = [k for k in fam["payloads"] if k not in payloads]
        if missing and not a.allow_missing_payloads:
            problems.append(f"{fam_name}: payloads not given: {', '.join(missing)}")
        jobs += [(fam_name, kind) for kind in wanted]
    # The trees are independent: a few at once (the probe and the tree writes wait on the disk more than on a core).
    with concurrent.futures.ProcessPoolExecutor(max_workers=max(1, min(a.jobs, len(jobs) or 1)),
                                                mp_context=multiprocessing.get_context("fork")) as pool:
        for got_rows, got_problems in pool.map(check_tree, [a.bundle] * len(jobs), *zip(*jobs)) if jobs else []:
            rows += got_rows
            problems += got_problems
    bad = [r for r in rows if not r["ok"]]
    for r in bad:
        print(f"RED {r['family']}/{r['device']}/{r['payload']}/{r['variant']} {r['system']}: {r['why']}")
    for p in problems:
        print(f"RED {p}")
    for n in notes:
        print(f"note {n}")
    # The documented exceptions: green, but said once per family, system and variant (which payloads and devices it held for).
    seen = {}
    for r in rows:
        if r.get("exception") and r["ok"]:
            seen.setdefault((r["family"], r["system"], r["variant"], r["exception"]), set()).add(r["device"])
    for (fam_name, system, variant, why), devices in sorted(seen.items()):
        print(f"exception {fam_name} {system}{'' if variant == 'default' else ' (' + variant + ')'}: {why} [{', '.join(sorted(devices))}]")
    elapsed = time.time() - t0
    verdict = "ok" if not bad and not problems else "RED"
    print(f"release routes {version}: {len(rows)} routes, {len(bad)} wrong, {len(problems)} other problems, "
          f"{len(seen)} documented exceptions, {elapsed:.0f} s: {verdict}")
    if a.receipt:
        with open(a.receipt, "w") as f:
            json.dump({"version": version, "verdict": verdict, "seconds": round(elapsed), "rows": rows, "problems": problems,
                       "exceptions": [{"family": k[0], "system": k[1], "variant": k[2], "why": k[3], "devices": sorted(v)}
                                      for k, v in sorted(seen.items())],
                       "notes": notes, "payloads": {k: {"source": p["source"], "sha256": p["sha256"]} for k, p in payloads.items()},
                       "bases": {fam["base"]: load_json_gz(os.path.join(a.bundle, "bases", fam["base"] + ".json.gz"))["source_sha256"]
                                 for fam in EXPECTED["families"].values() if fam.get("base")}}, f, indent=1)
    # --zero: the verdict is in the receipt (tools/air-build.sh brings files back only from a run that exits 0)
    sys.exit(0 if verdict == "ok" or a.zero else 1)


EXPECTED = {}
PAYLOADS = {}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("base")
    b.add_argument("source")
    b.add_argument("label")
    b.add_argument("outdir")
    b.add_argument("--tmp", default=None, help="folder for the unpacked image (a few GB)")
    b.add_argument("--source-file", help="with a mounted folder as SOURCE: the image (or its first 7z part) it came from")
    b.add_argument("--with-tar", action="append", help="with a folder as SOURCE: a tar inside it whose members join the base (dArkOS: roms.tar)")
    u = sub.add_parser("bundle")
    u.add_argument("--version", required=True)
    u.add_argument("--bases", required=True)
    u.add_argument("--out", required=True)
    u.add_argument("--expected", default=os.path.join(os.path.dirname(os.path.abspath(__file__)), "release-routes.json"))
    u.add_argument("--probe-rocknix", help="test only: run this ROCKNIX pocket-ui instead of the payloads' own")
    u.add_argument("--probe-brick", help="test only: run this Brick/dArkOS pocket-ui instead of the payloads' own")
    u.add_argument("payloads", nargs="+", metavar="KIND=PATH")
    r = sub.add_parser("run")
    r.add_argument("bundle")
    r.add_argument("expected")
    r.add_argument("--receipt")
    r.add_argument("--allow-missing-payloads", action="store_true", help="families whose payloads were not given are skipped")
    r.add_argument("--zero", action="store_true", help="exit 0 also when red; the verdict is in the receipt")
    r.add_argument("--jobs", type=int, default=4, help="trees checked at once (default 4)")
    a = ap.parse_args()
    {"base": cmd_base, "bundle": cmd_bundle, "run": cmd_run}[a.cmd](a)


if __name__ == "__main__":
    main()
