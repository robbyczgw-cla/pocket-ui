#!/usr/bin/env python3
"""The update file for handhelds without Wi-Fi: one zip per kind of card, copied onto the card as it is
(docs/UPDATE-CHANNEL.md "The update file", `update::card` on the device).

    python3 -I scripts/make-card-update.py --channel DIR [--assets DIR ...] [--out DIR] [--verify STORE_TOOL]

DIR is a channel folder as the release scripts write it, with `update.json`, `update.json.sig` and the app's tarball the
list names (`site-updates/updates/stable/`, `brick-public/updates/brick/`, dArkOS' `ota/updates/darkos/`). `--assets`
folders hold the Brick's and dArkOS' component packages (`assets/` beside the list, the release's new parts); every
package the signed tarball's `asset-packages.json` names that is found there goes in, so the device needs no server for
it. A package that isn't found is listed and, with `--all-assets`, fails the run.

Writes `pocket-ui-update-<version>-<rocknix|trimui|darkos>.zip` (stored, not compressed: the device reads the tarball
straight from the card; the same bytes for the same input) and prints its name, size and SHA-256. Nothing is signed or
published here: the list and its signature are the channel's own, byte for byte. With `--verify` the signature is checked
first with a built store-tool (`store-tool verify`).
"""
import argparse
import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path

README = """pocket-ui update {version} for handhelds without Wi-Fi ({kind})

Copy this file onto the SD card as it is. Do not unpack it.
- RG28XX card: onto the GAMES drive. Other pocket-ui cards: onto the small first part your computer shows, or onto
  the games card in the second slot.
- TrimUI Brick: onto the card, next to the pocket-ui folder.
- dArkOS: onto the EASYROMS part.
Put the card back and switch on: pocket-ui asks "Update to pocket-ui {version} found on the card". Press A.
Your games, saves and settings stay. More: https://pocketui.xyz/install
"""


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def target_of(listing):
    """The same rule as `update::card::target_of`."""
    if "target" in listing:
        return "darkos" if listing["target"] == "darkos-port" and not listing.get("os") else None
    name = listing["app"]["url"].rsplit("/", 1)[-1]
    if name.startswith("pocket-ui-brick-"):
        return "trimui"
    if name.startswith("pocket-ui-") and name.endswith("-aarch64.tar.gz"):
        return "rocknix"
    return None


def find(folders, size, digest, name=None):
    """A file of exactly this size and SHA-256 in `folders`, the named one first."""
    for folder in folders:
        if name and (folder / name).is_file() and (folder / name).stat().st_size == size and sha256(folder / name) == digest:
            return folder / name
    for folder in folders:
        for p in sorted(folder.iterdir()) if folder.is_dir() else []:
            if p.is_file() and p.stat().st_size == size and sha256(p) == digest:
                return p
    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--channel", required=True, type=Path)
    ap.add_argument("--assets", action="append", default=[], type=Path)
    ap.add_argument("--out", type=Path, default=Path("."))
    ap.add_argument("--verify", help="a built store-tool: check the list's signature first")
    ap.add_argument("--all-assets", action="store_true", help="fail when a component package isn't found")
    a = ap.parse_args()

    raw = (a.channel / "update.json").read_bytes()
    sig = (a.channel / "update.json.sig").read_bytes()
    if a.verify:
        subprocess.run([a.verify, "verify", str(a.channel / "update.json")], check=True, stdout=subprocess.DEVNULL)
    listing = json.loads(raw)
    if listing.get("format") != 1 or not listing.get("app"):
        sys.exit("make-card-update: the list has no app (format 1)")
    kind = target_of(listing)
    if kind is None:
        sys.exit("make-card-update: can't tell which card this list is for")
    app = listing["app"]
    version = app["version"]
    if not version or any(c in version for c in "/\\\n\r\0"):
        sys.exit(f"make-card-update: unusable version {version!r}")
    name = app["url"].rsplit("/", 1)[-1]
    tarball = find([a.channel] + a.assets, app["size"], app["sha256"].lower(), name)
    if tarball is None:
        sys.exit(f"make-card-update: {name} ({app['size']} bytes, the signed SHA-256) is not in {a.channel}")

    packages, missing = [], []
    with tarfile.open(tarball, "r:gz") as t:
        member = next((m for m in t.getmembers() if m.name.lstrip("./") == "asset-packages.json"), None)
        inventory = json.load(t.extractfile(member)) if member else {"assets": []}
    folders = a.assets
    seen = set()
    for asset in inventory.get("assets", []):
        pkg = asset["package"]
        if pkg["sha256"] in seen:
            continue
        seen.add(pkg["sha256"])
        found = find(folders, pkg["size"], pkg["sha256"].lower(), pkg["url"].rsplit("/", 1)[-1])
        (packages if found else missing).append(found or asset["path"])
    if missing:
        more = f" and {len(missing) - 5} more" if len(missing) > 5 else ""
        print(f"make-card-update: {len(missing)} component package(s) not in the given folders (the device keeps its installed"
              f" ones): {', '.join(missing[:5])}{more}", file=sys.stderr)
        if a.all_assets:
            sys.exit(1)

    a.out.mkdir(parents=True, exist_ok=True)
    out = a.out / f"pocket-ui-update-{version}-{kind}.zip"
    when = int(os.environ.get("SOURCE_DATE_EPOCH", "1735689600"))
    import time
    stamp = time.gmtime(max(when, 315532800))[:6]

    def put(z, arcname, data=None, path=None):
        info = zipfile.ZipInfo(arcname, date_time=stamp)
        info.compress_type = zipfile.ZIP_STORED
        info.external_attr = 0o100644 << 16
        if path is not None:
            with open(path, "rb") as src, z.open(info, "w", force_zip64=False) as dst:
                for block in iter(lambda: src.read(1 << 20), b""):
                    dst.write(block)
        else:
            z.writestr(info, data)

    tmp = out.with_suffix(".zip.part")
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_STORED, allowZip64=False) as z:
        put(z, "update.json", raw)
        put(z, "update.json.sig", sig)
        put(z, name, path=tarball)
        for p in sorted(packages, key=lambda p: p.name):
            put(z, f"assets/{p.name}", path=p)
        put(z, "README.txt", README.format(version=version, kind=kind).encode())
    with zipfile.ZipFile(tmp) as z:
        bad = z.testzip()
        if bad:
            sys.exit(f"make-card-update: {bad} reads back wrong")
    tmp.replace(out)
    print(f"{out.name}\t{out.stat().st_size}\t{sha256(out)}\t{len(packages)} package(s)")


if __name__ == "__main__":
    main()
