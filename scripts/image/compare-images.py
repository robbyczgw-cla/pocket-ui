#!/usr/bin/env python3
"""Compare two SD-card images (.img.xz or .img) file by file, without root and without loop mounts.

    compare-images.py A.img.xz B.img.xz [--work DIR] [--keep] [--rules FILE] [--quiet]

What it does: unpacks both images, compares the partition table (start, size, type, flags, label ids) and the
bytes outside the partitions, then lists each partition with `7z` (FAT, ext4) and every squashfs found inside
(SYSTEM) with `unsquashfs`, and compares every file by SHA-256 plus type, mode and symlink target.

The pocket-ui version folder on STORAGE (releases/0.1.11 vs releases/0.1.12, taken from the images themselves, so a name like
0.3.0-dryrun works) counts as the same, with the links to it; digits elsewhere (SYSTEM's libmnl.so.0.2.0) are left alone.

Differences that are expected between two builds (pocket-ui binaries, version stamps, README / source-offer / licence
texts) are listed under ALLOWED. Everything else is a FINDING. The rules are the ALLOWED table below; change them
in this file, or pass --rules FILE (one `regex<TAB>reason` per line) to replace them.

Exit code: 0 no findings, 1 findings, 2 usage or unpack error.
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile

# (regex on "<partition>:<path>[!<squashfs path>]", reason). First match wins. Paths are relative to the partition.
ALLOWED = [
    (r"(^|[/!])(pocket-ui|pocket-core|pocket-splash)$", "pocket-ui binary"),
    (r"(^|[/!])pocket-ui[^/]*\.(sha256|md5)$", "pocket-ui checksum"),
    (r"(^|/)(README|SOURCE-OFFER|LICENSE|LICENCE|NOTICE|THIRD-PARTY|CREDITS)[^/]*$", "text: README / source offer / licence"),
    (r"(^|/)(licenses?|licences?)/", "licence notes"),
    (r"(^|[/!])(VERSION|version|pocket-ui-os\.txt|pocket-ui\.version|\.pocket-ui-version|BUILD-INFO[^/]*)$", "version stamp"),
    (r"(^|[/!])usr/share/pocket-ui/(manual|docs)/", "on-device manual (built from docs)"),
    # Scripts of the program that travel with it (deploy/): they change whenever the program's launch does.
    (r"^p2:pocket-ui/run-pocket-ui\.sh$", "launcher script of the program (deploy/run-pocket-ui.sh)"),
    (r"^p2:pocket-ui/releases/[^/]+/portmaster-setup\.sh$", "helper script of the program (deploy/portmaster-setup.sh)"),
]

CHUNK = 1 << 20


def run(cmd, **kw):
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def unpack(path, work, name):
    """Return the raw image path; .xz is decompressed into the work dir."""
    if not path.endswith(".xz"):
        return path
    out = os.path.join(work, name + ".img")
    with open(out, "wb") as f:
        p = subprocess.run(["nice", "-n", "19", "xz", "-dc", path], stdout=f)
    if p.returncode:
        sys.exit(f"compare-images: xz failed on {path}")
    return out


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            b = f.read(CHUNK)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


def sha256_range(img, off, length):
    h = hashlib.sha256()
    with open(img, "rb") as f:
        f.seek(off)
        left = length
        while left > 0:
            b = f.read(min(CHUNK, left))
            if not b:
                break
            h.update(b)
            left -= len(b)
    return h.hexdigest()


def partition_table(img):
    p = run(["sfdisk", "-J", img])
    if p.returncode:
        sys.exit(f"compare-images: sfdisk failed on {img}: {p.stderr.strip()}")
    pt = json.loads(p.stdout)["partitiontable"]
    parts = []
    for q in pt["partitions"]:
        parts.append({
            "start": q["start"], "size": q["size"], "type": q["type"],
            "bootable": q.get("bootable", False), "name": q.get("name", ""),
            "uuid": q.get("uuid", ""),
        })
    return {"label": pt["label"], "id": pt.get("id", ""), "parts": parts}


def raw_gaps(img, parts):
    """Hashes of the bytes before the first partition and between partitions (bootloader area etc.)."""
    size = os.path.getsize(img)
    spans, pos = [], 0
    for q in sorted(parts, key=lambda q: q["start"]):
        s = q["start"] * 512
        if s > pos:
            spans.append((pos, s - pos))
        pos = max(pos, (q["start"] + q["size"]) * 512)
    if size > pos:
        spans.append((pos, size - pos))
    return {f"{o}+{n}": sha256_range(img, o, n) for o, n in spans}


def carve(img, q, out):
    with open(img, "rb") as f, open(out, "wb") as o:
        f.seek(q["start"] * 512)
        left = q["size"] * 512
        while left > 0:
            b = f.read(min(CHUNK, left))
            if not b:
                break
            o.write(b)
            left -= len(b)


def walk(root):
    """{relative path: (kind, mode, size, sha256|link target)} for every entry under root.

    Mode is the permission bits (7777). Entries that came out unreadable (mode 000 in squashfs) get u+rw only
    after their mode was recorded."""
    out = {}

    def rec(d):
        for n in sorted(os.listdir(d)):
            full = os.path.join(d, n)
            rel = os.path.relpath(full, root)
            st = os.lstat(full)
            mode = stat.S_IMODE(st.st_mode)
            if stat.S_ISLNK(st.st_mode):
                out[rel] = ("link", 0, 0, os.readlink(full))
            elif stat.S_ISDIR(st.st_mode):
                out[rel] = ("dir", mode, 0, "")
                os.chmod(full, mode | 0o700)
                rec(full)
            elif stat.S_ISREG(st.st_mode):
                os.chmod(full, mode | 0o600)
                out[rel] = ("file", mode, st.st_size, sha256_file(full))
            else:
                out[rel] = ("special", 0, 0, "")

    rec(root)
    return out


def is_ext(path):
    with open(path, "rb") as f:
        f.seek(1080)
        return f.read(2) == b"\x53\xef"


def extract_7z(part_file, dest):
    os.makedirs(dest)
    if is_ext(part_file):
        # debugfs keeps symlinks that point upward (7z drops those as "dangerous") and needs no root
        p = run(["debugfs", "-R", f"rdump / {dest}", part_file])
        return (dest, "") if os.listdir(dest) else (None, p.stderr.strip()[-300:])
    p = run(["nice", "-n", "19", "7z", "x", "-y", "-snl", f"-o{dest}", part_file])
    if p.returncode not in (0, 1) and not os.listdir(dest):
        return None, p.stderr.strip() or p.stdout.strip()[-300:]
    return dest, (p.stderr.strip() if p.returncode else "")


def is_squashfs(path):
    try:
        with open(path, "rb") as f:
            return f.read(4) in (b"hsqs", b"sqsh")
    except OSError:
        return False


def inventory(img, tag, work, notes):
    """Unpack one image into {partition: {path: entry}}; squashfs files are expanded under 'file!inner/path'."""
    pt = partition_table(img)
    inv = {}
    for i, q in enumerate(pt["parts"], 1):
        pf = os.path.join(work, f"{tag}-p{i}.raw")
        carve(img, q, pf)
        dest = os.path.join(work, f"{tag}-p{i}")
        d, err = extract_7z(pf, dest)
        os.unlink(pf)
        key = f"p{i}"
        if d is None:
            # not a filesystem 7z knows (raw boot area etc.): compare it as one blob
            inv[key] = {"<raw partition>": ("file", 0, q["size"] * 512, sha256_range(img, q["start"] * 512, q["size"] * 512))}
            notes.append(f"{tag} {key}: not listed as a file system, compared as raw bytes ({err[:80]})")
            continue
        if err:
            notes.append(f"{tag} {key}: 7z said: {err[:200]}")
        tree = walk(d)
        for rel in sorted(tree):
            full = os.path.join(d, rel)
            if tree[rel][0] == "file" and is_squashfs(full):
                sq = os.path.join(work, f"{tag}-p{i}-sq-{rel.replace('/', '_')}")
                p = run(["nice", "-n", "19", "unsquashfs", "-no-xattrs", "-f", "-d", sq, full])
                if not os.path.isdir(sq):
                    notes.append(f"{tag} {key}:{rel}: unsquashfs failed: {p.stderr.strip()[:200]}")
                    continue
                if p.returncode:
                    notes.append(f"{tag} {key}:{rel}: unsquashfs warnings (device nodes need root; skipped): "
                                 f"{p.stderr.strip().splitlines()[0][:120] if p.stderr.strip() else p.returncode}")
                for r2, e in walk(sq).items():
                    tree[f"{rel}!{r2}"] = e
                shutil.rmtree(sq, ignore_errors=True)
        inv[key] = tree
        shutil.rmtree(d, ignore_errors=True)
    return pt, inv


def release_dir(inv):
    """The one version folder in /pocket-ui/releases of the image's STORAGE (its pocket-ui version), or None."""
    names = set()
    for part, tree in inv.items():
        for rel, e in tree.items():
            m = re.fullmatch(r"pocket-ui/releases/([^/]+)", rel)
            if m and e[0] == "dir":
                names.add(m.group(1))
    return names.pop() if len(names) == 1 else None


