#!/usr/bin/env python3
"""PPSSPP's system files for the Brick: what the libretro core reads from <system dir>/PPSSPP (ppge_atlas.zim, flash0 fonts,
vfpu tables, lang/, compat.ini). Without them PSP dialogs (saving) draw nothing and the VFPU falls back to approximations.

  python3 -I scripts/brick-ppsspp-data.py SOURCE OUT_ASSETS

SOURCE is the PPSSPP source tree or its archive from brick-build-cores.sh (sources/ppsspp-<commit>.tar.gz). The files of its
assets/ folder (the same pinned set the Android package carries, scripts/android/package-cores.py; the web debugger
submodule is left out) go to OUT_ASSETS/bios/PPSSPP; brick-make-card.sh puts that into app/bios-data and Roms/bios
(= /storage/roms/bios, pocket-core's system dir), and app/start.sh seeds it once per version."""
from pathlib import Path, PurePosixPath
import shutil
import sys
import tarfile

REQUIRED = ['ppge_atlas.zim', 'compat.ini', 'flash0/font/ltn0.pgf', 'lang/en_US.ini', 'vfpu/vfpu_asin_lut65536.dat']


def wanted(rel):
    parts = PurePosixPath(rel).parts
    return bool(parts) and parts[0] != 'debugger' and '..' not in parts


def from_tree(source):
    root = source / 'assets'
    for p in sorted(root.rglob('*')):
        rel = p.relative_to(root).as_posix()
        if p.is_file() and not p.is_symlink() and wanted(rel):
            yield rel, p.read_bytes()


def from_archive(source):
    with tarfile.open(source) as t:
        members = [m for m in t.getmembers() if m.isfile()]
        names = {m.name for m in members}
        prefix = next((p for p in ('assets/', './assets/') if p + 'ppge_atlas.zim' in names), None)
        if prefix is None:
            prefix = next((n[:-len('ppge_atlas.zim')] for n in sorted(names)
                           if n.endswith('/assets/ppge_atlas.zim') and n.count('/') == 2), 'assets/')
        for m in sorted(members, key=lambda m: m.name):
            if m.name.startswith(prefix) and wanted(m.name[len(prefix):]):
                yield m.name[len(prefix):], t.extractfile(m).read()


def main(argv):
    if len(argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    source, out = Path(argv[1]), Path(argv[2]) / 'bios' / 'PPSSPP'
    files = list(from_tree(source) if source.is_dir() else from_archive(source))
    have = {rel for rel, _ in files}
    missing = [r for r in REQUIRED if r not in have]
    if missing:
        print(f'brick-ppsspp-data: {source} lacks {", ".join(missing)}', file=sys.stderr)
        return 1
    if out.exists():
        shutil.rmtree(out)
    for rel, data in files:
        target = out / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    print(f'PPSSPP: {len(files)} system files into {out}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
