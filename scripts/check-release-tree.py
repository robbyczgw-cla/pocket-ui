#!/usr/bin/env python3
"""Release gate: no BIOS, no firmware dumps, no ROMs or disc images, no third-party emulator binaries.

    scripts/check-release-tree.py                    # the repository itself (tracked + untracked, not ignored)
    scripts/check-release-tree.py DIR                # any directory, e.g. an unpacked image or an SD card
    scripts/check-release-tree.py FILE.tar.gz        # a tar / tar.gz / tar.xz / zip, listed without unpacking
    scripts/check-release-tree.py IMAGE.img          # a disk image (see below)
    scripts/check-release-tree.py --history          # additionally: every file name and blob ever committed

Options:
    --allowlist FILE   paths that are allowed on purpose (default: scripts/release-tree-allowlist.txt).
                       One glob per line, then "# reason". Paths are relative to the checked root / archive root.
    --md5-list FILE    file hashes to refuse (default: scripts/bios-md5.txt). Hashes only, never the files.
    --max-hash-mib N   hash files up to N MiB (default 16). BIOS dumps are far smaller than that.
    -v                 also list what the allowlist let through.

What it looks for (see the tables below):
    BIOS       a file name that is a known BIOS / firmware / key file (scph1001.bin, dc_boot.bin, gba_bios.bin ...)
    BIOS-MD5   a file whose content has the md5 of a known BIOS dump, whatever it is called
    ROM        a ROM, disc or cartridge extension (.sfc .gba .iso .chd .cue ...)
    CORE       a *_libretro.so/.dll/.dylib (a third-party core must never ride along in the repository)
    IMAGE      a ROCKNIX-style disk image found inside a tree (.img, .img.gz, .squashfs ...)
    ARCHIVE    a .7z/.rar that cannot be looked into on this machine
    HISTORY    the same checks on the names and blobs in the git history (--history)

Archives found inside a tree (.zip, .tar.gz ...) are opened and their members are checked the same way.
For a disk image the file list comes from `7z l` when 7-Zip is installed (it reads MBR/GPT, FAT, ext4, squashfs,
ISO). An .img, .img.xz or .img.gz is unpacked for real when 7z and unsquashfs are installed (partitions, FAT, ext4 and
ROCKNIX's squashfs SYSTEM; about 4 GB of $TMPDIR, deleted afterwards) and judged file by file, names and md5s.
Without them the image is only scanned for BIOS names in its raw bytes, which also hits names that are merely text in
configs and docs and cannot see into a compressed squashfs.

Exit code: 0 clean, 1 something found, 2 usage error. Nothing the script prints contains file contents.
The test is scripts/test-check-release-tree.sh.
"""
import fnmatch
import gzip
import hashlib
import lzma
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

HERE = Path(__file__).resolve().parent
REPO = HERE.parent

# --- what counts ---------------------------------------------------------------------------------------------

# Lower-case base names / globs of BIOS, firmware, key and boot-ROM files. Sources: the BIOS tables of the libretro
# documentation (docs.libretro.com/library/*), the PCSX2/DuckStation/Dolphin/Azahar/Flycast setup pages.
BIOS_NAMES = """
scph*  psxonpsp*  ps1_rom.bin  ps2-[0-9][0-9][0-9][0-9][a-z]-[0-9]*  ps2_bios*  mecha*.bin
sega_*.bin  mpr-*.bin  mpr-*.ic1  saturn_bios.bin  stvbios.zip
dc_boot.bin  dc_flash.bin  dc_bios.bin  dc_nvmem.bin  naomi.zip  awbios.zip  airlbios.zip
bios_cd_[uej].bin  bios_md.bin  bios_[uej].sms  bios.sms  bios.gg  sk.bin  sk2chip.bin  areplay.bin  ggenie.bin
gba_bios.bin  gb_bios.bin  gbc_bios.bin  sgb_bios.bin  sgb*.boot.rom  gb_boot.rom  gbc_boot.rom
dmg_boot.bin  dmg0_boot.bin  cgb_boot.bin  cgb0_boot.bin  agb_boot.bin  sgb_boot.bin  sgb2_boot.bin
bios7*  bios9*  biosnds7.rom  biosnds9.rom  dsi_*.bin  firmware.bin  nds_bios*
aes_keys.txt  seeddb.bin  boot9.bin  boot11.bin  secret_sector.bin  prod.keys  title.keys
ipl.bin  mcpx_*.bin  complete_bios.bin
syscard[123].pce  syscard3u.pce  gexpress.pce
lynxboot.img  5200.rom  7800*bios*  atarios[ab].rom  atarixl.rom  ataribas.rom  colecovision.rom  bios.col
exec.bin  grom.bin  o2rom.bin  c52.bin  g7400.bin  jopac.bin  disksys.rom  gamegenie.nes
bs-x.bin  stbios.bin  dsp[1-4]*.rom  st01[018].*.rom  st011.*.rom  cx4.data.rom  sgb[12].sfc
tos.img  kick[0-9]*.*  panafz*.bin  goldstar.bin  sanyotry.bin  3do_arcade_saot.bin
msx.rom  msx2.rom  msx2ext.rom  msx2p.rom  msx2pext.rom  neogeo.zip  neocdz.zip  bios.min
bios*.bin  *_bios.bin  *_bios.rom
""".split()