def renumber(tree, va, vb):
    """Rewrite version A to version B in A's pocket-ui/releases/<version> paths and in links to it (current), so releases/0.1.11
    lines up with releases/0.1.12. Only there: the same digits elsewhere (SYSTEM's libmnl.so.0.2.0) are not a version of ours."""
    if not va or not vb or va == vb:
        return tree
    out = {}
    for rel, e in tree.items():
        if e[0] == "link":
            e = (e[0], e[1], e[2], e[3].replace(f"releases/{va}", f"releases/{vb}"))
        out[rel.replace(f"pocket-ui/releases/{va}", f"pocket-ui/releases/{vb}")] = e
    return out


def load_rules(path):
    if not path:
        return [(re.compile(r), why) for r, why in ALLOWED]
    rules = []
    for line in open(path):
        line = line.rstrip("\n")
        if line and not line.startswith("#"):
            r, _, why = line.partition("\t")
            rules.append((re.compile(r), why or "allowed"))
    return rules


def classify(key, rules):
    for rx, why in rules:
        if rx.search(key):
            return why
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("a")
    ap.add_argument("b")
    ap.add_argument("--work", help="scratch dir (default: a new temp dir; needs a few GB)")
    ap.add_argument("--keep", action="store_true", help="keep the scratch dir")
    ap.add_argument("--rules", help="file with `regex<TAB>reason` lines replacing the built-in ALLOWED list")
    ap.add_argument("--quiet", action="store_true", help="do not list the allowed differences")
    a = ap.parse_args()
    for t in ("7z", "unsquashfs", "sfdisk", "xz"):
        if not shutil.which(t):
            sys.exit(f"compare-images: {t} not found")
    rules = load_rules(a.rules)
    work = a.work or tempfile.mkdtemp(prefix="compare-images-")
    os.makedirs(work, exist_ok=True)
    notes, findings, allowed = [], [], []
    try:
        ia, ib = unpack(a.a, work, "a"), unpack(a.b, work, "b")
        pa, pb = partition_table(ia), partition_table(ib)
        if pa["label"] != pb["label"]:
            findings.append(f"partition table type: {pa['label']} vs {pb['label']}")
        if pa["id"] != pb["id"]:
            findings.append(f"disk id: {pa['id']} vs {pb['id']}")
        if len(pa["parts"]) != len(pb["parts"]):
            findings.append(f"partition count: {len(pa['parts'])} vs {len(pb['parts'])}")
        for i, (x, y) in enumerate(zip(pa["parts"], pb["parts"]), 1):
            for k in x:
                if x[k] != y[k]:
                    findings.append(f"partition {i} {k}: {x[k]} vs {y[k]}")
        if os.path.getsize(ia) != os.path.getsize(ib):
            findings.append(f"image size: {os.path.getsize(ia)} vs {os.path.getsize(ib)}")
        ga, gb = raw_gaps(ia, pa["parts"]), raw_gaps(ib, pb["parts"])
        for span in sorted(set(ga) | set(gb)):
            if ga.get(span) != gb.get(span):
                findings.append(f"bytes outside partitions ({span} = offset+length) differ")
        _, inva = inventory(ia, "a", work, notes)
        _, invb = inventory(ib, "b", work, notes)
        va, vb = release_dir(inva), release_dir(invb)
        if va and vb and va != vb:
            notes.append(f"pocket-ui/releases/{va} in A is read as releases/{vb} (and the links to it)")
            inva = {k: renumber(t, va, vb) for k, t in inva.items()}
        total = 0
        for part in sorted(set(inva) | set(invb)):
            ta, tb = inva.get(part, {}), invb.get(part, {})
            total += len(ta) + len(tb)
            for rel in sorted(set(ta) | set(tb)):
                ea, eb = ta.get(rel), tb.get(rel)
                if ea == eb:
                    continue
                if ea is None:
                    what = f"only in B ({eb[0]})"
                elif eb is None:
                    what = f"only in A ({ea[0]})"
                elif ea[0] != eb[0]:
                    what = f"type {ea[0]} vs {eb[0]}"
                elif ea[1] != eb[1]:
                    what = f"mode {ea[1]:o} vs {eb[1]:o}" if ea[3] == eb[3] else f"content and mode differ ({ea[2]} vs {eb[2]} bytes)"
                elif ea[0] == "link":
                    what = f"link target {ea[3]} vs {eb[3]}"
                else:
                    what = f"content differs ({ea[2]} vs {eb[2]} bytes)"
                key = f"{part}:{rel}"
                why = classify(key, rules)
                (allowed if why else findings).append(f"{key}: {what}" + (f"  [{why}]" if why else ""))
        print(f"A: {a.a}\nB: {a.b}\npartitions: {len(pa['parts'])}, entries compared: {total // 2}")
        for n in notes:
            print("note:", n)
        if allowed and not a.quiet:
            print(f"\nALLOWED differences ({len(allowed)}):")
            for s in allowed:
                print("  ", s)
        elif allowed:
            print(f"\nALLOWED differences: {len(allowed)} (hidden by --quiet)")
        print(f"\nFINDINGS ({len(findings)}):" if findings else "\nFINDINGS: none, the images match apart from the allowed differences")
        for s in findings:
            print("  ", s)
        return 1 if findings else 0
    finally:
        if not a.keep:
            shutil.rmtree(work, ignore_errors=True)
        else:
            print(f"scratch kept: {work}")


if __name__ == "__main__":
    sys.exit(main())
