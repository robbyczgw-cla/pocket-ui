#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Extract the pinned PM tree with FAT-safe regular-file link materialization."""
import importlib.util
from pathlib import Path
import stat
import sys


def policy():
    spec = importlib.util.spec_from_file_location('portmaster_archive', Path(__file__).with_name('portmaster-archive.py'))
    portable = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(portable)
    return portable


def extract(archive, dest):
    policy().extract_portmaster_tar(archive, dest)


def prepare(dest, revision):
    if len(revision) != 40 or any(c not in '0123456789abcdef' for c in revision):
        raise ValueError('PortMaster source revision must be its pinned Git SHA.')
    pm = Path(dest) / 'PortMaster'
    fonts = pm / 'pylibs/resources/NotoSans.tar.xz'
    if fonts.is_file():
        portable = policy()
        portable.extract_tar(fonts, fonts.parent,
                             portable.Limits(max_file=128 * 1024**2, max_total=128 * 1024**2,
                                             max_entries=32))
        if not (fonts.parent / 'NotoSansJP-Regular.ttf').is_file():
            raise ValueError('PortMaster Unicode font archive is incomplete.')
        # funcs.txt otherwise asks stock tar to reopen XZ at every game launch.
        # Python has validated and materialized the complete bounded tree first.
        fonts.unlink()
    marker = pm / 'resources/do_init'
    if marker.exists():
        portable = policy()
        entries = []
        for source in sorted((pm / 'pylibs/resources').glob('*.ttf')):
            metadata = source.lstat()
            if not stat.S_ISREG(metadata.st_mode):
                raise ValueError('PortMaster font is not a regular file.')
            entries.append(portable.Entry(source.name, 'file', source, metadata.st_size))
        if not entries:
            raise ValueError('PortMaster font initialization has no fonts.')
        tree = portable.Tree(entries, portable.Limits(max_file=128 * 1024**2,
                                                    max_total=128 * 1024**2, max_entries=32))
        # Reuse preflight free-space/name/link checks and bounded atomic file
        # copies. Both GUI and port-message font sets remain available. This
        # whole preparation is the installer's tracked, cancellable child.
        portable.materialize(tree, marker.parent, lambda source: source.open('rb'))
        marker.unlink()  # Upstream's first-game cp is skipped only after success.
    (pm / 'version').write_text('git-' + revision + '\n', encoding='utf-8')


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == '--copy':
        policy().copy_path(*sys.argv[2:])
    elif len(sys.argv) == 4 and sys.argv[1] == '--prepare':
        prepare(*sys.argv[2:])
    else:
        extract(*sys.argv[1:])