# Extensions of ROMs, cartridge dumps, disc images and similar (lower case, no dot). ".md" (Mega Drive) is handled
# separately because Markdown files are everywhere: only a binary .md counts. Container formats are opened instead.
ROM_EXTS = set("""
nes unf unif fds sfc smc fig swc gb gbc sgb gba agb nds dsi ids 3ds cci cxi cia n64 z64 v64 ndd
gen smd 32x gg sms sg sgx pce a26 a52 a78 lnx jag j64 ws wsc ngp ngc vb vboy
iso cue ccd mds mdf nrg cdi gdi chd cso ciso pbp gcm gcz rvz wbfs wad tgc nsp xci xiso
adf d64 t64 tzx cas z64 rom
""".split())

CORE_RE = re.compile(r"_libretro(_android)?\.(so|dll|dylib)$", re.I)
IMAGE_SUFFIXES = (".img", ".img.gz", ".img.xz", ".img.zst", ".squashfs", ".qcow2", ".vmdk", ".vdi", ".vhd")
TAR_SUFFIXES = (".tar", ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2")

# For the raw scan of an image we cannot open: BIOS names that show up as plain bytes in a directory entry.
RAW_NAME_RE = re.compile(
    rb"(?i)(scph[-_ ]?\d{4,5}\.(?:bin|rom)|psxonpsp\d*\.bin|ps2-\d{4}[a-z]-\d{8}|dc_(?:boot|flash|bios|nvmem)\.bin"
    rb"|gba_bios\.bin|gb_bios\.bin|gbc_bios\.bin|sgb_bios\.bin|bios[79]\.bin|biosnds[79]\.rom|sega_\d{3}\.bin"
    rb"|mpr-1\d{4}\.bin|saturn_bios\.bin|panafz\w*\.bin|syscard[123]u?\.pce|lynxboot\.img|bios_cd_[uej]\.bin"
    rb"|neogeo\.zip|aes_keys\.txt|prod\.keys|title\.keys|boot9\.bin|boot11\.bin)"
)

SKIP_DIRS = {".git", "target", "node_modules"}


class Finding:
    def __init__(self, kind, where, why):
        self.kind, self.where, self.why = kind, where, why

    def line(self):
        return f"  {self.kind:<9} {self.where}   ({self.why})"


class Checker:
    def __init__(self, allow, md5s, max_hash):
        self.allow = allow  # list of (glob, reason)
        self.md5s = md5s  # md5 -> note
        self.max_hash = max_hash
        self.found = []
        self.notes = []
        self.allowed = []
        self.checked = 0

    # -- the single decision point for one path (and optionally its content hash) -----------------------------
    def judge(self, rel, size=None, md5=None, binary=None, source=""):
        self.checked += 1
        rel = rel.replace("\\", "/")
        while rel.startswith("./"):
            rel = rel[2:]
        base = PurePosixPath(rel).name.lower()
        ext = base.rsplit(".", 1)[-1] if "." in base else ""
        hits = []
        for pat in BIOS_NAMES:
            if fnmatch.fnmatchcase(base, pat):
                hits.append(("BIOS", f"known BIOS/firmware file name ({pat})"))
                break
        if md5 and md5 in self.md5s:
            hits.append(("BIOS-MD5", "content hash is on the BIOS list: " + self.md5s[md5]))
        if ext in ROM_EXTS:
            hits.append(("ROM", f"ROM / disc extension .{ext}"))
        elif ext == "md" and binary:
            hits.append(("ROM", "binary .md (Mega Drive ROM)"))
        if CORE_RE.search(base):
            hits.append(("CORE", "libretro core binary"))
        if source == "tree" and base.endswith(IMAGE_SUFFIXES):
            hits.append(("IMAGE", "disk image inside a source tree"))
        for kind, why in hits:
            reason = self.allow_reason(rel)
            if reason is not None:
                self.allowed.append((kind, rel, reason))
            else:
                self.found.append(Finding(kind, rel, why))

    def allow_reason(self, rel):
        for pat, reason in self.allow:
            if fnmatch.fnmatchcase(rel, pat) or ("/" not in pat and fnmatch.fnmatchcase(PurePosixPath(rel).name, pat)):
                return reason
        return None


# --- inputs ------------------------------------------------------------------------------------------------

def read_allowlist(path):
    out = []
    if not path or not Path(path).exists():
        return out
    for n, raw in enumerate(Path(path).read_text().splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        glob, _, reason = line.partition("#")
        glob, reason = glob.strip(), reason.strip()
        if not reason:
            die(f"{path}:{n}: every allowlist entry needs a reason after '#': {glob}")
        out.append((glob, reason))
    return out


def read_md5s(path):
    out = {}
    if not path or not Path(path).exists():
        return out
    for raw in Path(path).read_text().splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = re.match(r"([0-9a-fA-F]{32})\s*(.*)", line)
        if m:
            out[m.group(1).lower()] = m.group(2).strip() or "listed"
    return out


def die(msg):
    print(f"check-release-tree: {msg}", file=sys.stderr)
    sys.exit(2)


def md5_of(fileobj, limit=None):
    h = hashlib.md5(usedforsecurity=False)
    for chunk in iter(lambda: fileobj.read(1 << 20), b""):
        h.update(chunk)
    return h.hexdigest()


def looks_binary(head):
    return b"\0" in head


# --- directory ---------------------------------------------------------------------------------------------

def repo_files(root):
    """Tracked plus untracked-but-not-ignored files, when root is a git work tree; else None."""
    if not (root / ".git").exists():
        return None
    try:
        out = subprocess.run(["git", "-C", str(root), "ls-files", "-co", "--exclude-standard", "-z"],
                             capture_output=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return [p for p in out.decode("utf-8", "surrogateescape").split("\0") if p]


def check_dir(ck, root, depth=0):
    files = repo_files(root) if depth == 0 else None
    if files is None:
        files = []
        for dp, dns, fns in os.walk(root):
            dns[:] = [d for d in dns if d not in SKIP_DIRS]
            for fn in fns:
                files.append(os.path.relpath(os.path.join(dp, fn), root))
    for rel in sorted(files):
        full = root / rel
        if full.is_symlink() or not full.is_file():
            continue
        size = full.stat().st_size
        md5 = None
        head = b""
        try:
            with open(full, "rb") as f:
                head = f.read(4096)
                if size <= ck.max_hash:
                    f.seek(0)
                    md5 = md5_of(f)
        except OSError:
            pass
        ck.judge(rel, size, md5, looks_binary(head), source="tree")
        check_nested(ck, full, rel, depth)


def check_nested(ck, full, rel, depth):
    """An archive lying inside a tree: look into it."""
    low = rel.lower()
    if depth >= 2:
        return
    if low.endswith(TAR_SUFFIXES) or low.endswith(".zip"):
        try:
            check_archive(ck, full, prefix=rel + "!")
        except Exception as e:  # corrupt archive: say so, do not pass silently
            ck.found.append(Finding("ARCHIVE", rel, f"cannot be read: {e}"))
    elif low.endswith((".7z", ".rar")):
        names = list_with_7z(full)
        if names is None:
            ck.found.append(Finding("ARCHIVE", rel, "7z/rar cannot be inspected here (install 7-Zip)"))
        else:
            for n in names:
                ck.judge(rel + "!" + n, source="archive")


# --- archives ----------------------------------------------------------------------------------------------

def check_archive(ck, path, prefix=""):
    low = str(path).lower()
    if low.endswith(".zip") or zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as z:
            for info in z.infolist():
                if info.is_dir():
                    continue
                md5 = None
                if info.file_size <= ck.max_hash:
                    with z.open(info) as f:
                        md5 = md5_of(f)
                ck.judge(prefix + info.filename, info.file_size, md5, source="archive")
        return True
    t, member = open_tar(path)
    if t is not None:
        with t:
            while member is not None:
                if member.isfile():
                    md5 = None
                    if member.size <= ck.max_hash:
                        f = t.extractfile(member)
                        if f is not None:
                            md5 = md5_of(f)
                    ck.judge(prefix + member.name, member.size, md5, source="archive")
                member = t.next()
        return True
    return False


def open_tar(path):
    """(tarfile, first member) if `path` is a tar (any compression) with at least one member, else (None, None).
    A zero-filled disk image is not a tar even though tarfile.is_tarfile() says so."""
    try:
        t = tarfile.open(path, "r:*")
        first = t.next()
    except (tarfile.TarError, OSError, EOFError):
        return None, None
    if first is None:
        t.close()
        return None, None
    return t, first


def list_with_7z(path):
    exe = shutil.which("7zz") or shutil.which("7z") or shutil.which("7za")
    if not exe:
        return None
    r = subprocess.run([exe, "l", "-slt", "-ba", str(path)], capture_output=True)
    if r.returncode not in (0, 1):  # 1 = warnings (e.g. trailing data), the list is still good
        return None
    names = []
    for line in r.stdout.decode("utf-8", "replace").splitlines():
        if line.startswith("Path = "):
            names.append(line[7:])
    return names or None


def check_image(ck, path):
    """A raw disk image or a compressed single file: names through 7-Zip if possible, else raw bytes."""
    shown = os.path.basename(path)
    deep = check_image_deep(ck, path)
    if deep:
        return deep
    names = list_with_7z(path)
    if names is not None:
        for n in names:
            ck.judge(n, source="archive")
        return "listed with 7-Zip"
    low = str(path).lower()
    opener = gzip.open if low.endswith(".gz") else lzma.open if low.endswith(".xz") else open
    seen = set()
    with opener(path, "rb") as f:
        tail = b""
        while True:
            chunk = f.read(16 << 20)
            if not chunk:
                break
            buf = tail + chunk
            for enc in (None, "utf-16-le"):
                for m in RAW_NAME_RE.finditer(buf if enc is None else _utf16_view(buf)):
                    seen.add(m.group(0).decode("latin-1").lower())
            tail = buf[-256:]
    for name in sorted(seen):
        ck.judge(f"{shown}/{name}", source="archive")
    return "raw byte scan only (no 7-Zip): files inside compressed or unknown file systems are not seen"


def md5_of_file(path):
    with open(path, "rb") as f:
        return md5_of(f)


def pinned_system_md5s():
    """The md5 of the ROCKNIX SYSTEM the image is built on (scripts/image/pins.sh, checked by scripts/image/check.sh)."""
    try:
        text = (HERE / "image" / "pins.sh").read_text()
    except OSError:
        return set()
    return set(re.findall(r"^SYSTEM_MD5=([0-9a-f]{32})", text, re.M))


ROCKNIX_SYSTEM_MD5S = pinned_system_md5s()


def check_base_system(ck, sysdir, shown, notes):
    """ROCKNIX's own SYSTEM, unchanged (its md5 is the pinned one). It ships ROCKNIX's emulator cores, and configs that
    name BIOS files, so names alone are not a finding here. What must hold: no file has the md5 of a known BIOS dump
    (an empty allowlist: nothing is excused), and every other name hit is counted and listed in a note, with the
    BIOS-named files by name so a reader sees that the 14 names a byte scan finds are text, not files."""
    sub = Checker([], ck.md5s, ck.max_hash)
    check_dir(sub, sysdir, depth=1)
    ck.checked += sub.checked
    by = {}
    for f in sub.found:
        if f.kind == "BIOS-MD5":
            ck.found.append(Finding(f.kind, f"{shown}!SYSTEM:{f.where}", f.why))
        else:
            by.setdefault(f.kind, []).append(f.where)
    kinds = ", ".join(f"{len(v)} {k}" for k, v in sorted(by.items())) or "no name hits"
    notes.append(f"SYSTEM is ROCKNIX's, unchanged (md5 pinned in scripts/image/pins.sh): {sub.checked} entries, "
                 f"none with the md5 of a known BIOS dump; names only, not judged: {kinds}")
    for w in sorted(by.get("BIOS", [])):
        if "!" not in w:
            notes.append(f"  BIOS-named file in ROCKNIX's SYSTEM: {w}")
    raw_names = [w for w in by.get("BIOS", []) if RAW_NAME_RE.search(PurePosixPath(w).name.encode())]
    notes.append(f"  of those, {len(raw_names)} match the names a raw byte scan flags (neogeo.zip, dc_boot.bin ...)")


def check_image_deep(ck, path):
    """An .img / .img.xz / .img.gz: unpack it for real and judge the files, not the bytes.

    The partitions come out through 7-Zip, the FAT and ext4 ones are unpacked and checked as trees (names, extensions
    and the md5 list), and a squashfs inside (ROCKNIX's SYSTEM) is unpacked with unsquashfs and checked the same way.
    A byte scan finds BIOS names that only occur as text inside configs and docs; this finds files. Needs 7z and
    unsquashfs and room for the unpacked image in $TMPDIR (about 4 GB); returns None when either is missing, and the
    caller falls back to the listing / raw scan. Everything unpacked is deleted again."""
    exe = shutil.which("7zz") or shutil.which("7z") or shutil.which("7za")
    unsq = shutil.which("unsquashfs")
    low = str(path).lower()
    if not exe or not unsq or not low.endswith((".img", ".img.xz", ".img.gz")):
        return None
    tmp = Path(tempfile.mkdtemp(prefix="check-release-tree-"))
    notes = []
    try:
        raw = Path(path)
        if not low.endswith(".img"):
            raw = tmp / "raw.img"
            opener = gzip.open if low.endswith(".gz") else lzma.open
            with opener(path, "rb") as src, open(raw, "wb") as dst:
                shutil.copyfileobj(src, dst, 16 << 20)
        parts = tmp / "parts"
        r = subprocess.run([exe, "x", "-y", f"-o{parts}", str(raw)], capture_output=True)
        if r.returncode not in (0, 1) or not parts.is_dir():
            return None
        if raw != Path(path):
            raw.unlink()
        for part in sorted(parts.iterdir()):
            tree = tmp / ("tree-" + part.name)
            r = subprocess.run([exe, "x", "-y", f"-o{tree}", str(part)], capture_output=True)
            if (r.returncode not in (0, 1) or not tree.is_dir()) and shutil.which("debugfs"):
                # 7-Zip does not read every ext4 (STORAGE): debugfs does, without mounting anything
                tree.mkdir(exist_ok=True)
                r = subprocess.run(["debugfs", "-R", f"rdump / {tree}", str(part)], capture_output=True)
            if r.returncode not in (0, 1) or not tree.is_dir():
                ck.found.append(Finding("ARCHIVE", f"{os.path.basename(path)}!{part.name}", "partition cannot be unpacked"))
                continue
            part.unlink()
            for sq in sorted(tree.iterdir()):
                if sq.is_file() and sq.read_bytes()[:4] == b"hsqs":  # squashfs magic
                    sysdir = tmp / ("squashfs-" + sq.name)
                    r = subprocess.run([unsq, "-no-progress", "-no-xattrs", "-d", str(sysdir), str(sq)], capture_output=True)
                    if r.returncode != 0:
                        ck.found.append(Finding("ARCHIVE", f"{os.path.basename(path)}!{sq.name}", "squashfs cannot be unpacked"))
                        continue
                    if md5_of_file(sq) in ROCKNIX_SYSTEM_MD5S:
                        check_base_system(ck, sysdir, os.path.basename(path), notes)
                    else:
                        check_dir(ck, sysdir, depth=1)
                    sq.unlink()
                    shutil.rmtree(sysdir, ignore_errors=True)
            check_dir(ck, tree, depth=1)
            shutil.rmtree(tree, ignore_errors=True)
        ck.notes.extend(notes)
        return "unpacked and checked file by file (partitions, FAT, ext4, squashfs)"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def _utf16_view(buf):
    # FAT long names are UTF-16LE: keep every second byte of the stream when the high byte is zero.
    # Cheap approximation: drop NULs, which turns "S\0C\0P\0H\0" into "SCPH".
    return buf.replace(b"\0", b"")


# --- history -----------------------------------------------------------------------------------------------

def check_history(ck, root):
    """Every path and every small blob that was ever committed on any ref."""
    def vcs(*args, data=None):
        return subprocess.run(["git", "-C", str(root), *args], input=data, capture_output=True, check=True).stdout

    try:
        listing = vcs("rev-list", "--objects", "--all").decode("utf-8", "replace").splitlines()
    except (OSError, subprocess.CalledProcessError):
        die("--history needs a git repository")
    paths = {}  # object id -> a path it had
    for line in listing:
        sha, _, name = line.partition(" ")
        if name:
            paths.setdefault(sha, name)
    before = len(ck.found)
    for name in sorted(set(paths.values())):
        ck.judge(name, source="history")
    for f in ck.found[before:]:
        f.why += "; in git history"
    ids = "\n".join(paths).encode() + b"\n"
    small = []
    for line in vcs("cat-file", "--batch-check=%(objectname) %(objecttype) %(objectsize)", data=ids).decode().splitlines():
        sha, typ, size = line.split()
        if typ == "blob" and int(size) <= ck.max_hash:
            small.append(sha)
    for i in range(0, len(small), 300):
        out = vcs("cat-file", "--batch", data="\n".join(small[i:i + 300]).encode() + b"\n")
        pos = 0
        while pos < len(out):
            nl = out.index(b"\n", pos)
            sha, _typ, size = out[pos:nl].decode().split()
            size = int(size)
            data = out[nl + 1:nl + 1 + size]
            pos = nl + 1 + size + 1
            md5 = hashlib.md5(data, usedforsecurity=False).hexdigest()
            if md5 in ck.md5s:
                ck.found.append(Finding("HISTORY", f"blob {sha[:10]} ({paths.get(sha, '?')})", "BIOS hash: " + ck.md5s[md5]))


# --- main --------------------------------------------------------------------------------------------------

def main(argv):
    allow_path = HERE / "release-tree-allowlist.txt"
    md5_path = HERE / "bios-md5.txt"
    max_mib = 16
    verbose = history = False
    targets = []
    it = iter(argv)
    for a in it:
        if a == "--allowlist":
            allow_path = next(it, None) or die("--allowlist needs a file")
        elif a == "--md5-list":
            md5_path = next(it, None) or die("--md5-list needs a file")
        elif a == "--max-hash-mib":
            max_mib = int(next(it, None) or die("--max-hash-mib needs a number"))
        elif a == "--history":
            history = True
        elif a == "-v":
            verbose = True
        elif a in ("-h", "--help"):
            print(__doc__)
            return 0
        elif a.startswith("-") and a != "-":
            die(f"unknown option {a}")
        else:
            targets.append(a)
    ck = Checker(read_allowlist(allow_path), read_md5s(md5_path), max_mib << 20)
    if not ck.md5s and str(md5_path) != "/dev/null":
        print(f"note: no hashes loaded from {md5_path}", file=sys.stderr)

    if not targets:
        targets = [str(REPO)]
    notes = []
    for t in targets:
        p = Path(t)
        if not p.exists():
            die(f"{t}: no such file or directory")
        if p.is_dir():
            check_dir(ck, p.resolve())
        else:
            low = t.lower()
            if low.endswith(".7z") or low.endswith(".rar"):
                names = list_with_7z(p)
                if names is None:
                    ck.found.append(Finding("ARCHIVE", t, "cannot be inspected here (install 7-Zip)"))
                for n in names or []:
                    ck.judge(n, source="archive")
            elif not check_archive(ck, p):
                notes.append(f"{t}: {check_image(ck, p)}")
    if history:
        check_history(ck, REPO if len(targets) == 1 and Path(targets[0]).resolve() == REPO else Path(targets[0]).resolve())

    for n in notes + ck.notes:
        print(f"note: {n}")
    if verbose:
        for kind, rel, reason in ck.allowed:
            print(f"  allowed   {rel}   [{kind}] {reason}")
    if ck.found:
        print(f"FOUND {len(ck.found)} problem(s) in {', '.join(targets)}:")
        for f in ck.found:
            print(f.line())
        print("No BIOS, no ROMs, no third-party cores in a release. Remove them, or allowlist the path with a reason.")
        return 1
    print(f"OK: {ck.checked} entries checked in {', '.join(targets)}; nothing found"
          + (f" ({len(ck.allowed)} allowlisted)" if ck.allowed else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
